use std::path::{Path, PathBuf};

use ndarray::Array3;

use video_rs::encode::{Encoder, Settings};
use video_rs::time::Time;

use clap::{Parser, ValueEnum};

#[derive(Clone, ValueEnum)]
enum Codec {
    #[cfg(feature = "h264")]
    H264,
    #[cfg(feature = "vp9")]
    VP9,
    None,
}

impl Default for Codec {
    fn default() -> Self {
        if cfg!(feature = "h264") {
            Self::H264
        } else if cfg!(feature = "vp9") {
            Self::VP9
        } else {
            Self::None
        }
    }
}

impl Codec {
    fn settings(&self, width: usize, height: usize) -> Settings {
        match self {
            #[cfg(feature = "h264")]
            Self::H264 => Settings::preset_h264_yuv420p(width, height, true),
            #[cfg(feature = "vp9")]
            Self::VP9 => Settings::preset_vp9_yuv420p_realtime(width, height, None),
            Self::None => panic!("could not create settings"),
        }
    }
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    filename: PathBuf,

    #[arg(short, long)]
    codec: Option<Codec>,

    #[arg(long, default_value_t = 1280)]
    width: usize,

    #[arg(long, default_value_t = 720)]
    height: usize,
}

impl Args {
    fn detected_codec(&self) -> Codec {
        let ext = self
            .filename
            .extension()
            .expect("Filename must have an extension");
        if ext.eq_ignore_ascii_case("mp4") && cfg!(feature = "h264") {
            Codec::H264
        } else if ext.eq_ignore_ascii_case("webm") && cfg!(feature = "vp9") {
            Codec::VP9
        } else if ext.eq_ignore_ascii_case("mkv") {
            Codec::default()
        } else {
            panic!("Could not detect codec")
        }
    }

    fn settings(&self) -> Settings {
        if let Some(codec) = &self.codec {
            codec.settings(self.width, self.height)
        } else {
            self.detected_codec().settings(self.width, self.height)
        }
    }
}

fn main() {
    let args = Args::parse();

    video_rs::init().unwrap();

    let settings = args.settings();
    let mut encoder =
        Encoder::new(Path::new(&args.filename), settings).expect("failed to create encoder");

    let duration: Time = Time::from_nth_of_a_second(24);
    let mut position = Time::zero();
    for i in 0..256 {
        // This will create a smooth rainbow animation video!
        let frame = rainbow_frame(args.width, args.height, i as f32 / 256.0);

        encoder
            .encode(&frame, position)
            .expect("failed to encode frame");

        // Update the current position and add the inter-frame duration to it.
        position = position.aligned_with(duration).add();
    }

    encoder.finish().expect("failed to finish encoder");
    println!("Wrote {:?}", args.filename);
}

fn rainbow_frame(width: usize, height: usize, p: f32) -> Array3<u8> {
    // This is what generated the rainbow effect! We loop through the HSV color spectrum and convert
    // to RGB.
    let rgb = hsv_to_rgb(p * 360.0, 100.0, 100.0);

    // This creates a frame with height 720, width 1280 and three channels. The RGB values for each
    // pixel are equal, and determined by the `rgb` we chose above.
    Array3::from_shape_fn((height, width, 3), |(_y, _x, c)| rgb[c])
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let s = s / 100.0;
    let v = v / 100.0;
    let c = s * v;
    let x = c * (1.0 - (((h / 60.0) % 2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if (0.0..60.0).contains(&h) {
        (c, x, 0.0)
    } else if (60.0..120.0).contains(&h) {
        (x, c, 0.0)
    } else if (120.0..180.0).contains(&h) {
        (0.0, c, x)
    } else if (180.0..240.0).contains(&h) {
        (0.0, x, c)
    } else if (240.0..300.0).contains(&h) {
        (x, 0.0, c)
    } else if (300.0..360.0).contains(&h) {
        (c, 0.0, x)
    } else {
        (0.0, 0.0, 0.0)
    };
    [
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    ]
}
