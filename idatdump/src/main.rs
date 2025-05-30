//! IDAT dumper

use base64::prelude::*;
use pngss::{BitDepth, DeflateDecoder};
use std::{
    env::{self, args},
    fs::File,
    io::{Read, Write},
    process,
};

enum FilterMode {
    Raw,
    Filter,
    Decoded,
}

enum DumpMode {
    Hex,
    Bin,
    Base64,
}

fn main() {
    let mut args = args();
    let _ = args.next().unwrap();

    let mut dump_mode = DumpMode::Hex;
    let mut filter_mode = FilterMode::Raw;
    let mut path_input = None;
    let mut path_output = None;
    while let Some(arg) = args.next() {
        if arg.starts_with("-") {
            match arg.as_str() {
                "-bin" => dump_mode = DumpMode::Bin,
                "-hex" => dump_mode = DumpMode::Hex,
                "-b64" | "-base64" => dump_mode = DumpMode::Base64,
                "-raw" => filter_mode = FilterMode::Raw,
                "-filter" => filter_mode = FilterMode::Filter,
                "-decoded" => filter_mode = FilterMode::Decoded,
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

    let mut filter_stats = [0; 5];
    let buff = match filter_mode {
        FilterMode::Raw => {
            let inflated =
                pngss::DefaultDeflateDecoder::inflate(&idat, buffer_size).expect("inflate failed");
            inflated
        }
        FilterMode::Filter => {
            let inflated =
                pngss::DefaultDeflateDecoder::inflate(&idat, buffer_size).expect("inflate failed");
            let n_channels = info.color_type.n_channels().as_usize();
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
            filters
        }
        FilterMode::Decoded => {
            let decoded = decoder.decode().expect("cannot decode PNG data");
            decoded.raw_data().to_vec()
        }
    };

    match dump_mode {
        DumpMode::Hex => {
            println!(
                "# PNG {:?} IDAT {} <= {} ({:.02}%)",
                info,
                idat.len(),
                buffer_size,
                idat.len() as f64 / buffer_size as f64 * 100.0,
            );
            if matches!(filter_mode, FilterMode::Filter) {
                println!(
                    "# Filter stats: None(0) {} Sub(1) {} Up(2) {} Avg(3) {} Paeth(4) {}",
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
        DumpMode::Bin => {
            let mut stdout = std::io::stdout();
            stdout.write(&buff).unwrap();
            stdout.flush().unwrap();
        }
        DumpMode::Base64 => {
            let base64_data = BASE64_STANDARD.encode(&buff);
            println!("{}", base64_data);
        }
    }
}

fn usage() -> ! {
    let mut args = env::args_os();
    let arg = args.next().unwrap();
    eprintln!(
        "IDAT dumper\n\nusage: {} [mode] [-hex|-bin|-b64] INPUT\n",
        arg.to_str().unwrap()
    );
    eprintln!("Modes:");
    eprintln!("  -raw       Decompress IDAT chunks and dump them as is (default)");
    eprintln!("  -filter    Decompress IDAT chunks to show only filter types");
    eprintln!("  -decoded   Dumps the final decoding results of the image data");
    process::exit(1);
}
