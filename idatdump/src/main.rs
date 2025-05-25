//! IDAT dumper

use pngss::{DeflateDecoder, DeflateEncoder};
use std::{
    env::{self, args},
    fs::File,
    io::{Read, Write},
    process,
};

enum Mode {
    Hex,
    Bin,
}

fn main() {
    let mut args = args();
    let _ = args.next().unwrap();

    let mut mode = Mode::Hex;
    let mut path_input = None;
    let mut path_output = None;
    while let Some(arg) = args.next() {
        if arg.starts_with("-") {
            match arg.as_str() {
                "-bin" => mode = Mode::Bin,
                "-hex" => mode = Mode::Hex,
                "-o" => match args.next() {
                    Some(v) => path_output = Some(v),
                    None => usage(),
                },
                "--" => {
                    path_input = args.next();
                    break;
                }
                _ => panic!("unknown option: {}", arg),
            }
        } else {
            path_input = Some(arg);
            break;
        }
    }

    let path_input = match path_input {
        Some(v) => v,
        None => usage(),
    };
    let _ = path_output;

    let mut file = File::open(&path_input).expect("file cannot open");
    let mut data = Vec::new();
    file.read_to_end(&mut data).expect("file cannot read");

    let decoder = pngss::PngDecoder::new(&data).expect("unexpected file format");
    let info = decoder.info();
    let mut chunks = decoder.chunks().expect("cannot get chunks");
    let idat = chunks
        .get_idat_chunks(false)
        .expect("cannot get IDAT chunks");
    let buffer_size = decoder.decoded_buffer_size();
    let inflated =
        pngss::DefaultDeflateDecoder::inflate(&idat, buffer_size).expect("inflate failed");
    let deflated = pngss::DefaultDeflateEncoder::deflate(&inflated, pngss::CompressionLevel::Best)
        .expect("deflate failed");

    match mode {
        Mode::Hex => {
            println!(
                "PNG {:?} IDAT {} <= {} recomp {} ({:.02}%)",
                info,
                inflated.len(),
                idat.len(),
                deflated.len(),
                deflated.len() as f64 / idat.len() as f64 * 100.0
            );

            let mut iter = inflated.iter();
            for addr in (0..inflated.len()).step_by(16) {
                print!("{:08x}  ", addr);
                for _ in 0..16 {
                    if let Some(byte) = iter.next() {
                        print!("{:02x} ", byte);
                    } else {
                        print!("   ");
                    }
                }
                println!("");
            }
        }
        Mode::Bin => {
            let mut stdout = std::io::stdout();
            stdout.write(&inflated).unwrap();
            stdout.flush().unwrap();
        }
    }
}

fn usage() -> ! {
    let mut args = env::args_os();
    let arg = args.next().unwrap();
    eprintln!(
        "IDAT dumper\nusage: {} [-hex|-bin] INPUT",
        arg.to_str().unwrap()
    );
    process::exit(1);
}
