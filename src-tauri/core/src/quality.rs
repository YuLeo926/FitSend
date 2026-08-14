use image::{imageops::FilterType, DynamicImage, GenericImageView, GrayImage};

pub const BALANCED_IMAGE_SSIM: f64 = 0.990;
pub const SMALLEST_IMAGE_SSIM: f64 = 0.965;
pub const BALANCED_VIDEO_MEAN_SSIM: f64 = 0.985;
pub const BALANCED_VIDEO_MIN_SSIM: f64 = 0.970;
pub const SMALLEST_VIDEO_MEAN_SSIM: f64 = 0.950;
pub const SMALLEST_VIDEO_MIN_SSIM: f64 = 0.920;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoSimilarity {
    pub mean: f64,
    pub minimum: f64,
}

pub fn image_ssim(reference: &DynamicImage, candidate: &DynamicImage) -> Result<f64, String> {
    let (width, height) = reference.dimensions();
    if width == 0 || height == 0 {
        return Err("FitSend cannot compare an empty image.".to_string());
    }

    let reference = reference.to_luma8();
    let candidate = if candidate.dimensions() == (width, height) {
        candidate.to_luma8()
    } else {
        image::imageops::resize(&candidate.to_luma8(), width, height, FilterType::Lanczos3)
    };

    let mut total = 0.0;
    let mut windows = 0_u64;
    for y in (0..height).step_by(8) {
        for x in (0..width).step_by(8) {
            total += window_ssim(&reference, &candidate, x, y);
            windows += 1;
        }
    }

    if windows == 0 {
        return Err("FitSend could not sample this image for comparison.".to_string());
    }
    Ok((total / windows as f64).clamp(0.0, 1.0))
}

fn window_ssim(reference: &GrayImage, candidate: &GrayImage, start_x: u32, start_y: u32) -> f64 {
    let end_x = (start_x + 8).min(reference.width());
    let end_y = (start_y + 8).min(reference.height());
    let count = ((end_x - start_x) * (end_y - start_y)).max(1) as f64;

    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    for y in start_y..end_y {
        for x in start_x..end_x {
            sum_x += reference.get_pixel(x, y)[0] as f64;
            sum_y += candidate.get_pixel(x, y)[0] as f64;
        }
    }
    let mean_x = sum_x / count;
    let mean_y = sum_y / count;

    let mut variance_x = 0.0;
    let mut variance_y = 0.0;
    let mut covariance = 0.0;
    for y in start_y..end_y {
        for x in start_x..end_x {
            let x_value = reference.get_pixel(x, y)[0] as f64 - mean_x;
            let y_value = candidate.get_pixel(x, y)[0] as f64 - mean_y;
            variance_x += x_value * x_value;
            variance_y += y_value * y_value;
            covariance += x_value * y_value;
        }
    }
    let divisor = (count - 1.0).max(1.0);
    variance_x /= divisor;
    variance_y /= divisor;
    covariance /= divisor;

    let c1 = (0.01_f64 * 255.0).powi(2);
    let c2 = (0.03_f64 * 255.0).powi(2);
    ((2.0 * mean_x * mean_y + c1) * (2.0 * covariance + c2))
        / ((mean_x.powi(2) + mean_y.powi(2) + c1) * (variance_x + variance_y + c2))
}

pub fn parse_ffmpeg_ssim_stats(stats: &str) -> Result<VideoSimilarity, String> {
    let scores: Vec<f64> = stats
        .split_whitespace()
        .filter_map(|token| token.strip_prefix("All:"))
        .filter_map(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .collect();
    if scores.is_empty() {
        return Err("FitSend could not read video similarity measurements.".to_string());
    }
    let mean = scores.iter().sum::<f64>() / scores.len() as f64;
    let minimum = scores.iter().copied().fold(f64::INFINITY, f64::min);
    Ok(VideoSimilarity { mean, minimum })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    #[test]
    fn identical_images_score_one() {
        let image = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(32, 32, Rgb([80, 120, 160])));
        assert!((image_ssim(&image, &image).unwrap() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn visibly_different_images_score_lower() {
        let reference = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(32, 32, Rgb([0, 0, 0])));
        let candidate = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(32, 32, Rgb([255, 255, 255])));
        assert!(image_ssim(&reference, &candidate).unwrap() < 0.1);
    }

    #[test]
    fn resized_comparison_uses_reference_dimensions() {
        let reference = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(32, 32, Rgb([90, 90, 90])));
        let candidate = DynamicImage::ImageRgb8(ImageBuffer::from_pixel(16, 16, Rgb([90, 90, 90])));
        assert!(image_ssim(&reference, &candidate).unwrap() > 0.999);
    }

    #[test]
    fn parses_mean_and_minimum_video_scores() {
        let stats = "n:1 All:0.992\nn:2 All:0.971\nn:3 All:0.986\n";
        let score = parse_ffmpeg_ssim_stats(stats).unwrap();
        assert!((score.mean - 0.983).abs() < 0.001);
        assert_eq!(score.minimum, 0.971);
    }
}
