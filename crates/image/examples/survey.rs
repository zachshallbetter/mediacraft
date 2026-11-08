//! Probe and thumbnail every file named on stdin and print aggregate results: formats, failures,
//! orientations, which metadata was found, colour handling and timings. Prints no per-file
//! metadata values, so it is safe to run over a personal photo library.
//!
//! ```sh
//! find ~/Pictures -iname '*.jpg' | cargo run --release -p craft-image --example survey
//! ```

use craft_image::{ThumbnailOptions, probe, thumbnail};
use std::collections::BTreeMap;
use std::io::BufRead;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Tally {
    files: usize,
    probe_failed: BTreeMap<String, usize>,
    thumb_failed: BTreeMap<String, usize>,
    formats: BTreeMap<String, usize>,
    orientations: BTreeMap<String, usize>,
    found: BTreeMap<&'static str, usize>,
    profiles: BTreeMap<String, usize>,
    color: BTreeMap<String, usize>,
    sources: BTreeMap<String, usize>,
    warnings: BTreeMap<String, usize>,
    probe_time: Duration,
    thumb_time: Duration,
    thumbs: usize,
    slowest: (Duration, String),
}

fn bump<K: Ord>(m: &mut BTreeMap<K, usize>, k: K) {
    *m.entry(k).or_default() += 1;
}

fn main() {
    let mut t = Tally::default();
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        let path = std::path::PathBuf::from(line.trim());
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        t.files += 1;
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let start = Instant::now();
        let info = probe(&bytes);
        t.probe_time += start.elapsed();
        let info = match info {
            Ok(i) => i,
            Err(e) => {
                bump(&mut t.probe_failed, format!("{ext}: {e}"));
                continue;
            }
        };
        bump(&mut t.formats, info.header.format.to_string());
        bump(
            &mut t.orientations,
            format!("{:?}", info.header.orientation),
        );
        let m = &info.metadata;
        for (name, present) in [
            ("camera", m.camera_model.is_some()),
            ("lens", m.lens.is_some()),
            ("capture time", m.capture_time.is_some()),
            ("exposure", m.exposure_time.is_some()),
            ("gps", m.gps.is_some()),
            ("xmp", m.xmp.is_some()),
            ("iptc", m.iptc.is_some()),
            ("title/caption", m.title.is_some() || m.caption.is_some()),
            ("keywords", !m.keywords.is_empty()),
            ("icc", info.icc.is_some()),
            ("exif thumbnail", info.has_embedded_thumbnail),
            ("animated", info.header.animated),
        ] {
            if present {
                bump(&mut t.found, name);
            }
        }
        if let Some(icc) = &info.icc {
            bump(
                &mut t.profiles,
                icc.description
                    .clone()
                    .unwrap_or_else(|| "(no description)".into()),
            );
        }
        for w in &info.warnings {
            bump(&mut t.warnings, w.clone());
        }
        if !info.decodable {
            continue;
        }

        let start = Instant::now();
        let thumb = thumbnail(&bytes, &ThumbnailOptions::new(512));
        let took = start.elapsed();
        t.thumb_time += took;
        match thumb {
            Ok(th) => {
                t.thumbs += 1;
                bump(&mut t.sources, format!("{:?}", th.source));
                bump(
                    &mut t.color,
                    format!("{:?}", th.color).chars().take(60).collect(),
                );
                let (w, h) = info.header.display_size();
                let ok = (th.image.width() >= th.image.height()) == (w >= h) || w == h;
                if !ok {
                    bump(
                        &mut t.thumb_failed,
                        "thumbnail aspect disagrees with display size".into(),
                    );
                }
                if took > t.slowest.0 {
                    t.slowest = (
                        took,
                        format!(
                            "{} {}×{}",
                            info.header.format, info.header.width, info.header.height
                        ),
                    );
                }
            }
            Err(e) => bump(&mut t.thumb_failed, format!("{ext}: {e}")),
        }
    }

    println!("files read: {}", t.files);
    println!(
        "probe: {} ok, {:.2} ms average",
        t.files - t.probe_failed.values().sum::<usize>(),
        avg(t.probe_time, t.files)
    );
    println!(
        "thumbnails (512 px): {} ok, {:.1} ms average, slowest {:.0} ms ({})",
        t.thumbs,
        avg(t.thumb_time, t.thumbs),
        t.slowest.0.as_secs_f64() * 1e3,
        t.slowest.1
    );
    section("formats", &t.formats);
    section("probe failures", &t.probe_failed);
    section("thumbnail failures", &t.thumb_failed);
    section("orientations", &t.orientations);
    section("metadata found", &t.found);
    section("ICC profiles", &t.profiles);
    section("colour handling", &t.color);
    section("thumbnail sources", &t.sources);
    section("warnings", &t.warnings);
}

fn avg(total: Duration, n: usize) -> f64 {
    if n == 0 {
        0.0
    } else {
        total.as_secs_f64() * 1e3 / n as f64
    }
}

fn section<K: std::fmt::Display>(title: &str, m: &BTreeMap<K, usize>) {
    println!("\n{title}:");
    if m.is_empty() {
        println!("  (none)");
    }
    let mut rows: Vec<_> = m.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    for (k, v) in rows.into_iter().take(15) {
        println!("  {v:>5}  {k}");
    }
}
