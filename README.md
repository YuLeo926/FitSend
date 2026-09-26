# FitSend

**Make every file ready to send.**

FitSend 0.3.0 is a local-first Windows desktop utility for making image and video copies fit a real send plan. A plan defines both a byte ceiling and its scope: **Per file** proves every accepted file independently, while **All files together** proves the sum of the accepted files. FitSend never uploads media and never overwrites an original.

## Choose a destination plan

The six built-in rules were reviewed on **2026-08-30**. Provider limits are static information shipped with this release; FitSend does not contact providers or silently update them.

| Built-in rule | FitSend working ceiling | Scope | Source status on 2026-08-30 |
|---|---:|---|---|
| Discord Free — Safe | 9.8 MiB | Per file | Conservative working limit informed by Discord's current account-cap and attachment documentation |
| Discord Nitro Basic | 49 MiB | Per file | Below Discord's published 50 MB attachment limit |
| Discord Nitro | 490 MiB | Per file | Below Discord's published 500 MB attachment limit |
| Gmail personal | 24 MiB | All files together | Below Gmail's documented 25 MB total-attachment limit |
| Outlook internet email | 18 MiB | All files together | Leaves room below the common 20 MB whole-message limit |
| Web upload | 5 MiB | Per file | FitSend's generic conservative default, not a provider guarantee |

**Discord Safe** is intentionally named conservatively: Discord currently documents different free-account values in its account-cap material and attachment FAQ. It is not a claim that every Discord account has one universal 10 MB cap. Use a custom per-file plan when you know the exact limit for your account or server.

The Gmail rule is for personal Gmail's documented total attachments. Workspace administrators can apply different limits. The Outlook rule is for internet email and accounts for the whole message, not just raw attachments; Exchange administrators can configure different limits. Organizational users should use a custom plan based on their actual policy.

Custom plans accept an exact KB or MB ceiling, from 8 KiB through 10 GiB, and either scope. Up to 20 custom plans can be saved locally, reloaded after restart, explicitly replaced, or deleted while the batch is unlocked. Saved plans remain only in the local WebView profile; they are not accounts, synced preferences, or provider rules.

## Choose a quality guardrail

- **Precise fit** uses the most quality that still fits.
- **Balanced** keeps the file looking original and only accepts a worthwhile saving.
- **Smallest acceptable** searches for the smallest result without crossing its visual-quality floor.

You can select or drop several JPG, PNG, MP4, MOV, MKV, and WebM files together. Mixed image/video batches run sequentially to keep memory and CPU use predictable. A damaged, unsupported, cancelled, or quality-floor file is marked as needing attention without invalidating good outputs.

For an all-files-together plan, FitSend allocates the remaining byte budget to waiting files. After each accepted result it uses that file's **actual verified size** to redistribute unused space forward. It never goes back to re-encode an earlier file, and a later saving does not retry an earlier quality-floor failure.

The final receipt distinguishes complete and partial proof:

- Per-file proof says every accepted file is at or below the plan ceiling.
- Aggregate proof says the accepted files' actual combined bytes are at or below the total ceiling.
- Partial proof still verifies the accepted files, excludes failed or cancelled files from the byte sum, and clearly reports how many files need attention. It never implies every selected file is ready.

Once processing begins, the destination and quality rule stay attached to that batch. Clear the batch before choosing another send plan.

## Output and privacy behavior

- Media analysis, compression, and verification happen locally. Nothing is uploaded.
- Originals remain byte-identical and are never overwritten.
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

FitSend is a desktop application. Opening the Vite URL directly in a web browser only previews the interface; browser pages cannot access the native file picker or local media engine.

Images work without FFmpeg. Bundled Windows releases include FFmpeg and FFprobe, so end users do not need a separate FFmpeg installation. Development builds prefer tools bundled beside the application and fall back to `PATH`; if neither is available, FitSend explains that video processing is unavailable.

## Quality checks

```powershell
npm test
npm run build
cargo test -p fitsend-core --manifest-path src-tauri/Cargo.toml
cargo test --all-targets --manifest-path src-tauri/Cargo.toml
cargo clippy --workspace --all-targets --manifest-path src-tauri/Cargo.toml -- -D warnings
npm run test:acceptance
```

The 77-row real-media acceptance report covers strategy, media, corruption, collision, cancellation, cleanup and batch scenarios. Those include image, video, and mixed aggregate totals; source caps; forward redistribution; impossible reserves; quality-floor, corrupt, partial, and cancellation behavior; retained-original verification; and the per-file Discord regression. Reports in `output/acceptance` include structured batch scope, ceiling, allocations, accepted and attention counts, and actual accepted bytes.

## Build Windows packages

```powershell
$env:FITSEND_FFMPEG_SOURCE_DIR = 'C:\path\to\extracted\package'
$env:FITSEND_FFMPEG_SOURCE_ARCHIVE = 'C:\path\to\FitSend-FFmpeg-8.0.1-fitsend1-sources.tar.gz'
npm run bundle:windows
```

First build/download the matching binary and source artifacts using the [pinned FFmpeg recipe](scripts/ffmpeg/README.md). The release build verifies package checksums and matching source provenance, rejects builds marked `--enable-nonfree`, and preserves licenses and build metadata. It creates MSI, NSIS and portable ZIP artifacts, copies the corresponding source archive, and includes all four in `SHA256SUMS.txt`. Every app package includes FFmpeg, FFprobe, the GPL license, third-party notices and revision metadata.

The bundled FFmpeg 8.0.1-fitsend1 build is invoked as separate `ffmpeg.exe` and `ffprobe.exe` programs, under GPL v3 or later. Its [corresponding source bundle](https://github.com/YuLeo926/FitSend/releases/download/v0.3.0/FitSend-FFmpeg-8.0.1-fitsend1-sources.tar.gz) includes exact FFmpeg and dependency archives, checksums, configuration, license notices and rebuild scripts. Publish this bundle beside the app downloads. See `src-tauri/resources/ffmpeg/FITSEND-FFMPEG-NOTICE.txt`. These build records are not legal advice or a patent clearance opinion.

FitSend 0.3.0 installers are unsigned. Windows SmartScreen or antivirus software may warn about an unknown publisher.

## How fitting works

Images are tested across multiple quality and, when appropriate, resolution candidates. Precise searches from quality 100 downward; Balanced favors full resolution and enforces a stricter similarity floor; Smallest searches more aggressively while staying above its visual floor. Transparent pixels are preserved instead of flattened.

Videos use a duration-aware bitrate budget with a safety margin. FitSend performs two-pass encoding, measures the output, and can correct the bitrate. Balanced and Smallest also compare encoded frames against the source before accepting them. Targets that would require unusably low video data are rejected early.
