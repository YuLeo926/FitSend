use crate::domain::{CompressionPlan, CompressionStrategy, MediaAnalysis, MediaKind, PlanRequest};

const MIN_TARGET_BYTES: u64 = 8 * 1024;
const MIN_VIDEO_BITRATE_KBPS: u64 = 180;

pub fn build(request: &PlanRequest) -> Result<CompressionPlan, String> {
    let analysis = &request.analysis;
    let target_bytes = request.target_bytes;

    let source_fits = analysis.size_bytes <= target_bytes;
    if target_bytes < MIN_TARGET_BYTES && !source_fits {
        return Err("Choose a target of at least 8 KB.".to_string());
    }

    if source_fits
        && (request.strategy == CompressionStrategy::Precise || target_bytes < MIN_TARGET_BYTES)
    {
        return Ok(original_plan(request));
    }

    // A durationless video cannot be assigned an encoding bitrate, but a fitting original
    // can still be independently verified without creating a new output.
    if source_fits
        && matches!(analysis.kind, MediaKind::Video)
        && usable_video_duration(analysis).is_none()
    {
        return Ok(original_plan(request));
    }

    let plan = match analysis.kind {
        MediaKind::Image => image_plan(analysis, target_bytes, request.strategy),
        MediaKind::Video => video_plan(analysis, target_bytes, request.strategy),
    }?;
    // Encoding floors constrain new outputs, not a valid original already inside its allocation.
    if source_fits && !plan.feasible {
        return Ok(original_plan(request));
    }
    Ok(plan)
}

fn original_plan(request: &PlanRequest) -> CompressionPlan {
    let analysis = &request.analysis;
    CompressionPlan {
        strategy: request.strategy,
        already_fits: true,
        feasible: true,
        target_bytes: request.target_bytes,
        estimated_bytes: analysis.size_bytes,
        operation: "Verify original".to_string(),
        summary: "The original already fits. FitSend will verify it and leave it untouched."
            .to_string(),
        quality_label: "Original".to_string(),
        warnings: vec![],
        output_extension: analysis.extension.clone(),
        video_bitrate_kbps: None,
        audio_bitrate_kbps: None,
        width: analysis.width,
        height: analysis.height,
    }
}

fn image_plan(
    analysis: &MediaAnalysis,
    target_bytes: u64,
    strategy: CompressionStrategy,
) -> Result<CompressionPlan, String> {
    let ratio = target_bytes as f64 / analysis.size_bytes as f64;
    let quality_label = if ratio >= 0.7 {
        "Excellent"
    } else if ratio >= 0.35 {
        "Good"
    } else if ratio >= 0.12 {
        "Compact"
    } else {
        "Aggressive"
    };
    let mut warnings = Vec::new();
    if analysis.has_alpha {
        warnings.push(
            "This image contains transparency, which will be preserved in a PNG output."
                .to_string(),
        );
    }
    if ratio < 0.1 {
        warnings.push(
            "This is a severe reduction. FitSend may need to reduce image dimensions.".to_string(),
        );
    }
    if target_bytes < 12 * 1024 {
        warnings.push("Raise the limit to at least 12 KB for a usable image output.".to_string());
    }

    Ok(CompressionPlan {
        strategy,
        already_fits: false,
        feasible: target_bytes >= 12 * 1024,
        target_bytes,
        estimated_bytes: target_bytes.saturating_mul(98) / 100,
        operation: match strategy {
            CompressionStrategy::Precise => "Find the highest quality under the limit",
            CompressionStrategy::Balanced => "Reduce size with almost no visible change",
            CompressionStrategy::Smallest => "Find the smallest acceptable image",
        }
        .to_string(),
        summary: match strategy {
            CompressionStrategy::Precise => "FitSend will search from maximum quality and resize only when necessary.",
            CompressionStrategy::Balanced => "FitSend will keep the original when no worthwhile high-quality saving is available.",
            CompressionStrategy::Smallest => "FitSend will compare safe candidates and choose the smallest one above its quality floor.",
        }
        .to_string(),
        quality_label: quality_label.to_string(),
        warnings,
        output_extension: if analysis.has_alpha { "png" } else { "jpg" }.to_string(),
        video_bitrate_kbps: None,
        audio_bitrate_kbps: None,
        width: analysis.width,
        height: analysis.height,
    })
}

fn video_plan(
    analysis: &MediaAnalysis,
    target_bytes: u64,
    strategy: CompressionStrategy,
) -> Result<CompressionPlan, String> {
    let duration = usable_video_duration(analysis)
        .ok_or_else(|| "FitSend could not determine this video's duration.".to_string())?;
    let audio_kbps = if analysis.has_audio {
        if duration > 300.0 {
            64
        } else {
            96
        }
    } else {
        0
    };
    let total_kbps = ((target_bytes as f64 * 8.0 * 0.94) / duration / 1000.0).floor() as u64;
    let video_kbps = total_kbps.saturating_sub(audio_kbps);
    let feasible = analysis.ffmpeg_available && video_kbps >= MIN_VIDEO_BITRATE_KBPS;
    let max_width = if video_kbps >= 4_000 {
        analysis.width
    } else if video_kbps >= 2_000 {
        analysis.width.min(1920)
    } else if video_kbps >= 1_000 {
        analysis.width.min(1280)
    } else if video_kbps >= 500 {
        analysis.width.min(960)
    } else {
        analysis.width.min(640)
    };
    let (width, height) = scaled_even_dimensions(analysis.width, analysis.height, max_width);
    let quality_label = if video_kbps >= 4_000 {
        "Excellent"
    } else if video_kbps >= 2_000 {
        "Good"
    } else if video_kbps >= 900 {
        "Balanced"
    } else {
        "Compact"
    };
    let mut warnings = Vec::new();
    if !analysis.ffmpeg_available {
        warnings.push("Install FFmpeg to process video files locally.".to_string());
    }
    if video_kbps < MIN_VIDEO_BITRATE_KBPS {
        warnings.push(
            "The target leaves too little video data. Trim the clip, remove audio, or raise the limit."
                .to_string(),
        );
    }
    if width < analysis.width {
        warnings.push(format!(
            "Resolution will be reduced to approximately {width} × {height} to protect visible quality."
        ));
    }

    Ok(CompressionPlan {
        strategy,
        already_fits: false,
        feasible,
        target_bytes,
        estimated_bytes: target_bytes.saturating_mul(97) / 100,
        operation: "Fit video".to_string(),
        summary: "FitSend will encode a broadly compatible MP4, then retry if the measured file is too large."
            .to_string(),
        quality_label: quality_label.to_string(),
        warnings,
        output_extension: "mp4".to_string(),
        video_bitrate_kbps: Some(video_kbps),
        audio_bitrate_kbps: analysis.has_audio.then_some(audio_kbps),
        width,
        height,
    })
}

fn usable_video_duration(analysis: &MediaAnalysis) -> Option<f64> {
    analysis
        .duration_seconds
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn scaled_even_dimensions(width: u32, height: u32, max_width: u32) -> (u32, u32) {
    if width == 0 || height == 0 || max_width == 0 || width <= max_width {
        return (width, height);
    }
    let scaled_height = ((height as f64 * max_width as f64 / width as f64).round() as u32).max(2);
    (make_even(max_width), make_even(scaled_height))
}

fn make_even(value: u32) -> u32 {
    if value.is_multiple_of(2) {
        value
    } else {
        value.saturating_sub(1).max(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video_analysis() -> MediaAnalysis {
        MediaAnalysis {
            path: "clip.mov".to_string(),
            name: "clip.mov".to_string(),
            extension: "mov".to_string(),
            kind: MediaKind::Video,
            size_bytes: 100 * 1024 * 1024,
            width: 1920,
            height: 1080,
            duration_seconds: Some(60.0),
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
    fn keeps_a_file_that_already_fits() {
        let mut analysis = video_analysis();
        analysis.size_bytes = 4 * 1024 * 1024;
        let plan = build(&PlanRequest {
            analysis,
            target_bytes: 10 * 1024 * 1024,
            strategy: CompressionStrategy::Precise,
        })
        .unwrap();
        assert!(plan.already_fits);
        assert_eq!(plan.quality_label, "Original");
    }

    #[test]
    fn precise_skips_a_source_that_already_fits() {
        let mut analysis = video_analysis();
        analysis.size_bytes = 4 * 1024 * 1024;
        let plan = build(&PlanRequest {
            analysis,
            target_bytes: 10 * 1024 * 1024,
            strategy: crate::domain::CompressionStrategy::Precise,
        })
        .unwrap();
        assert!(plan.already_fits);
    }

    #[test]
    fn balanced_still_considers_a_source_that_already_fits() {
        let mut analysis = video_analysis();
        analysis.size_bytes = 4 * 1024 * 1024;
        let plan = build(&PlanRequest {
            analysis,
            target_bytes: 10 * 1024 * 1024,
            strategy: crate::domain::CompressionStrategy::Balanced,
        })
        .unwrap();
        assert!(!plan.already_fits);
    }

    #[test]
    fn unusable_duration_only_keeps_fitting_originals() {
        for duration_seconds in [None, Some(0.0), Some(f64::NAN), Some(f64::INFINITY)] {
            for strategy in [
                CompressionStrategy::Precise,
                CompressionStrategy::Balanced,
                CompressionStrategy::Smallest,
            ] {
                let mut analysis = video_analysis();
                analysis.size_bytes = 20 * 1024;
                analysis.duration_seconds = duration_seconds;
                let fitting = build(&PlanRequest {
                    analysis: analysis.clone(),
                    target_bytes: analysis.size_bytes,
                    strategy,
                })
                .unwrap();
                assert!(fitting.already_fits);
                assert!(build(&PlanRequest {
                    analysis: analysis.clone(),
                    target_bytes: analysis.size_bytes - 1,
                    strategy,
                })
                .is_err());
            }
        }
    }

    #[test]
    fn derives_a_video_bitrate_below_the_target() {
        let plan = build(&PlanRequest {
            analysis: video_analysis(),
            target_bytes: 10 * 1024 * 1024,
            strategy: CompressionStrategy::Precise,
        })
        .unwrap();
        assert!(plan.feasible);
        assert!(plan.video_bitrate_kbps.unwrap() > 1_000);
        assert!(plan.estimated_bytes < plan.target_bytes);
    }

    #[test]
    fn rejects_an_unusable_video_target() {
        let plan = build(&PlanRequest {
            analysis: video_analysis(),
            target_bytes: 400 * 1024,
            strategy: CompressionStrategy::Precise,
        })
        .unwrap();
        assert!(!plan.feasible);
    }

    #[test]
    fn explains_an_unusable_image_target() {
        let mut analysis = video_analysis();
        analysis.kind = MediaKind::Image;
        analysis.duration_seconds = None;
        analysis.has_audio = false;
        analysis.size_bytes = 100 * 1024;
        let plan = build(&PlanRequest {
            analysis,
            target_bytes: 10 * 1024,
            strategy: CompressionStrategy::Precise,
        })
        .unwrap();
        assert!(!plan.feasible);
        assert!(plan
            .warnings
            .iter()
            .any(|warning| warning.contains("12 KB")));
    }

    #[test]
    fn scales_to_even_dimensions() {
        assert_eq!(scaled_even_dimensions(1920, 1080, 959), (958, 538));
    }
}
