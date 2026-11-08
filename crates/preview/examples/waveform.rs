//! For each path on stdin, print one JSON line with the in-tree waveform's 100 column peaks
//! of the first 12 seconds, or the error. No file names are printed.

use std::io::BufRead;

fn main() {
    let opts = mediacraft_preview::WaveformOptions {
        columns: 100,
        max_seconds: 12.0,
    };
    for (i, line) in std::io::stdin()
        .lock()
        .lines()
        .map_while(Result::ok)
        .enumerate()
    {
        match mediacraft_preview::waveform(std::path::Path::new(line.trim()), &opts) {
            Ok(w) => {
                let peaks: Vec<String> = w.peaks.iter().map(|p| format!("{p:.4}")).collect();
                println!(
                    "{{\"i\":{i},\"ok\":true,\"codec\":\"{}\",\"seconds\":{:.3},\"peaks\":[{}]}}",
                    w.codec,
                    w.seconds,
                    peaks.join(",")
                );
            }
            Err(e) => println!(
                "{{\"i\":{i},\"ok\":false,\"error\":\"{}\"}}",
                e.to_string().replace('"', "'")
            ),
        }
    }
}
