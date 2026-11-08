//! For each image path on stdin, write `<index>.png` (an upright sRGB thumbnail fitted within
//! 256 px) to the directory in argv[1] and print one JSON line: `{"i", "ok", "error", "ms"}`.
//! No file names or metadata are printed.

use std::io::BufRead;
use std::time::Instant;

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    for (i, line) in std::io::stdin()
        .lock()
        .lines()
        .map_while(Result::ok)
        .enumerate()
    {
        let Ok(bytes) = std::fs::read(line.trim()) else {
            println!("{{\"i\":{i},\"ok\":false,\"error\":\"unreadable\"}}");
            continue;
        };
        let start = Instant::now();
        let t = craft_image::thumbnail(&bytes, &craft_image::ThumbnailOptions::new(256));
        let ms = start.elapsed().as_secs_f64() * 1e3;
        match t.and_then(|t| craft_image::encode_plain(&t.image, craft_image::OutputFormat::Png)) {
            Ok(png) => {
                let _ = std::fs::write(out.join(format!("{i}.png")), png);
                println!("{{\"i\":{i},\"ok\":true,\"ms\":{ms:.1}}}");
            }
            Err(e) => println!(
                "{{\"i\":{i},\"ok\":false,\"error\":\"{}\",\"ms\":{ms:.1}}}",
                e.to_string().replace('"', "'")
            ),
        }
    }
}
