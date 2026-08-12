use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Output,
    time::{SystemTime, UNIX_EPOCH},
};

use image::{
    codecs::jpeg::JpegEncoder,
    imageops::{resize, FilterType},
    DynamicImage, GenericImageView, ImageBuffer, ImageReader, Rgb, RgbImage,
};

use crate::{
    domain::{MediaKind, ProcessRequest, ProcessResult},
    planner, toolchain,
};

pub fn process(request: &ProcessRequest) -> Result<ProcessResult, String> {
    let plan = planner::build(&crate::domain::PlanRequest {
        analysis: request.analysis.clone(),
        target_bytes: request.target_bytes,
    })?;
    if !plan.feasible {
        return Err(plan
            .warnings
            .first()
            .cloned()
            .unwrap_or_else(|| "This target is not feasible for the selected file.".to_string()));
    }

    let requested_output = Path::new(&request.output_path);
    let output_path = non_overwriting_path(requested_output);
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("FitSend could not create the output folder: {error}"))?;
    }

    if plan.already_fits {
        fs::copy(&request.analysis.path, &output_path)
            .map_err(|error| format!("FitSend could not create the verified copy: {error}"))?;
        let metadata = fs::metadata(&output_path)
            .map_err(|error| format!("FitSend could not verify the output: {error}"))?;
        if metadata.len() > request.target_bytes {
            let _ = fs::remove_file(&output_path);
            return Err(
                "The source file changed after it was analyzed and no longer fits the target. Analyze it again."
                    .to_string(),
            );
        }
        return Ok(ProcessResult {
            output_path: output_path.to_string_lossy().to_string(),
            output_bytes: metadata.len(),
            target_bytes: request.target_bytes,
            verified: true,
            attempts: 1,
            width: request.analysis.width,
            height: request.analysis.height,
        });
    }

    let result = match request.analysis.kind {
        MediaKind::Image => process_image(request, &output_path),
        MediaKind::Video => process_video(request, &output_path),
    };

    if result.is_err() {
        let _ = fs::remove_file(&output_path);
    }
    result
}

fn process_image(request: &ProcessRequest, output_path: &Path) -> Result<ProcessResult, String> {
    let decoded = ImageReader::open(&request.analysis.path)
        .map_err(|error| format!("FitSend could not open this image: {error}"))?
        .with_guessed_format()
        .map_err(|error| format!("FitSend could not identify this image: {error}"))?
        .decode()
        .map_err(|error| format!("FitSend could not decode this image: {error}"))?;
    let rgb = flatten_to_white(&decoded);
    let (bytes, width, height) = best_jpeg_candidate(&rgb, request.target_bytes)?;
    fs::write(output_path, &bytes)
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

    Ok(ProcessResult {
        output_path: output_path.to_string_lossy().to_string(),
        output_bytes,
        target_bytes: request.target_bytes,
        verified: true,
        attempts: 1,
        width,
        height,
    })
}

fn process_video(request: &ProcessRequest, output_path: &Path) -> Result<ProcessResult, String> {
    let plan = planner::build(&crate::domain::PlanRequest {
        analysis: request.analysis.clone(),
        target_bytes: request.target_bytes,
    })?;
    let mut bitrate = plan
        .video_bitrate_kbps
        .ok_or_else(|| "FitSend could not calculate a video bitrate.".to_string())?;
    let audio_bitrate = plan.audio_bitrate_kbps.unwrap_or(0);

    for attempt in 1..=3 {
        if output_path.exists() {
            let _ = fs::remove_file(output_path);
        }
        encode_video_passes(
            &request.analysis.path,
            output_path,
            bitrate,
            audio_bitrate,
            request.analysis.has_audio,
            request.analysis.width,
            plan.width,
        )?;

        let output_bytes = fs::metadata(output_path)
            .map_err(|error| format!("FitSend could not verify the video output: {error}"))?
            .len();
        if output_bytes <= request.target_bytes {
            return Ok(ProcessResult {
                output_path: output_path.to_string_lossy().to_string(),
                output_bytes,
                target_bytes: request.target_bytes,
                verified: true,
                attempts: attempt,
                width: plan.width,
                height: plan.height,
            });
        }

        let correction = request.target_bytes as f64 / output_bytes as f64;
        bitrate = ((bitrate as f64 * correction * 0.95).floor() as u64).max(180);
    }

    Err("The encoded video remained above the target after three measured attempts. Try a larger limit or a shorter clip."
        .to_string())
}

fn encode_video_passes(
    input: &str,
    output_path: &Path,
    video_bitrate_kbps: u64,
    audio_bitrate_kbps: u64,
    has_audio: bool,
    source_width: u32,
    output_width: u32,
) -> Result<(), String> {
    let passlog = temporary_passlog();
    let null_output = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let bitrate = format!("{video_bitrate_kbps}k");
    let audio_bitrate = format!("{audio_bitrate_kbps}k");
    let passlog_value = passlog.to_string_lossy().to_string();

    let mut first = toolchain::command("ffmpeg");
    first.args(["-hide_banner", "-loglevel", "error", "-y", "-i"]);
    first.arg(input);
    first.args([
        "-map", "0:v:0", "-c:v", "libx264", "-preset", "medium", "-b:v", &bitrate, "-pix_fmt",
        "yuv420p",
    ]);
    if output_width > 0 && source_width > output_width {
        first.args(["-vf", &format!("scale={output_width}:-2")]);
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
    first.arg(null_output);
    if let Err(error) = ensure_success(first.output(), "first video pass") {
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
    if output_width > 0 && source_width > output_width {
        second.args(["-vf", &format!("scale={output_width}:-2")]);
    }
    second.args(["-pass", "2", "-passlogfile", &passlog_value]);
    if has_audio {
        second.args(["-map", "0:a:0?", "-c:a", "aac", "-b:a", &audio_bitrate]);
    } else {
        second.arg("-an");
    }
    second.args(["-movflags", "+faststart"]);
    second.arg(output_path);
    let result = ensure_success(second.output(), "second video pass");
    cleanup_passlog(&passlog);
    result
}

fn ensure_success(result: Result<Output, std::io::Error>, stage: &str) -> Result<(), String> {
    let output = result
        .map_err(|error| format!("FitSend could not start FFmpeg for the {stage}: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("unknown FFmpeg error")
        .trim();
    Err(format!("FFmpeg failed during the {stage}: {detail}"))
}

fn flatten_to_white(image: &DynamicImage) -> RgbImage {
    let rgba = image.to_rgba8();
    let (width, height) = image.dimensions();
    ImageBuffer::from_fn(width, height, |x, y| {
        let pixel = rgba.get_pixel(x, y);
        let alpha = pixel[3] as u16;
        let blend = |channel: u8| -> u8 {
            (((channel as u16 * alpha) + (255 * (255 - alpha))) / 255) as u8
        };
        Rgb([blend(pixel[0]), blend(pixel[1]), blend(pixel[2])])
    })
}

fn best_jpeg_candidate(
    source: &RgbImage,
    target_bytes: u64,
) -> Result<(Vec<u8>, u32, u32), String> {
    let scales = [1.0_f32, 0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.3, 0.22];
    let qualities = [92_u8, 86, 80, 74, 68, 60, 52, 44, 36, 28];
    let mut best: Option<(f32, Vec<u8>, u32, u32)> = None;

    for scale in scales {
        let width = ((source.width() as f32 * scale).round() as u32).max(1);
        let height = ((source.height() as f32 * scale).round() as u32).max(1);
        if source.width().min(source.height()) >= 96 && width.min(height) < 96 {
            continue;
        }
        let candidate = if width == source.width() && height == source.height() {
            source.clone()
        } else {
            resize(source, width, height, FilterType::Lanczos3)
        };

        for quality in qualities {
            let mut cursor = Cursor::new(Vec::new());
            let mut encoder = JpegEncoder::new_with_quality(&mut cursor, quality);
            encoder
                .encode_image(&DynamicImage::ImageRgb8(candidate.clone()))
                .map_err(|error| format!("FitSend could not encode a JPEG candidate: {error}"))?;
            let bytes = cursor.into_inner();
            if bytes.len() as u64 > target_bytes {
                continue;
            }

            let pixel_ratio = (width as f32 * height as f32)
                / (source.width() as f32 * source.height() as f32).max(1.0);
            let score = pixel_ratio.sqrt() * 0.55 + (quality as f32 / 100.0) * 0.45;
            if best
                .as_ref()
                .map(|current| score > current.0)
                .unwrap_or(true)
            {
                best = Some((score, bytes, width, height));
            }
        }
    }

    best.map(|(_, bytes, width, height)| (bytes, width, height))
        .ok_or_else(|| {
            "The image cannot reach this target without becoming unusably small. Raise the limit."
                .to_string()
        })
}

fn non_overwriting_path(requested: &Path) -> PathBuf {
    if !requested.exists() {
        return requested.to_path_buf();
    }
    let parent = requested.parent().unwrap_or_else(|| Path::new("."));
    let stem = requested
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("fitsend");
    let extension = requested.extension().and_then(|value| value.to_str());
    for index in 2..10_000 {
        let name = match extension {
            Some(extension) => format!("{stem}-{index}.{extension}"),
            None => format!("{stem}-{index}"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    parent.join(format!("{stem}-{}", unique_suffix()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{analyze, build, PlanRequest};
    use std::process::Command;

    fn sample_analysis(path: &Path) -> crate::MediaAnalysis {
        analyze(path.to_string_lossy().as_ref()).expect("sample media should be analyzable")
    }

    #[test]
    fn creates_a_new_name_when_output_exists() {
        let directory = tempfile::tempdir().unwrap();
        let requested = directory.path().join("clip.fitsend.mp4");
        fs::write(&requested, b"existing").unwrap();
        assert_eq!(
            non_overwriting_path(&requested)
                .file_name()
                .and_then(|value| value.to_str()),
            Some("clip.fitsend-2.mp4")
        );
    }

    #[test]
    fn image_candidate_is_below_the_target() {
        let source = ImageBuffer::from_fn(1200, 800, |x, y| {
            Rgb([(x % 255) as u8, (y % 255) as u8, ((x + y) % 255) as u8])
        });
        let (bytes, width, height) = best_jpeg_candidate(&source, 90 * 1024).unwrap();
        assert!(bytes.len() <= 90 * 1024);
        assert!(width > 0 && height > 0);
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
                video_codec: None,
                audio_codec: None,
                has_audio: false,
                has_alpha: false,
                ffmpeg_available: false,
            },
            target_bytes: 8 * 1024,
            output_path: output.to_string_lossy().to_string(),
        };

        let error = process(&request).unwrap_err();
        assert!(error.contains("changed after it was analyzed"));
        assert!(!output.exists());
    }

    #[test]
    fn processes_a_real_png_below_the_requested_limit() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("source.png");
        let output = directory.path().join("source.fitsend.jpg");
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
        assert!(analysis.size_bytes > 160 * 1024);
        let plan = build(&PlanRequest {
            analysis: analysis.clone(),
            target_bytes: 160 * 1024,
        })
        .unwrap();
        assert!(!plan.already_fits);
        assert!(plan
            .warnings
            .iter()
            .any(|warning| warning.contains("transparency")));

        let result = process(&ProcessRequest {
            analysis,
            target_bytes: 160 * 1024,
            output_path: output.to_string_lossy().to_string(),
        })
        .unwrap();

        assert!(result.verified);
        assert!(result.output_bytes <= 160 * 1024);
        assert!(output.exists());
        assert_eq!(
            image::ImageReader::open(output)
                .unwrap()
                .with_guessed_format()
                .unwrap()
                .format(),
            Some(image::ImageFormat::Jpeg)
        );
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
        })
        .unwrap();
        assert!(plan.feasible);
        assert!(!plan.already_fits);

        let result = process(&ProcessRequest {
            analysis,
            target_bytes: 700 * 1024,
            output_path: output.to_string_lossy().to_string(),
        })
        .unwrap();

        assert!(result.verified);
        assert!(result.output_bytes <= 700 * 1024);
        assert!(result.attempts <= 3);
        let output_analysis = sample_analysis(&output);
        assert_eq!(output_analysis.video_codec.as_deref(), Some("h264"));
        assert_eq!(output_analysis.audio_codec.as_deref(), Some("aac"));
    }
}
