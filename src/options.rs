extern crate ffmpeg_next as ffmpeg;

use std::collections::HashMap;

use ffmpeg::Dictionary as AvDictionary;

/// A wrapper type for ffmpeg options.
pub struct Options<'a>(AvDictionary<'a>);

impl Options<'_> {
    /// Creates options such that ffmpeg will prefer TCP transport when reading RTSP stream (over
    /// the default UDP format).
    ///
    /// This sets the `rtsp_transport` to `tcp` in ffmpeg options.
    pub fn new_with_rtsp_transport_tcp() -> Self {
        let mut opts = AvDictionary::new();
        opts.set("rtsp_transport", "tcp");

        Self(opts)
    }

    /// Creates options such that ffmpeg will prefer TCP transport when reading RTSP stream (over
    /// the default UDP format). It also adds some options to reduce the socket and I/O timeouts to
    /// 4 seconds.
    ///
    /// This sets the `rtsp_transport` to `tcp` in ffmpeg options, it also sets `rw_timeout` to
    /// `4000000` and `stimeout` to `4000000`.
    pub fn new_with_rtsp_transport_tcp_and_sane_timeouts() -> Self {
        let mut opts = AvDictionary::new();
        opts.set("rtsp_transport", "tcp");
        // These can't be too low because ffmpeg takes its sweet time when connecting to RTSP
        // sources sometimes.
        opts.set("rw_timeout", "16000000");
        opts.set("stimeout", "16000000");

        Self(opts)
    }

    /// Creates options such that ffmpeg is instructed to fragment output and mux to fragmented mp4
    /// container format.
    ///
    /// This modifies the `movflags` key to supported fragmented output. The muxer output will not
    /// have a header and each packet contains enough metadata to be streamed without the header.
    /// Muxer output should be compatiable with MSE.
    pub fn new_with_fragmented_mov() -> Self {
        let mut opts = AvDictionary::new();
        opts.set(
            "movflags",
            "faststart+frag_keyframe+frag_custom+empty_moov+omit_tfhd_offset",
        );

        Self(opts)
    }

    /// Default options for a H264 encoder.
    pub fn new_h264() -> Self {
        let mut opts = AvDictionary::new();
        // Set H264 encoder to the medium preset.
        opts.set("preset", "medium");

        Self(opts)
    }

    /// Options for a H264 encoder that are tuned for low-latency encoding such as for real-time
    /// streaming.
    pub fn new_h264_realtime() -> Self {
        let mut opts = AvDictionary::new();
        // Set H264 encoder to the medium preset.
        opts.set("preset", "medium");
        // Tune for low latency
        opts.set("tune", "zerolatency");

        Self(opts)
    }

    /// Create custom options from a `HashMap`.
    ///
    /// # Arguments
    ///
    /// * `from` - `HashMap` to make options out of.
    pub fn new_from_hashmap(from: &HashMap<String, String>) -> Self {
        let mut opts = AvDictionary::new();
        for (k, v) in from {
            opts.set(&k.clone(), &v.clone());
        }

        Self(opts)
    }

    /// Get option value.
    #[inline(always)]
    pub(super) fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key)
    }

    /// Convert back to ffmpeg native dictionary, which can be used with `ffmpeg_next` functions.
    pub(super) fn to_dict(&self) -> AvDictionary {
        self.0.clone()
    }
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self(AvDictionary::new())
    }
}

impl From<HashMap<String, String>> for Options<'_> {
    /// Converts from `HashMap` to `Options`.
    ///
    /// Note: Calls [`Options::new_from_hashmap()`] internally.
    ///
    /// # Arguments
    ///
    /// * `item` - Item to convert from.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let my_opts = HashMap::new();
    /// options.insert(
    ///     "my_option".to_string(),
    ///     "my_value".to_string(),
    /// );
    ///
    /// let opts: Options = my_opts.into();
    /// ```
    fn from(item: HashMap<String, String>) -> Self {
        Self::new_from_hashmap(&item)
    }
}

unsafe impl Send for Options<'_> {}
unsafe impl Sync for Options<'_> {}
