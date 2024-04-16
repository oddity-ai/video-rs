extern crate ffmpeg_next as ffmpeg;

#[cfg(feature = "ndarray")]
use ndarray::Array3;

use ffmpeg::codec::codec::Codec;
use ffmpeg::codec::context::Context;
use ffmpeg::encoder::video::Video;
use ffmpeg::format::context::Output;
use ffmpeg::util::frame::video::Video as Frame;
use ffmpeg::{Error, Rational};

#[cfg(feature = "ndarray")]
use ffmpeg::util::format::Pixel;

use ffmpeg::ffi::*;

/// This function is similar to the existing bindings in ffmpeg-next like `output` and `output_as`,
/// but does not assume that it is opening a file-like context. Instead, it opens a raw output,
/// without a file attached.
///
/// Combined with the `output_raw_buf_start` and `output_raw_buf_end` functions, this can be used to
/// write to a buffer instead of a file.
///
/// # Arguments
///
/// * `format` - String to indicate the container format, like "mp4".
///
/// # Example
///
/// ```ignore
/// let output = ffi::output_raw("mp4");
///
/// output_raw_buf_start(&mut output);
/// output.write_header()?;
/// let buf output_raw_buf_end(&mut output);
/// println!("{}", buf.len());
/// ```
pub fn output_raw(format: &str) -> Result<Output, Error> {
    unsafe {
        let mut output_ptr = std::ptr::null_mut();
        let format = std::ffi::CString::new(format).unwrap();
        match avformat_alloc_output_context2(
            &mut output_ptr,
            std::ptr::null_mut(),
            format.as_ptr(),
            std::ptr::null(),
        ) {
            0 => Ok(Output::wrap(output_ptr)),
            e => Err(Error::from(e)),
        }
    }
}

/// This function initializes a dynamic buffer and inserts it into an output context to allow a
/// write to happen. Afterwards, the callee can use `output_raw_buf_end` to retrieve what was
/// written.
///
/// # Arguments
///
/// * `output` - Output context to start write on.
pub fn output_raw_buf_start(output: &mut Output) {
    unsafe {
        // Here we initialize a raw pointer (mutable) as nullptr initially. We then call the
        // `avio_open_dyn_buf` which expects a ptr ptr, and place the result in p. In case of
        // success, we override the `pb` pointer inside the output context to point to the dyn buf.
        let mut p: *mut AVIOContext = std::ptr::null_mut();
        match avio_open_dyn_buf((&mut p) as *mut *mut AVIOContext) {
            0 => {
                (*output.as_mut_ptr()).pb = p;
            }
            _ => {
                panic!("Failed to open dynamic buffer for output context.");
            }
        }
    }
}

/// This function cleans up the dynamic buffer used for the write and returns the buffer as a vector
/// of bytes.
///
/// # Arguments
///
/// * `output` - Output context to end write on.
pub fn output_raw_buf_end(output: &mut Output) -> Vec<u8> {
    unsafe {
        // First, we acquire a raw pointer to the AVIOContext in the `pb` field of the output
        // context. We stored the dyn buf there when we called `output_raw_buf_start`. Secondly, the
        // `close_dyn_buf` function will place a pointer to the starting address of the buffer in
        // `buffer_raw` through a ptr ptr. It also returns the size of that buffer.
        let output_pb = (*output.as_mut_ptr()).pb;
        let mut buffer_raw: *mut u8 = std::ptr::null_mut();
        let buffer_size = avio_close_dyn_buf(output_pb, (&mut buffer_raw) as *mut *mut u8) as usize;

        // Reset the `pb` field or `avformat_close` will try to free it!
        ((*output.as_mut_ptr()).pb) = std::ptr::null_mut::<AVIOContext>();

        // Create a Rust `Vec` from the buffer (copying).
        let buffer = std::slice::from_raw_parts(buffer_raw, buffer_size).to_vec();

        // Now deallocate the original backing buffer.
        av_free(buffer_raw as *mut std::ffi::c_void);

        buffer
    }
}

/// This function initializes an IO context for the `Output` that packetizes individual writes. Each
/// write is pushed onto a packet buffer (a collection of buffers, each being a packet).
///
/// The callee must invoke `output_raw_packetized_buf_end` soon after calling this function. The
/// `Vec` pointed to by `packet_buffer` must live between invocation of this function and
/// `output_raw_packetized_buf_end`!
///
/// Not calling `output_raw_packetized_buf_end` after calling this function will result in memory
/// leaking.
///
/// # Arguments
///
/// * `output` - Output context to start write on.
/// * `packet_buffer` - Packet buffer to push buffers onto. Must live until
///   `output_raw_packetized_buf`.
/// * `max_packet_size` - Maximum size per packet.
pub fn output_raw_packetized_buf_start(
    output: &mut Output,
    packet_buffer: &mut Vec<Vec<u8>>,
    max_packet_size: usize,
) {
    unsafe {
        let buffer = av_malloc(max_packet_size) as *mut u8;

        // Create a custom IO context around our buffer.
        let io: *mut AVIOContext = avio_alloc_context(
            buffer,
            max_packet_size.try_into().unwrap(),
            // Set stream to WRITE.
            1,
            // Pass on a pointer *UNSAFE* to the packet buffer, assuming the packet buffer will live
            // long enough.
            packet_buffer as *mut Vec<Vec<u8>> as *mut std::ffi::c_void,
            // No `read_packet`.
            None,
            // Passthrough for `write_packet`.
            // XXX: Doing a manual transmute here to match the expected callback function
            // signature. Since it changed since ffmpeg 7 and we don't know during compile time
            // what verion we're dealing with, this trick will convert to the either the signature
            // where the buffer argument is `*const u8` or `*mut u8`.
            Some(std::mem::transmute::<*const (), _>(
                output_raw_buf_start_callback as _,
            )),
            // No `seek`.
            None,
        );

        // Setting `max_packet_size` will let the underlying IO stream know that this buffer must be
        // treated as packetized.
        (*io).max_packet_size = max_packet_size.try_into().unwrap();

        // Assign IO to output context.
        (*output.as_mut_ptr()).pb = io;
    }
}

/// This function cleans up the IO context used for packetized writing created by
/// `output_raw_packetized_buf_start`.
///
/// # Arguments
///
/// * `output` - Output context to end write on.
pub fn output_raw_packetized_buf_end(output: &mut Output) {
    unsafe {
        let output_pb = (*output.as_mut_ptr()).pb;

        // One last flush (might incur write, most likely won't).
        avio_flush(output_pb);

        // Note: No need for handling `opaque` as it is managed by Rust code anyway and will be
        // freed by it.

        // We do need to free the buffer itself though (we allocatd it manually earlier).
        av_free((*output_pb).buffer as *mut std::ffi::c_void);
        // And deallocate the entire IO context.
        av_free(output_pb as *mut std::ffi::c_void);

        // Reset the `pb` field or `avformat_close` will try to free it!
        ((*output.as_mut_ptr()).pb) = std::ptr::null_mut::<AVIOContext>();
    }
}

/// Flush the output. This can be useful in some circumstances.options
///
/// For example: It is used to flush fragments when outputting fragmented mp4 packets in combination
/// with the `frag_custom` option.
///
/// # Arguments
///
/// * `output` - Output context to flush.
pub fn flush_output(output: &mut Output) -> Result<(), Error> {
    unsafe {
        match av_write_frame(output.as_mut_ptr(), std::ptr::null_mut()) {
            0 => Ok(()),
            1 => Ok(()),
            e => Err(Error::from(e)),
        }
    }
}

/// Initialize a new codec context using a specific codec.
///
/// # Arguments
///
/// * `codec` - Codec to initialize with.
pub fn codec_context_as(codec: &Codec) -> Result<Context, Error> {
    unsafe {
        let context_ptr = ffmpeg::ffi::avcodec_alloc_context3(codec.as_ptr());
        if !context_ptr.is_null() {
            Ok(Context::wrap(context_ptr, None))
        } else {
            Err(Error::Unknown)
        }
    }
}

/// Set the `time_base` field of a decoder. (Not natively supported in the public API.)
///
/// # Arguments
///
/// * `decoder_context` - Decoder context.
/// * `time_base` - Time base to assign.
pub fn set_decoder_context_time_base(decoder_context: &mut Context, time_base: Rational) {
    unsafe {
        (*decoder_context.as_mut_ptr()).time_base = time_base.into();
    }
}

/// Get the `time_base` field of an encoder. (Not natively supported in the public API.)
///
/// # Arguments
///
/// * `encoder` - Encoder to get `time_base` of.
pub fn get_encoder_time_base(encoder: &Video) -> Rational {
    unsafe { (*encoder.0.as_ptr()).time_base.into() }
}

/// Copy frame properties from `src` to `dst`.
///
/// # Arguments
///
/// * `src` - Frame to get properties from.
/// * `dst` - Frame to copy properties to.
pub fn copy_frame_props(src: &Frame, dst: &mut Frame) {
    unsafe {
        av_frame_copy_props(dst.as_mut_ptr(), src.as_ptr());
    }
}

/// A frame array is the `ndarray` version of `AVFrame`. It is 3-dimensional array with dims `(H, W,
/// C)` and type byte.
#[cfg(feature = "ndarray")]
pub type FrameArray = Array3<u8>;

/// Converts an `ndarray` to an RGB24 video `AVFrame` for ffmpeg.
///
/// # Arguments
///
/// * `frame_array` - Video frame to convert. The frame format must be `(H, W, C)`.
///
/// # Return value
///
/// An ffmpeg-native `AvFrame`.
#[cfg(feature = "ndarray")]
pub fn convert_ndarray_to_frame_rgb24(frame_array: &FrameArray) -> Result<Frame, Error> {
    unsafe {
        assert!(frame_array.is_standard_layout());

        let (frame_height, frame_width, _) = frame_array.dim();

        // Temporary frame structure to place correctly formatted data and linesize stuff in, which
        // we'll copy later.
        let mut frame_tmp = Frame::empty();
        let frame_tmp_ptr = frame_tmp.as_mut_ptr();

        // This does not copy the data, but it sets the `frame_tmp` data and linesize pointers
        // correctly.
        let bytes_copied = av_image_fill_arrays(
            (*frame_tmp_ptr).data.as_ptr() as *mut *mut u8,
            (*frame_tmp_ptr).linesize.as_ptr() as *mut i32,
            frame_array.as_ptr(),
            AVPixelFormat::AV_PIX_FMT_RGB24,
            frame_width as i32,
            frame_height as i32,
            1,
        );

        if bytes_copied != frame_array.len() as i32 {
            return Err(Error::from(bytes_copied));
        }

        let mut frame = Frame::new(Pixel::RGB24, frame_width as u32, frame_height as u32);
        let frame_ptr = frame.as_mut_ptr();

        // Do the actual copying.
        av_image_copy(
            (*frame_ptr).data.as_ptr() as *mut *mut u8,
            (*frame_ptr).linesize.as_ptr() as *mut i32,
            (*frame_tmp_ptr).data.as_ptr() as *mut *const u8,
            (*frame_tmp_ptr).linesize.as_ptr(),
            AVPixelFormat::AV_PIX_FMT_RGB24,
            frame_width as i32,
            frame_height as i32,
        );

        Ok(frame)
    }
}

/// Converts an RGB24 video `AVFrame` produced by ffmpeg to an `ndarray`.
///
/// # Arguments
///
/// * `frame` - Video frame to convert.
///
/// # Return value
///
/// A three-dimensional `ndarray` with dimensions `(H, W, C)` and type byte.
#[cfg(feature = "ndarray")]
pub fn convert_frame_to_ndarray_rgb24(frame: &mut Frame) -> Result<FrameArray, Error> {
    unsafe {
        let frame_ptr = frame.as_mut_ptr();
        let frame_width: i32 = (*frame_ptr).width;
        let frame_height: i32 = (*frame_ptr).height;
        let frame_format =
            std::mem::transmute::<std::ffi::c_int, AVPixelFormat>((*frame_ptr).format);
        assert_eq!(frame_format, AVPixelFormat::AV_PIX_FMT_RGB24);

        let mut frame_array =
            FrameArray::default((frame_height as usize, frame_width as usize, 3_usize));

        let bytes_copied = av_image_copy_to_buffer(
            frame_array.as_mut_ptr(),
            frame_array.len() as i32,
            (*frame_ptr).data.as_ptr() as *const *const u8,
            (*frame_ptr).linesize.as_ptr(),
            frame_format,
            frame_width,
            frame_height,
            1,
        );

        if bytes_copied == frame_array.len() as i32 {
            Ok(frame_array)
        } else {
            Err(Error::from(bytes_copied))
        }
    }
}

/// Retrieve a reference to the extradata bytes in codec parameters of an output stream.
///
/// # Arguments
///
/// * `output` - Output that contains stream to get extradata from.
/// * `stream_index` - Index of stream.
pub fn extradata(output: &Output, stream_index: usize) -> Result<&[u8], Error> {
    let parameters = output
        .stream(stream_index)
        .map(|stream| stream.parameters())
        .ok_or(Error::StreamNotFound)?;

    Ok(unsafe {
        std::slice::from_raw_parts(
            (*parameters.as_ptr()).extradata,
            (*parameters.as_ptr()).extradata_size as usize,
        )
    })
}

/// Whether or not the output format context is configured to use H.264 packetization mode 0.
///
/// # Arguments
///
/// * `output` - Output format context.
pub fn rtp_h264_mode_0(output: &Output) -> bool {
    unsafe {
        av_opt_flag_is_set(
            (*output.as_ptr()).priv_data,
            "rtpflags".as_ptr() as *const std::ffi::c_char,
            "h264_mode0".as_ptr() as *const std::ffi::c_char,
        ) != 0
    }
}

/// Get the current sequence number and timestamp of the RTP muxer.
///
/// Note: This method is only safe to use on RTP output formats.
pub fn rtp_seq_and_timestamp(output: &Output) -> (u16, u32) {
    unsafe {
        let rtp_mux_context = &*((*output.as_ptr()).priv_data as *const RTPMuxContext);
        (rtp_mux_context.seq, rtp_mux_context.timestamp)
    }
}

/// Create SDP file contents for the given output. Useful for RTP muxers.
///
/// A media entry will be created for each stream in the output. This function will take care of all
/// details, such as setting the correct media attributes needed by any SDP consumers.
///
/// # Arguments
///
/// * `output` - Output to generate SDP file for.
///
/// # Return value
///
/// A string with the SDP file contents.
pub fn sdp(output: &Output) -> Result<String, Error> {
    const BUF_SIZE: i32 = 4096;

    unsafe {
        let mut buf: [std::ffi::c_char; BUF_SIZE as usize] = [0; BUF_SIZE as usize];
        let buf_ptr = &mut buf as *mut std::ffi::c_char;
        let mut output_format_context = output.as_ptr();
        let output_format_context_ptr = &mut output_format_context as *mut *const AVFormatContext;
        // WARNING! Casting from const ptr to mutable ptr here!
        let output_format_context_ptr = output_format_context_ptr as *mut *mut AVFormatContext;
        let ret = av_sdp_create(output_format_context_ptr, 1, buf_ptr, BUF_SIZE);

        if ret == 0 {
            let sdp_c_str = std::ffi::CStr::from_ptr(buf_ptr);
            let sdp = sdp_c_str.to_string_lossy().to_string();
            Ok(sdp)
        } else {
            Err(Error::from(ret))
        }
    }
}

/// Initialize the logging handler. This will redirect all ffmpeg logging to the Rust `tracing`
/// crate and any subscribers to it.
pub fn init_logging() {
    unsafe {
        av_log_set_callback(Some(log_callback));
    }
}

/// Passthrough function that is passed to `libavformat` in `avio_alloc_context` and pushes buffers
/// from a packetized stream onto the packet buffer held in `opaque`.
extern "C" fn output_raw_buf_start_callback(
    opaque: *mut std::ffi::c_void,
    buffer: *const u8,
    buffer_size: i32,
) -> i32 {
    unsafe {
        // Acquire a reference to the packet buffer transmuted from the `opaque` gotten through
        // `libavformat`.
        let packet_buffer: &mut Vec<Vec<u8>> = &mut *(opaque as *mut Vec<Vec<u8>>);
        // Push the current packet onto the packet buffer.
        packet_buffer.push(std::slice::from_raw_parts(buffer, buffer_size as usize).to_vec());
    }

    // Number of bytes written.
    buffer_size
}

/// Internal function with C-style callback behavior that receives all log messages from ffmpeg and
/// handles them with the `log` crate, the Rust way.
///
/// # Arguments
///
/// * `avcl` - Internal struct with log message data.
/// * `level_no` - Log message level integer.
/// * `fmt` - Log message format string.
/// * `vl` - Variable list with format string items.
unsafe extern "C" fn log_callback(
    avcl: *mut std::ffi::c_void,
    level_no: std::ffi::c_int,
    fmt: *const std::ffi::c_char,
    #[cfg(all(target_arch = "x86_64", target_family = "unix"))] vl: *mut __va_list_tag,
    #[cfg(not(all(target_arch = "x86_64", target_family = "unix")))] vl: va_list,
) {
    // Check whether or not the message would be printed at all.
    let event_would_log = match level_no {
        // These are all error states.
        AV_LOG_PANIC | AV_LOG_FATAL | AV_LOG_ERROR => tracing::enabled!(tracing::Level::ERROR),
        AV_LOG_WARNING => tracing::enabled!(tracing::Level::WARN),
        AV_LOG_INFO => tracing::enabled!(tracing::Level::INFO),
        // There is no "verbose" in `log`, so we just put it in the "debug" category.
        AV_LOG_VERBOSE | AV_LOG_DEBUG => tracing::enabled!(tracing::Level::DEBUG),
        AV_LOG_TRACE => tracing::enabled!(tracing::Level::TRACE),
        _ => {
            return;
        }
    };

    if event_would_log {
        // Allocate some memory for the log line (might be truncated). 1024 bytes is the number used
        // by ffmpeg itself, so it should be mostly fine.
        let mut line = [0; 1024];
        let mut print_prefix: std::ffi::c_int = 1;
        // Use the ffmpeg default formatting.
        let ret = av_log_format_line2(
            avcl,
            level_no,
            fmt,
            vl,
            line.as_mut_ptr(),
            (line.len()) as std::ffi::c_int,
            (&mut print_prefix) as *mut std::ffi::c_int,
        );
        // Simply discard the log message if formatting fails.
        if ret > 0 {
            if let Ok(line) = std::ffi::CStr::from_ptr(line.as_mut_ptr()).to_str() {
                let line = line.trim();
                if log_filter_hacks(line) {
                    match level_no {
                        // These are all error states.
                        AV_LOG_PANIC | AV_LOG_FATAL | AV_LOG_ERROR => {
                            tracing::error!(target: "video", "{}", line)
                        }
                        AV_LOG_WARNING => tracing::warn!(target: "video", "{}", line),
                        AV_LOG_INFO => tracing::info!(target: "video", "{}", line),
                        // There is no "verbose" in `log`, so we just put it in the "debug"
                        // category.
                        AV_LOG_VERBOSE | AV_LOG_DEBUG => {
                            tracing::debug!(target: "video", "{}", line)
                        }
                        AV_LOG_TRACE => tracing::trace!(target: "video", "{}", line),
                        _ => {}
                    };
                }
            }
        }
    }
}

/// Helper function to filter out any lines that we don't want to log because they contaminate.
/// Currently, it includes the following log line hacks:
///
/// * **Pelco H264 encoding issue**. Pelco cameras and encoders have a problem with their SEI NALs
///   that causes ffmpeg to complain but does not hurt the stream. It does cause continuous error
///   messages though which we filter out here.
fn log_filter_hacks(line: &str) -> bool {
    /* Hack 1 */
    const HACK_1_PELCO_NEEDLE_1: &str = "SEI type 5 size";
    const HACK_1_PELCO_NEEDLE_2: &str = "truncated at";
    if line.contains(HACK_1_PELCO_NEEDLE_1) && line.contains(HACK_1_PELCO_NEEDLE_2) {
        return false;
    }

    true
}

/// Rust version of the `RTPMuxContext` struct in `libavformat`.
#[repr(C)]
struct RTPMuxContext {
    _av_class: *const AVClass,
    _ic: *mut AVFormatContext,
    _st: *mut AVStream,
    pub payload_type: std::ffi::c_int,
    pub ssrc: u32,
    pub cname: *const std::ffi::c_char,
    pub seq: u16,
    pub timestamp: u32,
    pub base_timestamp: u32,
    pub cur_timestamp: u32,
    pub max_payload_size: std::ffi::c_int,
}
