# FitSend 0.2.0 Strategies and Batch Processing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship FitSend 0.2.0 with Precise Fit, Balanced, and Smallest Acceptable strategies plus sequential multi-file image/video processing.

**Architecture:** Keep Tauri commands file-oriented and reuse the existing progress/cancellation registry. Add strategy-aware planners and isolated image/video quality evaluators in `fitsend-core`, then build a pure TypeScript batch state model and a React queue controller that invokes the existing commands sequentially. Every accepted output is written through a temporary sibling file and verified before atomic publication.

**Tech Stack:** React 19, TypeScript 5.8, Vitest 3, Tauri 2, Rust 2021, `image` 0.25, bundled FFmpeg/FFprobe, PowerShell acceptance and Windows packaging scripts.

## Global Constraints

- Preserve Discord, Email attachment, Web upload, and Custom limit destinations.
- Apply the selected byte ceiling separately to every file; never treat it as an aggregate batch allowance.
- Expose only three non-technical strategies: Precise Fit, Balanced (default), and Smallest Acceptable.
- Balanced image SSIM floor is `0.990`; Smallest Acceptable image SSIM floor is `0.965`.
- Balanced video mean/minimum sampled-frame SSIM floors are `0.985`/`0.970`; Smallest Acceptable video floors are `0.950`/`0.920`.
- Balanced creates an optional optimization for an already-fitting source only when savings are at least 10%.
- Process one file at a time; one file failure does not stop later files.
- Do not overwrite or modify source files; no-change results use the source path and create no duplicate.
- Preserve image transparency, media orientation, video duration, audio presence, and audio sync.
- Continue bundling FFmpeg and FFprobe; no external user installation is allowed.
- Release version is `0.2.0`.

---

## File Structure

### New files

- `src/domain/strategies.ts` — public strategy metadata and lookup.
- `src/domain/batch.ts` — pure batch item types, state transitions, totals, and aggregate progress.
- `src/hooks/useBatchQueue.ts` — Tauri file analysis, sequential execution, progress routing, and cancellation.
- `src/components/FileQueue.tsx` — selected-file and per-item result rows.
- `src/components/StrategyPicker.tsx` — three strategy choices with plain-language copy.
- `src/components/BatchSummary.tsx` — aggregate completion receipt.
- `src-tauri/core/src/quality.rs` — image and video similarity policies and score parsing.
- `src-tauri/core/src/output.rs` — collision-safe names, temporary sibling files, cleanup, and atomic publication.
- `src-tauri/core/src/image_processor.rs` — strategy-aware image candidate generation and transparency-safe encoding.
- `src-tauri/core/src/video_processor.rs` — strategy-aware FFmpeg candidate generation, retry, and similarity measurement.

### Modified files

- `src/domain/types.ts` — strategy, plan/result outcomes, and batch-facing command contracts.
- `src/domain/domain.test.ts` — strategy and output path helper tests.
- `src/App.tsx` — replace single-file state with the batch hook and compose the new components.
- `src/styles.css` — queue, strategy, progress, result, responsive, and focus styles.
- `src-tauri/core/src/domain.rs` — serialized strategy/outcome types and request fields.
- `src-tauri/core/src/analyzer.rs` — read frame rate and display rotation needed by video verification.
- `src-tauri/core/src/planner.rs` — strategy-specific no-change/feasibility summaries.
- `src-tauri/core/src/processor.rs` — orchestration only; delegate to media processors and output transaction.
- `src-tauri/core/src/lib.rs` — export new shared contracts and register modules.
- `src-tauri/core/examples/acceptance_matrix.rs` — strategy/media regression matrix.
- `src-tauri/src/lib.rs` — pass strategy-aware request/result types through existing commands.
- `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/core/Cargo.toml`, `src-tauri/tauri.conf.json` — set version `0.2.0`.
- `README.md` — document the three strategies and multi-file workflow.

---

### Task 1: Add Shared Strategy and Outcome Contracts

**Files:**
- Create: `src/domain/strategies.ts`
- Modify: `src/domain/types.ts`
- Modify: `src/domain/domain.test.ts`
- Modify: `src-tauri/core/src/domain.rs`
- Modify: `src-tauri/core/src/analyzer.rs`
- Modify: `src-tauri/core/src/planner.rs`
- Modify: `src-tauri/core/src/lib.rs`

**Interfaces:**
- Produces Rust `CompressionStrategy::{Precise,Balanced,Smallest}` and `ProcessOutcome::{Created,NoChange}`.
- Produces TypeScript `CompressionStrategy = "precise" | "balanced" | "smallest"` and matching `ProcessOutcome`.
- Extends `MediaAnalysis` with optional `frameRate` and integer `rotationDegrees`.
- Extends `PlanRequest` and `ProcessRequest` with `strategy`.
- Extends `ProcessResult` with `outcome`, `reason`, and optional `qualityScore`.

- [ ] **Step 1: Write failing Rust contract/planner tests**

Add serde round-trip coverage and planner cases:

```rust
fn image_analysis(size_bytes: u64) -> MediaAnalysis {
    MediaAnalysis {
        path: "photo.png".to_string(),
        name: "photo.png".to_string(),
        extension: "png".to_string(),
        kind: MediaKind::Image,
        size_bytes,
        width: 1254,
        height: 1254,
        duration_seconds: None,
        frame_rate: None,
        rotation_degrees: 0,
        video_codec: None,
        audio_codec: None,
        has_audio: false,
        has_alpha: false,
        ffmpeg_available: true,
    }
}

#[test]
fn precise_skips_a_source_that_already_fits() {
    let plan = build(&PlanRequest {
        analysis: image_analysis(900_000),
        target_bytes: 1_000_000,
        strategy: CompressionStrategy::Precise,
    }).unwrap();
    assert!(plan.already_fits);
}

#[test]
fn balanced_still_considers_an_already_fitting_source() {
    let plan = build(&PlanRequest {
        analysis: image_analysis(900_000),
        target_bytes: 1_000_000,
        strategy: CompressionStrategy::Balanced,
    }).unwrap();
    assert!(!plan.already_fits);
}
```

- [ ] **Step 2: Run the focused Rust tests and verify failure**

Run: `cargo test -p fitsend-core planner::tests --manifest-path src-tauri/Cargo.toml`

Expected: compilation fails because `CompressionStrategy` and the `strategy` request field do not exist.

- [ ] **Step 3: Implement the serialized Rust contracts**

Add the exact public enums and fields:

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CompressionStrategy { Precise, Balanced, Smallest }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProcessOutcome { Created, NoChange }

pub struct PlanRequest {
    pub analysis: MediaAnalysis,
    pub target_bytes: u64,
    pub strategy: CompressionStrategy,
}

pub struct ProcessRequest {
    pub analysis: MediaAnalysis,
    pub target_bytes: u64,
    pub output_path: String,
    pub strategy: CompressionStrategy,
}
```

Add `strategy: CompressionStrategy` to `CompressionPlan`, and add these fields to `ProcessResult`:

```rust
pub outcome: ProcessOutcome,
pub reason: String,
pub quality_score: Option<f64>,
```

Add `frame_rate: Option<f64>` and `rotation_degrees: i32` to Rust `MediaAnalysis`, with `frameRate: number | null` and `rotationDegrees: number` in TypeScript. In `analyzer.rs`, parse `avg_frame_rate` fractions such as `30000/1001`, then read rotation from `side_data_list[].rotation` with `tags.rotate` as fallback and normalize it to `0`, `90`, `180`, or `270`. For images, construct the decoder explicitly, read its EXIF orientation, apply the same orientation transform used by the processor, and report display dimensions; image analysis then returns `frame_rate = None` and `rotation_degrees = 0` because pixels are normalized before output.

Update `planner::build` so only Precise returns `already_fits = true` immediately. Balanced and Smallest return a processing plan even when `analysis.size_bytes <= target_bytes`.

- [ ] **Step 4: Add matching TypeScript contracts and strategy metadata**

In `src/domain/types.ts`:

```ts
export type CompressionStrategy = "precise" | "balanced" | "smallest";
export type ProcessOutcome = "created" | "noChange";

export type ProcessResult = {
  outputPath: string;
  outputBytes: number;
  targetBytes: number;
  verified: boolean;
  attempts: number;
  width: number;
  height: number;
  durationMs: number;
  outcome: ProcessOutcome;
  reason: string;
  qualityScore: number | null;
};
```

Create `src/domain/strategies.ts`:

```ts
import type { CompressionStrategy } from "./types";

export const strategies = [
  { id: "precise", name: "Precise fit", description: "Highest quality under the limit" },
  { id: "balanced", name: "Balanced", description: "Smaller with almost no visible change" },
  { id: "smallest", name: "Smallest acceptable", description: "As small as possible above a safe quality floor" },
] as const satisfies ReadonlyArray<{ id: CompressionStrategy; name: string; description: string }>;
```

Extend both request types with `strategy: CompressionStrategy` and the plan type with the same field.

- [ ] **Step 5: Add and run frontend contract tests**

Add assertions that there are exactly three unique strategy IDs and Balanced is the second/public default chosen by the app.

Run: `npm test`

Expected: all frontend tests pass.

- [ ] **Step 6: Run Rust checks and commit**

Run:

```powershell
cargo test -p fitsend-core --manifest-path src-tauri/Cargo.toml
cargo clippy --workspace --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings
```

Commit: `feat: add compression strategy contracts`

---

### Task 2: Add Quality Measurement and Atomic Output Utilities

**Files:**
- Create: `src-tauri/core/src/quality.rs`
- Create: `src-tauri/core/src/output.rs`
- Modify: `src-tauri/core/src/lib.rs`
- Modify: `src-tauri/core/Cargo.toml`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces `image_ssim(reference: &DynamicImage, candidate: &DynamicImage) -> Result<f64, String>`.
- Produces `VideoSimilarity { mean: f64, minimum: f64 }` and `parse_ffmpeg_ssim_stats(&str)`.
- Produces `OutputTransaction::new(requested: &Path)`, `.temporary_path()`, `.publish()`, and automatic cleanup on drop.

- [ ] **Step 1: Write failing quality tests**

Cover identical images, visibly changed images, resized comparison, and FFmpeg stats parsing:

```rust
#[test]
fn identical_images_score_one() {
    let image = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(32, 32, Rgb([80, 120, 160])));
    assert!((image_ssim(&image, &image).unwrap() - 1.0).abs() < 1e-9);
}

#[test]
fn parses_mean_and_minimum_video_scores() {
    let stats = "n:1 All:0.992\nn:2 All:0.971\nn:3 All:0.986\n";
    let score = parse_ffmpeg_ssim_stats(stats).unwrap();
    assert!((score.mean - 0.983).abs() < 0.001);
    assert_eq!(score.minimum, 0.971);
}
```

- [ ] **Step 2: Run quality tests and verify failure**

Run: `cargo test -p fitsend-core quality::tests --manifest-path src-tauri/Cargo.toml`

Expected: compilation fails because `quality` does not exist.

- [ ] **Step 3: Implement image SSIM and video stats parsing**

Use luminance values and 8x8 windows. Resize the candidate to reference dimensions with Lanczos3 for measurement only. Use `C1=(0.01*255)^2` and `C2=(0.03*255)^2`, average all window scores, and clamp the result to `0.0..=1.0`. Parse every `All:<score>` token from FFmpeg's stats file and reject empty or non-finite score sets.

Expose policy constants:

```rust
pub const BALANCED_IMAGE_SSIM: f64 = 0.990;
pub const SMALLEST_IMAGE_SSIM: f64 = 0.965;
pub const BALANCED_VIDEO_MEAN_SSIM: f64 = 0.985;
pub const BALANCED_VIDEO_MIN_SSIM: f64 = 0.970;
pub const SMALLEST_VIDEO_MEAN_SSIM: f64 = 0.950;
pub const SMALLEST_VIDEO_MIN_SSIM: f64 = 0.920;
```

- [ ] **Step 4: Write failing output transaction tests**

Cover collision naming, temporary cleanup on drop, exact-path stale-registry cleanup, publication failure cleanup, and atomic publication:

```rust
#[test]
fn publishes_to_the_next_collision_safe_name() {
    let directory = tempfile::tempdir().unwrap();
    let requested = directory.path().join("photo.fitsend.jpg");
    fs::write(&requested, b"existing").unwrap();
    let mut transaction = OutputTransaction::new(&requested).unwrap();
    fs::write(transaction.temporary_path(), b"new").unwrap();
    let published = transaction.publish().unwrap();
    assert_eq!(published.file_name().unwrap(), "photo.fitsend-2.jpg");
    assert_eq!(fs::read(requested).unwrap(), b"existing");
}
```

- [ ] **Step 5: Implement `OutputTransaction`**

Create a temporary sibling name `<stem>.fitsend-working-<pid>-<nonce>.tmp`, allocate the final collision-safe path once, and use `fs::rename` only after verification. `Drop` removes an unpublished temporary file. Record each exact absolute working path in a JSON registry under the operating-system temporary directory before encoding, and remove its registry entry after publish/drop. Export `cleanup_stale_outputs()` to read only those recorded absolute paths, require the `.fitsend-working-` filename marker, delete surviving files, and clear the registry. Call it once in `src-tauri/src/lib.rs::run` before constructing the Tauri builder.

- [ ] **Step 6: Run checks and commit**

Run:

```powershell
cargo test -p fitsend-core quality::tests --manifest-path src-tauri/Cargo.toml
cargo test -p fitsend-core output::tests --manifest-path src-tauri/Cargo.toml
cargo clippy --workspace --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings
```

Commit: `feat: add media quality and atomic output checks`

---

### Task 3: Implement Strategy-aware Image Processing

**Files:**
- Create: `src-tauri/core/src/image_processor.rs`
- Modify: `src-tauri/core/src/processor.rs`
- Modify: `src-tauri/core/src/planner.rs`
- Modify: `src-tauri/core/src/lib.rs`

**Interfaces:**
- Consumes `CompressionStrategy`, SSIM policy constants, `OutputTransaction`, and `ProcessProgress` callback.
- Produces `ImageDecision::{Created(ImageCandidate),NoChange(String)}`.
- Produces `ImageCandidate { bytes, extension, width, height, quality_score, encoded_quality }`.

- [ ] **Step 1: Write failing image strategy regression tests**

Create deterministic noisy and transparent PNG fixtures. The test module defines:

```rust
fn noisy_rgb_fixture(width: u32, height: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| {
        Rgb([
            ((x * 31 + y * 17) % 256) as u8,
            ((x * 7 + y * 29) % 256) as u8,
            ((x + y * 3) % 256) as u8,
        ])
    }))
}

fn alpha_fixture() -> DynamicImage {
    DynamicImage::ImageRgba8(ImageBuffer::from_fn(256, 256, |x, y| {
        Rgba([x as u8, y as u8, 160, ((x + y) % 256) as u8])
    }))
}
```

Cover:

```rust
#[test]
fn precise_uses_quality_100_when_it_fits() {
    let source = noisy_rgb_fixture(1254, 1254);
    let q100 = encode_jpeg(&source, 100).unwrap();
    let target = q100.len() as u64 + 1024;
    let decision = choose_image_candidate(&source, q100.len() as u64 + 100_000, target, CompressionStrategy::Precise, |_| true).unwrap();
    let ImageDecision::Created(candidate) = decision else { panic!("expected output") };
    assert_eq!(candidate.encoded_quality, Some(100));
    assert_eq!((candidate.width, candidate.height), (1254, 1254));
}

#[test]
fn balanced_keeps_source_when_savings_are_under_ten_percent() {
    let source = noisy_rgb_fixture(320, 240);
    let compact = encode_jpeg(&source, 88).unwrap();
    let decision = choose_image_candidate(&source, compact.len() as u64, compact.len() as u64 + 4096, CompressionStrategy::Balanced, |_| true).unwrap();
    assert!(matches!(decision, ImageDecision::NoChange(_)));
}

#[test]
fn transparent_input_never_becomes_jpeg() {
    let source = alpha_fixture();
    let decision = choose_image_candidate(&source, 500_000, 400_000, CompressionStrategy::Smallest, |_| true).unwrap();
    let ImageDecision::Created(candidate) = decision else { panic!("expected output") };
    assert_eq!(candidate.extension, "png");
}
```

- [ ] **Step 2: Run image tests and verify failure**

Run: `cargo test -p fitsend-core image_processor::tests --manifest-path src-tauri/Cargo.toml`

Expected: compilation fails because `image_processor` and its strategy decisions do not exist.

- [ ] **Step 3: Implement candidate generation**

Use these exact search policies:

```rust
const PRECISE_MIN_JPEG_QUALITY: u8 = 30;
const PRECISE_MIN_LONG_EDGE: u32 = 640;
const BALANCED_QUALITIES: [u8; 9] = [100, 98, 96, 94, 92, 90, 88, 86, 84];
const SMALLEST_QUALITIES: [u8; 15] = [30, 34, 38, 42, 46, 50, 54, 58, 62, 66, 70, 76, 82, 90, 100];
const SCALE_TIERS: [f32; 7] = [1.0, 0.9, 0.8, 0.7, 0.6, 0.5, 0.4];
```

- Precise checks full-resolution quality 100 first, then binary-searches the highest fitting quality. It moves through lower resolution tiers only if quality 30 cannot fit.
- Balanced checks full-resolution candidates only, rejects SSIM below `0.990`, and uses the smallest passing candidate. Apply the 10% savings rule only when the source was already under the destination limit.
- Smallest searches candidates in expected-size order, rejects SSIM below `0.965`, stops before the longest edge falls below `min(source longest edge, 1280)`, and selects the smallest passing result.
- Non-alpha candidates use JPEG 4:4:4. Alpha candidates use PNG with best compression and retain RGBA pixels; PNG strategies vary only permitted resolution tiers.
- Decode through an orientation-aware helper that applies EXIF orientation before candidate generation. Add a rotated JPEG fixture and assert output pixels/dimensions have the same displayed orientation.

- [ ] **Step 4: Integrate decisions into the processor**

Replace `flatten_to_white` and `best_jpeg_candidate` calls with `image_processor::process`. A no-change decision returns:

```rust
ProcessResult {
    output_path: request.analysis.path.clone(),
    output_bytes: request.analysis.size_bytes,
    target_bytes: request.target_bytes,
    verified: request.analysis.size_bytes <= request.target_bytes,
    attempts: 0,
    width: request.analysis.width,
    height: request.analysis.height,
    duration_ms,
    outcome: ProcessOutcome::NoChange,
    reason,
    quality_score: Some(1.0),
}
```

A created decision writes to `OutputTransaction::temporary_path()`, decodes the result, checks bytes, dimensions, alpha, and SSIM, then publishes.

- [ ] **Step 5: Run image, core, and lint checks**

Run:

```powershell
cargo test -p fitsend-core image_processor::tests --manifest-path src-tauri/Cargo.toml
cargo test -p fitsend-core --manifest-path src-tauri/Cargo.toml
cargo clippy --workspace --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings
```

Expected: the 2.1 MB-to-2 MB style fixture selects quality 100 when it fits; alpha tests retain transparency; all existing core tests pass after request fixtures include a strategy.

- [ ] **Step 6: Commit**

Commit: `feat: add precise balanced and smallest image modes`

---

### Task 4: Implement Strategy-aware Video Processing and Verification

**Files:**
- Create: `src-tauri/core/src/video_processor.rs`
- Modify: `src-tauri/core/src/processor.rs`
- Modify: `src-tauri/core/src/planner.rs`
- Modify: `src-tauri/core/src/toolchain.rs`
- Modify: `src-tauri/core/src/lib.rs`

**Interfaces:**
- Consumes strategy, target bytes, media analysis, quality thresholds, output transaction, bundled FFmpeg/FFprobe, and progress callback.
- Produces `VideoDecision::{Created(VideoCandidate),NoChange(String)}`.
- Produces verified H.264/AAC MP4 candidates and `VideoSimilarity` evidence.

- [ ] **Step 1: Write failing video policy and parser tests**

Add pure tests for candidate ordering and floor enforcement. Define `const MB: u64 = 1024 * 1024` and test-local `analysis_1080p()`/`analysis_4k()` builders that return complete `MediaAnalysis` values with 30-second duration, H.264 video, AAC audio, and `ffmpeg_available = true`:

```rust
#[test]
fn balanced_never_changes_dimensions_or_frame_rate() {
    let candidates = video_candidates(&analysis_1080p(), 8 * MB, CompressionStrategy::Balanced).unwrap();
    assert!(candidates.iter().all(|item| item.width == 1920 && item.height == 1080));
    assert!(candidates.iter().all(|item| item.frame_rate.is_none()));
}

#[test]
fn smallest_never_drops_hd_below_720p() {
    let candidates = video_candidates(&analysis_4k(), 8 * MB, CompressionStrategy::Smallest).unwrap();
    assert!(candidates.iter().all(|item| item.width >= 1280 && item.height >= 720));
}
```

- [ ] **Step 2: Run video tests and verify failure**

Run: `cargo test -p fitsend-core video_processor::tests --manifest-path src-tauri/Cargo.toml`

Expected: compilation fails because the video policy module does not exist.

- [ ] **Step 3: Move the current two-pass encoder behind video candidate options**

Represent an encoding explicitly:

```rust
struct VideoCandidatePlan {
    mode: VideoRateMode,
    width: u32,
    height: u32,
    frame_rate: Option<u32>,
    audio_kbps: u64,
}

enum VideoRateMode {
    TwoPassKbps(u64),
    Crf(u8),
}
```

- Precise starts with the calculated two-pass bitrate, measures actual bytes, and allows three corrected retries. Resolution tiers stop at orientation-aware 426x240, frame rate stops at 15 fps, and audio stops at 48 kbps.
- Balanced tests same-resolution/same-frame-rate CRF values `[23, 22, 21, 20, 19, 18]` from smallest expected output toward highest quality, accepting the smallest candidate that passes both SSIM floors and the byte ceiling.
- Smallest tests CRF `[30, 29, 28, 27, 26, 25, 24]` across orientation-preserving resolution tiers down to a 1280 px longest edge for HD sources, accepting the smallest candidate that passes both SSIM floors and the byte ceiling.

- [ ] **Step 4: Implement sampled FFmpeg SSIM verification**

Generate an FFmpeg comparison command with two inputs. Scale the decoded candidate back to source dimensions for measurement only, sample the beginning plus evenly spaced points at `max(duration / 8, 0.25)` seconds, and write per-frame `All:` scores to a unique stats file. Parse mean/minimum scores with `quality::parse_ffmpeg_ssim_stats`, then delete the stats file on success, error, or cancellation.

Reject Balanced unless mean/minimum are at least `0.985`/`0.970`; reject Smallest unless they are at least `0.950`/`0.920`. Precise is selected by highest supported resolution/quality under the byte ceiling and still receives decodability, duration, orientation, and audio checks.

- [ ] **Step 5: Add final video probes before publication**

Analyze the temporary output with the existing analyzer and require:

```rust
output.size_bytes <= request.target_bytes
    && output.video_codec.as_deref() == Some("h264")
    && output.has_audio == request.analysis.has_audio
    && (output.duration_seconds.unwrap_or(0.0) - request.analysis.duration_seconds.unwrap_or(0.0)).abs() <= 0.10
    && (output.frame_rate.unwrap_or(0.0) - expected_frame_rate).abs() <= 0.01
    && output.rotation_degrees == expected_rotation_degrees
```

Also require dimensions to equal the candidate plan and keep orientation. Read the first audio/video stream start times with FFprobe and reject an absolute start-time delta above 0.10 seconds. Publish only after all checks pass.

- [ ] **Step 6: Run real video tests and cancellation checks**

Run:

```powershell
npm run stage:ffmpeg
$env:FITSEND_FFMPEG_PATH = (Resolve-Path 'src-tauri/resources/ffmpeg/ffmpeg.exe')
$env:FITSEND_FFPROBE_PATH = (Resolve-Path 'src-tauri/resources/ffmpeg/ffprobe.exe')
cargo test -p fitsend-core video_processor::tests --manifest-path src-tauri/Cargo.toml -- --nocapture
cargo test -p fitsend-core processor::tests::cancelling_video_processing_stops_before_output --manifest-path src-tauri/Cargo.toml -- --nocapture
cargo clippy --workspace --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings
```

- [ ] **Step 7: Commit**

Commit: `feat: add verified video compression strategies`

---

### Task 5: Build the Pure Batch State Model and Sequential Hook

**Files:**
- Create: `src/domain/batch.ts`
- Create: `src/hooks/useBatchQueue.ts`
- Create: `src/domain/batch.test.ts`
- Modify: `src/domain/types.ts`
- Modify: `src/domain/format.ts`
- Modify: `src/domain/domain.test.ts`

**Interfaces:**
- Produces `BatchItem`, `BatchStatus`, `batchTotals(items)`, and `overallProgress(items)`.
- Produces `useBatchQueue({ targetBytes, strategy })` with `addPaths`, `removeItem`, `clear`, `start`, and `cancel`.
- Consumes Tauri commands `analyze_media`, `build_plan`, `process_media`, and `cancel_process` one item at a time.

- [ ] **Step 1: Write failing reducer/totals tests**

```ts
it("counts mixed terminal states and bytes", () => {
  const totals = batchTotals([
    item({ status: "completed", analysis: media(1000), result: created(600) }),
    item({ status: "noChange", analysis: media(500), result: noChange(500) }),
    item({ status: "failed", analysis: media(800), error: "broken" }),
  ]);
  expect(totals).toMatchObject({ completed: 1, noChange: 1, failed: 1, originalBytes: 2300, sendableBytes: 1100 });
});

it("includes the active item fraction in overall progress", () => {
  expect(overallProgress([terminalItem(), processingItem(50), waitingItem()])).toBeCloseTo(50);
});
```

- [ ] **Step 2: Run frontend tests and verify failure**

Run: `npm test -- src/domain/batch.test.ts`

Expected: test module fails to resolve because `batch.ts` does not exist.

- [ ] **Step 3: Implement pure batch types and calculations**

Use these states:

```ts
export type BatchStatus = "analyzing" | "waiting" | "processing" | "completed" | "noChange" | "failed" | "cancelled";

export type BatchItem = {
  id: string;
  path: string;
  status: BatchStatus;
  analysis: MediaAnalysis | null;
  plan: CompressionPlan | null;
  progress: ProcessProgress | null;
  result: ProcessResult | null;
  error: string | null;
};
```

Deduplicate added paths case-insensitively on Windows while preserving first-selection order. Failed analysis remains a visible failed item.

- [ ] **Step 4: Implement the sequential queue hook**

The execution loop uses a cancellation ref and one active job ID:

```ts
for (const item of runnableItems) {
  if (cancelRequested.current) break;
  const jobId = crypto.randomUUID();
  activeJobId.current = jobId;
  markProcessing(item.id, jobId);
  try {
    const result = await invoke<ProcessResult>("process_media", {
      jobId,
      request: {
        analysis: item.analysis,
        targetBytes,
        outputPath: outputDefaultPath(item.analysis.path, item.plan.outputExtension),
        strategy,
      },
    });
    markResult(item.id, result);
  } catch (reason) {
    markFailureOrCancellation(item.id, String(reason));
  }
}
```

Register one progress listener for the hook lifetime and route events by the active job ID. `cancel()` sets the batch cancellation ref, invokes `cancel_process` for the active job, and marks untouched waiting items cancelled after the active invocation settles.

- [ ] **Step 5: Run tests and build**

Run:

```powershell
npm test
npm run build
```

Expected: reducer tests and existing formatting/profile tests pass; TypeScript accepts the hook contracts.

- [ ] **Step 6: Commit**

Commit: `feat: add sequential multi-file queue`

---

### Task 6: Replace the Single-file UI with the Batch Workflow

**Files:**
- Create: `src/components/FileQueue.tsx`
- Create: `src/components/StrategyPicker.tsx`
- Create: `src/components/BatchSummary.tsx`
- Modify: `src/App.tsx`
- Modify: `src/styles.css`

**Interfaces:**
- Consumes `useBatchQueue`, destinations, strategies, formatting helpers, and opener/dialog plugins.
- Produces one-file and multi-file flows through the same queue UI.

- [ ] **Step 1: Add a browser-safe rendering test fixture**

Extend the existing Vitest domain coverage with presentation helper assertions for:

```ts
expect(primaryActionLabel(1)).toBe("Optimize 1 file");
expect(primaryActionLabel(8)).toBe("Optimize 8 files");
expect(statusLabel("noChange")).toBe("No change needed");
expect(statusLabel("failed")).toBe("Needs attention");
```

- [ ] **Step 2: Implement `StrategyPicker`**

Render a second radio group below Destination with Balanced selected by default. Use the exact public names/descriptions from `strategies.ts`, `aria-checked`, keyboard-focus styles, and disable changes while the queue is running.

- [ ] **Step 3: Implement `FileQueue` and `BatchSummary`**

`FileQueue` renders file kind, truncated name, source facts, per-item state, progress, result bytes/savings, failure reason, Remove before processing, and Show in folder/Copy path after completion. `BatchSummary` renders original total, sendable total, savings, completed/no-change/failed counts, Add more, and Clear.

Do not render a global success state that hides failed rows. The batch is complete when every item is terminal, and mixed success/failure remains visible in one list.

- [ ] **Step 4: Integrate multi-selection and multi-drop in `App.tsx`**

Change the native dialog to:

```ts
const selected = await open({ multiple: true, directory: false, filters: fileFilters });
const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
await queue.addPaths(paths);
```

Pass every path from `event.payload.paths` to `addPaths`. Replace the single `analysis`, `plan`, `result`, and `phase` state with `useBatchQueue`. Keep the browser-preview guard message. Remove the per-file Save dialog because outputs are automatically placed beside each source with collision-safe names.

- [ ] **Step 5: Add focused responsive styling**

Keep the current warm paper/coral/ink visual language. Add styles for `.strategy-options`, `.file-queue`, `.queue-row`, `.queue-status`, `.batch-progress`, and `.batch-summary`. At widths below 860 px, keep settings above the queue; below 540 px, stack row actions and result metrics. Preserve `prefers-reduced-motion` and visible focus outlines.

- [ ] **Step 6: Run frontend validation**

Run:

```powershell
npm test
npm run build
```

Start the desktop development build and manually verify one-file selection, multiple selection, mixed drag/drop, removal, all three strategy selectors, processing locks, cancellation, and mixed result rows.

- [ ] **Step 7: Commit**

Commit: `feat: add batch compression interface`

---

### Task 7: Expand Acceptance Coverage and Ship 0.2.0 Artifacts

**Files:**
- Modify: `src-tauri/core/examples/acceptance_matrix.rs`
- Modify: `scripts/run-acceptance.ps1`
- Modify: `README.md`
- Modify: `package.json`
- Modify: `package-lock.json`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/core/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes all strategy and media processors.
- Produces the final acceptance report and Windows MSI, installer, and portable ZIP for `0.2.0`.

- [ ] **Step 1: Add the mandatory image regression and strategy matrix**

Generate a deterministic 1254x1254 noisy PNG, set the target to 2 MB, and assert Precise chooses the highest full-resolution quality candidate. Record strategy, SSIM, outcome, dimensions, input bytes, output bytes, target bytes, and duration in every report row.

For both images and videos, add below-limit, slightly-over, far-over, and impossible-target cases for each strategy. Include transparent image, rotated metadata, audio/no-audio, corrupt input, cancellation, collision, and an injected output-write failure that proves the temporary file is removed and the source is unchanged.

- [ ] **Step 2: Add batch-state and desktop acceptance coverage**

Use the pure TypeScript tests for queue state combinations. During the packaged desktop check, click the picker, Ctrl-select three generated fixtures, verify all three names appear, then drag two additional mixed-media fixtures and verify five total rows. Process a set containing one corrupt file and confirm later valid rows complete. Cancel during the active video and confirm waiting rows become cancelled and no `.fitsend-working-`, passlog, or SSIM stats files remain. Record these observations in `output/acceptance/desktop-0.2.0.md` beside the generated acceptance report.

- [ ] **Step 3: Run the complete verification suite**

Run:

```powershell
npm test
npm run build
cargo test --workspace --manifest-path src-tauri/Cargo.toml
cargo clippy --workspace --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings
npm run test:acceptance
```

Expected: all frontend, Rust, lint, build, real-image, real-video, cancellation, and acceptance matrix checks pass.

- [ ] **Step 4: Update release version and documentation**

Set `0.2.0` in both npm files, both Cargo manifests/lock packages, `tauri.conf.json`, and the app footer. Update README with the two-axis Destination + Strategy model, multi-file selection, automatic output names, no-change behavior, local-only processing, and bundled FFmpeg statement.

- [ ] **Step 5: Build and inspect Windows artifacts**

Run: `npm run bundle:windows`

Verify these exact outputs exist and are non-empty:

```text
release/FitSend_0.2.0_x64_en-US.msi
release/FitSend_0.2.0_x64-setup.exe
release/FitSend_0.2.0_portable.zip
```

Install or launch the packaged desktop build and re-run multi-select, mixed media, Precise, Balanced, Smallest, cancel, Show in folder, and original-file-integrity checks.

- [ ] **Step 6: Commit**

Commit: `release: prepare FitSend 0.2.0`

---

## Final Verification Checklist

- [ ] The source files are byte-for-byte unchanged after every success, failure, and cancellation test.
- [ ] Every created result is verified at or below its per-file target.
- [ ] Precise does not create a duplicate when the source already fits.
- [ ] Balanced honors quality floors and the conditional 10% threshold.
- [ ] Smallest chooses the smallest candidate above its quality floor.
- [ ] Transparent input keeps transparency.
- [ ] Video keeps duration, orientation, and audio presence.
- [ ] Queue progress is monotonic and mixed failures do not stop later files.
- [ ] No temporary or FFmpeg passlog/stat files remain.
- [ ] Native picker and drag/drop accept multiple files.
- [ ] The browser preview shows the desktop-only guard instead of a dead picker.
- [ ] The packaged app works without system FFmpeg.
- [ ] MSI, installer, and portable ZIP are present for version `0.2.0`.
