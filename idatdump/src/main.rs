//! IDAT dumper

use base64::prelude::*;
use pngss::{BitDepth, DeflateDecoder};
use std::{
    borrow::Cow,
    env::{self, args},
    fs::File,
    io::{Read, Write},
    process,
};

enum FilterMode {
    None,
    Filter,
}

enum Mode {
    Hex,
    Bin,
    Base64,
}

fn main() {
    let mut args = args();
    let _ = args.next().unwrap();

    let mut mode = Mode::Hex;
    let mut filter_mode = FilterMode::None;
    let mut path_input = None;
    let mut path_output = None;
    while let Some(arg) = args.next() {
        if arg.starts_with("-") {
            match arg.as_str() {
                "-bin" => mode = Mode::Bin,
                "-hex" => mode = Mode::Hex,
                "-base64" => mode = Mode::Base64,
                "-filter" => filter_mode = FilterMode::Filter,
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

    let mut filter_stats = [0; 5];
    let buff: Cow<[u8]> = match filter_mode {
        FilterMode::None => Cow::Borrowed(&inflated),
        FilterMode::Filter => {
            let n_channels = info.image_type.n_channels().as_usize();
            let stride = 1 + if info.bit_depth > BitDepth::Eight {
                info.width as usize * n_channels
            } else {
                (info.width as usize * n_channels * info.bit_depth as usize + 7) / 8
            };
            let mut filters = Vec::new();
            for line in inflated.chunks_exact(stride) {
                let filter_type = line[0];
                filter_stats[filter_type as usize] += 1;
                filters.push(filter_type);
            }
            filters.into()
        }
    };

    match mode {
        Mode::Hex => {
            println!("# PNG {:?} IDAT {} <= {}", info, inflated.len(), idat.len(),);
            if matches!(filter_mode, FilterMode::Filter) {
                println!(
                    "# Filter stats: None {} Sub {} Up {} Avg {} Paeth {}",
                    filter_stats[0],
                    filter_stats[1],
                    filter_stats[2],
                    filter_stats[3],
                    filter_stats[4],
                );
            }

            let mut iter = buff.iter();
            for addr in (0..buff.len()).step_by(16) {
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
            stdout.write(&buff).unwrap();
            stdout.flush().unwrap();
        }
        Mode::Base64 => {
            let base64_data = BASE64_STANDARD.encode(&buff);
            println!("{}", base64_data);
        }
    }
}

fn usage() -> ! {
    let mut args = env::args_os();
    let arg = args.next().unwrap();
    eprintln!(
        "IDAT dumper\nusage: {} [-hex|-bin|-base64] INPUT",
        arg.to_str().unwrap()
    );
    process::exit(1);
}
