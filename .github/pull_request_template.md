## What and why

<!-- What does this change, and why? Link the issue if there is one. -->

## How it was verified

- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo fmt --check`
- [ ] `cargo check --target wasm32-unknown-unknown -p <crate>` for any staged library crate I changed (the full list is in the README)

## Notes

<!-- Risks, follow-ups, anything a reviewer should know. -->
