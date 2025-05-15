//! bench

use std::{env, fs::File, io::Read, time::Duration};

fn main() {
    let mut args = env::args();
    let _ = args.next().unwrap();

    let arg = args.next().expect("file name not given");
    let mut file = File::open(&arg).expect("file cannot open");
    let mut data = Vec::new();
    file.read_to_end(&mut data).expect("file cannot read");

    {
        let decoder = pngss::PngDecoder::new(&data).expect("unexpected file format");
        println!("{:?}", decoder.info());
    }

    let threshold = Duration::from_millis(500);
    let mut times = 10;
    loop {
        let time_inflate0 = std::time::Instant::now();
        for _ in 0..times {
            let mut decoder = pngss::PngDecoder::new(&data).unwrap();
            let data = decoder.get_idat_chunks(true).unwrap();
            compress::deflate::Deflate::inflate(&data, usize::MAX).unwrap();
            drop(decoder);
        }
        let time_inflate1 = time_inflate0.elapsed();

        let time_pngss0 = std::time::Instant::now();
        for _ in 0..times {
            let mut decoder = pngss::PngDecoder::new(&data).unwrap();
            let decoded = decoder.decode().unwrap();
            decoded.to_rgb_bytes();
            drop(decoder);
        }
        let time_pngss1 = time_pngss0.elapsed();

        let time_png0 = std::time::Instant::now();
        for _ in 0..times {
            let decoder = png::Decoder::new(data.as_slice());
            let mut reader = decoder.read_info().unwrap();
            let mut buf = vec![0; reader.output_buffer_size()];
            let _info = reader.next_frame(&mut buf).unwrap();
            drop(reader);
        }
        let time_png1 = time_png0.elapsed();

        if time_pngss1 >= threshold || time_png1 >= threshold {
            println!(
                "times {}, inflate: {:.03}s, pngss: {:.03}s, png: {:.03}s, {:.03}%",
                times,
                time_inflate1.as_secs_f64(),
                time_pngss1.as_secs_f64(),
                time_png1.as_secs_f64(),
                time_pngss1.as_secs_f64() / time_png1.as_secs_f64() * 100.0,
            );
            break;
        } else {
            times *= 10;
        }
    }
}
