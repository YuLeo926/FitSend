use std::path::Path;

use crate::{
    domain::{CompressionStrategy, MediaAnalysis},
    quality::{
        parse_ffmpeg_ssim_stats, VideoSimilarity, BALANCED_VIDEO_MEAN_SSIM,
        BALANCED_VIDEO_MIN_SSIM, SMALLEST_VIDEO_MEAN_SSIM, SMALLEST_VIDEO_MIN_SSIM,
    },
    toolchain,
};

#[derive(Debug, Clone)]
pub struct VideoCandidatePlan {
    pub video_bitrate_kbps: u64,
    pub audio_bitrate_kbps: u64,
    pub width: u32,
    pub height: u32,
}

pub fn candidate_plans(
    analysis: &MediaAnalysis,
    target_bytes: u64,
    strategy: CompressionStrategy,
) -> Result<Vec<VideoCandidatePlan>, String> {
    let duration = analysis
        .duration_seconds
        .filter(|value| *value > 0.0)
        .ok_or_else(|| "FitSend could not determine this video's duration.".to_string())?;
    let source_total = ((analysis.size_bytes as f64 * 8.0) / duration / 1000.0).max(1.0);
    let target_total = ((target_bytes as f64 * 8.0 * 0.94) / duration / 1000.0).max(1.0);
    let (factors, audio_kbps, max_long_edge): (&[f64], u64, u32) = match strategy {
        CompressionStrategy::Balanced => (&[0.72, 0.82, 0.90], 96, u32::MAX),
        CompressionStrategy::Smallest => (&[0.30, 0.42, 0.55, 0.70, 0.84], 64, 1280),
        CompressionStrategy::Precise => (&[1.0], if analysis.has_audio { 96 } else { 0 }, u32::MAX),
    };
    let audio_kbps = if analysis.has_audio { audio_kbps } else { 0 };
    let (width, height) = bounded_dimensions(analysis.width, analysis.height, max_long_edge);
    let mut plans = Vec::new();
    for factor in factors {
        let total = (source_total * factor).min(target_total).floor() as u64;
        let video = total.saturating_sub(audio_kbps).max(180);
        if plans
            .last()
            .is_some_and(|previous: &VideoCandidatePlan| previous.video_bitrate_kbps == video)
        {
            continue;
        }
        plans.push(VideoCandidatePlan {
            video_bitrate_kbps: video,
            audio_bitrate_kbps: audio_kbps,
            width,
            height,
        });
    }
    Ok(plans)
}

pub fn similarity_passes(strategy: CompressionStrategy, score: VideoSimilarity) -> bool {
    match strategy {
        CompressionStrategy::Precise => true,
        CompressionStrategy::Balanced => {
            score.mean >= BALANCED_VIDEO_MEAN_SSIM && score.minimum >= BALANCED_VIDEO_MIN_SSIM
        }
        CompressionStrategy::Smallest => {
            score.mean >= SMALLEST_VIDEO_MEAN_SSIM && score.minimum >= SMALLEST_VIDEO_MIN_SSIM
        }
    }
}

pub fn measure_similarity(
    source: &str,
    candidate: &Path,
    source_width: u32,
    source_height: u32,
) -> Result<VideoSimilarity, String> {
    let null_output = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let filter = format!(
        "[0:v]setpts=PTS-STARTPTS[ref];[1:v]scale={source_width}:{source_height}:flags=lanczos,setpts=PTS-STARTPTS[cmp];[ref][cmp]ssim=stats_file=-"
    );
    let output = toolchain::command("ffmpeg")
        .args(["-hide_banner", "-loglevel", "info", "-i"])
        .arg(source)
        .arg("-i")
        .arg(candidate)
        .args(["-lavfi", &filter, "-an", "-f", "null", null_output])
        .output()
        .map_err(|error| format!("FitSend could not start video quality verification: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "FitSend could not compare the encoded video: {}",
            concise_error(&output.stderr)
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_ffmpeg_ssim_stats(&format!("{stdout}\n{stderr}"))
}

fn bounded_dimensions(width: u32, height: u32, max_long_edge: u32) -> (u32, u32) {
    let long_edge = width.max(height);
    if long_edge == 0 || long_edge <= max_long_edge {
        return (make_even(width), make_even(height));
    }
    let scale = max_long_edge as f64 / long_edge as f64;
    (
        make_even((width as f64 * scale).round() as u32),
        make_even((height as f64 * scale).round() as u32),
    )
}

fn make_even(value: u32) -> u32 {
    if value.is_multiple_of(2) {
        value.max(2)
    } else {
        value.saturating_sub(1).max(2)
    }
}

fn concise_error(stderr: &[u8]) -> String {
    String::from_utf8_lossy(stderr)
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("unknown FFmpeg error")
        .trim()
        .chars()
        .take(240)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::MediaKind;

    fn analysis(width: u32, height: u32) -> MediaAnalysis {
        MediaAnalysis {
            path: "clip.mp4".to_string(),
            name: "clip.mp4".to_string(),
            extension: "mp4".to_string(),
            kind: MediaKind::Video,
            size_bytes: 30 * 1024 * 1024,
            width,
            height,
            duration_seconds: Some(30.0),
            frame_rate: Some(30.0),
            rotation_degrees: 0,
            video_codec: Some("h264".to_string()),
            audio_codec: Some("aac".to_string()),
            has_audio: true,
            has_alpha: false,
            ffmpeg_available: true,
        }
    }

    #[test]
    fn balanced_keeps_original_dimensions() {
        let plans = candidate_plans(&analysis(1920, 1080), 8 * 1024 * 1024, CompressionStrategy::Balanced).unwrap();
        assert!(plans.iter().all(|plan| (plan.width, plan.height) == (1920, 1080)));
    }

    #[test]
    fn smallest_caps_hd_media_at_720p_long_edge() {
        let plans = candidate_plans(&analysis(3840, 2160), 8 * 1024 * 1024, CompressionStrategy::Smallest).unwrap();
        assert!(plans.iter().all(|plan| (plan.width, plan.height) == (1280, 720)));
    }

    #[test]
    fn applies_documented_similarity_floors() {
        assert!(similarity_passes(
            CompressionStrategy::Balanced,
            VideoSimilarity { mean: 0.99, minimum: 0.98 }
        ));
        assert!(!similarity_passes(
            CompressionStrategy::Smallest,
            VideoSimilarity { mean: 0.94, minimum: 0.93 }
        ));
    }
}
