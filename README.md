# FitSend

**Make every file ready to send.**

FitSend is a local-first desktop utility built around one promise: choose where an image or video is going, then get a verified copy under that destination's per-file limit without crossing a conservative visual-quality floor.

## Choose the limit and the strategy

FitSend uses two independent choices:

- **Destination:** Discord, email attachment, web upload, or an exact custom KB/MB ceiling. FitSend treats that destination as a concrete per-file acceptance rule.
- **Quality guardrail:** **Precise fit** uses the most quality that still fits; **Balanced** keeps the file looking original and only accepts a worthwhile saving; **Smallest acceptable** searches for the smallest result without crossing its visual-quality floor.

You can select or drop several JPG, PNG, MP4, MOV, MKV, and WebM files together. Mixed image/video batches run one file at a time to keep memory and CPU use predictable. A damaged or unsupported file is reported on its own row and does not stop later files.

Each row shows whether the file already fits or which verified copy is ready to send. The final summary proves the accepted files are under the selected limit and reports the original total, verified total, bytes saved, created results, unchanged originals, and failures.

Once processing begins, the destination and quality rule stay attached to that batch so its verification claim cannot be changed after the fact. Clear the batch before choosing a different send plan.

## Output and privacy behavior

- Originals are never overwritten and nothing is uploaded.
- New results are saved beside their source with an automatic `.fitsend` name. Existing names get a numbered suffix instead of being replaced.
- Precise fit creates no duplicate when the source already fits.
- Balanced can keep the source when recompression would save too little or miss its quality floor.
- Transparent PNG input remains transparent PNG.
- Video output is compatible H.264/AAC MP4 and is checked for size, duration, dimensions, audio presence, and visual similarity.
- Cancellation removes incomplete outputs, FFmpeg pass logs, and quality-check residue.

## Run it locally

Requirements:

- Node.js 22 or later
- Rust stable
- FFmpeg and FFprobe on `PATH` for unpackaged development video support
- Windows WebView2 (normally included with current Windows releases)

```powershell
npm install
npm run tauri dev
```

FitSend is a desktop application. Opening the Vite URL directly in a web browser only previews the interface; browser pages cannot access the native file picker or the local media engine.

Images work without FFmpeg. Bundled Windows releases include FFmpeg and FFprobe, so end users do not need to install them. Development builds prefer tools bundled beside the application and fall back to `PATH`; if neither is available, FitSend explains that video processing is unavailable instead of failing silently.

## Quality checks

```powershell
npm test
npm run build
npm run test:acceptance
cd src-tauri
cargo test
```

The media engine is an independent Rust crate, so it can be verified without launching the desktop shell:

```powershell
cd src-tauri
cargo test -p fitsend-core
```

The acceptance command generates a strategy/media matrix covering below-limit, slightly-over, far-over, and infeasible targets for all three strategies. It also covers the 1254×1254 regression, transparency, rotated video metadata, audio/no-audio, WebM/MKV inputs, Unicode and spaced paths, corruption, cancellation, output-name collisions, and output-write failure cleanup. Reports include strategy, outcome, quality score, dimensions, sizes, target, and processing time in `output/acceptance`.

## Build a Windows installer

```powershell
npm run bundle:windows
```

The build stages the locally installed FFmpeg distribution, rejects builds marked `--enable-nonfree`, includes its license and precise build metadata, then produces MSI, NSIS, and portable ZIP releases. Set `FITSEND_FFMPEG_SOURCE_DIR` to an extracted distribution root when the tools are not on `PATH`.

The local preview currently uses the GPL v3 Gyan.dev Essentials build because FitSend calls its separate `ffmpeg.exe` and `ffprobe.exe` programs for H.264/AAC processing. Before publishing a download, provide the complete corresponding FFmpeg source for the exact bundled revision at the same download location and review all applicable external-library obligations. See `src-tauri/resources/ffmpeg/FITSEND-FFMPEG-NOTICE.txt`. This is a distribution checkpoint, not legal advice.

The release remains unsigned during early development, so Windows may show its standard warning for an unknown publisher.

## How fitting works

Images are tested across multiple quality and, when appropriate, resolution candidates. Precise searches from quality 100 downward; Balanced favors full resolution and enforces a stricter similarity floor; Smallest searches more aggressively while staying above its visual floor. Transparent pixels are preserved instead of flattened.

Videos use a duration-aware bitrate budget with a safety margin. FitSend performs two-pass encoding, measures the output, and can correct the bitrate. Balanced and Smallest also compare the encoded frames against the source before accepting them. Targets that would require unusably low video data are rejected early.

The source file is always left untouched. If an output name already exists, FitSend creates a numbered copy.
