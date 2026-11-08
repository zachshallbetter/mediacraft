//! Prints the raw metadata of files: `cargo run -p craft-raw --example inspect -- a.cr2 b.nef`
//! (handy while adding corpus files; the oracle expectations in `tests/it/raw_corpus.rs` come from it).
use std::env;

fn main() {
    for path in env::args().skip(1) {
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                println!("{path}: read error {e}");
                continue;
            }
        };
        match craft_raw::decode(&bytes, &craft_raw::Limits::default()) {
            Ok(s) => {
                println!(
                    "{path}: {:?} make={:?} model={:?} {}x{} samples={} cfa={:?} wb={:?} black={:?} white={:?} crop={:?}..{:?} orientation={} warnings={}",
                    s.format,
                    s.make,
                    s.model,
                    s.width,
                    s.height,
                    s.samples,
                    s.cfa,
                    s.camera_wb,
                    s.black,
                    s.white,
                    s.active,
                    s.crop,
                    s.orientation,
                    s.warnings.len(),
                );
            }
            Err(e) => println!("{path}: decode error {e}"),
        }
    }
}
