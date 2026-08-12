# FitSend 0.1.2 Acceptance and Progress Implementation Plan

1. Add public progress types and a callback-based processing entry point to `fitsend-core`, preserving the existing API as a wrapper.
2. Instrument image candidate search and two-pass FFmpeg execution with monotonic progress; implement cooperative cancellation and cleanup tests.
3. Add a Tauri job registry, progress event emission, and a cancellation command keyed by job ID.
4. Extend the frontend domain and processing state with job IDs, progress, elapsed time, ETA, cancelling, and comparison metrics.
5. Create a generated acceptance-matrix executable covering at least 24 image, video, compatibility, path, and failure cases; emit JSON and Markdown reports.
6. Add unit and integration coverage for progress monotonicity, cancellation cleanup, job isolation, and result calculations.
7. Run frontend tests/build, Rust workspace tests, Clippy, acceptance matrix, browser UI QA, Windows bundle build, and packaged-app smoke tests.
8. Version the verified release as 0.1.2, document its behavior, and commit the checkpoint.
