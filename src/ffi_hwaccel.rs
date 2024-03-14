extern crate ffmpeg_next as ffmpeg;

use crate::hwaccel::HardwareAccelerationDeviceType;

pub struct HardwareDeviceContext {
    ptr: *mut ffmpeg::ffi::AVBufferRef,
}

impl HardwareDeviceContext {
    pub fn new(
        device_type: HardwareAccelerationDeviceType,
    ) -> Result<HardwareDeviceContext, ffmpeg::error::Error> {
        let mut ptr: *mut ffmpeg::ffi::AVBufferRef = std::ptr::null_mut();

        unsafe {
            match ffmpeg::ffi::av_hwdevice_ctx_create(
                (&mut ptr) as *mut *mut ffmpeg::ffi::AVBufferRef,
                device_type.into(),
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
            ) {
                0 => Ok(HardwareDeviceContext { ptr }),
                e => Err(ffmpeg::error::Error::from(e)),
            }
        }
    }

    unsafe fn ref_raw(&self) -> *mut ffmpeg::ffi::AVBufferRef {
        ffmpeg::ffi::av_buffer_ref(self.ptr)
    }
}

impl Drop for HardwareDeviceContext {
    fn drop(&mut self) {
        unsafe {
            ffmpeg::ffi::av_buffer_unref(&mut self.ptr);
        }
    }
}

pub fn hwdevice_list_available_device_types() -> Vec<HardwareAccelerationDeviceType> {
    let mut hwdevice_types = Vec::new();
    let mut hwdevice_type = unsafe {
        ffmpeg::ffi::av_hwdevice_iterate_types(ffmpeg::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_NONE)
    };
    while hwdevice_type != ffmpeg::ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_NONE {
        hwdevice_types.push(HardwareAccelerationDeviceType::from(hwdevice_type).unwrap());
        hwdevice_type = unsafe { ffmpeg::ffi::av_hwdevice_iterate_types(hwdevice_type) };
    }
    hwdevice_types
}

pub fn hwdevice_transfer_frame(
    target_frame: &mut ffmpeg::frame::Frame,
    hwdevice_frame: &ffmpeg::frame::Frame,
) -> Result<(), ffmpeg::error::Error> {
    unsafe {
        match ffmpeg::ffi::av_hwframe_transfer_data(
            target_frame.as_mut_ptr(),
            hwdevice_frame.as_ptr(),
            0,
        ) {
            0 => Ok(()),
            e => Err(ffmpeg::error::Error::from(e)),
        }
    }
}

pub fn codec_find_corresponding_hwaccel_pixfmt(
    codec: &ffmpeg::codec::codec::Codec,
    hwaccel_type: HardwareAccelerationDeviceType,
) -> Option<ffmpeg::format::pixel::Pixel> {
    let mut i = 0;
    loop {
        unsafe {
            let hw_config = ffmpeg::ffi::avcodec_get_hw_config(codec.as_ptr(), i);
            if !hw_config.is_null() {
                let hw_config_supports_codec = (((*hw_config).methods) as i32
                    & ffmpeg::ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32)
                    != 0;
                if hw_config_supports_codec && (*hw_config).device_type == hwaccel_type.into() {
                    break Some((*hw_config).pix_fmt.into());
                }
            } else {
                break None;
            }
        }
        i += 1;
    }
}

pub fn codec_context_hwaccel_set_get_format(
    codec_context: &mut ffmpeg::codec::context::Context,
    hw_pixfmt: ffmpeg::format::pixel::Pixel,
) {
    unsafe {
        (*codec_context.as_mut_ptr()).opaque =
            ffmpeg::ffi::AVPixelFormat::from(hw_pixfmt) as i32 as _;
        (*codec_context.as_mut_ptr()).get_format = Some(hwaccel_get_format);
    }
}

pub fn codec_context_hwaccel_set_hw_device_ctx(
    codec_context: &mut ffmpeg::codec::context::Context,
    hardware_device_context: &HardwareDeviceContext,
) {
    unsafe {
        (*codec_context.as_mut_ptr()).hw_device_ctx = hardware_device_context.ref_raw();
    }
}

#[no_mangle]
unsafe extern "C" fn hwaccel_get_format(
    ctx: *mut ffmpeg::ffi::AVCodecContext,
    pix_fmts: *const ffmpeg::ffi::AVPixelFormat,
) -> ffmpeg::ffi::AVPixelFormat {
    let mut p = pix_fmts;
    while *p != ffmpeg::ffi::AVPixelFormat::AV_PIX_FMT_NONE {
        if *p == std::mem::transmute((*ctx).opaque as i32) {
            return *p;
        }
        p = p.add(1);
    }
    ffmpeg::ffi::AVPixelFormat::AV_PIX_FMT_NONE
}
