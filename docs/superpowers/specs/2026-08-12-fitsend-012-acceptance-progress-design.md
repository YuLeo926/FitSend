# FitSend 0.1.2 Acceptance and Progress Design

Date: 2026-08-12  
Status: Approved by the user's instruction to implement roadmap items 1–3 and test them autonomously.

## Objective

FitSend 0.1.2 turns the working MVP into a beta-ready build by proving its output across a repeatable real-media matrix and making long-running video work understandable and interruptible.

The release must answer three questions:

1. Does FitSend reliably meet real upload limits across representative images and videos?
2. Can a user see meaningful processing progress and an estimated remaining time?
3. Can a user safely cancel without leaving a misleading or partial output?

## Scope

### 1. Generated acceptance matrix

An automated native runner generates at least 24 cases rather than storing large copyrighted fixtures in Git:

- JPEG and PNG inputs, including alpha, landscape, portrait, small, large, and noisy content.
- MP4 inputs covering landscape, portrait, audio, silent, short, longer, already-fitting, and severe-but-feasible limits.
- Unicode and space-containing file names.
- Unsupported and intentionally corrupt input failures.

Every successful case verifies the measured byte ceiling, nonempty output, expected media kind, dimensions, and compatible video codecs. Failure cases verify that no partial output remains. The runner writes a machine-readable JSON report and a concise Markdown report under `output/acceptance/`.

### 2. Real progress and cancellation

The media core exposes a callback-based processing API independent of Tauri. Progress events contain a bounded percentage, current stage, and optional encoded-media time.

- Image progress advances from actual decode, candidate-search, write, and verification work.
- Video progress reads FFmpeg's `-progress pipe:1` output. Two-pass encoding maps measured media time into the overall job percentage.
- A retry resets only the encoding portion and names the attempt in the stage text.
- The callback returns whether processing should continue. Cancellation kills the active FFmpeg child, removes pass logs and partial output, and returns a distinct cancellation error.

The existing synchronous `process` API remains as a wrapper for tests and future noninteractive clients.

### 3. Desktop job bridge

Each desktop processing request has a unique job ID. Tauri stores an atomic cancellation flag for active jobs and emits progress events tagged by job ID. A cancel command only affects the matching active job.

The React UI listens before starting the native call and ignores events from older jobs. It shows:

- A determinate progress bar and numeric percentage.
- A plain-language stage such as “Analyzing frames,” “Encoding pass 1 of 2,” or “Verifying output.”
- Elapsed time and an estimated remaining time after enough progress has been observed.
- A Cancel button that changes to “Cancelling…” until native cleanup completes.

Closing or replacing a file while processing remains disabled. Cancelling returns the user to the ready plan without showing the cancellation as an application error.

### 4. Before/after comparison

The success state shows measurable results, not a heavy media preview:

- Original and output bytes.
- Percentage saved.
- Original and output dimensions.
- Processing duration.
- Number of measured encoding attempts.

Visual side-by-side media preview is deferred because it introduces asset-protocol, playback, and memory work without improving upload-limit verification.

## Architecture

### Media core

- `progress`: event type, cancellation outcome, and callback contract.
- `processor`: reports actual work and checks cancellation at safe boundaries.
- `toolchain`: continues resolving bundled FFmpeg before system tools.
- `acceptance_matrix` example: generates fixtures, invokes the public API, validates results, and writes reports.

### Tauri shell

- `JobRegistry`: `job_id -> Arc<AtomicBool>` guarded by a mutex.
- `process_media`: registers the job, forwards progress through `AppHandle::emit`, and always unregisters it.
- `cancel_process`: sets the atomic flag and returns whether an active job was found.

### React interface

- Adds `ProcessProgress` and `ProcessResult.durationMs` domain fields.
- Keeps progress, elapsed time, ETA, and cancellation state local to the active job.
- Uses one event listener for the app lifetime and filters by job ID.

## Error and cleanup guarantees

- Cancellation is identified by a stable `PROCESS_CANCELLED` error token.
- Partial image/video output is removed on failure or cancellation.
- FFmpeg child processes are killed and waited for before the command returns.
- Two-pass logs are removed after success, error, or cancellation.
- A job is removed from the registry even if the blocking task panics or emits an error.
- Progress is monotonic for a given attempt and never exceeds 100.

## Acceptance criteria

- At least 24 generated acceptance cases complete with a report and zero unexpected failures.
- Real PNG-to-JPEG and MP4-to-H.264/AAC cases finish below their requested ceilings.
- Progress events are monotonic and include actual FFmpeg media-time progress.
- Cancelling a running video stops the child process and leaves no output file.
- The UI shows progress, ETA, cancel state, and before/after metrics without console errors.
- Existing frontend tests, Rust tests, formatting, Clippy, production build, and Windows packaging all pass.

## Deferred

- Batch queue and concurrent jobs.
- Thumbnail or playback comparison.
- Pause/resume.
- Hardware encoder selection.
- PDF support.
