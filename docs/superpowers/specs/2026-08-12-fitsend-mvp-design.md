# FitSend MVP Design

Date: 2026-08-12  
Status: Approved for implementation by the user's instruction to proceed from the agreed product direction without additional step-by-step review.

## Product definition

FitSend is a local-first desktop application that turns a file into the highest-quality copy that satisfies a destination's upload constraints.

Tagline:

> Make your file accepted anywhere.

The MVP is not a general-purpose converter and does not claim that every file can be compressed to an arbitrary size. It focuses on images and videos, where FitSend can make deliberate quality trade-offs, verify the actual output, and explain when a target is unrealistic.

## Target user and job

The first user is someone blocked by a file upload limit: a developer sharing a screen recording in Discord, an office user attaching media to email, or a creator preparing a file for a web upload.

Their job is:

> When a platform rejects my file, I want a compatible copy that fits on the first attempt, without learning codecs or uploading the source to another service.

## Goals

- Process files entirely on the user's computer.
- Support common image and video inputs.
- Let the user select a destination profile or enter a custom byte limit.
- Maximize useful quality while staying below the target.
- Verify the actual exported file rather than trusting an estimate.
- Preserve the source file and write a clearly named copy.
- Explain destructive trade-offs before processing.
- Keep the compression engine independent from the interface so a CLI can reuse it later.

## Non-goals for v0.1

- PDF, Office, archive, or arbitrary binary compression.
- Cloud processing, accounts, sync, payments, or analytics.
- AI-generated settings.
- A large remote profile marketplace.
- Promising visually lossless results for unrealistic targets.
- Bundling or silently installing FFmpeg.

## Considered approaches

### 1. Browser-only application

Fastest distribution and no install, but large videos stress browser memory, encoding support varies, and desktop workflow integration is weak.

### 2. Cross-platform Tauri desktop application — selected

Tauri provides a compact cross-platform shell, native file access, local processing, and a path to Explorer/Finder integration. A React/TypeScript interface can remain independent of the Rust processing engine.

### 3. Separate native Windows and macOS applications

Best operating-system integration, but it doubles the implementation and maintenance burden before product demand is validated.

## MVP experience

The main screen is a single focused workspace rather than a multi-page editor.

1. The user drops or chooses one file.
2. FitSend analyzes type, size, dimensions, duration, codecs, and engine availability.
3. The user selects a destination:
   - Discord: 9.8 MB safe output target.
   - Gmail: 24 MB safe target to leave message-encoding headroom.
   - Web Upload: 5 MB conservative default.
   - Custom: user-entered KB or MB.
4. FitSend presents one recommended plan in plain language.
5. The user starts processing and chooses an output location.
6. FitSend verifies the actual file size and format.
7. The completion state offers Reveal, Copy path, and Start another.

The app never overwrites the source.

## Constraint model

A destination profile is data, not interface logic:

```ts
type DestinationProfile = {
  id: string;
  name: string;
  description: string;
  maxBytes: number;
  safetyMargin: number;
  acceptedKinds: Array<'image' | 'video'>;
  output: {
    imageFormats?: string[];
    videoContainer?: string;
    videoCodec?: string;
    audioCodec?: string;
  };
};
```

Built-in profiles are versioned source files. Custom size creates an in-memory profile. A later release can load community-contributed profiles without changing the compression engine.

## Architecture

### Desktop shell

- Tauri 2
- React and TypeScript
- Vite
- Native file picker through the Tauri dialog plugin

### Domain layer

The TypeScript domain layer owns profile selection, display formatting, plan presentation, and UI state. It communicates with Rust through typed Tauri commands.

### Rust processing engine

The Rust engine is divided into four units:

1. `analyzer`: identifies supported media and returns normalized metadata.
2. `planner`: determines whether the source already fits and derives a conservative output plan.
3. `processor`: compresses images directly and delegates video encoding to the locally installed FFmpeg.
4. `verifier`: re-reads the result, checks byte size and output format, and triggers bounded retries when required.

The interface never constructs FFmpeg arguments. This keeps command safety and encoding behavior in one testable boundary.

## Processing behavior

### Images

- v0.1 inputs: JPEG and PNG.
- If the source already fits, FitSend offers a verified copy without recompression.
- Otherwise the output is JPEG for maximum compatibility.
- PNG transparency is flattened onto a white background with an explicit warning.
- The image encoder searches JPEG quality and, if necessary, progressively reduces dimensions.
- The selected output must remain below the byte limit.
- A minimum quality and minimum dimension guard prevents silently producing unusable output.

### Videos

- v0.1 inputs: MP4, MOV, MKV, and WebM when FFprobe can read them.
- Output is MP4 with H.264 video and AAC audio for broad compatibility.
- The planner reserves container and audio overhead, then derives a video bitrate from duration and target bytes.
- Encoding uses a local FFmpeg executable discovered on `PATH`.
- FitSend verifies the output size and may retry at a lower bitrate a maximum of two times.
- If the calculated video bitrate is below the minimum useful threshold, FitSend refuses to hide the trade-off and recommends trimming, removing audio, or raising the limit.

## Error handling

- Unsupported files receive a clear supported-format list.
- Missing FFmpeg disables video processing but leaves image processing available.
- Corrupt or unreadable inputs fail during analysis before output selection.
- Permission failures identify the file or directory involved.
- Partial output files are removed after a failed job.
- Existing output paths are never overwritten; a numeric suffix is added.
- Processing cancellation is a post-MVP feature; v0.1 prevents duplicate submissions while a job runs.

## Security and privacy

- No file content leaves the device.
- No shell command is assembled as a single string.
- FFmpeg is invoked with a fixed executable and structured argument list.
- File paths are treated as opaque values.
- Output is written only to a user-selected location.
- No analytics, telemetry, or network request is included in v0.1.

## Interface direction

The visual language should feel like a precise shipping instrument rather than a generic converter:

- Warm off-white canvas with deep ink text.
- Electric coral as the action color and cool blue for verified states.
- Large file card and destination chips as the primary controls.
- A compact constraint receipt shows input, target, planned output, and quality impact.
- Technical settings are translated into plain language and kept out of the primary path.
- Motion is limited to file intake, plan calculation, processing progress, and verification success.

## Testing strategy

### Unit tests

- Profile safety-margin calculations.
- Byte and duration formatting.
- Video bitrate planning and infeasible-target detection.
- Output naming without overwrites.
- Image search terminating under the requested byte limit.

### Integration tests

- Analyze sample JPEG, PNG, MP4, and unsupported files.
- Compress images against several size limits and verify actual bytes.
- Run the video pipeline when FFmpeg is available.
- Confirm a failed process leaves no partial output.

### UI tests

- Empty, analyzed, ready, processing, success, and error states.
- Destination selection and custom-unit conversion.
- Start button disabled when constraints are invalid or processing is active.

## Acceptance criteria

The MVP is ready when:

- It builds and launches as a Tauri desktop app on Windows.
- A user can select a JPEG or PNG, choose a target, and export a verified result below the limit.
- A user with FFmpeg installed can select a supported video and export an MP4 below the limit.
- The app clearly reports missing FFmpeg, unsupported input, impossible targets, and processing failures.
- The source file is never modified.
- Unit tests and the production frontend build pass.

## Deferred roadmap

1. Bundled FFmpeg builds and hardware-encoder detection.
2. PDF structural optimization with searchable-text preservation.
3. Destination dimension, aspect-ratio, duration, and filename rules.
4. Explorer/Finder quick actions.
5. Batch processing and saved custom profiles.
6. Community profile registry and CLI.

