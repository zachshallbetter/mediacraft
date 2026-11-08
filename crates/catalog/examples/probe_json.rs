//! Print the catalogue's stream probe of each path on stdin as one JSON line:
//! `{"path", "video", "audio", "duration"}`. Used to compare MediaCraft's in-tree probing with
//! other tools; it reads only headers.

use std::io::BufRead;

fn main() {
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let path = std::path::PathBuf::from(line.trim());
        let row = match mediacraft_catalog::scanner::probe_file(&path) {
            Some(a) => serde_json::json!({
                "path": path, "video": a.video, "audio": a.audio, "duration": a.duration_secs,
            }),
            None => {
                serde_json::json!({ "path": path, "video": null, "audio": null, "duration": null })
            }
        };
        println!("{row}");
    }
}
