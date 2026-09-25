use std::{fs, path::Path, process::ExitStatus};

use image::{GenericImageView, ImageReader};
use serde_json::Value;

use crate::{
    domain::{MediaAnalysis, MediaKind},
    toolchain,
};

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png"];
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "mkv", "webm"];

pub fn analyze(path_value: &str) -> Result<MediaAnalysis, String> {
    let path = Path::new(path_value);
    let metadata = fs::metadata(path).map_err(|error| friendly_io_error("read", path, error))?;
    if !metadata.is_file() {
        return Err("FitSend currently accepts one file at a time.".to_string());
    }

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Untitled file")
        .to_string();

    if IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        let image = ImageReader::open(path)
            .map_err(|error| format!("FitSend could not open this image: {error}"))?
            .with_guessed_format()
            .map_err(|error| format!("FitSend could not identify this image: {error}"))?
            .decode()
            .map_err(|error| format!("This image appears to be damaged or unsupported: {error}"))?;
        let (width, height) = image.dimensions();

        return Ok(MediaAnalysis {
            path: path_value.to_string(),
            name,
            extension,
            kind: MediaKind::Image,
            size_bytes: metadata.len(),
            width,
            height,
            duration_seconds: None,
            frame_rate: None,
            rotation_degrees: 0,
            video_codec: None,
            audio_codec: None,
            has_audio: false,
            has_alpha: image.color().has_alpha(),
            ffmpeg_available: command_available("ffmpeg"),
        });
    }

    if VIDEO_EXTENSIONS.contains(&extension.as_str()) {
        return analyze_video(path, path_value, name, extension, metadata.len());
    }

    Err(format!(
        "Unsupported file type. FitSend accepts JPG, PNG, MP4, MOV, MKV, and WebM. Received: .{}",
        if extension.is_empty() {
            "unknown"
        } else {
            &extension
        }
    ))
}

fn analyze_video(
    path: &Path,
    path_value: &str,
    name: String,
    extension: String,
    size_bytes: u64,
) -> Result<MediaAnalysis, String> {
    if !command_available("ffprobe") {
        return Err(
            "Video analysis needs FFprobe. Install FFmpeg and reopen FitSend; image processing remains available."
                .to_string(),
        );
    }

    let output = toolchain::command("ffprobe")
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
        .map_err(|error| format!("FitSend could not start FFprobe: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "This video could not be analyzed. FFprobe reported: {}",
            concise_process_error(&stderr, output.status)
        ));
    }

    let payload: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("FFprobe returned unreadable metadata: {error}"))?;
    let streams = payload["streams"]
        .as_array()
        .ok_or_else(|| "FFprobe did not return any media streams.".to_string())?;
    let video = streams
        .iter()
        .find(|stream| stream["codec_type"].as_str() == Some("video"))
        .ok_or_else(|| "No video track was found in this file.".to_string())?;
    let audio = streams
        .iter()
        .find(|stream| stream["codec_type"].as_str() == Some("audio"));

    let duration_seconds = video["duration"]
        .as_str()
        .or_else(|| payload["format"]["duration"].as_str())
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| *value > 0.0);
    let frame_rate = video["avg_frame_rate"]
        .as_str()
        .and_then(parse_fraction)
        .filter(|value| *value > 0.0);
    let rotation_degrees = video["side_data_list"]
        .as_array()
        .and_then(|items| items.iter().find_map(|item| item["rotation"].as_i64()))
        .or_else(|| {
            video["tags"]["rotate"]
                .as_str()
                .and_then(|value| value.parse::<i64>().ok())
        })
        .map(normalize_rotation)
        .unwrap_or(0);
    let (width, height) = display_dimensions(
        video["width"].as_u64().unwrap_or(0) as u32,
        video["height"].as_u64().unwrap_or(0) as u32,
        rotation_degrees,
    );

    Ok(MediaAnalysis {
        path: path_value.to_string(),
        name,
        extension,
        kind: MediaKind::Video,
        size_bytes,
        width,
        height,
        duration_seconds,
        frame_rate,
        rotation_degrees,
        video_codec: video["codec_name"].as_str().map(str::to_string),
        audio_codec: audio
            .and_then(|stream| stream["codec_name"].as_str())
            .map(str::to_string),
        has_audio: audio.is_some(),
        has_alpha: false,
        ffmpeg_available: command_available("ffmpeg"),
    })
}

fn parse_fraction(value: &str) -> Option<f64> {
    let (numerator, denominator) = value.split_once('/')?;
    let numerator = numerator.parse::<f64>().ok()?;
    let denominator = denominator.parse::<f64>().ok()?;
    (denominator.abs() > f64::EPSILON).then_some(numerator / denominator)
}

fn normalize_rotation(value: i64) -> i32 {
    let normalized = value.rem_euclid(360) as i32;
    match normalized {
        45..=134 => 90,
        135..=224 => 180,
        225..=314 => 270,
        _ => 0,
    }
}

fn display_dimensions(width: u32, height: u32, rotation_degrees: i32) -> (u32, u32) {
    if matches!(rotation_degrees, 90 | 270) {
        (height, width)
    } else {
        (width, height)
    }
}

pub fn command_available(command: &str) -> bool {
    toolchain::available(command)
}

fn concise_process_error(stderr: &str, status: ExitStatus) -> String {
    let detail: String = stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("no error details were returned")
        .trim()
        .chars()
        .take(240)
        .collect();
    // Windows failures can be NTSTATUS values rather than FFprobe error codes.
    // Preserve both decimal and hexadecimal forms instead of hiding empty stderr.
    let exit = match status.code() {
        Some(code) => format!("exit code {code}, 0x{:08X}", code as u32),
        None => status.to_string(),
    };
    format!("{detail} ({exit})")
}

fn friendly_io_error(action: &str, path: &Path, error: std::io::Error) -> String {
    format!(
        "FitSend could not {action} '{}': {error}",
        path.to_string_lossy()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsupported_files_with_a_clear_message() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("notes.txt");
        fs::write(&path, b"not media").unwrap();
        let error = analyze(path.to_string_lossy().as_ref()).unwrap_err();
        assert!(error.contains("Unsupported file type"));
        assert!(error.contains(".txt"));
    }

    #[test]
    fn parses_common_frame_rates_and_rotation() {
        assert_eq!(parse_fraction("30000/1001").unwrap().round(), 30.0);
        assert_eq!(parse_fraction("0/0"), None);
        assert_eq!(normalize_rotation(-90), 270);
        assert_eq!(normalize_rotation(89), 90);
        assert_eq!(display_dimensions(1920, 1080, 90), (1080, 1920));
        assert_eq!(display_dimensions(1920, 1080, 180), (1920, 1080));
    }

    #[cfg(windows)]
    #[test]
    fn reports_exit_status_even_when_ffprobe_returns_no_error_text() {
        use std::os::windows::process::ExitStatusExt;
        let detail = concise_process_error("", ExitStatus::from_raw(0xC000_013A));
        assert!(detail.contains("no error details were returned"));
        assert!(detail.contains("exit code -1073741510, 0xC000013A"));
        assert!(!detail.contains("unknown error"));
        let detail = concise_process_error("first line\ninvalid media\n", ExitStatus::from_raw(1));
        assert_eq!(detail, "invalid media (exit code 1, 0x00000001)");
    }

    #[test]
    fn corrupt_video_does_not_poison_concurrent_or_subsequent_valid_analysis() {
        if !command_available("ffmpeg") || !command_available("ffprobe") {
            eprintln!("Skipping real-media analysis isolation test: FFmpeg/FFprobe unavailable");
            return;
        }
        let directory = tempfile::tempdir().unwrap();
        let valid = directory.path().join("valid.mp4");
        let corrupt = directory.path().join("corrupt.mp4");
        fs::write(&corrupt, b"not a valid video").unwrap();
        let generated = toolchain::command("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:s=160x90:r=10",
                "-t",
                "0.5",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&valid)
            .output()
            .unwrap();
        assert!(generated.status.success(), "{generated:?}");
        let valid_bytes = fs::read(&valid).unwrap();
        let corrupt_bytes = fs::read(&corrupt).unwrap();
        let check_valid = |analysis: MediaAnalysis| {
            assert_eq!(analysis.path, valid.to_string_lossy());
            assert_eq!((analysis.width, analysis.height), (160, 90));
            assert_eq!(analysis.duration_seconds, Some(0.5));
            assert_eq!(analysis.size_bytes, valid_bytes.len() as u64);
        };

        for _ in 0..4 {
            let barrier = std::sync::Barrier::new(2);
            std::thread::scope(|scope| {
                let bad = scope.spawn(|| {
                    barrier.wait();
                    analyze(corrupt.to_str().unwrap())
                });
                let good = scope.spawn(|| {
                    barrier.wait();
                    analyze(valid.to_str().unwrap())
                });
                assert!(bad
                    .join()
                    .unwrap()
                    .unwrap_err()
                    .contains("FFprobe reported"));
                check_valid(good.join().unwrap().unwrap());
            });
            // A native picker selection is analyzed sequentially by useBatchQueue.
            assert!(analyze(corrupt.to_str().unwrap()).is_err());
            check_valid(analyze(valid.to_str().unwrap()).unwrap());
        }
        assert_eq!(fs::read(valid).unwrap(), valid_bytes);
        assert_eq!(fs::read(corrupt).unwrap(), corrupt_bytes);
    }
}
