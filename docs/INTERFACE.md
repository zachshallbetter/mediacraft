# MediaCraft — UI Screen Map
**Proposed design inventory · v0.1**

Based on the ArtCraft references and our MediaCraft iterations, I would organize the application around **five primary workspaces, six supporting interfaces, and a shared set of dialogs and contextual components**.

The objective is to mock up the entire application systematically, not just produce variations of the encoding queue.

## 1. Application structure

```text
MediaCraft
│
├── 01. Queue
│   ├── Empty / First Launch
│   ├── Populated Queue
│   ├── Active Encoding
│   ├── Batch Operations
│   ├── Completed / Failed Jobs
│   └── Queue History
│
├── 02. Inspect
│   ├── Source Inspector
│   ├── Video / Audio Properties
│   ├── Stream Inspector
│   ├── Source vs. Output Comparison
│   ├── Metadata
│   └── Technical Analysis
│
├── 03. Presets
│   ├── Preset Browser
│   ├── Preset Editor
│   ├── Video Settings
│   ├── Audio Settings
│   ├── Container Settings
│   ├── Filters / Processing
│   └── Preset Comparison
│
├── 04. Automate
│   ├── Watch Folders
│   ├── Watch Folder Rules
│   ├── Workflow Builder
│   ├── Batch Processing
│   ├── Scheduling
│   └── Automation History
│
├── 05. Deliver
│   ├── Output Browser
│   ├── Validation Results
│   ├── Quality Analysis
│   ├── Delivery Destinations
│   └── Export Reports
│
├── Supporting Interfaces
│   ├── Media Browser
│   ├── Job Settings Inspector
│   ├── Encoding Activity
│   ├── Performance Monitor
│   ├── Logs / Diagnostics
│   └── Application Settings
│
└── Dialogs / Overlays
    ├── Add Media
    ├── New Encoding Job
    ├── Output Settings
    ├── Create Preset
    ├── Unsupported Format
    ├── Encoding Error
    ├── Confirmation
    └── Encoding Complete
```

These are proposed UI surfaces, not claims that the underlying functionality is implemented.

---

## 2. Primary workspaces

| ID | Screen | Purpose | Priority |
|---|---|---|---|
| MC-01 | Queue — Empty | First launch, import affordances, getting started | P0 |
| MC-02 | Queue — Populated | Primary encoding workspace | P0 |
| MC-03 | Queue — Encoding | Live progress, job states, controls | P0 |
| MC-04 | Queue — Batch Selection | Multi-job operations and bulk editing | P1 |
| MC-05 | Queue — Completed | Results, failures, retries, output access | P0 |
| MC-06 | Inspect — Source | Preview and technical source information | P0 |
| MC-07 | Inspect — Streams | Video, audio, subtitle, and metadata tracks | P1 |
| MC-08 | Inspect — Comparison | Source/output comparison and playback | P1 |
| MC-09 | Inspect — Analysis | Technical diagnostics and media analysis | P2 |
| MC-10 | Presets — Library | Browse, search, organize, apply presets | P0 |
| MC-11 | Presets — Editor | Detailed encoding configuration | P0 |
| MC-12 | Presets — Comparison | Compare encoding configurations | P2 |
| MC-13 | Automate — Overview | Automation management | P1 |
| MC-14 | Automate — Watch Folders | Folder monitoring and processing rules | P1 |
| MC-15 | Automate — Workflow Builder | Multi-step processing workflows | P2 |
| MC-16 | Automate — Scheduling | Scheduled encoding and execution | P2 |
| MC-17 | Deliver — Outputs | Completed output browser | P1 |
| MC-18 | Deliver — Validation | Technical verification and compliance | P1 |
| MC-19 | Deliver — Quality | Quality metrics and visual comparison | P2 |
| MC-20 | Deliver — Destinations | Local, network, and supported remote delivery | P2 |

**20 primary screens or states.**

Importantly, these do not require 20 independent layouts. Many should share dockable panels and switch their content based on selection or workspace.

## 3. Shared panels

The existing Craft applications consistently use contextual inspectors, dockable panels, tabs, and compact controls.

MediaCraft should establish reusable versions of these components.

| ID | Panel | Contents |
|---|---|---|
| MP-01 | Media Browser | Folders, thumbnails, media collections |
| MP-02 | Queue | Jobs, destinations, progress, status |
| MP-03 | Source Monitor | Source playback, timecode, frame navigation |
| MP-04 | Output Monitor | Encoded preview and comparison |
| MP-05 | Properties | Contextual video/audio/output settings |
| MP-06 | Preset Browser | Hierarchical presets and favorites |
| MP-07 | Metadata | Source and output metadata |
| MP-08 | Streams | Track and stream information |
| MP-09 | Activity | Recent operations and events |
| MP-10 | Logs | Structured diagnostics |
| MP-11 | Performance | CPU/GPU utilization, throughput, estimates |
| MP-12 | Validation | Output checks and verification results |
| MP-13 | Watch Folders | Monitored locations and rules |
| MP-14 | History | Completed jobs and previous sessions |

These panels form the foundation of the interface system.

A user should be able to rearrange compatible panels, save layouts, and restore a workspace.

---

## 4. Important dialogs and interactions

Not every feature warrants a full workspace.

Several deserve carefully designed dialogs.

| ID | Dialog / Interaction | Priority |
|---|---|---|
| MD-01 | Add Media / Import | P0 |
| MD-02 | Create Encoding Job | P0 |
| MD-03 | Output Destination | P0 |
| MD-04 | Advanced Encoding Settings | P0 |
| MD-05 | Create / Save Preset | P0 |
| MD-06 | Encoding Error Details | P0 |
| MD-07 | Unsupported Codec / Container | P0 |
| MD-08 | Queue Completion Summary | P0 |
| MD-09 | Watch Folder Configuration | P1 |
| MD-10 | Batch Edit Settings | P1 |
| MD-11 | Compare Outputs | P2 |
| MD-12 | Application Preferences | P1 |
| MD-13 | Hardware / Encoder Selection | P1 |
| MD-14 | Export Validation Report | P1 |

I would also explicitly mock confirmation dialogs for overwriting files, stopping active jobs, and removing queued items.

These are important parts of the actual workflow.

## 5. States we should not overlook

A complete design needs more than the happy path.

For the Queue workspace alone, we should demonstrate:

| State | What changes |
|---|---|
| Empty | Import instructions and available actions |
| Ready | Jobs waiting to process |
| Encoding | Progress, speed, ETA, pause/cancel controls |
| Paused | Resume controls and retained progress |
| Completed | Output location and validation status |
| Failed | Error explanation and retry |
| Unsupported | Codec/container compatibility explanation |
| Multiple selection | Bulk preset, destination, and removal actions |
| Missing source | Relink or remove |
| Output conflict | Rename, replace, skip, or cancel |
| Validation failed | Distinguish encoding success from verified output |
| Hardware unavailable | Explain fallback or block unsupported settings |

These states should be represented in our mockups because they materially affect the controls, not merely the status labels.

---

# 6. Recommended mockup sequence

I would organize the visual design work into four phases.

### Phase 1 — Core application

This establishes the application's visual system and principal workflow.

| Order | Mockup | Focus |
|---|---|---|
| 01 | Queue — Empty | First-launch experience |
| 02 | Queue — Populated | Default workspace |
| 03 | Queue — Encoding | Active operations |
| 04 | Queue — Completed / Failed | Results and recovery |
| 05 | Job Settings | Contextual inspector |
| 06 | Preset Editor | Full encoding configuration |

**Six mockups.**

We have already explored versions of 02, 03, and 05, so these should be refinements rather than entirely new designs.

### Phase 2 — Media intelligence

This establishes the inspection and validation capabilities that could distinguish MediaCraft from a basic encoder.

| Order | Mockup | Focus |
|---|---|---|
| 07 | Source Inspector | Technical metadata |
| 08 | Stream Inspector | Tracks, codecs, timing |
| 09 | Source / Output Comparison | Visual comparison |
| 10 | Validation Results | Verified deliverables |
| 11 | Quality Analysis | Metrics and diagnostics |

**Five mockups.**

### Phase 3 — Automation

This covers the more advanced batch-processing capabilities.

| Order | Mockup | Focus |
|---|---|---|
| 12 | Watch Folder Manager | Monitored locations |
| 13 | Watch Folder Rules | Automatic presets and destinations |
| 14 | Workflow Builder | Multi-stage processing |
| 15 | Automation History | Runs, failures, recovery |

**Four mockups.**

### Phase 4 — System and polish

| Order | Mockup | Focus |
|---|---|---|
| 16 | Preferences | General settings |
| 17 | Encoder / Hardware Settings | Available encoders and capabilities |
| 18 | Batch Edit | Multi-selection operations |
| 19 | Error / Recovery | Actionable failure states |
| 20 | Workspace Customization | Panel docking and saved layouts |

**Five mockups.**

---

## 7. Suggested workspace navigation

I would refine the primary navigation slightly from our previous iterations.

**Queue · Inspect · Automate · Deliver**

Presets should remain a first-class panel and menu, but they do not necessarily need a dedicated top-level workspace.

This follows the references more closely: FilmCraft organizes its major workflows around activities, while VectorCraft and PhotoCraft use panels for supporting tools and properties.

The workspace relationships would be:

```text
                         MEDIACRAFT
                              │
             ┌────────────────┼────────────────┐
             │                │                │
          INPUT           PROCESS           OUTPUT
             │                │                │
       Media Browser        Queue            Deliver
       Watch Folders       Presets          Validation
       Source Inspect      Settings         Quality
             │                │                │
             └────────── AUTOMATION ───────────┘
                              │
                        Shared Panels
                              │
                     Docking / Workspaces
```

## 8. What I'd prioritize visually

The highest-value screens to design next are:

**MC-01 — Empty Queue:** We haven't established what the application looks like before media is imported.

**MC-11 — Preset Editor:** This is likely the most complex configuration interface and will establish our input, dropdown, tab, slider, and advanced-setting patterns.

**MC-18 — Validation Results:** This could become one of MediaCraft's strongest distinguishing features, especially if it clearly separates a completed encode from a technically verified deliverable.

**MC-14 — Watch Folders:** This establishes the automation interface and the relationship between input folders, processing rules, presets, and destinations.

**MC-08 — Source/Output Comparison:** This provides a distinct visual workspace rather than another variation of the queue.

### Scope

The proposed inventory contains **20 primary screens/states, 14 reusable panels, and 14 dialogs/interactions**.

That does not mean we need 48 separate full-screen mockups. Many can be demonstrated through shared workspaces and contextual states.

**I recommend producing 20 carefully selected mockups, beginning with the empty queue and preset editor.** Those will expose the most important remaining gaps in the application's visual and interaction system.