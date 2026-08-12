# FitSend MVP Implementation Plan

Date: 2026-08-12

## Milestone 1: Project foundation

1. Create the Vite + React + TypeScript application configuration.
2. Create the Tauri 2 Rust crate and application configuration.
3. Add the dialog and opener plugins.
4. Add shared TypeScript domain types and built-in destination profiles.
5. Add formatting and constraint calculation unit tests.

Verification:

- `npm test`
- `npm run build`
- `cargo check --manifest-path src-tauri/Cargo.toml`

## Milestone 2: Media analysis and planning

1. Implement Rust request/response types shared by Tauri commands.
2. Implement extension validation and image metadata analysis.
3. Implement FFmpeg/FFprobe discovery and video metadata analysis.
4. Implement image and video plan generation.
5. Add Rust unit tests for target calculations and impossible targets.

Verification:

- `cargo test --manifest-path src-tauri/Cargo.toml`
- Manual analysis of sample JPEG, PNG, and MP4 files.

## Milestone 3: Processing and verification

1. Implement non-overwriting output path generation.
2. Implement direct image compression with bounded quality and dimension search.
3. Implement FFmpeg video processing with conservative bitrate budgeting.
4. Verify actual output bytes and retry video processing when necessary.
5. Remove partial outputs on failure.
6. Add integration tests with generated media fixtures.

Verification:

- Compressed image is below the requested byte limit.
- Compressed video is below the requested byte limit when FFmpeg is present.
- Source hashes remain unchanged.
- Failed jobs leave no partial output.

## Milestone 4: Product interface

1. Build the single-workspace file intake screen.
2. Add profile selection and custom limit input.
3. Add file analysis, plan receipt, and engine status.
4. Add output selection and processing state.
5. Add verified success and actionable error states.
6. Add responsive layout, keyboard focus, reduced motion, and accessible contrast.

Verification:

- Every state remains usable at 1024x700 and 1440x900.
- Keyboard focus reaches every interactive element.
- Start is disabled for invalid or unsupported combinations.
- No technical encoding control appears in the primary path.

## Milestone 5: Release readiness

1. Add README with product promise, supported formats, privacy model, and development steps.
2. Add license and repository hygiene files.
3. Run formatters, tests, frontend production build, Rust checks, and Tauri bundle build.
4. Inspect the built application and fix critical visual or functional defects.
5. Record known limitations without overstating compatibility.

Verification:

- Clean working tree except intentionally uncommitted release artifacts.
- All automated checks pass.
- Windows installer or executable is produced.

