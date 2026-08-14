# FitSend 0.2.0 Compression Strategies and Batch Processing Design

**Status:** Approved in conversation on 2026-08-14
**Scope:** Add three compression strategies and multi-file processing while preserving the existing destination presets and local-only workflow.

## Goals

- Keep the existing Discord, email, web upload, and custom size destinations.
- Add a separate compression strategy choice: Precise Fit, Balanced, or Smallest Acceptable.
- Let users select or drop multiple images and videos in one batch.
- Apply one destination and one strategy to the whole batch, with the size ceiling enforced separately for every file.
- Prevent coarse quality selection from over-compressing files that only slightly exceed a limit.
- Preserve source files and verify every accepted output before reporting success.
- Keep the product non-technical: no codec controls, quality sliders, or external FFmpeg installation.

## Non-goals

- Per-file destinations or strategies within one batch.
- Parallel video encoding.
- A user-selectable output folder.
- Advanced codec, bitrate, resolution, or quality controls.
- Combining a batch into a ZIP or enforcing one aggregate size across the batch.
- Cloud processing or uploads.

## Product Model

Destination and compression strategy are independent choices.

### Destination

The existing destination options remain unchanged:

- Discord
- Email attachment
- Web upload
- Custom limit

The selected destination supplies a maximum byte count for each file. A custom limit never means a combined allowance for the batch. For example, a 20 MB custom limit means every output must be at or below 20 MB.

### Compression Strategy

The UI labels are:

- **Precise Fit**
- **Balanced** (default)
- **Smallest Acceptable**

The strategy changes how FitSend searches for an output; it does not change the destination ceiling.

## User Experience

### File selection

- The native file picker allows multiple selection.
- Drag and drop accepts multiple files, including a mixed image/video selection.
- After selection, the upload area becomes a file list with file count and total input size.
- Before processing, users may add more files, remove individual files, or clear the list.
- All files share the selected destination and strategy.
- The primary action includes the count, for example, `Optimize 8 files`.

### Batch progress

- Files have the states `Waiting`, `Processing`, `Completed`, `No change needed`, `Failed`, and `Cancelled`.
- FitSend processes one file at a time to bound CPU and memory usage.
- Overall progress is `(terminal items + current file progress) / total items`; failed and no-change items count as terminal.
- While processing, list editing and setting changes are disabled.
- Cancelling the batch stops the active encoder, removes its temporary output, and marks all remaining items as cancelled.
- A failure in one file does not stop the rest of the queue.

### Results

- The completion summary shows total original size, total accepted output size, total savings, completed count, no-change count, and failure count.
- Every row shows original size, output size, savings, and the relevant action or error.
- Accepted outputs are written beside their source files.
- Output names use `<stem>.fitsend.<extension>`; collisions use `<stem>.fitsend-2.<extension>`, then increment further.
- FitSend never overwrites or modifies a source file.
- A no-change result keeps the source as the sendable file and does not create a duplicate.

## Strategy Semantics

### Precise Fit

Purpose: produce the highest-quality result that satisfies the selected destination limit.

- If the source is already at or below the limit, return `No change needed` without re-encoding.
- For images, search the full-resolution quality range from highest to lowest with fine-grained refinement. Only move to a smaller resolution tier when no full-resolution candidate fits.
- Candidate ranking is resolution first, then measured visual quality, then encoded quality setting. Output size is only a constraint; FitSend must not lower quality merely to make the result numerically close to the ceiling.
- For video, derive a target bitrate from duration and audio allowance, encode, measure the actual result, and refine until it is within the limit or the safe search space is exhausted.
- The safe search space has an absolute product floor. A lossy image candidate does not go below a JPEG-equivalent quality of 30 or a longest edge of `min(source longest edge, 640 px)`. A landscape video candidate does not go below 426x240 (240x426 for portrait), 15 fps, or 48 kbps audio when audio is present. If a target requires crossing those limits, Precise Fit fails clearly.
- The accepted file may be materially below the ceiling when the highest-quality encoding naturally lands there. FitSend never pads or enlarges an output to approach the limit.
- If no supported candidate can satisfy the limit, return a clear failure instead of claiming the file is ready to send.

This strategy corrects the current coarse JPEG behavior. The 2.1 MB PNG fixture targeting 2 MB must test quality levels above 92 and choose the highest admissible full-resolution candidate. If quality 100 fits, quality 92 is not an acceptable result even though both are below the limit.

### Balanced

Purpose: make a worthwhile reduction while keeping the result visually very close to the source.

- Balanced runs whether or not the source is already under the destination limit.
- It preserves the original dimensions and frame rate. If that prevents the file from meeting the limit within the Balanced quality floor, it fails and suggests Precise Fit or Smallest Acceptable instead of silently resizing.
- An image candidate must meet the balanced visual-similarity policy; the initial acceptance threshold is full-image SSIM of at least `0.990`.
- A video candidate must stay within the conservative balanced codec policy and pass sampled-frame similarity checks. Sampling includes the beginning, end, and evenly spaced positions through the duration so a single easy frame cannot determine acceptance. Initial video thresholds are mean sampled-frame SSIM of at least `0.985`, with no sampled frame below `0.970`.
- Every accepted result must satisfy the destination limit. When the source already satisfies that limit, a new output is accepted only if it saves at least 10% versus the source.
- If an already-fitting source has no candidate that saves at least 10%, return `No worthwhile reduction` and keep the source. When the source is over the limit, accept any candidate that meets both the limit and the Balanced quality floor; the 10% rule does not block a required fit.
- If meeting the destination limit would require crossing the balanced quality floor, report that Balanced cannot safely satisfy the limit and suggest Precise Fit or Smallest Acceptable.

Quality thresholds are internal policy constants, not user controls. They may be made more conservative without changing the public strategy contract, but they must not be relaxed below the documented release thresholds without a new design decision and regression coverage.

### Smallest Acceptable

Purpose: produce the smallest result that remains above FitSend's automatic visible-quality floor.

- It searches all candidates that pass the smallest-acceptable quality policy and chooses the smallest one, rather than the first one below the destination ceiling.
- The initial image similarity floor is full-image SSIM of at least `0.965`.
- It may reduce image or video dimensions in conservative tiers. It never upscales a small source and never reduces the longest edge below `min(source longest edge, 1280 px)`, which corresponds to 720p for ordinary 16:9 media.
- Video may use a more aggressive codec quality setting and lower resolution than Balanced, but must preserve duration, audio sync, and orientation. Initial video thresholds are mean sampled-frame SSIM of at least `0.950`, with no sampled frame below `0.920`.
- When dimensions differ, quality evaluation scales the decoded candidate back to the source dimensions with the same fixed high-quality comparison scaler before calculating SSIM. This comparison scaling is measurement-only and does not alter the output.
- If the smallest acceptable candidate is still over the destination limit, report `Cannot fit within the limit without unacceptable quality` and do not publish an over-limit output.
- If no candidate is smaller than the source, return a no-change result.

### Format preservation

- Orientation is normalized correctly for display and is not accidentally rotated.
- Images with transparency retain transparency. FitSend must not silently flatten an alpha channel to JPEG.
- A non-transparent image may change to a broadly compatible output format only when the candidate passes the active strategy's quality policy and is smaller.
- Video outputs remain broadly compatible and preserve audio unless the input has no audio.

## Architecture

The feature is divided into small units with explicit responsibilities.

### Frontend file collection

Owns the selected file list, additions/removals, batch summary, and row rendering. It does not calculate compression candidates.

### Batch queue

Owns sequential scheduling, aggregate progress, cancellation, and the transition of each item through its states. A queue item contains a stable ID, source path, analysis, target bytes, strategy, progress, and optional result/error.

### Strategy planner

Accepts media analysis, target bytes, and a `CompressionStrategy` enum. It produces ordered candidates or a video encoding budget without knowing about UI state or batch scheduling. Image and video planning remain separate behind the same strategy contract.

### Quality evaluator

Evaluates image similarity and sampled video frames against the active strategy's policy. It returns structured pass/fail evidence for the result verifier and tests. These measurements remain internal; the UI translates them into plain-language explanations.

### Media processor

Executes one candidate into a temporary path and exposes progress and cancellation through the existing job mechanism. Bundled FFmpeg remains the video runtime, so users do not install FFmpeg separately.

### Result verifier

Checks actual byte size, decodability/playability, dimensions, duration, audio presence/sync metadata, orientation, transparency requirements, and strategy quality evidence. Only verified files are atomically moved to the final output path.

## Processing Flow

1. Collect and validate all selected paths.
2. Analyze each file and create a queue item. Unsupported or unreadable files become failed rows without blocking valid items.
3. Resolve the selected destination to target bytes separately for every item.
4. For the next waiting item, ask the strategy planner for candidates.
5. Encode a candidate to a temporary path and report progress.
6. Measure quality and verify the real output.
7. Accept the candidate, try the next candidate, or report a strategy-specific failure.
8. Atomically move accepted output to a collision-safe final path.
9. Continue until all items are terminal, then show the batch summary.

## Error Handling

- Unsupported, missing, unreadable, or corrupt input: fail only that row with a plain-language reason.
- Insufficient disk space: remove the temporary file, fail that row, and continue when safe.
- Encoder failure: include a concise user-facing reason and retain diagnostic detail for logs.
- Output exceeds the ceiling: reject it and continue the strategy search; never mark it verified.
- Output violates the quality floor: reject it and continue searching; if none pass, return the strategy-specific quality failure.
- Cancellation: terminate the active process, remove its temporary file, and never publish a partial output.
- Name collision: allocate the next numbered output name without overwriting anything.
- Application restart or crash: temporary files use a recognizable FitSend suffix and are cleaned on the next startup when they are not associated with an active job.

## Testing and Acceptance

### Unit and component coverage

- Destination and custom-limit conversion remains correct and is applied per item.
- Strategy planner ordering and stop conditions are deterministic.
- Precise Fit searches qualities above the old 92 ceiling and ranks the highest-quality fitting candidate first.
- Balanced enforces its similarity and 10% meaningful-savings thresholds.
- Smallest Acceptable selects the smallest passing candidate and refuses candidates below its quality floor.
- Output naming is collision-safe.
- Queue state transitions, aggregate progress, failure continuation, and cancellation are covered independently.

### Media integration matrix

Cover all three strategies for both images and videos, including:

- Source already below the limit.
- Source slightly above the limit.
- Source far above the limit.
- Target too small for the strategy's quality floor.
- Transparent image.
- Rotated image/video metadata.
- Video with audio and video without audio.
- Corrupt or unsupported input.

The known 2.1 MB PNG to 2 MB regression fixture is mandatory. At original dimensions, the chosen output must be the highest-quality candidate that fits; a coarse quality-92 result is a failure when a higher-quality candidate fits.

### Batch integration matrix

- Multiple images.
- Multiple videos.
- Mixed images and videos.
- Files from different source directories.
- One invalid file among valid files.
- Existing output-name collisions.
- Cancellation during image and video processing.
- Simulated disk-space or output-write failure.
- Correct totals when the batch contains completed, no-change, failed, and cancelled items.

### Desktop acceptance

- Native picker genuinely allows multi-selection.
- Multi-file drag and drop works in the packaged desktop application.
- Adding and removing files before processing updates counts and totals.
- The UI remains responsive during encoding.
- All verified outputs are at or below their per-file ceiling and open successfully.
- Original files remain byte-for-byte untouched.
- The packaged app processes video on a clean machine without a separately installed FFmpeg.

## Release

This is a user-visible workflow and processing-policy change and will ship as FitSend `0.2.0`. Existing single-file use remains supported as a one-item batch.
