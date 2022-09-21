<h1 align="center">
  <code>video-rs</code>
</h1>
<p align="center">High-level video toolkit based on ffmpeg.</p>

## 🎬 Introduction

`video-rs` is a general-purpose video library for Rust that uses the
`libav`-family libraries from `ffmpeg`. It aims to provide a stable and Rusty
interface to many common video tasks such as reading, writing, muxing, encoding
and decoding.

## 🛠 S️️tatus

⚠️ This project is still a work-in-progress, and will contain bugs. Some parts of
the API have not been flushed out yet. Use with caution.

## 📦 Setup

First, install the `ffmpeg` libraries. The `ffmpeg-next` project has
[excellent instructions](https://github.com/zmwangx/rust-ffmpeg/wiki/Notes-on-building#dependencies)
on this (`video-rs` depends on the `ffmpeg-next` crate).

Then, add the following to your dependencies in `Cargo.toml`:

```toml
video-rs = "0.1"
```

Use the `ndarray` feature to be able to use raw frames with the
[`ndarray`](https://github.com/rust-ndarray/ndarray) crate:

```toml
video-rs = { version = "0.1", features = ["ndarray"] }
```

## 📖 Examples

Decode a video and print the RGB value for the top left pixel:

```rust
use video_rs::{
  self,
  Locator,
  Decoder,
};

fn main() {
  video_rs::init();
  
  let source = Locator::Url("http://commondatastorage.googleapis.com/gtv-videos-bucket/sample/BigBuckBunny.mp4".parse().unwrap());
  let mut decoder = Decoder::new(&source)
    .expect("failed to create decoder");

  for frame in decoder.decode_iter() {
    if let Ok((_, frame)) = frame {
      let rgb = frame
        .slice(ndarray::s![0, 0, ..])
        .to_slice()
        .unwrap();
      println!(
        "pixel at 0, 0: {}, {}, {}",
        rgb[0], rgb[1], rgb[2],
      );
    } else {
      break;
    }
  }

}
```

Encode a 🌈 video, using `ndarray` to create each frame:

```rust
use std::path::PathBuf;
use std::time::Duration;

use ndarray::Array3;

use video_rs::{
  Locator,
  Encoder,
  EncoderSettings,
  Time,
};

fn main() {
  video_rs::init();

  let destination: Locator = PathBuf::from("rainbow.mp4").into();
  let settings = EncoderSettings::for_h264_yuv420p(1280, 720, false);
  
  let mut encoder = Encoder::new(&destination, settings)
    .expect("failed to create encoder");

  // By determining the duration of each frame, we are essentially determing
  // the true frame rate of the output video. We choose 24 here.
  let duration: Time = Duration::from_nanos(1_000_000_000 / 24).into();

  // Keep track of the current video timestamp.
  let mut position = Time::zero();

  for i in 0..256 {
    // This will create a smooth rainbow animation video!
    let frame = rainbow_frame(i as f32 / 256.0);

    encoder.encode(&frame, &position)
      .expect("failed to encode frame");

    // Update the current position and add `duration` to it.
    position = position.aligned_with(&duration).add();
  }

  encoder.finish()
    .expect("failed to finish encoder");
}

fn rainbow_frame(p: f32) -> Array3<u8> {
  // This is what generated the rainbow effect! We loop through the HSV
  // color spectrum and convert to RGB.
  let rgb = hsv_to_rgb(p * 360.0, 100.0, 100.0);

  // This creates a frame with height 720, width 1280 and three
  // channels. The RGB values for each pixel are equal, and determined
  // by the `rgb` we chose above.
  Array3::from_shape_fn(
    (720, 1280, 3),
    |(_y, _x, c)| {
      rgb[c]
    })
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
  let s = s / 100.0;
  let v = v / 100.0;
  let c = s * v;
  let x = c * (1.0 - (((h / 60.0) % 2.0) - 1.0).abs());
  let m = v - c;
  let (r, g, b) = 
    if h >= 0.0 && h < 60.0 {
      (c, x, 0.0)
    } else if h >= 60.0 && h < 120.0 {
      (x, c, 0.0)
    } else if h >= 120.0 && h < 180.0 {
      (0.0, c, x)
    } else if h >= 180.0 && h < 240.0 {
      (0.0, x, c)
    } else if h >= 240.0 && h < 300.0 {
      (x, 0.0, c)
    } else if h >= 300.0 && h < 360.0 {
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
```

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
