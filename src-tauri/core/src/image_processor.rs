use std::io::Cursor;

use image::{
    codecs::{jpeg::JpegEncoder, png::PngEncoder},
    imageops::FilterType,
    DynamicImage, ImageEncoder,
};

use crate::{
    domain::CompressionStrategy,
    progress::{report, ProcessProgress},
    quality::{image_ssim, BALANCED_IMAGE_SSIM, SMALLEST_IMAGE_SSIM},
};

const BALANCED_QUALITIES: [u8; 9] = [100, 98, 96, 94, 92, 90, 88, 86, 84];
const SMALLEST_QUALITIES: [u8; 15] = [30, 34, 38, 42, 46, 50, 54, 58, 62, 66, 70, 76, 82, 90, 100];
const SCALE_TIERS: [f32; 10] = [1.0, 0.9, 0.8, 0.7, 0.6, 0.5, 0.4, 0.32, 0.25, 0.18];

#[derive(Debug)]
pub struct ImageCandidate {
    pub bytes: Vec<u8>,
    pub extension: &'static str,
    pub width: u32,
    pub height: u32,
    pub quality_score: f64,
    pub encoded_quality: Option<u8>,
}

#[derive(Debug)]
pub enum ImageDecision {
    Created(ImageCandidate),
    NoChange(String),
}

pub fn choose_image_candidate(
    source: &DynamicImage,
    source_bytes: u64,
    target_bytes: u64,
    strategy: CompressionStrategy,
    callback: &mut dyn FnMut(ProcessProgress) -> bool,
) -> Result<ImageDecision, String> {
    if source.width() == 0 || source.height() == 0 {
        return Err("FitSend cannot process an empty image.".to_string());
    }
    if source.color().has_alpha() {
        choose_png_candidate(source, source_bytes, target_bytes, strategy, callback)
    } else {
        choose_jpeg_candidate(source, source_bytes, target_bytes, strategy, callback)
    }
}

fn choose_jpeg_candidate(
    source: &DynamicImage,
    source_bytes: u64,
    target_bytes: u64,
    strategy: CompressionStrategy,
    callback: &mut dyn FnMut(ProcessProgress) -> bool,
) -> Result<ImageDecision, String> {
    match strategy {
        CompressionStrategy::Precise => precise_jpeg(source, target_bytes, callback),
        CompressionStrategy::Balanced => {
            let mut best: Option<ImageCandidate> = None;
            for (index, quality) in BALANCED_QUALITIES.into_iter().enumerate() {
                report_candidate(callback, index, BALANCED_QUALITIES.len(), "Checking balanced quality")?;
                let candidate = jpeg_candidate(source, source, quality)?;
                if candidate.bytes.len() as u64 <= target_bytes
                    && candidate.quality_score >= BALANCED_IMAGE_SSIM
                    && best.as_ref().is_none_or(|current| candidate.bytes.len() < current.bytes.len())
                {
                    best = Some(candidate);
                }
            }
            accept_optional_optimization(best, source_bytes, target_bytes, 10)
        }
        CompressionStrategy::Smallest => {
            let min_long_edge = source.width().max(source.height()).min(1280);
            let mut best: Option<ImageCandidate> = None;
            let total = SCALE_TIERS.len() * SMALLEST_QUALITIES.len();
            let mut index = 0;
            for scale in SCALE_TIERS {
                let resized = scaled_image(source, scale);
                if resized.width().max(resized.height()) < min_long_edge {
                    continue;
                }
                for quality in SMALLEST_QUALITIES {
                    report_candidate(callback, index, total, "Finding the smallest good-looking image")?;
                    index += 1;
                    let candidate = jpeg_candidate(source, &resized, quality)?;
                    if candidate.quality_score >= SMALLEST_IMAGE_SSIM
                        && candidate.bytes.len() as u64 <= target_bytes
                        && best.as_ref().is_none_or(|current| candidate.bytes.len() < current.bytes.len())
                    {
                        best = Some(candidate);
                    }
                }
            }
            accept_optional_optimization(best, source_bytes, target_bytes, 1)
        }
    }
}

fn precise_jpeg(
    source: &DynamicImage,
    target_bytes: u64,
    callback: &mut dyn FnMut(ProcessProgress) -> bool,
) -> Result<ImageDecision, String> {
    let min_long_edge = source.width().max(source.height()).min(640);
    for (scale_index, scale) in SCALE_TIERS.into_iter().enumerate() {
        let resized = scaled_image(source, scale);
        if resized.width().max(resized.height()) < min_long_edge {
            continue;
        }
        report_candidate(callback, scale_index, SCALE_TIERS.len(), "Finding the highest image quality")?;
        let highest = jpeg_candidate(source, &resized, 100)?;
        if highest.bytes.len() as u64 <= target_bytes {
            return Ok(ImageDecision::Created(highest));
        }
        let lowest = jpeg_candidate(source, &resized, 30)?;
        if lowest.bytes.len() as u64 > target_bytes {
            continue;
        }
        let mut low = 30_u8;
        let mut high = 99_u8;
        let mut best = lowest;
        while low <= high {
            let quality = low + (high - low) / 2;
            let candidate = jpeg_candidate(source, &resized, quality)?;
            if candidate.bytes.len() as u64 <= target_bytes {
                best = candidate;
                low = quality.saturating_add(1);
            } else if quality == 0 {
                break;
            } else {
                high = quality - 1;
            }
        }
        return Ok(ImageDecision::Created(best));
    }
    Err("The image cannot reach this limit inside Precise Fit's safety floor.".to_string())
}

fn choose_png_candidate(
    source: &DynamicImage,
    source_bytes: u64,
    target_bytes: u64,
    strategy: CompressionStrategy,
    callback: &mut dyn FnMut(ProcessProgress) -> bool,
) -> Result<ImageDecision, String> {
    let min_long_edge = match strategy {
        CompressionStrategy::Precise => source.width().max(source.height()).min(640),
        CompressionStrategy::Balanced => source.width().max(source.height()),
        CompressionStrategy::Smallest => source.width().max(source.height()).min(1280),
    };
    let mut best: Option<ImageCandidate> = None;
    for (index, scale) in SCALE_TIERS.into_iter().enumerate() {
        let resized = scaled_image(source, scale);
        if resized.width().max(resized.height()) < min_long_edge {
            continue;
        }
        report_candidate(callback, index, SCALE_TIERS.len(), "Optimizing transparent image")?;
        let candidate = png_candidate(source, &resized)?;
        let quality_floor = match strategy {
            CompressionStrategy::Precise => 0.0,
            CompressionStrategy::Balanced => BALANCED_IMAGE_SSIM,
            CompressionStrategy::Smallest => SMALLEST_IMAGE_SSIM,
        };
        if candidate.bytes.len() as u64 <= target_bytes
            && candidate.quality_score >= quality_floor
            && best.as_ref().is_none_or(|current| match strategy {
                CompressionStrategy::Precise => {
                    (candidate.width as u64 * candidate.height as u64)
                        > (current.width as u64 * current.height as u64)
                }
                _ => candidate.bytes.len() < current.bytes.len(),
            })
        {
            best = Some(candidate);
            if strategy == CompressionStrategy::Precise {
                break;
            }
        }
    }
    let minimum_savings = if strategy == CompressionStrategy::Balanced { 10 } else { 1 };
    accept_optional_optimization(best, source_bytes, target_bytes, minimum_savings)
}

fn accept_optional_optimization(
    candidate: Option<ImageCandidate>,
    source_bytes: u64,
    target_bytes: u64,
    minimum_savings_percent: u64,
) -> Result<ImageDecision, String> {
    let Some(candidate) = candidate else {
        if source_bytes <= target_bytes {
            return Ok(ImageDecision::NoChange(
                "No worthwhile reduction passed the selected quality check.".to_string(),
            ));
        }
        return Err("FitSend cannot meet this limit without crossing the selected quality floor.".to_string());
    };
    let required_bytes = source_bytes.saturating_mul(100 - minimum_savings_percent) / 100;
    if source_bytes <= target_bytes && candidate.bytes.len() as u64 > required_bytes {
        return Ok(ImageDecision::NoChange(
            "The possible saving is too small to justify recompressing this image.".to_string(),
        ));
    }
    if candidate.bytes.len() as u64 >= source_bytes {
        return Ok(ImageDecision::NoChange(
            "The original is already smaller than the safe optimized copy.".to_string(),
        ));
    }
    Ok(ImageDecision::Created(candidate))
}

fn jpeg_candidate(
    reference: &DynamicImage,
    candidate: &DynamicImage,
    quality: u8,
) -> Result<ImageCandidate, String> {
    let mut cursor = Cursor::new(Vec::new());
    JpegEncoder::new_with_quality(&mut cursor, quality)
        .encode_image(candidate)
        .map_err(|error| format!("FitSend could not encode a JPEG candidate: {error}"))?;
    candidate_from_bytes(reference, cursor.into_inner(), "jpg", Some(quality))
}

fn png_candidate(reference: &DynamicImage, candidate: &DynamicImage) -> Result<ImageCandidate, String> {
    let rgba = candidate.to_rgba8();
    let mut bytes = Vec::new();
    PngEncoder::new(&mut bytes)
        .write_image(&rgba, rgba.width(), rgba.height(), image::ExtendedColorType::Rgba8)
        .map_err(|error| format!("FitSend could not encode a transparent PNG candidate: {error}"))?;
    candidate_from_bytes(reference, bytes, "png", None)
}

fn candidate_from_bytes(
    reference: &DynamicImage,
    bytes: Vec<u8>,
    extension: &'static str,
    encoded_quality: Option<u8>,
) -> Result<ImageCandidate, String> {
    let decoded = image::load_from_memory(&bytes)
        .map_err(|error| format!("FitSend could not verify an image candidate: {error}"))?;
    let quality_score = image_ssim(reference, &decoded)?;
    Ok(ImageCandidate {
        bytes,
        extension,
        width: decoded.width(),
        height: decoded.height(),
        quality_score,
        encoded_quality,
    })
}

fn scaled_image(source: &DynamicImage, scale: f32) -> DynamicImage {
    if (scale - 1.0).abs() < f32::EPSILON {
        return source.clone();
    }
    let width = ((source.width() as f32 * scale).round() as u32).max(1);
    let height = ((source.height() as f32 * scale).round() as u32).max(1);
    source.resize_exact(width, height, FilterType::Lanczos3)
}

fn report_candidate(
    callback: &mut dyn FnMut(ProcessProgress) -> bool,
    index: usize,
    total: usize,
    stage: &str,
) -> Result<(), String> {
    let fraction = index as f64 / total.max(1) as f64;
    report(callback, 15 + (fraction * 72.0).round() as u8, stage, None, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb, Rgba};

    fn noisy_rgb_fixture(width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([
                ((x * 31 + y * 17) % 256) as u8,
                ((x * 7 + y * 29) % 256) as u8,
                ((x + y * 3) % 256) as u8,
            ])
        }))
    }

    #[test]
    fn precise_uses_quality_100_when_it_fits() {
        let source = noisy_rgb_fixture(320, 320);
        let q100 = jpeg_candidate(&source, &source, 100).unwrap();
        let target = q100.bytes.len() as u64 + 1024;
        let decision = choose_image_candidate(
            &source,
            target + 100_000,
            target,
            CompressionStrategy::Precise,
            &mut |_| true,
        )
        .unwrap();
        let ImageDecision::Created(candidate) = decision else {
            panic!("expected output")
        };
        assert_eq!(candidate.encoded_quality, Some(100));
        assert_eq!((candidate.width, candidate.height), (320, 320));
    }

    #[test]
    fn balanced_keeps_source_when_savings_are_under_ten_percent() {
        let source = noisy_rgb_fixture(160, 120);
        let decision = choose_image_candidate(
            &source,
            100,
            1_000_000,
            CompressionStrategy::Balanced,
            &mut |_| true,
        )
        .unwrap();
        assert!(matches!(decision, ImageDecision::NoChange(_)));
    }

    #[test]
    fn transparent_input_never_becomes_jpeg() {
        let source = DynamicImage::ImageRgba8(ImageBuffer::from_fn(256, 256, |x, y| {
            Rgba([x as u8, y as u8, 160, ((x + y) % 256) as u8])
        }));
        let decision = choose_image_candidate(
            &source,
            500_000,
            400_000,
            CompressionStrategy::Smallest,
            &mut |_| true,
        )
        .unwrap();
        let ImageDecision::Created(candidate) = decision else {
            panic!("expected output")
        };
        assert_eq!(candidate.extension, "png");
        assert!(image::load_from_memory(&candidate.bytes).unwrap().color().has_alpha());
    }
}
