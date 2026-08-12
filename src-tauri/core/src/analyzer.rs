use std::{fs, path::Path, process::Command};

use image::{GenericImageView, ImageReader};
use serde_json::Value;

use crate::domain::{MediaAnalysis, MediaKind};

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
        "Unsupported file type. FitSend v0.1 accepts JPG, PNG, MP4, MOV, MKV, and WebM. Received: .{}",
        if extension.is_empty() { "unknown" } else { &extension }
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

    let output = Command::new("ffprobe")
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
            concise_process_error(&stderr)
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

    Ok(MediaAnalysis {
        path: path_value.to_string(),
        name,
        extension,
        kind: MediaKind::Video,
        size_bytes,
        width: video["width"].as_u64().unwrap_or(0) as u32,
        height: video["height"].as_u64().unwrap_or(0) as u32,
        duration_seconds,
        video_codec: video["codec_name"].as_str().map(str::to_string),
        audio_codec: audio
            .and_then(|stream| stream["codec_name"].as_str())
            .map(str::to_string),
        has_audio: audio.is_some(),
        has_alpha: false,
        ffmpeg_available: command_available("ffmpeg"),
    })
}

pub fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("-version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn concise_process_error(stderr: &str) -> String {
    stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("unknown error")
        .trim()
        .chars()
        .take(240)
        .collect()
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
}
