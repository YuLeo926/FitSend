# FitSend

**Make your file accepted anywhere.**

FitSend is a local-first desktop utility that creates the highest-quality image or video it can under a destination's file-size limit. It measures the final file before calling the job complete.

## What the first release does

- Accepts JPG, PNG, MP4, MOV, MKV, and WebM files.
- Includes safe presets for Discord, email attachments, and web uploads.
- Supports an exact custom KB or MB limit.
- Optimizes images locally and exports JPEG when compression is needed.
- Encodes videos as compatible H.264/AAC MP4 files with FFmpeg.
- Retries video compression when the measured output misses the target.
- Never overwrites the original file or uploads it to a server.

PDF support and batch processing are intentionally deferred until the core single-file workflow is proven.

## Run it locally

Requirements:

- Node.js 22 or later
- Rust stable
- FFmpeg and FFprobe on `PATH` for video support
- Windows WebView2 (normally included with current Windows releases)

```powershell
npm install
npm run tauri dev
```

Images work without FFmpeg. If FFmpeg is unavailable, FitSend explains that video processing is unavailable instead of failing silently.

## Quality checks

```powershell
npm test
npm run build
cd src-tauri
cargo test
```

The media engine is an independent Rust crate, so it can be verified without launching the desktop shell:

```powershell
cd src-tauri
cargo test -p fitsend-core
```

These tests include real PNG-to-JPEG processing and, when FFmpeg is available, a generated H.264/AAC video that must be measured below its requested limit.

## Build a Windows installer

```powershell
npm run tauri build
```

Tauri produces both MSI and NSIS installers under `src-tauri/target/release/bundle`. The release remains unsigned during early development, so Windows may show its standard warning for an unknown publisher.

## How fitting works

Images are tested across multiple resolution and JPEG-quality candidates. The best candidate that actually fits wins. Transparent pixels are placed on white and called out before processing.

Videos use a duration-aware bitrate budget with a safety margin. FitSend performs two-pass encoding, measures the output, and can correct the bitrate up to three times. Targets that would require unusably low video data are rejected early.

The source file is always left untouched. If an output name already exists, FitSend creates a numbered copy.
