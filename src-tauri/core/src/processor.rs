use std::{
    fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use image::ImageReader;

use crate::{
    domain::{MediaKind, ProcessOutcome, ProcessRequest, ProcessResult},
    image_processor::{choose_image_candidate, ImageDecision},
    output::OutputTransaction,
    planner,
    progress::{report, ProcessProgress},
    toolchain,
};

pub fn process(request: &ProcessRequest) -> Result<ProcessResult, String> {
    process_with_progress(request, |_| true)
}

pub fn process_with_progress<F>(
    request: &ProcessRequest,
    mut callback: F,
) -> Result<ProcessResult, String>
where
    F: FnMut(ProcessProgress) -> bool,
{
    let started_at = Instant::now();
    report(&mut callback, 1, "Preparing the fit plan", None, 1)?;
    let plan = planner::build(&crate::domain::PlanRequest {
        analysis: request.analysis.clone(),
        target_bytes: request.target_bytes,
        strategy: request.strategy,
    })?;
    if !plan.feasible {
        return Err(plan
            .warnings
            .first()
            .cloned()
            .unwrap_or_else(|| "This target is not feasible for the selected file.".to_string()));
    }

    if plan.already_fits {
        report(&mut callback, 20, "Checking the original", None, 1)?;
        let metadata = fs::metadata(&request.analysis.path)
            .map_err(|error| format!("FitSend could not verify the source: {error}"))?;
        if metadata.len() > request.target_bytes {
            return Err(
                "The source file changed after it was analyzed and no longer fits the target. Analyze it again."
                    .to_string(),
            );
        }
        report(&mut callback, 100, "Original is ready", None, 1)?;
        return Ok(ProcessResult {
            output_path: request.analysis.path.clone(),
            output_bytes: metadata.len(),
            target_bytes: request.target_bytes,
            verified: true,
            attempts: 0,
            width: request.analysis.width,
            height: request.analysis.height,
            duration_ms: elapsed_millis(started_at),
            outcome: crate::domain::ProcessOutcome::NoChange,
            reason: "The original already fits, so FitSend left it untouched.".to_string(),
            quality_score: Some(1.0),
        });
    }

    let transaction = OutputTransaction::new(Path::new(&request.output_path))?;
    let output_path = transaction.temporary_path().to_path_buf();

    let result = match request.analysis.kind {
        MediaKind::Image => process_image(request, &output_path, &mut callback),
        MediaKind::Video => process_video(request, &output_path, &mut callback),
    };

    let mut completed = result?;
    if completed.outcome == ProcessOutcome::NoChange {
        completed.duration_ms = elapsed_millis(started_at);
        return Ok(completed);
    }
    let published_path = transaction.publish()?;
    completed.output_path = published_path.to_string_lossy().to_string();
    completed.duration_ms = elapsed_millis(started_at);
    Ok(completed)
}

fn process_image(
    request: &ProcessRequest,
    output_path: &Path,
    callback: &mut dyn FnMut(ProcessProgress) -> bool,
) -> Result<ProcessResult, String> {
    report(callback, 5, "Decoding image", None, 1)?;
    let decoded = ImageReader::open(&request.analysis.path)
        .map_err(|error| format!("FitSend could not open this image: {error}"))?
        .with_guessed_format()
        .map_err(|error| format!("FitSend could not identify this image: {error}"))?
        .decode()
        .map_err(|error| format!("FitSend could not decode this image: {error}"))?;
    report(callback, 14, "Preparing image pixels", None, 1)?;
    let decision = choose_image_candidate(
        &decoded,
        request.analysis.size_bytes,
        request.target_bytes,
        request.strategy,
        callback,
    )?;
    let candidate = match decision {
        ImageDecision::Created(candidate) => candidate,
        ImageDecision::NoChange(reason) => {
            report(callback, 100, "Original is already the best choice", None, 1)?;
            return Ok(ProcessResult {
                output_path: request.analysis.path.clone(),
                output_bytes: request.analysis.size_bytes,
                target_bytes: request.target_bytes,
                verified: request.analysis.size_bytes <= request.target_bytes,
                attempts: 0,
                width: request.analysis.width,
                height: request.analysis.height,
                duration_ms: 0,
                outcome: ProcessOutcome::NoChange,
                reason,
                quality_score: Some(1.0),
            });
        }
    };
    report(callback, 92, "Writing image output", None, 1)?;
    fs::write(output_path, &candidate.bytes)
        .map_err(|error| format!("FitSend could not write the image output: {error}"))?;
    let output_bytes = fs::metadata(output_path)
        .map_err(|error| format!("FitSend could not verify the image output: {error}"))?
        .len();
    if output_bytes > request.target_bytes {
        return Err(format!(
            "The image output measured {output_bytes} bytes, above the {} byte target.",
            request.target_bytes
        ));
    }
    let verified = ImageReader::open(output_path)
        .map_err(|error| format!("FitSend could not reopen the image output: {error}"))?
        .with_guessed_format()
        .map_err(|error| format!("FitSend could not identify the image output: {error}"))?
        .decode()
        .map_err(|error| format!("FitSend could not decode the image output: {error}"))?;
    if request.analysis.has_alpha && !verified.color().has_alpha() {
        return Err("FitSend rejected an output that lost image transparency.".to_string());
    }
    report(callback, 100, "Verified and ready", None, 1)?;

    Ok(ProcessResult {
        output_path: output_path.to_string_lossy().to_string(),
        output_bytes,
        target_bytes: request.target_bytes,
        verified: true,
        attempts: 1,
        width: candidate.width,
        height: candidate.height,
        duration_ms: 0,
        outcome: ProcessOutcome::Created,
        reason: format!(
            "Created a verified {}{} at {:.3} visual similarity.",
            candidate.extension.to_uppercase(),
            candidate
                .encoded_quality
                .map(|quality| format!(" quality {quality}"))
                .unwrap_or_default(),
            candidate.quality_score
        ),
        quality_score: Some(candidate.quality_score),
    })
}

fn process_video(
    request: &ProcessRequest,
    output_path: &Path,
    callback: &mut dyn FnMut(ProcessProgress) -> bool,
) -> Result<ProcessResult, String> {
    let plan = planner::build(&crate::domain::PlanRequest {
        analysis: request.analysis.clone(),
        target_bytes: request.target_bytes,
        strategy: request.strategy,
    })?;
    let mut bitrate = plan
        .video_bitrate_kbps
        .ok_or_else(|| "FitSend could not calculate a video bitrate.".to_string())?;
    let audio_bitrate = plan.audio_bitrate_kbps.unwrap_or(0);

    for attempt in 1..=3 {
        if output_path.exists() {
            let _ = fs::remove_file(output_path);
        }
        let encoding = VideoEncodingOptions {
            video_bitrate_kbps: bitrate,
            audio_bitrate_kbps: audio_bitrate,
            has_audio: request.analysis.has_audio,
            source_width: request.analysis.width,
            output_width: plan.width,
            duration_seconds: request.analysis.duration_seconds.unwrap_or(1.0),
        };
        encode_video_passes(
            &request.analysis.path,
            output_path,
            &encoding,
            attempt,
            callback,
        )?;

        report(callback, 94, "Measuring encoded output", None, attempt)?;
        let output_bytes = fs::metadata(output_path)
            .map_err(|error| format!("FitSend could not verify the video output: {error}"))?
            .len();
        if output_bytes <= request.target_bytes {
            report(callback, 100, "Verified and ready", None, attempt)?;
            return Ok(ProcessResult {
                output_path: output_path.to_string_lossy().to_string(),
                output_bytes,
                target_bytes: request.target_bytes,
                verified: true,
                attempts: attempt,
                width: plan.width,
                height: plan.height,
                duration_ms: 0,
                outcome: crate::domain::ProcessOutcome::Created,
                reason: "Created and verified under the selected limit.".to_string(),
                quality_score: None,
            });
        }

        report(
            callback,
            3,
            format!("Refining bitrate for attempt {}", attempt + 1),
            None,
            attempt + 1,
        )?;
        let correction = request.target_bytes as f64 / output_bytes as f64;
        bitrate = ((bitrate as f64 * correction * 0.95).floor() as u64).max(180);
    }

    Err("The encoded video remained above the target after three measured attempts. Try a larger limit or a shorter clip."
        .to_string())
}

struct VideoEncodingOptions {
    video_bitrate_kbps: u64,
    audio_bitrate_kbps: u64,
    has_audio: bool,
    source_width: u32,
    output_width: u32,
    duration_seconds: f64,
}

fn encode_video_passes(
    input: &str,
    output_path: &Path,
    encoding: &VideoEncodingOptions,
    attempt: u8,
    callback: &mut dyn FnMut(ProcessProgress) -> bool,
) -> Result<(), String> {
    let passlog = temporary_passlog();
    let null_output = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let bitrate = format!("{}k", encoding.video_bitrate_kbps);
    let audio_bitrate = format!("{}k", encoding.audio_bitrate_kbps);
    let passlog_value = passlog.to_string_lossy().to_string();

    let mut first = toolchain::command("ffmpeg");
    first.args(["-hide_banner", "-loglevel", "error", "-y", "-i"]);
    first.arg(input);
    first.args([
        "-map", "0:v:0", "-c:v", "libx264", "-preset", "medium", "-b:v", &bitrate, "-pix_fmt",
        "yuv420p",
    ]);
    if encoding.output_width > 0 && encoding.source_width > encoding.output_width {
        first.args(["-vf", &format!("scale={}:-2", encoding.output_width)]);
    }
    first.args([
        "-pass",
        "1",
        "-passlogfile",
        &passlog_value,
        "-an",
        "-f",
        "null",
    ]);
    first.args(["-progress", "pipe:1", "-nostats"]);
    first.arg(null_output);
    if let Err(error) = run_ffmpeg_with_progress(
        first,
        "Encoding pass 1 of 2",
        encoding.duration_seconds,
        6,
        45,
        attempt,
        callback,
    ) {
        cleanup_passlog(&passlog);
        return Err(error);
    }

    let mut second = toolchain::command("ffmpeg");
    second.args(["-hide_banner", "-loglevel", "error", "-y", "-i"]);
    second.arg(input);
    second.args([
        "-map", "0:v:0", "-c:v", "libx264", "-preset", "medium", "-b:v", &bitrate, "-pix_fmt",
        "yuv420p",
    ]);
    if encoding.output_width > 0 && encoding.source_width > encoding.output_width {
        second.args(["-vf", &format!("scale={}:-2", encoding.output_width)]);
    }
    second.args(["-pass", "2", "-passlogfile", &passlog_value]);
    if encoding.has_audio {
        second.args(["-map", "0:a:0?", "-c:a", "aac", "-b:a", &audio_bitrate]);
    } else {
        second.arg("-an");
    }
    second.args(["-movflags", "+faststart"]);
    second.args(["-progress", "pipe:1", "-nostats"]);
    second.arg(output_path);
    let result = run_ffmpeg_with_progress(
        second,
        "Encoding pass 2 of 2",
        encoding.duration_seconds,
        46,
        90,
        attempt,
        callback,
    );
    cleanup_passlog(&passlog);
    result
}

#[allow(clippy::too_many_arguments)]
fn run_ffmpeg_with_progress(
    mut command: Command,
    stage: &str,
    duration_seconds: f64,
    start_percent: u8,
    end_percent: u8,
    attempt: u8,
    callback: &mut dyn FnMut(ProcessProgress) -> bool,
) -> Result<(), String> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("FitSend could not start FFmpeg for {stage}: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "FitSend could not read FFmpeg progress.".to_string())?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| "FitSend could not read FFmpeg errors.".to_string())?;
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr.read_to_end(&mut bytes);
        bytes
    });

    if report(callback, start_percent, stage, Some(0.0), attempt).is_err() {
        let _ = child.kill();
        let _ = child.wait();
        let _ = stderr_reader.join();
        return Err(crate::progress::PROCESS_CANCELLED.to_string());
    }
    for line in BufReader::new(stdout).lines() {
        let line =
            line.map_err(|error| format!("FitSend could not read FFmpeg progress: {error}"))?;
        let Some(value) = line.strip_prefix("out_time_us=") else {
            continue;
        };
        let encoded_seconds = value.parse::<f64>().unwrap_or(0.0) / 1_000_000.0;
        let fraction = (encoded_seconds / duration_seconds.max(0.001)).clamp(0.0, 1.0);
        let range = end_percent.saturating_sub(start_percent) as f64;
        let percent = start_percent.saturating_add((fraction * range).round() as u8);
        if report(callback, percent, stage, Some(encoded_seconds), attempt).is_err() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stderr_reader.join();
            return Err(crate::progress::PROCESS_CANCELLED.to_string());
        }
    }

    let status = child
        .wait()
        .map_err(|error| format!("FitSend could not wait for FFmpeg during {stage}: {error}"))?;
    let stderr = stderr_reader.join().unwrap_or_default();
    if status.success() {
        report(
            callback,
            end_percent,
            stage,
            Some(duration_seconds),
            attempt,
        )?;
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&stderr);
    let detail = stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("unknown FFmpeg error")
        .trim();
    Err(format!("FFmpeg failed during {stage}: {detail}"))
}

fn temporary_passlog() -> PathBuf {
    std::env::temp_dir().join(format!(
        "fitsend-pass-{}-{}",
        std::process::id(),
        unique_suffix()
    ))
}

fn cleanup_passlog(base: &Path) {
    for suffix in ["-0.log", "-0.log.mbtree", ".log", ".log.mbtree"] {
        let value = format!("{}{}", base.to_string_lossy(), suffix);
        let _ = fs::remove_file(value);
    }
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

fn elapsed_millis(started_at: Instant) -> u64 {
    started_at.elapsed().as_millis().min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analyze, build, PlanRequest};
    use image::{DynamicImage, ImageBuffer};

    fn sample_analysis(path: &Path) -> crate::MediaAnalysis {
        analyze(path.to_string_lossy().as_ref()).expect("sample media should be analyzable")
    }

    #[test]
    fn creates_a_new_name_when_output_exists() {
        let directory = tempfile::tempdir().unwrap();
        let requested = directory.path().join("clip.fitsend.mp4");
        fs::write(&requested, b"existing").unwrap();
        assert_eq!(
            crate::output::non_overwriting_path(&requested)
                .file_name()
                .and_then(|value| value.to_str()),
            Some("clip.fitsend-2.mp4")
        );
    }

    #[test]
    fn rejects_a_source_that_grew_after_analysis() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("source.jpg");
        let output = directory.path().join("output.jpg");
        fs::write(&input, vec![b'x'; 9 * 1024]).unwrap();
        let request = ProcessRequest {
            analysis: crate::domain::MediaAnalysis {
                path: input.to_string_lossy().to_string(),
                name: "source.jpg".to_string(),
                extension: "jpg".to_string(),
                kind: MediaKind::Image,
                size_bytes: 4,
                width: 1,
                height: 1,
                duration_seconds: None,
                frame_rate: None,
                rotation_degrees: 0,
                video_codec: None,
                audio_codec: None,
                has_audio: false,
                has_alpha: false,
                ffmpeg_available: false,
            },
            target_bytes: 8 * 1024,
            output_path: output.to_string_lossy().to_string(),
            strategy: crate::CompressionStrategy::Precise,
        };

        let error = process(&request).unwrap_err();
        assert!(error.contains("changed after it was analyzed"));
        assert!(!output.exists());
    }

    #[test]
    fn processes_a_real_png_below_the_requested_limit() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("source.png");
        let output = directory.path().join("source.fitsend.png");
        let source = ImageBuffer::from_fn(1400, 900, |x, y| {
            let mixed = x.wrapping_mul(31) ^ y.wrapping_mul(17) ^ (x * y);
            image::Rgba([
                (mixed % 251) as u8,
                ((mixed / 7 + x) % 253) as u8,
                ((mixed / 11 + y) % 255) as u8,
                if (x + y) % 7 == 0 { 180 } else { 255 },
            ])
        });
        DynamicImage::ImageRgba8(source)
            .save_with_format(&input, image::ImageFormat::Png)
            .unwrap();

        let analysis = sample_analysis(&input);
        assert!(analysis.has_alpha);
        assert!(analysis.size_bytes > 2 * 1024 * 1024);
        let plan = build(&PlanRequest {
            analysis: analysis.clone(),
            target_bytes: 2 * 1024 * 1024,
            strategy: crate::CompressionStrategy::Precise,
        })
        .unwrap();
        assert!(!plan.already_fits);
        assert!(plan
            .warnings
            .iter()
            .any(|warning| warning.contains("transparency")));

        let result = process(&ProcessRequest {
            analysis,
            target_bytes: 2 * 1024 * 1024,
            output_path: output.to_string_lossy().to_string(),
            strategy: crate::CompressionStrategy::Precise,
        })
        .unwrap();

        assert!(result.verified);
        assert!(result.output_bytes <= 2 * 1024 * 1024);
        assert!(output.exists());
        assert_eq!(
            image::ImageReader::open(&output)
                .unwrap()
                .with_guessed_format()
                .unwrap()
                .format(),
            Some(image::ImageFormat::Png)
        );
        assert!(image::open(&output).unwrap().color().has_alpha());
    }

    #[test]
    fn reports_monotonic_image_progress() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("progress-source.png");
        let output = directory.path().join("progress-output.jpg");
        let source = ImageBuffer::from_fn(900, 600, |x, y| {
            image::Rgb([
                (x.wrapping_mul(19) % 255) as u8,
                (y.wrapping_mul(23) % 255) as u8,
                ((x ^ y) % 255) as u8,
            ])
        });
        DynamicImage::ImageRgb8(source)
            .save_with_format(&input, image::ImageFormat::Png)
            .unwrap();
        let analysis = sample_analysis(&input);
        let mut percentages = Vec::new();
        let result = process_with_progress(
            &ProcessRequest {
                analysis,
                target_bytes: 200 * 1024,
                output_path: output.to_string_lossy().to_string(),
                strategy: crate::CompressionStrategy::Precise,
            },
            |progress| {
                percentages.push(progress.percent);
                true
            },
        )
        .unwrap();

        assert!(result.verified);
        assert!(percentages.len() > 5);
        assert!(percentages.windows(2).all(|pair| pair[0] <= pair[1]));
        assert_eq!(percentages.last(), Some(&100));
    }

    #[test]
    fn cancelling_image_processing_removes_partial_output() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("cancel-source.png");
        let output = directory.path().join("cancel-output.jpg");
        let source = ImageBuffer::from_fn(1200, 800, |x, y| {
            image::Rgb([(x % 255) as u8, (y % 255) as u8, ((x + y) % 255) as u8])
        });
        DynamicImage::ImageRgb8(source)
            .save_with_format(&input, image::ImageFormat::Png)
            .unwrap();
        let analysis = sample_analysis(&input);
        let error = process_with_progress(
            &ProcessRequest {
                analysis,
                target_bytes: 80 * 1024,
                output_path: output.to_string_lossy().to_string(),
                strategy: crate::CompressionStrategy::Precise,
            },
            |progress| progress.percent < 20,
        )
        .unwrap_err();

        assert_eq!(error, crate::PROCESS_CANCELLED);
        assert!(!output.exists());
    }

    #[test]
    fn processes_a_real_video_below_the_requested_limit() {
        if !crate::analyzer::command_available("ffmpeg") {
            eprintln!("FFmpeg is unavailable; skipping real video pipeline test.");
            return;
        }

        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("source.mp4");
        let output = directory.path().join("source.fitsend.mp4");
        let generated = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1280x720:rate=30:duration=5",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=880:sample_rate=48000:duration=5",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-b:v",
                "5M",
                "-c:a",
                "aac",
                "-b:a",
                "128k",
                "-shortest",
            ])
            .arg(&input)
            .output()
            .unwrap();
        assert!(
            generated.status.success(),
            "sample video generation failed: {}",
            String::from_utf8_lossy(&generated.stderr)
        );

        let analysis = sample_analysis(&input);
        assert_eq!(analysis.kind, MediaKind::Video);
        assert!(analysis.has_audio);
        assert!(analysis.size_bytes > 700 * 1024);
        let plan = build(&PlanRequest {
            analysis: analysis.clone(),
            target_bytes: 700 * 1024,
            strategy: crate::CompressionStrategy::Precise,
        })
        .unwrap();
        assert!(plan.feasible);
        assert!(!plan.already_fits);

        let result = process(&ProcessRequest {
            analysis,
            target_bytes: 700 * 1024,
            output_path: output.to_string_lossy().to_string(),
            strategy: crate::CompressionStrategy::Precise,
        })
        .unwrap();

        assert!(result.verified);
        assert!(result.output_bytes <= 700 * 1024);
        assert!(result.attempts <= 3);
        let output_analysis = sample_analysis(&output);
        assert_eq!(output_analysis.video_codec.as_deref(), Some("h264"));
        assert_eq!(output_analysis.audio_codec.as_deref(), Some("aac"));
    }

    #[test]
    fn cancelling_video_processing_stops_before_output() {
        if !crate::analyzer::command_available("ffmpeg") {
            return;
        }

        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("cancel-video-source.mp4");
        let output = directory.path().join("cancel-video-output.mp4");
        let generated = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1280x720:rate=30:duration=8",
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-b:v",
                "5M",
            ])
            .arg(&input)
            .output()
            .unwrap();
        assert!(generated.status.success());

        let analysis = sample_analysis(&input);
        let error = process_with_progress(
            &ProcessRequest {
                analysis,
                target_bytes: 900 * 1024,
                output_path: output.to_string_lossy().to_string(),
                strategy: crate::CompressionStrategy::Precise,
            },
            |progress| progress.stage != "Encoding pass 1 of 2",
        )
        .unwrap_err();

        assert_eq!(error, crate::PROCESS_CANCELLED);
        assert!(!output.exists());
    }
}
