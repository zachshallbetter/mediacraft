# MediaCraft
**MediaCraft is a professional desktop application for browsing, organizing, inspecting, converting, processing, and delivering digital media.**

It unifies digital asset management, media inspection, and format transformation into an intelligent, coherent workflow.

The goal is straightforward:

**Open virtually any media file, understand what's inside it, and do something useful with it.**

MediaCraft isn't intended to replace FilmCraft, PhotoCraft, SoundCraft, or EffectCraft. It handles the media operations that surround those creative applications.

---

## 1. What MediaCraft actually does

MediaCraft has six core responsibilities.

| Capability | Description |
|---|---|
| **Browse** | Explore files, folders, drives, and collections with rich media previews. |
| **Organize** | Manage metadata, tags, ratings, collections, duplicates, and related assets. |
| **Inspect** | Understand codecs, streams, resolution, color, audio, metadata, and file integrity. |
| **Convert** | Transform audio, video, images, and image sequences between supported formats. |
| **Process** | Resize, optimize, extract, normalize, create proxies, and prepare professional deliverables. |
| **Automate** | Run batch operations, watch folders, manage queues, and integrate with other applications. |

These aren't six separate applications. They're different capabilities of one media workspace.

### Example

You open a folder containing 200 videos, 150 photographs, and 40 audio recordings.

MediaCraft immediately provides thumbnails, previews, metadata, and technical information for the formats it supports.

You select 30 videos and choose **Create Editing Proxies**.

MediaCraft identifies their codecs, resolutions, frame rates, and timecodes. It determines suitable output settings, generates the proxies, verifies the results, and associates them with their originals.

You can then use those proxies in FilmCraft.

The original files remain untouched.

---

## 2. The defining feature: Universal media transformation

Traditional encoding utilities expose a long list of codecs, containers, and presets.

MediaCraft should offer comparable format breadth over time, but with a better interaction model.

Instead of requiring users to understand the technical details first, MediaCraft starts with what they want to accomplish.

| User intent | MediaCraft operation |
|---|---|
| Make this video smaller | Optimize |
| Convert this MOV to MP4 | Convert |
| Get the audio from this video | Extract |
| Turn these images into a video | Create sequence |
| Convert these RAW photographs to JPEG | Batch convert |
| Make these clips easier to edit | Generate proxies |
| Prepare this video for YouTube | Prepare delivery |
| Check whether this file is valid | Analyze |
| Convert every new file in this folder | Automate |

Advanced users retain full access to codec settings, bitrate, resolution, frame rate, color management, audio configuration, and container options.

The distinction is that **technical complexity is available when needed, rather than mandatory for every operation**.

---

## 3. MediaCraft is also an asset manager

This is what makes integrated digital asset management central to the product.

A traditional encoder sees a file as an input to a job.

MediaCraft sees it as an asset that may have metadata, versions, relationships, and processing history.

For example:

```text
Interview.mov
│
├── Original
│
├── Editing Proxy
│   └── Interview_Proxy.mp4
│
├── Web Delivery
│   └── Interview_Web.mp4
│
├── Audio Extraction
│   └── Interview_Audio.wav
│
└── Thumbnail
    └── Interview_Poster.jpg
```

MediaCraft understands that these files belong together.

It can identify which derivatives are current, how they were produced, and whether they need to be regenerated.

It should also provide rich asset organization: collections, smart collections, ratings, keywords, metadata editing, batch renaming, duplicate detection, and fast search.

**Importantly, MediaCraft is filesystem-native.** It doesn't require users to move their assets into a proprietary library.

An optional index maintains thumbnails, metadata, relationships, and processing history without taking ownership of the underlying files.

---

## 4. MediaCraft understands what's inside a file

A major capability is technical inspection.

For video, this includes codecs, containers, individual streams, frame rates, bit depth, color spaces, HDR information, timecode, subtitles, and audio tracks.

For audio, it includes sample rate, channel layout, encoding format, bitrate, duration, loudness, and other available metadata.

For images, it includes dimensions, color profiles, bit depth, compression, camera metadata, and supported RAW information.

The application should also distinguish between what it knows and what it cannot determine.

For example, an ambiguous color profile shouldn't silently be treated as a confirmed Rec.709 source.

This inspection capability supports browsing, conversion, compatibility checking, and technical validation.

---

## 5. Processing should be intelligent

One of the more interesting technical ideas is a **media transformation planner**.

Consider a user who wants to convert an MKV file into MP4.

If its video and audio streams are already compatible with MP4, MediaCraft may be able to change the container without re-encoding.

That operation is faster and avoids unnecessary quality loss.

If the streams are incompatible, it can determine which ones require transcoding.

The processing model becomes:

**Inspect → Plan → Execute → Verify → Deliver**

The planner evaluates available codecs, compatibility requirements, hardware acceleration, quality constraints, and processing cost.

A future version could accept requirements such as:

> Produce a web-compatible video under 100 MB while preserving as much visual quality as possible.

MediaCraft would select a suitable processing strategy, execute it, and verify whether the resulting file satisfies those requirements.

That is a proposed capability, not something we've established as implemented today.

---

## 6. What the desktop application looks like

The application should align with the existing ArtCraft visual system.

The current identity direction is a **black-and-white engraved octopus on a vermilion-red rounded-square icon**.

The interface itself should follow the established Craft application conventions rather than introducing an unrelated design language.

I would organize it around four primary workspaces:

| Workspace | Purpose |
|---|---|
| **Library** | Browse, search, preview, organize, and manage assets. |
| **Inspect** | Examine technical properties, streams, metadata, and relationships. |
| **Process** | Configure conversions, optimizations, extractions, and other operations. |
| **Queue** | Execute, monitor, retry, and verify processing jobs. |

Collections and automation would be accessible throughout the application.

The library should be the default starting point—not an empty encoding queue.

---

## 7. How MediaCraft fits into ArtCraft

MediaCraft has two distinct roles.

**As an application**, it provides a standalone workspace for people managing and delivering media.

**As part of the ecosystem**, it can become a consumer of reusable media infrastructure that also serves other Craft applications.

For example:

| Application | Potential shared capability |
|---|---|
| FilmCraft | Video encoding, proxies, export packaging |
| SoundCraft | Audio conversion, AAC/M4A delivery, loudness analysis |
| EffectCraft | Render encoding, image sequences, video export |
| PhotoCraft | Batch image conversion, metadata, color handling |
| MediaCraft | General-purpose media browsing, processing, and delivery |

The architecture should distinguish reusable technical mechanisms from application-specific workflows.

That means `craft-libs` could eventually contain qualified media codecs, container handling, inspection, and processing components.

MediaCraft would own its browser, collections, user workflows, and queue presentation.

We should not assume all these shared components already exist or need to become separate crates immediately.

---

## 8. What we're building first

The complete product is ambitious. The initial release should establish a smaller, useful workflow.

**MediaCraft 0.1 should let users browse a folder, inspect supported media, select files, convert them, and verify the results.**

The first release should prioritize:

1. Filesystem browsing with thumbnails and previews.
2. Technical media inspection and metadata.
3. Search, filtering, and basic collections.
4. Practical video, audio, and image conversions.
5. Batch operations and reusable presets.
6. A persistent background processing queue.
7. Non-destructive outputs and processing history.
8. Basic output verification.

Broader codec coverage, advanced asset intelligence, broadcast delivery, distributed processing, semantic search, and sophisticated automation can follow.

The existing MediaCraft lab notes describe work on a transcode daemon, queue state machine, AAC priming compensation, and technical UI experiments. Those findings provide a starting point, but they don't establish that the complete application described here is already implemented. LAB\_NOTES.md

---

## 9. What MediaCraft is not

MediaCraft is not a video editor, digital audio workstation, image editor, compositing application, or cloud asset-management platform.

It should not compete with the creative capabilities of FilmCraft, SoundCraft, PhotoCraft, or EffectCraft.

Its responsibility is everything around those applications: **understanding, organizing, transforming, and delivering media.**

That boundary helps keep the product focused.

---

## 10. Integrated application references and capability decisions

MediaCraft synthesizes capabilities from several media disciplines into a unified architecture. These are **architectural references and proposed capabilities**, not assertions that equivalent functionality exists in MediaCraft today.

| Capability Domain | Core Strength | MediaCraft Integration | Priority |
|---|---|---|---|
| Visual Asset Browsing | Fast visual browsing, tags, smart folders | Dense, responsive asset grid, saved collections, keyboard-first filtering | P0/P1 |
| Filesystem & Metadata | Local volumes, previews, ratings, batch metadata, sidecars | Direct filesystem authority, non-proprietary catalog, IPTC/XMP sync | P0/P1 |
| Encoding & Queue Engine | Format breadth, presets, queues, batch processing | Outcome-first processing, format negotiation, advanced codec controls | P0–P2 |
| Stream Preservation | Fast stream-preserving operations | Remux, extract, compatible joins and keyframe-aware trims without re-encoding | P1 |
| Verified Media Ingest | Multi-destination ingest, hashes, copy verification | Checksum manifests, verified offload, write receipts | P2 |
| Rapid Culling | Full-screen preview, compare, ratings, keyboard workflows | Quick tagging, side-by-side comparison, rejection workflow | P1/P2 |
| Offline Storage Catalogs | Index disconnected volumes, identify storage location | Relink safely, volume tracking, proxy associations | P1/P2 |
| Rule Automation | Watch folders, media-aware predicates, automated recipes | Background folder monitoring, conditional transcode pipelines | P2 |
| Semantic Discovery | Optional transcript and scene search, cross-volume query | Opt-in local models, searchable speech and visual features | P3 |
| Review & Version Compare | Local comparison, timecoded annotations, approval metadata | Side-by-side diff, technical metric comparison | P2/P3 |
| Media Enhancement | Model-based denoise, upscale, interpolation with provenance | Bounded offline utilities with explicit provenance tracking | P3 |
| Deep Metadata Inspection | Typed EXIF/IPTC/XMP read and controlled batch updates | Comprehensive stream and header inspection in Inspector panel | P1/P2 |
| Production Proxy Pipeline | Source/proxy relinking, timecode and metadata preservation | Seamless proxy association with creative editing suites | P1/P2 |

The strongest integration is **fast visual discovery + filesystem-native access + robust transcode processing + stream-preserving quick operations**. Verified ingest and rules follow; semantic discovery and enhancement remain optional.

### Lifecycle model

`Acquire → Browse → Organize → Inspect → Prepare → Transform → Review → Verify → Deliver → Archive`

Each stage must be independently usable. The application should not require an ingest step to browse a folder, a catalog import to convert a file, or a cloud account to run local processing.

### Verified ingest

A camera card or source folder can be copied to one or more destinations, with checksum verification, source preservation, progress, error handling, a human-readable report, and a machine-readable receipt. Proxy generation is a dependent operation that runs **after** source verification. A successful copy operation is not equivalent to a verified ingest.

### Quick operations

Remuxing, stream selection/extraction, supported lossless splitting/joining, and metadata-only operations should avoid re-encoding when technically valid. Frame-exact cuts cannot be promised for arbitrary inter-frame codecs using stream copy; the planner must explain keyframe constraints and offer re-encoding when necessary.

### Local review and version comparison

Compare technical metadata, duration, size, streams, loudness, and optionally visual differences between asset versions. Local timecoded notes and comparison results can be attached to an asset relationship; a hosted approval/collaboration service is not part of the first release.

### Search and optional intelligence

Combine deterministic facets (codec, frame rate, date, tags, location, missing proxy) with optional transcript/scene/similarity indexes. Any inferred label must remain distinguishable from user-entered or embedded source metadata. Local/offline operation remains complete without ML.

---

## 11. Consolidated desktop UI definition

The two supplied UI mockups are **conceptual references**, not implemented screenshots. They agree on a dark, high-information-density media workspace: left sources/collections, central asset grid, selected-asset preview and inspector, contextual processing/presets, and a visible queue. They differ in navigation and how much processing UI is permanently exposed. The merged interaction model is:

| Region | Default behavior | Expanded behavior |
|---|---|---|
| Top toolbar | MediaCraft, Library, Inspect, Process, Queue; global search and view controls | Collections and Automations accessible as destinations, not always separate top tabs |
| Left navigation | Favorites, locations, folders, collections, filter facets | Offline volumes, saved searches, ingest sources, rule-managed folders |
| Center | Mixed-media grid/list with badges, selection, sorting, thumbnail scaling | Compare, contact sheet, waveform, sequence grouping and keyboard culling |
| Preview / inspector | Selected asset preview and tabs for Info, Streams, Metadata, Analysis, History, Related | Timeline scrubbing, filmstrip, waveform, color/HDR details, relationship graph |
| Contextual process sheet | Suggested actions and compact presets for selection | Advanced codec/container, color, audio, subtitle, validation and delivery contract controls |
| Activity / queue drawer | Collapsible job summary with progress, pause, cancel and verification state | Full queue view with dependency graph, retries, logs, resource budget and receipts |

**UI decisions:**

1. **Library first.** The application opens to files and collections, not an empty queue. A selected asset stays visible while a processing action is configured.
2. **Contextual controls.** The first mockup's six-operation grid (Convert, Optimize, Extract, Analyze, Proxy, Image Sequence) is useful as a contextual action chooser, not a permanent full-height form. The second mockup's searchable, categorized preset list belongs inside Process or a temporary side sheet.
3. **Preview is central.** Use the large preview and inspector from both designs, with optional filmstrip and tabs for Streams, Analysis, Color, History and Related.
4. **Queue is always discoverable, not always expanded.** Keep a compact status/activity strip with expandable queue; the dedicated Queue workspace handles complex batches.
5. **Progressive disclosure.** Simple operation cards lead to presets, then to advanced settings; technical experts can directly open the full configuration.
6. **Responsive density.** Panels are resizable/collapsible; narrower windows prioritize browsing and preview over simultaneous four-column layouts.
7. **Visual alignment.** Match ArtCraft/Craft dark workspace typography, separators, iconography, focus/selection states and interaction behavior. MediaCraft's identity is the engraved black-and-white octopus on a vermilion-red rounded-square icon. **Blue is not the permanent MediaCraft brand accent**; any blue selection states in mockups are provisional and should be harmonized with the suite's shared component tokens.
8. **Accessibility.** Full keyboard navigation, clear focus, scalable text, non-color-only job status, accessible progress and reduced-motion behavior.

**Primary screens:** Library (default), Inspect (deep inspection), Process (single/batch setup), Queue (execution), Collections (saved and smart), Automations (rules), and an Ingest flow (verified copy). These are views of one shared asset and job model, not disconnected tools.

---

## 12. Three engines and ownership boundaries

| Engine | Responsibilities | Initial owner |
|---|---|---|
| **Asset Engine** | Filesystem observation, stable identity, thumbnails, metadata, indexing, collections, offline catalog, provenance and derivative relationships | MediaCraft; shared extraction only with proven demand |
| **Media Engine** | Probe, capabilities, stream copy/remux, decode, filter, encode, package, analyze and verify | App orchestration over qualified reusable media primitives |
| **Workflow Engine** | Ingest, durable queue, dependencies, presets, rules, receipts, retries, resource policies, integrations | MediaCraft; selectively reusable contracts |

The application composes these engines. **`craft-libs` is not a destination for the entire application**: only narrowly scoped technical mechanisms with independent tests and demonstrated consumers should be proposed for extraction. The AAC/M4A cross-app contract remains a separate, focused shared-library initiative.

### Source, asset and job records

- **Source file observation:** location, filesystem identity where available, size, timestamps, fingerprints and online/offline state. Path alone is not stable identity.
- **Asset:** stable catalog record with independently sourced metadata; may have multiple file observations and versions.
- **Derivative:** typed source→output edge with recipe/version, source fingerprint, output hash, encoder implementation, timestamp and validation result.
- **Job:** requested outcome, resolved plan, explicit inputs/outputs, status, dependencies, logs, retries and receipt.
- **Delivery contract:** versioned technical constraints with accept/warn/reject results, distinct from convenience presets.
- **Metadata provenance:** embedded, sidecar, user-entered, measured and ML-inferred values remain distinguishable.

---

## 13. Updated release sequence and acceptance criteria

| Milestone | User-visible result | Evidence required |
|---|---|---|
| M0 — Media foundation | Narrow shared media encoding contract where justified | Codec vectors, M4A timing/packaging tests, verified second consumer |
| M1 — Library | Open folders, browse and index mixed media | Incremental scans, cache bounds, file-change handling, unknown formats |
| M2 — Inspect | Preview supported image/audio/video, inspect streams and metadata | Fixtures, latency targets, color/metadata accuracy, graceful unsupported state |
| M3 — Process | Convert, remux/extract where supported, presets, durable queue | Real-file conversions, cancellation/restart, source preservation, output verification |
| M4 — Organize | Collections, tags, smart filters, batch rename | Sidecar interoperability, collision/undo tests, relink and offline fixtures |
| M5 — Relationships | Proxies, multiple outputs, source/derivative history, contracts | Staleness detection, timecode preservation, verified output receipts |
| M6 — Ingest + automate | Verified copy, watch folders, rule-based workflows | Checksum receipts, partially written files, idempotence, dependent jobs |
| M7 — Advanced professional | Expanded codecs, color/HDR, captions, QC, local comparison | Format-specific conformance and quality fixtures |
| M8 — Optional intelligence | Transcript/semantic discovery, similarity, enhancement, remote workers | Privacy and opt-in gates, reproducible models, performance and fallback tests |

**Initial shippable slice:** Open a folder → preview/select → inspect → choose an outcome → review a plan → queue → process → verify → reveal output. It must work offline without a proprietary media import, avoid modifying originals by default, and clearly identify unsupported operations.

---

## 14. Status and non-goals

This document is a **product definition and implementation plan**, not evidence that every described capability exists. The accompanying `LAB_NOTES.md` reports daemon, priming-oracle and UI experiments; those claims need reproducible fixture/commit receipts before being treated as end-to-end validation. In particular, AAC priming delay is implementation-specific, and the observed faststart atom order is not a universal playback guarantee.

MediaCraft is **not** a nonlinear editor, photo editor, DAW, compositing tool, mandatory cloud DAM, social review network, or automatic AI content generator. Editing, collaboration, and enhancement functionality are bounded utilities in service of media preparation and delivery, not replacements for the other Craft applications.

---

## The project definition

**MediaCraft — Universal Media Workspace**

*Browse anything. Convert anything supported. Deliver with confidence.*

MediaCraft is a filesystem-native desktop application that unifies digital asset management, media inspection, format conversion, batch processing, and automated delivery.

It combines professional digital asset organization with high-throughput batch transcoding, using an integrated workflow that understands media properties, preserves originals, tracks derivatives, and verifies outputs.

It is designed as both a standalone professional tool and a consumer of reusable media infrastructure within the ArtCraft ecosystem.

**The central product idea is simple: MediaCraft manages the entire lifecycle of a media asset, from discovery through delivery, without requiring users to leave the application.**