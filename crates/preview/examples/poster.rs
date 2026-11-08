//! For each video path on stdin, write `<index>.png` (the frame at `AT` seconds, default 1.0,
//! fitted within 256 px) to the directory in argv[1], and print one JSON line per file:
//! `{"i", "ok", "codec", "error", "ms"}`. No file names or metadata are printed.

use std::io::BufRead;
use std::time::Instant;

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    let at: f64 = std::env::var("AT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0);
    for (i, line) in std::io::stdin()
        .lock()
        .lines()
        .map_while(Result::ok)
        .enumerate()
    {
        let path = std::path::PathBuf::from(line.trim());
        let start = Instant::now();
        let opts = mediacraft_preview::FrameOptions {
            max_edge: 256,
            ..mediacraft_preview::FrameOptions::at(at)
        };
        let res = mediacraft_preview::poster_frame(&path, &opts);
        let ms = start.elapsed().as_secs_f64() * 1e3;
        match res {
            Ok(f) => {
                let png = craft_image::encode_plain(&f.image, craft_image::OutputFormat::Png);
                if let Ok(png) = png {
                    let _ = std::fs::write(out.join(format!("{i}.png")), png);
                }
                println!(
                    "{{\"i\":{i},\"ok\":true,\"codec\":\"{}\",\"ms\":{ms:.1}}}",
                    f.codec
                );
            }
            Err(e) => {
                let msg = e.to_string().replace('"', "'");
                println!("{{\"i\":{i},\"ok\":false,\"error\":\"{msg}\",\"ms\":{ms:.1}}}");
            }
        }
    }
}
