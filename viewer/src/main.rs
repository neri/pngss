//! sample PNG image viewer

use embedded_graphics::{image::Image, image::ImageRaw, pixelcolor::Rgb888, prelude::*};
use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorDisplay, Window};
use std::{env, fs::File, io::Read, path::Path};

fn main() {
    let mut args = env::args();
    let _ = args.next().unwrap();

    let arg = args.next().expect("file name not given");
    let mut file = File::open(&arg).expect("file cannot open");
    let mut data = Vec::new();
    file.read_to_end(&mut data).expect("file cannot read");

    let decoder = pngss::PngDecoder::new(&data).expect("unexpected file format");
    let image_info = decoder.info().clone();
    println!("{:?}", image_info);
    let decoded = decoder.decode().expect("decode failed");
    let image_data = decoded.to_rgb_bytes();
    let raw = ImageRaw::<Rgb888>::new(&image_data, image_info.width);

    let window_size = Size::new(
        128.max(image_info.width + 16),
        64.max(image_info.height + 16),
    );
    let padding = Point::new(
        (window_size.width - image_info.width) as i32 / 2,
        (window_size.height - image_info.height) as i32 / 2,
    );
    let image = Image::new(&raw, padding);
    let mut display = SimulatorDisplay::<Rgb888>::new(window_size);
    image.draw(&mut display).unwrap();

    let output_settings = OutputSettingsBuilder::new().build();
    Window::new(
        &format!(
            "{} - Image Viewer",
            &Path::new(&arg).file_name().unwrap().to_str().unwrap()
        ),
        &output_settings,
    )
    .show_static(&display);
}
