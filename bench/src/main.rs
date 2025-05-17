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

    let mut scale = 0;
    let mut vec_inflate = Vec::new();
    let mut vec_pngss = Vec::new();
    let mut vec_png = Vec::new();
    for _ in 0..5 {
        let threshold = Duration::from_millis(500);
        let mut times = 10;
        loop {
            let time0 = std::time::Instant::now();
            for _ in 0..times {
                let decoder = pngss::PngDecoder::new(&data).unwrap();
                let data = decoder.chunks_unchecked().get_idat_chunks(false).unwrap();
                compress::deflate::Deflate::inflate(&data, usize::MAX).unwrap();
                drop(decoder);
            }
            let time_inflate1 = time0.elapsed();

            let time0 = std::time::Instant::now();
            for _ in 0..times {
                let decoder = pngss::PngDecoder::new(&data).unwrap();
                let decoded = decoder.decode().unwrap();
                decoded.to_rgb_bytes();
                drop(decoder);
            }
            let time_pngss1 = time0.elapsed();

            let time0 = std::time::Instant::now();
            for _ in 0..times {
                let decoder = png::Decoder::new(data.as_slice());
                let mut reader = decoder.read_info().unwrap();
                let mut buf = vec![0; reader.output_buffer_size()];
                let _info = reader.next_frame(&mut buf).unwrap();
                drop(reader);
            }
            let time_png1 = time0.elapsed();

            if time_pngss1 >= threshold || time_png1 >= threshold {
                vec_inflate.push(time_inflate1.as_secs_f64() / times as f64);
                vec_pngss.push(time_pngss1.as_secs_f64() / times as f64);
                vec_png.push(time_png1.as_secs_f64() / times as f64);
                scale = scale.max(times);
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

    let avg_inflate = average(&vec_inflate) * scale as f64;
    let avg_pngss = average(&vec_pngss) * scale as f64;
    let avg_png = average(&vec_png) * scale as f64;

    println!(
        "# average: {}, inflate: {:.03}s, pngss: {:.03}s, png: {:.03}s, {:.03}%",
        scale,
        avg_inflate,
        avg_pngss,
        avg_png,
        avg_pngss / avg_png * 100.0,
    );
}

fn average(v: &[f64]) -> f64 {
    assert!(v.len() >= 5, "too few samples");
    let mut v = v.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v.pop();
    v.remove(0);

    let sum: f64 = v.iter().sum();
    sum / v.len() as f64
}
