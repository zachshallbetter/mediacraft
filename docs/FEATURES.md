# MediaCraft — Universal Media Workspace: Product Direction v0.2

**Updated:** 2026-10-13  
**Status:** Product concept and research; not an implementation claim.  
**Supersedes the encoder-only product framing; preserves the original capability inventory below.**

## Product definition

**MediaCraft is a filesystem-native workspace for browsing, organizing, inspecting, converting, analyzing, and delivering creative assets.** It unifies digital asset management with comprehensive media processing and format conversion, avoiding legacy multi-window panel sprawl.

**Principles:** Filesystem is authoritative for file bytes and paths; an optional index stores derived metadata and relationships. Original media is never silently modified. The user chooses an *outcome* (make smaller, compatible, extract, proxy, prepare, analyze) before being asked to choose codecs. Advanced technical controls remain available. All outputs should carry reproducible provenance and, where specified, verified delivery results. Offline-first by default; cloud/ML optional.

## Primary workflows and surfaces

| Surface | User task | Key interactions | Initial priority |
|---|---|---|---|
| Browse | Find and select files | Folders, grid/list, previews, search, filters, drag-and-drop | P0 |
| Inspect | Understand an asset | Preview, streams, EXIF/XMP, color, audio, provenance, related files | P0/P1 |
| Collections | Organize across folders | Ratings, labels, tags, saved searches, smart collections, stacks | P1 |
| Process | Specify an outcome | Quick actions, compatible destinations, custom settings, dry-run | P0/P1 |
| Queue | Execute and verify | Multiple outputs/source, priority, progress, logs, pause, retry | P0 |
| Automate | Repeat workflows | Watch folders, rule-based collections, saved recipes, CLI/API | P1/P2 |

**UI direction:** A restrained dark professional workspace with a compact left navigation rail, generous central asset grid, optional right inspector, and a collapsible bottom activity drawer. The process configuration opens as a contextual side sheet, not a permanently visible encoder form. Search and filters are always available. Asset browsing is the default launch state, not an empty queue. Use progressive disclosure rather than simultaneous multi-quadrant clutter.

## Digital asset management feature inventory

| Capability | Behavior | Priority |
|---|---|---|
| Filesystem-native browsing | Local and mounted volumes; no mandatory import or relocation | P0 |
| Asset indexing | Incremental scan, cancellation, resilient restart, background thumbnail cache | P0 |
| Grid/list/contact sheet | Adjustable density, sorting, thumbnails, keyboard navigation | P0 |
| Rich previews | Images, video scrub, audio waveform, supported RAW and image sequences | P1 |
| Search and facets | Filename, path, type, size, date, resolution, duration, codec, tags | P0/P1 |
| Metadata inspection | Technical stream metadata plus EXIF/IPTC/XMP where supported | P0/P1 |
| Ratings, labels, keywords | Bulk operations; portable sidecar strategy where applicable | P1 |
| Collections and smart collections | References across folders, saved dynamic predicates | P1 |
| Batch rename and metadata edits | Preview, collision checking, undoable operation log | P1 |
| Duplicates and similarity | Hash-identical P1; perceptual near-duplicate P2 | P1/P2 |
| Asset stacks and versions | Group related takes, originals, proxies, and revisions | P2 |
| Offline and relink | Preserve catalog references when files move or disconnect | P1 |
| Contact sheets and exports | Generate browsable and printable asset overviews | P2 |
| Optional semantic discovery | Transcription, scene/object tags, visual/audio similarity | P3 |

## Asset and derivative model

- **Asset identity:** stable catalog identity independent of mutable path; content fingerprint and file observations recorded separately. Do not infer identity solely from filename.
- **File instance:** an observed file at a location, with online/offline status, size, timestamps, and fingerprint confidence.
- **Streams:** typed video, audio, caption, and metadata tracks; each has independent properties.
- **Derivation:** immutable relationship from source asset/version to output, including operation, preset/contract version, encoder identity, timestamp, and verification receipt.
- **Collections:** references, not filesystem copies. Sidecar/export interoperability and user privacy must be specified before implementation.
- **Safe mutations:** originals unchanged by default; file moves/renames need collision preflight, rollback/undo where possible, and clear failure reporting.

Example: `interview.mov` → `interview_proxy.mp4` (proxy), `interview_web.mp4` (delivery), `interview_audio.wav` (extraction), each linked to its source and recipe. Stale derivatives can be detected when the observed source version changes.

## Outcome-first processing and delivery contracts

| Intent | Planner candidate | Verification |
|---|---|---|
| Make smaller | Encode using source-aware quality target | File size and decode validity |
| Make compatible | Negotiate container, codec, profile, device target | Required stream/profile constraints |
| Extract | Stream copy or decode/export audio, frames, captions | Stream/sample/frame count as appropriate |
| Create proxy | Transcode with source timing/timecode preserved | Timing and relink constraints |
| Change container | Remux without re-encoding when legal | Container and stream equivalence |
| Prepare delivery | Build outputs to named constraints | Explicit accept/warn/reject result |
| Analyze | Probe, loudness, integrity, color, quality | Machine-readable findings |

**Plan → execute → verify:** probe the source; determine feasible transformations; prefer lossless stream copy/remux when compatible; surface tradeoffs (quality, size, speed, licensing, hardware); execute; inspect output; verify user-defined delivery constraints. A *preset* is a set of parameters; a *delivery contract* is a testable statement about acceptable output. Estimates are estimates, never guarantees.

## Mockup specification (desktop, 16:10)

- Top bar: MediaCraft identity, workspace switcher, global search, import, activity indicator.
- Left rail: Browse, Collections, Process, Queue, Automations; beneath, local drives and favorites.
- Main: breadcrumb, title `Summer Campaign`, filter chips (`All`, `Video`, `Photos`, `Audio`, `Projects`), asset grid with mixed thumbnail types, small format badges and duration, selected state.
- Right inspector: large preview of selected video, filename, codec, dimensions, duration, audio, color, file size; contextual actions `Make smaller`, `Create proxy`, `Extract audio`, `More…`.
- Bottom activity drawer: compact two-job queue with progress and verified completion badge; expandable details, pause/resume.
- Use high-information density without crowding; avoid heavy gradients, neon sci-fi styling, rigid permanent quad-panels, and invented production claims.

## Architecture and scope boundaries

| Layer | Responsibility | Ownership |
|---|---|---|
| `craft-media-*` primitives | Media types, codecs, containers, probe/validation, negotiation | Candidate shared crates, gated by evidence and maintainer review |
| Transform planner | Stream copy vs remux vs decode/filter/encode, contract checks | Candidate shared service after consumer validation |
| Asset catalog | Filesystem observations, index, metadata, relationships, collections | Initially MediaCraft-owned; extract only on demonstrated cross-app demand |
| MediaCraft UI | Browse, inspector, collections, processing, queue, automation | Application-owned |
| Integration adapters | FilmCraft/SoundCraft/EffectCraft submissions and export receipts | Explicit contracts, consumer-tested |

**No blanket crate migration.** The initial shared-library proposal remains a narrow encoder contract and one proven extraction/adoption, subject to `craft-libs` maintainers. An integrated asset workspace is a product direction, not justification for putting a catalog or UI into `craft-libs`.

## Revised implementation milestones

| Phase | Outcome | Acceptance evidence |
|---|---|---|
| M0 | Shared media contract and AAC/M4A proof | Golden vectors, packaging/timing checks, second consumer |
| M1 | Filesystem browse + probe + index | Mixed-file fixtures, incremental rescan, offline/relink cases |
| M2 | Preview + inspector + search | Video/audio/image coverage, metadata correctness, responsive large folders |
| M3 | Contextual conversion + queue | Multiple outputs, durable jobs, dry-run, source-preserving behavior |
| M4 | Collections + batch operations | Saved searches, tags, safe rename/move, recovery |
| M5 | Derivative graph + delivery validation | Source/output provenance, stale-output detection, receipt tests |
| M6 | Watch folders + advanced media processing | Automation integration, professional formats, compliance tests |
| M7 | Optional advanced discovery and distributed execution | Privacy, capability, performance, and reliability gates |

## Open research questions

1. What can be previewed/probed natively and what requires third-party codec licensing?
2. Which metadata should be indexed, sidecar-written, or never modified?
3. What are the catalog identity and move/relink rules across external volumes?
4. How do preview caches and large folders perform under constrained memory?
5. Which cross-app export workflows can validate shared crates without coupling app models?
6. How do we distinguish documented daemon/oracle test outcomes from real-media end-to-end conformance?

---

# Appendix A — Original Media Infrastructure Research (preserved)

# Media Processing & Transcoding — Comprehensive Feature & Capability Research

**Research scope:** A professional media encoding application and processing engine, with particular attention to capabilities that could be implemented as reusable Rust crates in `craft-libs`.

The goal is to identify both the capabilities of a standalone **MediaCraft** application and the underlying infrastructure that FilmCraft, SoundCraft, EffectCraft, and other Craft applications could share.

This is a proposed capability inventory, **not a claim that these features already exist in the Craft repositories**.

## 1. What a complete media encoder actually does

A media encoder is more than a collection of codecs. A comprehensive implementation supports the entire processing lifecycle:

```text
MEDIA SOURCES
    │
    ├── Video files
    ├── Audio files
    ├── Image sequences
    ├── Application projects
    ├── Cameras / capture devices
    ├── Network sources
    └── Live streams
    │
    ▼
MEDIA INSPECTION
    │
    ├── Container identification
    ├── Codec detection
    ├── Stream enumeration
    ├── Metadata extraction
    ├── Technical validation
    └── Capability negotiation
    │
    ▼
DECODING
    │
    ├── Video decoding
    ├── Audio decoding
    ├── Subtitle decoding
    └── Hardware acceleration
    │
    ▼
PROCESSING
    │
    ├── Video filters
    ├── Audio processing
    ├── Color management
    ├── Frame-rate conversion
    ├── Scaling
    ├── Timeline trimming
    └── Stream synchronization
    │
    ▼
ENCODING
    │
    ├── Video codecs
    ├── Audio codecs
    ├── Image codecs
    ├── Software encoding
    └── Hardware encoding
    │
    ▼
PACKAGING
    │
    ├── Containers
    ├── Metadata
    ├── Captions
    ├── Chapters
    └── Streaming manifests
    │
    ▼
VALIDATION
    │
    ├── Decode verification
    ├── Quality measurements
    ├── Loudness compliance
    ├── Stream compatibility
    └── File integrity
    │
    ▼
DELIVERY
    │
    ├── Local files
    ├── Watch folders
    ├── Network destinations
    ├── Streaming packages
    └── Automated workflows
```

Modern media pipelines emphasize persistent queues, presets, watch folders, stream operations, technical analysis, and hardware-specific encoding capabilities.

---

# 2. Complete feature inventory

The following tables distinguish proposed priority from implementation status.

**Priority definitions:** P0 = essential foundation; P1 = competitive general-purpose encoder; P2 = professional workflows; P3 = specialized or advanced capabilities.

## A. Media ingestion and source management

| Feature | Description | Priority |
|---|---|---|
| Drag-and-drop import | Add files directly to the encoding queue | P0 |
| File and folder import | Import individual files or entire directories | P0 |
| Recursive folder scanning | Discover media within nested folders | P1 |
| Media browser | Browse sources with thumbnails and metadata | P1 |
| Batch import | Import hundreds or thousands of files | P0 |
| File-type filtering | Restrict sources by format or extension | P1 |
| Automatic format detection | Identify actual formats independently of filenames | P0 |
| Media probing | Read codec, resolution, duration, bitrate, and stream details | P0 |
| Multi-stream discovery | Detect multiple audio, video, and subtitle tracks | P0 |
| Image-sequence detection | Recognize numbered image sequences | P1 |
| Image-sequence import | Interpret sequences as video sources | P1 |
| Audio-only sources | Support audio transcoding without video | P0 |
| Video-only sources | Support silent video and video-only streams | P0 |
| Project import | Accept exports or render jobs from Craft applications | P1 |
| Watch folders | Automatically ingest new files | P1 |
| Network files | Read supported remote or mounted sources | P2 |
| URL ingestion | Read supported HTTP/HTTPS media sources | P2 |
| Camera/card structures | Recognize professional acquisition directories | P2 |
| Growing files | Process media while source files are still being written | P3 |
| Capture-device input | Ingest cameras, microphones, or capture devices | P3 |
| Live-stream input | Ingest RTMP, SRT, or other supported streams | P3 |
| Duplicate detection | Detect repeated files or identical content | P1 |
| Corruption detection | Identify invalid or incomplete sources | P1 |
| Source fingerprinting | Track inputs using hashes and technical metadata | P2 |
| Source change detection | Detect modified inputs before processing | P1 |

A significant design requirement is distinguishing a **media asset** from a **media stream**. A single MOV file may contain video, several audio tracks, subtitles, timecode, and metadata.

---

## B. Encoding queue and job management

This is one of the most important application-level capabilities.

MediaCraft supports persistent queues, multiple outputs from one source, automated execution, and watch-folder processing.

| Feature | Description | Priority |
|---|---|---|
| Encoding queue | Maintain ordered encoding jobs | P0 |
| Multiple outputs per source | Produce several formats from one input | P0 |
| Job presets | Assign reusable configurations | P0 |
| Start/pause queue | Control queue execution | P0 |
| Cancel individual job | Stop one encoding operation | P0 |
| Cancel all jobs | Stop the queue safely | P0 |
| Retry failed jobs | Reattempt recoverable failures | P1 |
| Resume interrupted jobs | Resume supported jobs after interruption | P2 |
| Persistent queue | Restore jobs after application restart | P0 |
| Background encoding | Continue processing without foreground interaction | P0 |
| Concurrent jobs | Run multiple independent encodes | P1 |
| Priority scheduling | Prioritize selected jobs | P1 |
| Dependency scheduling | Run jobs after prerequisite operations | P2 |
| CPU/GPU resource limits | Restrict resource consumption | P1 |
| Estimated completion time | Display approximate remaining duration | P1 |
| Encoding speed | Display frames per second and realtime multiplier | P0 |
| Per-job progress | Show current progress and stage | P0 |
| Aggregate progress | Display overall queue completion | P1 |
| Detailed job logs | Preserve encoder messages and failures | P0 |
| Job history | Record completed operations | P1 |
| Output verification | Check completed output before marking success | P1 |
| Automatic shutdown | Shut down or sleep after queue completion | P2 |
| Notifications | Notify when jobs complete or fail | P1 |
| Job duplication | Create variations of existing jobs | P1 |
| Queue import/export | Save and load job collections | P2 |
| Job templates | Reusable multi-output workflows | P2 |
| Scheduled encoding | Start processing at specified times | P2 |
| Distributed workers | Send jobs to other computers | P3 |
| Remote queue management | Monitor and control jobs remotely | P3 |

**Architectural distinction:** The encoding engine should not own the desktop queue interface.

A shared job execution service could eventually be useful across FilmCraft, EffectCraft, and MediaCraft, but application-specific queue presentation should remain separate.

---

# 3. Video encoding capabilities

## A. Video codec inventory

The codec matrix below represents a research target. Support must be qualified independently for encoding, decoding, platform availability, licensing, and conformance.

| Codec | Decode | Encode | Primary use | Priority |
|---|---|---|---|---|
| H.264 / AVC | Yes | Yes | General delivery, compatibility | P0 |
| H.265 / HEVC | Yes | Yes | High-efficiency video, HDR | P1 |
| AV1 | Yes | Yes | Modern distribution and streaming | P1 |
| VP9 | Yes | Yes | Web delivery | P2 |
| VP8 | Yes | Evaluate | Legacy WebM | P3 |
| MPEG-2 Video | Yes | Yes | Legacy broadcast and DVD workflows | P2 |
| MPEG-4 Part 2 | Yes | Evaluate | Legacy interchange | P3 |
| Apple ProRes | Yes | Yes | Professional editing and mastering | P1 |
| Avid DNxHD | Yes | Yes | Professional editing | P2 |
| Avid DNxHR | Yes | Yes | High-resolution editing | P2 |
| FFV1 | Yes | Yes | Lossless archival | P2 |
| HuffYUV | Yes | Evaluate | Legacy lossless workflows | P3 |
| Ut Video | Yes | Evaluate | Lossless intermediates | P3 |
| CineForm | Evaluate | Evaluate | Editing intermediates | P3 |
| JPEG 2000 | Yes | Evaluate | Cinema and archival workflows | P3 |
| Motion JPEG | Yes | Yes | Camera and legacy workflows | P2 |
| Uncompressed video | Yes | Yes | Production interchange | P1 |
| H.266 / VVC | Evaluate | Evaluate | Emerging high-efficiency delivery | P3 |
| JPEG XS | Evaluate | Evaluate | Low-latency professional transport | P3 |
| AV1 lossless modes | Evaluate | Evaluate | Specialized lossless workflows | P3 |

Here, “Yes” means **desired capability**, not verified Craft implementation.

### Encoder configuration

| Capability | Description | Priority |
|---|---|---|
| Constant quality | CRF/CQ-style quality targets | P0 |
| Constant bitrate | Target fixed or constrained bitrate | P1 |
| Average bitrate | Target average bitrate | P0 |
| Variable bitrate | Allow bitrate variation | P0 |
| Two-pass encoding | Analyze first, encode second | P1 |
| Multi-pass encoding | Additional analysis passes where supported | P2 |
| Encoder speed presets | Trade encoding speed against compression efficiency | P0 |
| Codec profiles | Select supported codec profiles | P1 |
| Codec levels | Set level constraints | P1 |
| Bit depth | Support 8-, 10-, and higher-bit-depth paths | P1 |
| Chroma subsampling | Support 4:2:0, 4:2:2, 4:4:4 where available | P1 |
| GOP configuration | Configure group-of-pictures structure | P1 |
| Keyframe interval | Configure keyframe frequency | P1 |
| B-frame configuration | Configure supported B-frame behavior | P2 |
| Reference-frame settings | Configure prediction references | P2 |
| Lookahead | Enable encoder lookahead | P2 |
| Adaptive quantization | Adjust quality allocation | P2 |
| Psychovisual tuning | Optimize perceptual quality | P2 |
| Film/animation/grain tuning | Apply content-specific tuning | P2 |
| HDR metadata | Preserve or generate supported HDR signaling | P1 |
| Alpha-channel encoding | Preserve transparency in supported formats | P2 |
| Lossless encoding | Produce lossless video where supported | P1 |
| Low-latency encoding | Minimize encoder buffering | P2 |
| Hardware encoding | Use supported media engines | P1 |
| Software fallback | Fall back when hardware encoding is unavailable | P1 |

These controls are codec-dependent. A capability registry should expose supported options rather than pretending every encoder implements the same configuration.

---

# 4. Audio encoding capabilities

This category is especially relevant to the proposed `craft-media-enc` extraction.

## A. Audio codec inventory

| Codec / format | Intended support | Primary use | Priority |
|---|---|---|---|
| PCM WAV | Encode/decode | Uncompressed interchange | P0 |
| AIFF | Encode/decode | Professional audio interchange | P0 |
| FLAC | Encode/decode | Lossless compressed audio | P0 |
| AAC-LC | Encode/decode | General audio and video delivery | P0 |
| MP3 | Encode/decode | Broad compatibility | P1 |
| Opus | Encode/decode | Streaming, speech, modern audio | P1 |
| Vorbis | Encode/decode | Open media workflows | P2 |
| ALAC | Encode/decode | Apple lossless workflows | P2 |
| AC-3 | Decode; encode subject to qualification | Broadcast and surround sound | P2 |
| E-AC-3 | Evaluate | Professional delivery | P3 |
| WavPack | Evaluate | Lossless archival | P3 |
| DSD | Evaluate | Specialized high-resolution audio | P3 |
| AMR | Evaluate | Legacy speech media | P3 |

### Audio processing

| Feature | Description | Priority |
|---|---|---|
| Sample-rate conversion | Convert between sample rates | P0 |
| Bit-depth conversion | Convert PCM precision | P0 |
| Dithering | Reduce quantization artifacts | P1 |
| Channel remapping | Change channel ordering | P0 |
| Mono/stereo conversion | Downmix or upmix supported layouts | P0 |
| Multichannel audio | Preserve surround configurations | P1 |
| Channel-layout metadata | Explicit speaker positions and layouts | P1 |
| Audio normalization | Peak or loudness-based normalization | P1 |
| LUFS analysis | Measure integrated loudness | P1 |
| True-peak measurement | Detect intersample peaks | P1 |
| Loudness targeting | Normalize to configured LUFS targets | P1 |
| Limiting | Control excessive peaks | P2 |
| Dynamic-range compression | Apply configurable compression | P2 |
| Silence detection | Identify silent sections | P2 |
| Silence trimming | Remove leading/trailing silence | P2 |
| Audio extraction | Extract audio from video | P0 |
| Audio replacement | Replace selected audio streams | P1 |
| Track splitting | Export individual tracks | P1 |
| Track merging | Combine compatible audio streams | P1 |
| Audio offsets | Correct synchronization offsets | P1 |
| Time stretching | Change duration while preserving pitch | P2 |
| Pitch shifting | Change pitch independently | P3 |
| Audio fades | Apply fades and crossfades | P2 |
| Noise reduction | Reduce background noise | P3 |
| Stem separation | Separate vocals/instruments using optional models | P3 |
| Speech transcription | Generate text or captions | P3 |
| Audio fingerprinting | Identify repeated or matching audio | P3 |

Professional audio processing extends beyond basic transcoding to include LUFS analysis, normalization, track remapping, polyphase resampling, audio extraction, and metadata preservation.

---

# 5. Containers and stream packaging

A codec does not automatically produce a playable media file.

The system needs a separate packaging layer.

| Container | Intended support | Priority |
|---|---|---|
| MP4 | Read/write | P0 |
| M4A | Read/write | P0 |
| MOV / QuickTime | Read/write | P1 |
| Matroska / MKV | Read/write | P1 |
| WebM | Read/write | P1 |
| WAV / RF64 | Read/write | P0 |
| AIFF | Read/write | P0 |
| FLAC native container | Read/write | P0 |
| Ogg | Read/write | P1 |
| MPEG-TS | Read/write | P2 |
| MXF | Read/write | P2 |
| AVI | Read; write where justified | P2 |
| MPEG-PS | Evaluate | P3 |
| CAF | Read/write | P2 |
| ADTS | Read/write | P1 |
| Raw elementary streams | Read/write | P1 |

### Stream operations

| Feature | Description | Priority |
|---|---|---|
| Remux | Change container without re-encoding | P0 |
| Stream copy | Preserve compressed streams | P0 |
| Stream selection | Choose individual tracks | P0 |
| Stream reordering | Change stream order | P1 |
| Stream extraction | Export individual streams | P0 |
| Stream replacement | Replace audio or subtitle streams | P1 |
| Track metadata | Preserve language, title, and disposition | P1 |
| Chapter preservation | Retain chapter markers | P1 |
| Timecode preservation | Retain supported timecode metadata | P1 |
| Fast-start MP4 | Optimize MP4 for progressive playback | P1 |
| Fragmented MP4 | Produce fragmented outputs | P2 |
| Edit-list handling | Preserve timing offsets and edits | P2 |
| Gapless audio | Correctly represent encoder delay and padding | P1 |
| Variable-frame-rate timing | Preserve valid source timestamps | P1 |
| Interleaving | Correctly interleave audio/video packets | P0 |
| Container validation | Verify structural correctness | P1 |
| Recoverable finalization | Safely handle interrupted writes where feasible | P2 |

**For AAC/M4A specifically, gapless timing and container correctness are important acceptance criteria.** Successfully encoding AAC frames alone does not establish that a usable M4A export exists.

---

# 6. Video processing and filtering

| Category | Capabilities | Priority |
|---|---|---|
| Scaling | Resize, aspect-ratio preservation, fit/fill | P0 |
| Cropping | Crop by dimensions, percentage, or preset | P0 |
| Padding | Add letterboxing or pillarboxing | P1 |
| Rotation | Rotate and flip | P0 |
| Frame-rate conversion | CFR/VFR conversion and frame duplication/drop | P1 |
| Frame interpolation | Motion-based intermediate frames | P2 |
| Deinterlacing | YADIF/BWDIF and equivalent methods | P1 |
| Inverse telecine | Restore progressive frames from telecine | P2 |
| Denoising | Spatial and temporal noise reduction | P2 |
| Sharpening | Unsharp and other sharpening filters | P2 |
| Deblocking | Reduce compression artifacts | P2 |
| Stabilization | Correct camera movement | P2 |
| Color correction | Exposure, contrast, gamma, saturation | P1 |
| Color conversion | Convert color spaces and transfer functions | P1 |
| HDR-to-SDR | Tone mapping | P1 |
| SDR-to-HDR | Controlled conversion and signaling | P3 |
| LUT processing | Apply 1D/3D LUTs | P2 |
| Alpha processing | Preserve or transform transparency | P2 |
| Watermarking | Image and text overlays | P1 |
| Timecode burn-in | Render visible timecode | P2 |
| Subtitle burn-in | Render captions into video | P1 |
| Frame extraction | Export individual frames | P1 |
| Thumbnail generation | Produce contact sheets and thumbnails | P1 |
| Scene detection | Detect scene boundaries | P2 |
| Black-frame detection | Identify black or missing frames | P2 |
| Freeze detection | Identify repeated or frozen video | P2 |
| Letterbox detection | Detect existing black borders | P2 |
| Duplicate-frame removal | Remove repeated frames | P2 |
| Motion-blur synthesis | Generate motion blur during conversion | P3 |

Advanced video transformations encompass frame-rate conforming, deinterlacing, color-space conversion, 3D LUT application, stabilization, and black-frame detection.

---

# 7. Color management and HDR

This deserves its own subsystem because it is shared by video, photography, graphics, and rendering applications.

| Capability | Description | Priority |
|---|---|---|
| Color-space identification | Identify primaries, transfer function, and matrix coefficients | P0 |
| Range identification | Distinguish full-range and limited-range values | P0 |
| Rec.709 | SDR video workflows | P0 |
| sRGB | Computer and web media | P0 |
| Display P3 | Wide-gamut display workflows | P1 |
| Rec.2020 | Wide-gamut video workflows | P1 |
| PQ / ST 2084 | HDR transfer function | P1 |
| HLG | Broadcast HDR transfer function | P2 |
| ICC profile handling | Read and apply supported profiles | P2 |
| Color metadata preservation | Retain valid source color signaling | P0 |
| RGB/YUV conversion | Correctly convert pixel representations | P0 |
| Chroma resampling | Convert between subsampling formats | P1 |
| Tone mapping | Convert HDR luminance to SDR | P1 |
| Gamut mapping | Handle out-of-gamut colors | P2 |
| 3D LUT application | Apply creative and technical LUTs | P2 |
| HDR10 metadata | Preserve or set supported metadata | P1 |
| Dolby Vision | Evaluate licensed/available workflows | P3 |
| Color validation | Check signaling against encoded pixel data | P2 |
| High-bit-depth pipelines | Avoid unnecessary precision loss | P1 |

A key requirement is **never silently treating missing or ambiguous color metadata as proof of a particular color space**.

The encoder should expose assumptions and transformations explicitly.

---

# 8. Subtitles, captions, and accessibility

| Capability | Description | Priority |
|---|---|---|
| Subtitle stream detection | Identify embedded subtitles | P1 |
| SRT | Import/export | P1 |
| WebVTT | Import/export | P1 |
| ASS/SSA | Import/export | P2 |
| CEA-608 | Preserve/convert where supported | P2 |
| CEA-708 | Preserve/convert where supported | P2 |
| TTML | Evaluate | P2 |
| IMSC | Evaluate | P3 |
| Subtitle burn-in | Render subtitles into video | P1 |
| Soft subtitles | Package selectable subtitle tracks | P1 |
| Subtitle extraction | Export embedded captions | P1 |
| Subtitle conversion | Convert between supported formats | P2 |
| Language metadata | Assign language tags | P1 |
| Forced subtitles | Preserve forced-caption disposition | P2 |
| Subtitle synchronization | Apply timing offsets | P1 |
| Caption validation | Detect invalid timing or overlaps | P2 |
| Speech-to-text | Generate captions from audio | P3 |
| Caption translation | Optional language-model-assisted translation | P3 |

---

# 9. Image sequences and still-image encoding

A dedicated media encoder should also handle image sequences and still-image conversion.

| Capability | Description | Priority |
|---|---|---|
| PNG | Read/write | P0 |
| JPEG | Read/write | P0 |
| WebP | Read/write | P1 |
| AVIF | Read/write | P1 |
| TIFF | Read/write | P1 |
| OpenEXR | Read/write | P2 |
| JPEG XL | Evaluate | P2 |
| HEIF/HEIC | Evaluate | P2 |
| BMP | Read/write | P2 |
| GIF | Read/write, including animation | P2 |
| APNG | Read/write | P2 |
| Animated WebP | Read/write | P2 |
| Image sequence to video | Convert frames to video | P1 |
| Video to image sequence | Export frames | P1 |
| Image sequence frame rate | Assign rational frame rates | P1 |
| Missing-frame detection | Identify gaps in sequences | P1 |
| Sequence renumbering | Normalize frame filenames | P2 |
| Alpha preservation | Preserve supported transparency | P1 |
| HDR image processing | Preserve high-dynamic-range image data | P2 |
| Batch image conversion | Convert many images with presets | P1 |
| Image metadata | Preserve supported EXIF/XMP fields | P2 |

---

# 10. Professional production workflows

This category differentiates a basic transcoder from a production-oriented application.

| Capability | Description | Priority |
|---|---|---|
| Editing intermediates | ProRes, DNxHR, lossless outputs | P1 |
| Proxy generation | Produce smaller editing copies | P1 |
| Proxy presets | Match source timecode and metadata | P1 |
| Relinkable proxies | Preserve identity and relationships | P2 |
| Camera-original preservation | Avoid unintended modification of source media | P0 |
| Timecode-aware exports | Preserve start timecode | P1 |
| Reel/tape metadata | Preserve supported production metadata | P2 |
| Broadcast loudness | EBU R128 and ATSC A/85-oriented workflows | P2 |
| Broadcast-safe levels | Validate and convert supported video levels | P2 |
| MXF OP1a | Professional broadcast interchange | P2 |
| MXF OP-Atom | Avid-oriented interchange | P2 |
| AS-10 / AS-11 | Evaluate broadcast delivery profiles | P3 |
| IMF packaging | Evaluate interoperable master packages | P3 |
| DCP creation | Evaluate digital cinema packaging | P3 |
| Multichannel masters | Preserve supported surround layouts | P1 |
| Alpha intermediates | Produce video with alpha where supported | P2 |
| Conform workflows | Generate deliverables from source/timecode relationships | P3 |
| Render handles | Include additional frames around edits | P2 |
| Production metadata | Preserve camera, scene, take, and reel information | P2 |
| Technical delivery reports | Produce structured validation results | P1 |

---

# 11. Streaming and web delivery

| Capability | Description | Priority |
|---|---|---|
| Web-optimized MP4 | Progressive download support | P1 |
| HLS packaging | Generate HLS media packages | P2 |
| MPEG-DASH | Generate DASH packages | P2 |
| Adaptive bitrate ladders | Produce multiple renditions | P2 |
| Fragmented MP4 | Support segmented delivery | P2 |
| CMAF | Evaluate compatible fragmented-media packaging | P2 |
| Segment alignment | Align segment and keyframe boundaries | P2 |
| Manifest generation | Generate supported playlists and manifests | P2 |
| Audio-only streaming | Produce audio streaming renditions | P2 |
| Multiple audio languages | Package alternate language tracks | P2 |
| Alternate subtitle tracks | Package subtitles for streaming | P2 |
| Low-latency HLS | Evaluate | P3 |
| DRM packaging | Evaluate separately from encryption and licensing | P3 |
| Streaming compliance validation | Verify manifests and segments | P2 |
| Upload integration | Deliver to supported remote destinations | P2 |
| CDN integration | Optional publication adapters | P3 |

---

# 12. Media analysis and quality assurance

This is an area where a new Craft application could become especially useful.

Most users care about whether an output is actually correct—not merely whether the encoder returned success.

| Feature | Description | Priority |
|---|---|---|
| File integrity | Verify output exists and is readable | P0 |
| Decode verification | Decode generated media to detect errors | P1 |
| Stream verification | Compare expected and actual streams | P1 |
| Duration verification | Check output duration against expectations | P1 |
| Frame-count verification | Check expected frame counts | P1 |
| Audio sample-count verification | Check sample counts and encoder delay | P1 |
| Metadata verification | Check required metadata | P1 |
| Container validation | Validate file structure | P1 |
| PSNR | Objective pixel-difference metric | P2 |
| SSIM | Structural image-quality metric | P2 |
| VMAF | Perceptual video-quality estimation | P2 |
| Audio loudness | Integrated, short-term, momentary loudness | P1 |
| True-peak analysis | Detect intersample clipping | P1 |
| Clipping detection | Identify clipped samples | P1 |
| Silence detection | Identify unexpected silence | P2 |
| Black-frame detection | Identify unexpected black frames | P2 |
| Freeze detection | Identify unexpected frozen frames | P2 |
| A/V synchronization | Detect timing differences | P2 |
| Timestamp monotonicity | Validate packet timestamp ordering | P1 |
| Color metadata consistency | Detect contradictory color signaling | P2 |
| Quality regression testing | Compare output against pinned reference fixtures | P1 |
| Reproducibility reports | Record encoder settings, versions, and environment | P1 |
| Compliance profiles | Validate against named delivery specifications | P2 |
| Machine-readable reports | Export JSON results | P1 |
| Human-readable reports | Generate technical summaries | P1 |

Quality control mechanisms encompass VMAF objective scoring, BS.1770-4 loudness and True-Peak analysis, black-frame detection, and stream conformance validation.

A particularly valuable design choice would be to make validation part of the job contract:

```text
ENCODE
   │
   ▼
FINALIZE
   │
   ▼
PROBE OUTPUT
   │
   ▼
VERIFY STREAMS
   │
   ▼
VERIFY TECHNICAL REQUIREMENTS
   │
   ▼
OPTIONAL QUALITY COMPARISON
   │
   ▼
ACCEPT / REJECT / WARN
```

That would distinguish an encoding operation that merely finished from an output that satisfied its delivery requirements.

---

# 13. Automation and integration

For the Craft ecosystem, automation should be treated as a first-class capability.

| Feature | Description | Priority |
|---|---|---|
| CLI | Perform encoding without a GUI | P0 |
| Headless operation | Run on machines without displays | P0 |
| JSON job specifications | Define jobs programmatically | P0 |
| Preset serialization | Save and exchange encoding configurations | P0 |
| Structured progress | Emit machine-readable progress events | P0 |
| Structured errors | Return typed failures | P0 |
| Batch execution | Execute many jobs from one specification | P0 |
| Watch-folder automation | Trigger workflows when files arrive | P1 |
| MCP interface | Expose safe encoding operations to agents | P1 |
| Local control API | Submit and inspect jobs programmatically | P1 |
| Application integration | Accept render/export jobs from Craft applications | P1 |
| Capability discovery | Query supported codecs and options | P0 |
| Dry-run validation | Check jobs without executing them | P1 |
| Resource estimates | Estimate disk, memory, and compute requirements | P2 |
| Webhooks | Emit completion/failure events | P2 |
| Job dependency graphs | Express multi-stage workflows | P2 |
| Remote workers | Execute jobs on other machines | P3 |
| Distributed scheduling | Assign jobs across worker pools | P3 |
| Reproducible job manifests | Record inputs, outputs, and tool versions | P1 |
| Audit trails | Record significant execution decisions | P2 |

### Example capability query

```json
{
  "operation": "media.capabilities",
  "requirements": {
    "container": "mp4",
    "video_codec": "h264",
    "audio_codec": "aac",
    "hardware_preferred": true
  }
}
```

A response should identify actual available implementations, supported constraints, and reasons a requested combination cannot be fulfilled.

It should not simply return a static list of known codec names.

---

# 14. Hardware acceleration and performance

An important architectural invariant: hardware encoding does not imply that decoding, filtering, audio processing, and muxing also run on hardware. Each stage must be evaluated separately.

| Capability | Description | Priority |
|---|---|---|
| CPU encoding | Portable software encoding | P0 |
| Hardware capability detection | Identify supported accelerators | P1 |
| Apple VideoToolbox | macOS hardware media processing | P1 |
| Intel Quick Sync | Intel hardware acceleration | P2 |
| NVIDIA NVENC/NVDEC | NVIDIA hardware acceleration | P2 |
| AMD AMF/VCN | AMD hardware acceleration | P2 |
| Windows Media Foundation | Windows media processing | P2 |
| Hardware decode | Decode supported formats on accelerators | P1 |
| Hardware encode | Encode supported formats on accelerators | P1 |
| Hardware scaling | Scale supported frames on accelerators | P2 |
| GPU color conversion | Perform supported pixel/color conversions | P2 |
| Zero-copy buffers | Avoid unnecessary CPU/GPU transfers | P2 |
| Automatic fallback | Use software implementations when necessary | P1 |
| Explicit device selection | Select preferred accelerator | P2 |
| Encoder benchmarking | Measure speed and output characteristics | P1 |
| Thermal-aware scheduling | Adjust concurrency under thermal constraints | P3 |
| Memory budgeting | Bound in-flight media buffers | P1 |
| Disk-space preflight | Check estimated output storage | P1 |
| Parallel encoding | Process independent jobs concurrently | P1 |
| Deterministic mode | Reduce nondeterminism where feasible | P2 |

Hardware and software encoders can produce different outputs at the same nominal bitrate. The system should record the actual implementation used.

---

# 15. Presets and delivery profiles

Presets are not merely saved settings. They can represent explicit compatibility targets.

Production presets span device-oriented, web-oriented, production-oriented, and hardware-specific configurations.

| Preset family | Examples | Priority |
|---|---|---|
| General | Fast, balanced, high quality | P0 |
| Web | Web MP4, WebM, AV1 | P1 |
| Social media | Portrait, square, landscape | P1 |
| Mobile | iPhone, Android, tablets | P1 |
| Desktop | General desktop playback | P1 |
| Streaming | HLS/DASH ladders | P2 |
| Editing | ProRes, DNxHR, proxy | P1 |
| Archival | FFV1, FLAC, lossless | P2 |
| Broadcast | MXF and constrained delivery | P2 |
| Audio | Podcast, music, speech, lossless | P1 |
| Image | Web images, archival images | P2 |
| Custom | User-defined profiles | P0 |
| Organization | Shared team presets | P2 |
| Application | FilmCraft/SoundCraft/EffectCraft export profiles | P1 |

I would distinguish three types of presets:

**Convenience presets** optimize common workflows.

**Compatibility presets** specify playback constraints.

**Compliance presets** define testable technical requirements.

Only the third category should imply formal compliance, and only when the required validation is actually implemented.

---

# 16. Proposed reusable Rust crate architecture

This is where the research connects directly to `craft-libs`.

I would not propose migrating every feature into shared libraries immediately.

Instead, separate the reusable technical mechanisms from the application-level workflow.

| Proposed crate | Responsibility | Candidate source |
|---|---|---|
| `craft-media` | Common media descriptions, stream metadata, capability interfaces | New shared contract |
| `craft-media-enc` | Encoding orchestration and encoder interfaces | New contract; existing codec implementations as candidates |
| `craft-media-dec` | Decoder interfaces and decoding orchestration | Evaluate existing integrations |
| `craft-media-probe` | Media inspection and technical metadata | Evaluate existing probe implementations |
| `craft-media-audio` | Audio buffers, formats, channel layouts, conversion contracts | SoundCraft/FilmCraft |
| `craft-media-video` | Video frames, pixel formats, frame metadata | FilmCraft/EffectCraft |
| `craft-media-container` | Container interfaces and stream packaging | FilmCraft |
| `craft-media-filter` | Reusable media-processing operations | Multiple applications |
| `craft-media-quality` | Objective analysis and output validation | New/evaluate |
| `craft-aac` | AAC implementation | FilmCraft |
| `craft-flac` | FLAC implementation | FilmCraft/SoundCraft |
| `craft-opus` | Opus implementation | EffectCraft |
| `craft-h264` | H.264 implementation | FilmCraft |
| `craft-prores` | ProRes implementation | FilmCraft |
| `craft-isobmff` | MP4/M4A container implementation | FilmCraft |

These names represent **potential architectural boundaries**, not a recommendation to create 15 crates immediately.

The immediate extraction should remain narrow enough to prove that shared media encoding works across two applications.

### What remains application-owned

| Component | Owner |
|---|---|
| Encoding queue UI | MediaCraft |
| Preset browser UI | MediaCraft |
| Film timeline exports | FilmCraft |
| DAW bounce operations | SoundCraft |
| Composition rendering | EffectCraft |
| Image export workflows | PhotoCraft |
| Application project formats | Respective applications |
| Application-specific compatibility behavior | Respective applications |

The distinction is important: `craft-media-enc` should provide encoding capabilities, not become a universal creative-application engine.

---

# 17. Proposed standalone MediaCraft application

If the shared libraries mature, a dedicated media encoder could become a natural additional Craft application.

A useful product architecture would have six primary surfaces.

| Surface | Primary capabilities |
|---|---|
| **Queue** | Sources, outputs, status, priorities, progress |
| **Presets** | Search, organize, create, and apply export configurations |
| **Inspector** | Source streams, codecs, metadata, technical characteristics |
| **Output Settings** | Video, audio, container, captions, color, filters |
| **Analysis** | Loudness, quality metrics, technical validation |
| **Automation** | Watch folders, job templates, integrations, scheduling |

### Differentiating features worth investigating

Beyond parity with existing encoders, I see five promising areas.

**1. Explainable encoding**

Instead of showing an unexplained list of settings, the application explains why a codec, profile, bitrate, or hardware path is available or unavailable.

**2. Verified deliverables**

Users can specify technical requirements, and the application verifies the finished output against them.

**3. Shared Craft export infrastructure**

FilmCraft, SoundCraft, and EffectCraft can submit encoding jobs to a common backend without duplicating codecs and container logic.

**4. Reproducible encoding**

Each job can record source identity, preset version, encoder implementation, toolchain revision, and validation results.

**5. Intelligent output planning**

Given a destination and technical constraints, the application can recommend a compatible encoding configuration, estimate tradeoffs, and explain its decisions.

These are proposed differentiators, not documented capabilities of the existing Craft implementations.

---

# 18. Research conclusions and recommended scope

The complete capability inventory is broad, but the first implementation should be substantially smaller.

| Milestone | Capability target |
|---|---|
| **M0 — Shared contract** | Media types, stream metadata, encoder capabilities, typed errors |
| **M1 — First codec extraction** | AAC encoder isolated from FilmCraft with baseline tests |
| **M2 — First container workflow** | Valid AAC/M4A output with timing and metadata checks |
| **M3 — Cross-application adoption** | FilmCraft behavior preserved; SoundCraft or EffectCraft verified as another consumer |
| **M4 — General encoding** | Additional audio/video codecs, container negotiation, software/hardware selection |
| **M5 — MediaCraft MVP** | Queue, presets, media inspection, batch processing, progress, logs |
| **M6 — Professional workflows** | Proxies, color management, captions, technical validation |
| **M7 — Advanced workflows** | Streaming packages, distributed workers, specialized formats |

### Critical questions for the next research pass

Before writing the GitHub proposal, the source-level investigation should answer:

1. Which codecs are already implemented in FilmCraft, SoundCraft, and EffectCraft?
2. Which implementations are self-contained versus coupled to application-specific models?
3. What existing conformance tests and reference vectors are available?
4. Which codec implementations require third-party dependencies or carry licensing constraints?
5. Which muxers and demuxers already exist?
6. How are timestamps, channel layouts, pixel formats, and color metadata represented?
7. Which hardware acceleration paths are already implemented?
8. What capabilities are documented but not yet empirically verified?
9. Which consumer would provide the smallest meaningful second adoption?
10. What is the minimum shared API that supports that workflow without constraining future codecs?

**Research recommendation:** Treat the full inventory as the long-term capability landscape for a MediaCraft application. Keep the initial `craft-libs` proposal focused on reusable media encoding contracts and a single proven extraction.

The most valuable result would be a **verified source-to-target capability matrix** showing what already exists in the Craft repositories, what can be extracted, what must be built, and what requires additional research.
---

# Appendix B — Integrated application references and UI direction (2026-10-13)

The canonical consolidated product definition is `DEFINITION.md`. The two provided desktop mockups are exploratory designs, not running-application screenshots. The agreed UI model uses Library-first navigation, a mixed-media grid, selected-asset preview and inspector, contextual Process presets, and a collapsible Queue drawer. Collections, Automations and verified Ingest remain first-class flows without permanently occupying every panel.

| Domain Capability | New capability focus | Priority |
|---|---|---|
| Visual Asset Management | Visual browse, smart collections, filesystem-native metadata | P0/P1 |
| Stream Preservation | Keyframe-aware stream-preserving quick operations | P1 |
| Rapid Culling | Fast culling, compare, contact sheets | P1/P2 |
| Offline Cataloging | Offline volume catalog and safe relink | P1/P2 |
| Verified Ingest | Multi-destination verified ingest and checksummed receipts | P2 |
| Rule Automation | Media-aware watch-folder rules and idempotent actions | P2 |
| Review & Versioning | Local version comparison and timecoded notes | P2/P3 |
| Media Intelligence | Optional semantic and transcript discovery | P3 |
| Media Enhancement | Optional, provenance-tracked enhancement | P3 |
| Metadata Interoperability | Deep typed metadata and sidecar interoperability | P1/P2 |

**Release sequence:** M0 shared media contract (independent); M1 library/index; M2 preview/inspect; M3 process/queue; M4 organization; M5 derivatives/contracts; M6 verified ingest/rules; M7 advanced media/QC/review; M8 optional intelligence. The earlier milestones in Appendix A are historical encoder-first research and are superseded for the application roadmap by this sequence.

**ArtCraft style:** engraved octopus on vermilion-red rounded-square icon; reuse Craft theme primitives. Mockup blue highlights are exploratory, not canonical brand tokens. Preserve accessible non-color status indicators.
