# Licensing policy

MediaCraft is MIT OR Apache-2.0. Everything it links must keep it that way for anyone who embeds it,
including Hermit CMS and the browser build. `deny.toml` enforces the list below; run
`cargo deny check licenses` before adding or upgrading a dependency.

## Allowed

- **Permissive:** MIT, MIT-0, Apache-2.0 (also WITH LLVM-exception), BSD-2-Clause, BSD-3-Clause,
  ISC, Zlib, Unicode-3.0, BSL-1.0, CC0-1.0.
- **Fonts and other data:** OFL-1.1 and Ubuntu-font-1.0, for bundled font files only (egui's
  default fonts in the desktop app, craft-fonts).
- **MPL-2.0, under conditions** (accepted 2026-11-07):
  1. **Use it unmodified, as a dependency from crates.io or a pinned git rev.** Don't copy MPL source
     files into MediaCraft crates.
  2. **If an MPL file has to be patched,** the patched file stays MPL-2.0. It lives in its own
     vendored crate under `vendor/` with its licence header and a note in `docs/VENDORED.md`, and
     the patched source is published with MediaCraft. Never mix MPL code into an MIT/Apache file.
  3. **Behind an optional cargo feature,** so a consumer that wants a permissive-only tree (for
     example `craft-image` with `default-features = false`) never compiles it. Name the feature
     after the capability (`mp3`, `vorbis`, `dwg`), not after the crate.
  4. **List it in `NOTICE`** with its source URL.

## Not allowed

GPL, LGPL, AGPL, SSPL, EUPL, the Commons Clause, non-commercial licences, and anything without a
licence. That covers model weights and data too: LightCraft's optional GPL-3.0 `rawnind-model`
weights stay out.

## Clean-room code

Code written for MediaCraft is clean-room work from public specifications. Don't consult or copy GPL
or LGPL implementations (ffmpeg, libheif, LibRaw, dcraw and the like). Using them as black-box test
oracles is fine.
