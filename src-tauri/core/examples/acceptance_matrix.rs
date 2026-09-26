use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use fitsend_core::{
    analyze, build, build_budget, process, process_with_progress, rebalance_budget,
    AcceptedBudgetItem, BatchBudget, BatchBudgetRequest, BudgetItemRequest, CompressionStrategy,
    ItemAllocation, LimitScope, MediaAnalysis, MediaKind, PlanRequest, ProcessOutcome,
    ProcessRequest, ProcessResult, MIN_ITEM_BUDGET_BYTES, PROCESS_CANCELLED,
};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CaseResult {
    name: String,
    category: String,
    strategy: Option<CompressionStrategy>,
    outcome: Option<ProcessOutcome>,
    passed: bool,
    expected: String,
    detail: String,
    input_bytes: Option<u64>,
    output_bytes: Option<u64>,
    target_bytes: Option<u64>,
    duration_ms: Option<u64>,
    quality_score: Option<f64>,
    width: Option<u32>,
    height: Option<u32>,
    batch: Option<BatchCaseMetrics>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchCaseMetrics {
    scope: LimitScope,
    ceiling_bytes: u64,
    allocation_bytes: Vec<u64>,
    accepted_files: usize,
    attention_files: usize,
    accepted_bytes: u64,
}

struct AggregateCaseResult {
    results: Vec<Result<ProcessResult, String>>,
    allocations: Vec<ItemAllocation>,
    accepted_bytes: u64,
    ceiling_bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AcceptanceReport {
    generated_at_unix_seconds: u64,
    passed: usize,
    failed: usize,
    total: usize,
    cases: Vec<CaseResult>,
}

fn main() {
    let report_root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../output/acceptance"));
    let run_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let run_directory = report_root.join(format!("run-{run_id}"));
    let fixtures = run_directory.join("fixtures");
    let outputs = run_directory.join("outputs");
    fs::create_dir_all(&fixtures).expect("acceptance fixtures directory should be created");
    fs::create_dir_all(&outputs).expect("acceptance outputs directory should be created");

    let mut cases = Vec::new();
    run_image_cases(&fixtures, &outputs, &mut cases);
    run_video_cases(&fixtures, &outputs, &mut cases);
    run_strategy_matrix(&fixtures, &outputs, &mut cases);
    run_failure_cases(&fixtures, &outputs, &mut cases);
    run_batch_cases(&fixtures, &outputs, &mut cases);
    run_fitting_original_cases(&fixtures, &outputs, &mut cases);

    let passed = cases.iter().filter(|case| case.passed).count();
    let report = AcceptanceReport {
        generated_at_unix_seconds: run_id,
        passed,
        failed: cases.len() - passed,
        total: cases.len(),
        cases,
    };
    let json = serde_json::to_string_pretty(&report).expect("acceptance report should serialize");
    fs::create_dir_all(&report_root).expect("acceptance report directory should be created");
    fs::write(report_root.join("latest.json"), json).expect("JSON report should be written");
    fs::write(report_root.join("latest.md"), markdown_report(&report))
        .expect("Markdown report should be written");

    println!(
        "FitSend acceptance matrix: {}/{} passed. Report: {}",
        report.passed,
        report.total,
        report_root.join("latest.md").display()
    );
    if report.failed > 0 {
        std::process::exit(1);
    }
}

fn run_fitting_original_cases(fixtures: &Path, outputs: &Path, cases: &mut Vec<CaseResult>) {
    let tiny = fixtures.join("review-tiny-unpadded.png");
    image::RgbaImage::from_pixel(16, 16, Rgba([20, 40, 80, 180]))
        .save(&tiny)
        .unwrap();
    assert!(fs::metadata(&tiny).unwrap().len() < 8192);
    let small = fixtures.join("review-small-unpadded.png");
    // Pick an actual encoded image between the planner's former 8 and 12 KiB floors.
    for size in 40..80 {
        generate_image(&small, size, size, true, 31).unwrap();
        if (8192..12288).contains(&fs::metadata(&small).unwrap().len()) {
            break;
        }
    }
    assert!((8192..12288).contains(&fs::metadata(&small).unwrap().len()));
    let low = fixtures.join("review-low-bitrate.mp4");
    let ffmpeg = std::env::var_os("FITSEND_FFMPEG_PATH").unwrap_or_else(|| "ffmpeg".into());
    let status = Command::new(ffmpeg)
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=black:size=64x64:rate=10:duration=60",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&low)
        .status()
        .unwrap();
    assert!(status.success());
    let low_analysis = analyze(low.to_str().unwrap()).unwrap();
    assert!(low_analysis.size_bytes >= 8192);
    assert!((low_analysis.size_bytes as f64 * 8.0 / 60.0 / 1000.0) < 180.0);
    for (label, input) in [
        ("tiny-unpadded-image", tiny),
        ("small-unpadded-image", small),
        ("low-bitrate-video", low),
    ] {
        let modified = fs::metadata(&input).unwrap().modified().unwrap();
        for strategy in [
            CompressionStrategy::Precise,
            CompressionStrategy::Balanced,
            CompressionStrategy::Smallest,
        ] {
            let name = format!("review-{label}-{}", strategy_slug(strategy));
            let result = run_aggregate_batch(
                std::slice::from_ref(&input),
                &outputs.join(&name),
                24 * 1024 * 1024,
                strategy,
            );
            let passed = result.results.iter().all(|entry| {
                entry.as_ref().is_ok_and(|value| {
                    value.verified
                        && value.output_bytes == fs::metadata(&value.output_path).unwrap().len()
                        && value.output_bytes <= result.allocations[0].target_bytes
                })
            }) && fs::metadata(&input).unwrap().modified().unwrap() == modified;
            cases.push(batch_case(&name, passed, "source-capped allocation builds a feasible plan and yields measured valid media without touching the original",
                format!("results={:?}; sourceModificationTimeUnchanged=true", result.results), strategy, LimitScope::BatchTotal, &result, 0));
        }
    }
}

fn run_image_cases(fixtures: &Path, outputs: &Path, cases: &mut Vec<CaseResult>) {
    let specifications = [
        (
            "image-landscape-jpeg",
            "landscape.jpg",
            1600,
            900,
            false,
            150,
        ),
        (
            "image-portrait-alpha",
            "portrait-alpha.png",
            720,
            1280,
            true,
            1000,
        ),
        ("image-square-png", "square.png", 1000, 1000, false, 160),
        ("image-unicode-path", "旅行 照片.png", 1280, 720, true, 1000),
        (
            "image-spaces-path",
            "team screenshot wide.jpg",
            1800,
            800,
            false,
            130,
        ),
        ("image-wide", "extra-wide.png", 1800, 500, false, 85),
        ("image-tall", "extra-tall.png", 500, 1800, false, 85),
        (
            "image-small-target",
            "small-target.png",
            800,
            600,
            false,
            160,
        ),
        ("image-alpha-dark", "alpha dark.png", 1100, 700, true, 1500),
        (
            "image-large-noise",
            "large-noise.jpg",
            2200,
            1400,
            false,
            170,
        ),
        ("image-compact", "compact.png", 640, 480, false, 200),
    ];

    for (index, (name, filename, width, height, alpha, target_kb)) in
        specifications.into_iter().enumerate()
    {
        let input = fixtures.join(filename);
        let output_extension = if alpha { "png" } else { "jpg" };
        let output = outputs.join(format!("{name}.{output_extension}"));
        if let Err(error) = generate_image(&input, width, height, alpha, index as u32) {
            cases.push(failed_generation(name, "image", error));
            continue;
        }
        cases.push(run_success_case(
            name,
            "image",
            &input,
            &output,
            target_kb * 1024,
            MediaKind::Image,
            CompressionStrategy::Precise,
            Some(false),
        ));
    }

    let input = fixtures.join("already-fits.jpg");
    let output = outputs.join("image-already-fits.jpg");
    if let Err(error) = generate_image(&input, 240, 160, false, 91) {
        cases.push(failed_generation("image-already-fits", "image", error));
    } else {
        let size = fs::metadata(&input)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        cases.push(run_success_case(
            "image-already-fits",
            "image",
            &input,
            &output,
            size + 4096,
            MediaKind::Image,
            CompressionStrategy::Precise,
            Some(true),
        ));
    }
}

fn run_video_cases(fixtures: &Path, outputs: &Path, cases: &mut Vec<CaseResult>) {
    if !fitsend_core::video_tools_available() {
        cases.push(failed_generation(
            "video-toolchain",
            "video",
            "FFmpeg or FFprobe is unavailable".to_string(),
        ));
        return;
    }

    let specifications = [
        (
            "video-landscape-audio",
            "landscape-audio.mp4",
            1280,
            720,
            3.0,
            true,
            520,
        ),
        (
            "video-portrait-audio",
            "portrait-audio.mp4",
            720,
            1280,
            2.5,
            true,
            430,
        ),
        (
            "video-landscape-silent",
            "landscape-silent.mp4",
            1280,
            720,
            4.0,
            false,
            430,
        ),
        (
            "video-unicode-path",
            "演示 视频.mp4",
            960,
            540,
            2.0,
            true,
            360,
        ),
        (
            "video-spaces-path",
            "screen recording sample.mp4",
            1024,
            576,
            3.5,
            true,
            500,
        ),
        (
            "video-aggressive",
            "aggressive.mp4",
            1280,
            720,
            4.0,
            true,
            190,
        ),
        (
            "video-1080p-scale",
            "full-hd.mp4",
            1920,
            1080,
            2.0,
            true,
            390,
        ),
        (
            "video-webm-input",
            "browser-capture.webm",
            960,
            540,
            2.0,
            true,
            360,
        ),
        (
            "video-mkv-input",
            "camera-capture.mkv",
            960,
            540,
            2.0,
            false,
            320,
        ),
    ];

    for (name, filename, width, height, duration, audio, target_kb) in specifications {
        let input = fixtures.join(filename);
        let output = outputs.join(format!("{name}.mp4"));
        if let Err(error) = generate_video(&input, width, height, duration, audio) {
            cases.push(failed_generation(name, "video", error));
            continue;
        }
        cases.push(run_success_case(
            name,
            "video",
            &input,
            &output,
            target_kb * 1024,
            MediaKind::Video,
            CompressionStrategy::Precise,
            Some(false),
        ));
    }

    let input = fixtures.join("video-already-fits.mp4");
    let output = outputs.join("video-already-fits.mp4");
    match generate_video(&input, 640, 360, 1.0, true) {
        Ok(()) => {
            let size = fs::metadata(&input)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            cases.push(run_success_case(
                "video-already-fits",
                "video",
                &input,
                &output,
                size + 4096,
                MediaKind::Video,
                CompressionStrategy::Precise,
                Some(true),
            ));
        }
        Err(error) => cases.push(failed_generation("video-already-fits", "video", error)),
    }
}

fn run_strategy_matrix(fixtures: &Path, outputs: &Path, cases: &mut Vec<CaseResult>) {
    let regression_input = fixtures.join("regression-1254.png");
    if let Err(error) = generate_image(&regression_input, 1254, 1254, false, 211) {
        cases.push(failed_generation(
            "image-1254-precise-regression",
            "image",
            error,
        ));
    } else {
        let mut result = run_success_case(
            "image-1254-precise-regression",
            "image",
            &regression_input,
            &outputs.join("image-1254-precise-regression.jpg"),
            2 * 1024 * 1024,
            MediaKind::Image,
            CompressionStrategy::Precise,
            Some(false),
        );
        result.passed &= result.width == Some(1254) && result.height == Some(1254);
        if result.width != Some(1254) || result.height != Some(1254) {
            result
                .detail
                .push_str("; expected the highest-quality full-resolution candidate");
        }
        cases.push(result);
    }

    let image_input = fixtures.join("strategy-matrix-image.png");
    if let Err(error) = generate_image(&image_input, 1200, 800, false, 233) {
        cases.push(failed_generation("image-strategy-matrix", "image", error));
    } else {
        let image_size = fs::metadata(&image_input)
            .map(|value| value.len())
            .unwrap_or(0);
        let targets = [
            ("below", image_size + 4096),
            ("slightly-over", image_size.saturating_mul(90) / 100),
            (
                "far-over",
                (image_size.saturating_mul(20) / 100).max(80 * 1024),
            ),
        ];
        for strategy in [
            CompressionStrategy::Precise,
            CompressionStrategy::Balanced,
            CompressionStrategy::Smallest,
        ] {
            for (scenario, target) in targets {
                let slug = strategy_slug(strategy);
                let name = format!("image-{slug}-{scenario}");
                let output = outputs.join(format!("image-{slug}-{scenario}.jpg"));
                if scenario == "far-over" && strategy != CompressionStrategy::Precise {
                    cases.push(run_expected_processing_failure(
                        &name,
                        &image_input,
                        &output,
                        target,
                        strategy,
                        "quality floor",
                    ));
                } else {
                    cases.push(run_success_case(
                        &name,
                        "image-strategy",
                        &image_input,
                        &output,
                        target,
                        MediaKind::Image,
                        strategy,
                        if strategy == CompressionStrategy::Precise && scenario == "below" {
                            Some(true)
                        } else {
                            None
                        },
                    ));
                }
            }
            cases.push(run_expected_infeasible_plan(
                &format!("image-{}-impossible", strategy_slug(strategy)),
                &image_input,
                MediaKind::Image,
                strategy,
                10 * 1024,
            ));
        }

        let collision_output = outputs.join("collision-output.jpg");
        let marker = b"existing output must remain";
        fs::write(&collision_output, marker).unwrap();
        let mut collision = run_success_case(
            "output-name-collision",
            "output-safety",
            &image_input,
            &collision_output,
            (image_size.saturating_mul(35) / 100).max(120 * 1024),
            MediaKind::Image,
            CompressionStrategy::Precise,
            Some(false),
        );
        let numbered_output = outputs.join("collision-output-2.jpg");
        collision.passed &= fs::read(&collision_output).ok().as_deref() == Some(marker)
            && numbered_output.is_file();
        collision.detail.push_str(&format!(
            "; originalOutputPreserved={}; numberedOutputExists={}",
            fs::read(&collision_output).ok().as_deref() == Some(marker),
            numbered_output.is_file()
        ));
        cases.push(collision);

        let blocked_parent = outputs.join("blocked-output-parent");
        fs::write(&blocked_parent, b"not a directory").unwrap();
        let source_before = fs::read(&image_input).unwrap();
        let analysis = analyze(image_input.to_string_lossy().as_ref()).unwrap();
        let failed_output = blocked_parent.join("result.jpg");
        let result = process(&ProcessRequest {
            analysis,
            target_bytes: (image_size.saturating_mul(35) / 100).max(120 * 1024),
            output_path: failed_output.to_string_lossy().to_string(),
            strategy: CompressionStrategy::Precise,
        });
        let source_unchanged = fs::read(&image_input).ok().as_deref() == Some(&source_before);
        let no_working_files = !contains_working_files(outputs);
        cases.push(CaseResult {
            name: "output-write-failure-cleans-up".to_string(),
            category: "output-safety".to_string(),
            strategy: Some(CompressionStrategy::Precise),
            outcome: None,
            passed: result.is_err()
                && source_unchanged
                && no_working_files
                && !failed_output.exists(),
            expected: "write failure preserves the source and leaves no temporary output"
                .to_string(),
            detail: format!(
                "error={}; sourceUnchanged={source_unchanged}; noWorkingFiles={no_working_files}",
                result.err().unwrap_or_else(|| "none".to_string())
            ),
            input_bytes: Some(image_size),
            output_bytes: None,
            target_bytes: None,
            duration_ms: None,
            quality_score: None,
            width: None,
            height: None,
            batch: None,
        });
    }

    if !fitsend_core::video_tools_available() {
        return;
    }
    let video_input = fixtures.join("strategy-matrix-video.mp4");
    if let Err(error) = generate_video(&video_input, 960, 540, 1.5, true) {
        cases.push(failed_generation("video-strategy-matrix", "video", error));
        return;
    }
    let video_size = fs::metadata(&video_input)
        .map(|value| value.len())
        .unwrap_or(0);
    let targets = [
        ("below", video_size + 4096),
        ("slightly-over", video_size.saturating_mul(85) / 100),
        (
            "far-over",
            (video_size.saturating_mul(35) / 100).max(220 * 1024),
        ),
    ];
    for strategy in [
        CompressionStrategy::Precise,
        CompressionStrategy::Balanced,
        CompressionStrategy::Smallest,
    ] {
        for (scenario, target) in targets {
            let slug = strategy_slug(strategy);
            cases.push(run_success_case(
                &format!("video-{slug}-{scenario}"),
                "video-strategy",
                &video_input,
                &outputs.join(format!("video-{slug}-{scenario}.mp4")),
                target,
                MediaKind::Video,
                strategy,
                if strategy == CompressionStrategy::Precise && scenario == "below" {
                    Some(true)
                } else {
                    None
                },
            ));
        }
        cases.push(run_expected_infeasible_plan(
            &format!("video-{}-impossible", strategy_slug(strategy)),
            &video_input,
            MediaKind::Video,
            strategy,
            50 * 1024,
        ));
    }

    let rotated_input = fixtures.join("rotated-portrait.mp4");
    match generate_rotated_video(&rotated_input) {
        Ok(()) => {
            let rotated_size = fs::metadata(&rotated_input)
                .map(|value| value.len())
                .unwrap_or(0);
            cases.push(run_success_case(
                "video-rotated-metadata",
                "video-metadata",
                &rotated_input,
                &outputs.join("video-rotated-metadata.mp4"),
                (rotated_size.saturating_mul(70) / 100).max(260 * 1024),
                MediaKind::Video,
                CompressionStrategy::Precise,
                Some(false),
            ));
        }
        Err(error) => cases.push(failed_generation(
            "video-rotated-metadata",
            "video-metadata",
            error,
        )),
    }
}

fn run_expected_infeasible_plan(
    name: &str,
    input: &Path,
    kind: MediaKind,
    strategy: CompressionStrategy,
    target_bytes: u64,
) -> CaseResult {
    let analysis = analyze(input.to_string_lossy().as_ref());
    let plan = analysis.and_then(|analysis| {
        build(&PlanRequest {
            analysis,
            target_bytes,
            strategy,
        })
    });
    let (passed, detail) = match plan {
        Ok(plan) => (!plan.feasible, format!("feasible={}", plan.feasible)),
        Err(error) => (false, error),
    };
    CaseResult {
        name: name.to_string(),
        category: format!(
            "{}-strategy",
            match kind {
                MediaKind::Image => "image",
                MediaKind::Video => "video",
            }
        ),
        strategy: Some(strategy),
        outcome: None,
        passed,
        expected: "planner rejects the impossible target".to_string(),
        detail,
        input_bytes: fs::metadata(input).ok().map(|value| value.len()),
        output_bytes: None,
        target_bytes: Some(target_bytes),
        duration_ms: None,
        quality_score: None,
        width: None,
        height: None,
        batch: None,
    }
}

fn run_expected_processing_failure(
    name: &str,
    input: &Path,
    output: &Path,
    target_bytes: u64,
    strategy: CompressionStrategy,
    expected_text: &str,
) -> CaseResult {
    let source_before = fs::read(input).unwrap_or_default();
    let analysis = analyze(input.to_string_lossy().as_ref());
    let result = analysis.and_then(|analysis| {
        process(&ProcessRequest {
            analysis,
            target_bytes,
            output_path: output.to_string_lossy().to_string(),
            strategy,
        })
    });
    let detail = result
        .as_ref()
        .err()
        .cloned()
        .unwrap_or_else(|| "processing unexpectedly succeeded".to_string());
    let source_unchanged = fs::read(input).ok().as_deref() == Some(source_before.as_slice());
    let clean = !output.exists()
        && !contains_working_files(output.parent().unwrap_or_else(|| Path::new(".")));
    CaseResult {
        name: name.to_string(),
        category: "image-strategy".to_string(),
        strategy: Some(strategy),
        outcome: None,
        passed: result.is_err()
            && detail.to_ascii_lowercase().contains(expected_text)
            && source_unchanged
            && clean,
        expected: format!("processing rejects a result below the {expected_text}"),
        detail: format!("{detail}; sourceUnchanged={source_unchanged}; clean={clean}"),
        input_bytes: Some(source_before.len() as u64),
        output_bytes: None,
        target_bytes: Some(target_bytes),
        duration_ms: None,
        quality_score: None,
        width: None,
        height: None,
        batch: None,
    }
}

fn strategy_slug(strategy: CompressionStrategy) -> &'static str {
    match strategy {
        CompressionStrategy::Precise => "precise",
        CompressionStrategy::Balanced => "balanced",
        CompressionStrategy::Smallest => "smallest",
    }
}

fn contains_working_files(path: &Path) -> bool {
    fs::read_dir(path).ok().is_some_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains(".fitsend-working-")
        })
    })
}

fn contains_processing_residue(outputs: &Path) -> bool {
    if contains_working_files(outputs) {
        return true;
    }
    fs::read_dir(std::env::temp_dir())
        .ok()
        .is_some_and(|entries| {
            entries.filter_map(Result::ok).any(|entry| {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                name.starts_with("fitsend-pass-")
                    || name.contains(".fitsend-working-")
                    || name.starts_with("fitsend-ssim-")
            })
        })
}

fn run_failure_cases(fixtures: &Path, outputs: &Path, cases: &mut Vec<CaseResult>) {
    let unsupported = fixtures.join("notes.txt");
    fs::write(&unsupported, b"not media").unwrap();
    cases.push(run_expected_analysis_failure(
        "failure-unsupported-extension",
        &unsupported,
        "Unsupported file type",
    ));

    let corrupt = fixtures.join("corrupt.mp4");
    fs::write(&corrupt, b"not actually an mp4").unwrap();
    cases.push(run_expected_analysis_failure(
        "failure-corrupt-video",
        &corrupt,
        "could not be analyzed",
    ));

    let image = fixtures.join("infeasible-image.png");
    generate_image(&image, 1200, 800, false, 77).unwrap();
    let image_analysis = analyze(image.to_string_lossy().as_ref()).unwrap();
    let image_plan = build(&PlanRequest {
        analysis: image_analysis,
        target_bytes: 10 * 1024,
        strategy: CompressionStrategy::Precise,
    })
    .unwrap();
    cases.push(CaseResult {
        name: "failure-infeasible-image".to_string(),
        category: "failure".to_string(),
        strategy: Some(CompressionStrategy::Precise),
        outcome: None,
        passed: !image_plan.feasible,
        expected: "planner rejects unusable image target".to_string(),
        detail: format!("feasible={}", image_plan.feasible),
        input_bytes: None,
        output_bytes: None,
        target_bytes: Some(10 * 1024),
        duration_ms: None,
        quality_score: None,
        width: None,
        height: None,
        batch: None,
    });

    let video = fixtures.join("infeasible-video.mp4");
    if let Err(error) = generate_video(&video, 960, 540, 4.0, true) {
        cases.push(failed_generation(
            "failure-infeasible-video",
            "failure",
            error,
        ));
    } else {
        let video_analysis = analyze(video.to_string_lossy().as_ref()).unwrap();
        let video_plan = build(&PlanRequest {
            analysis: video_analysis,
            target_bytes: 80 * 1024,
            strategy: CompressionStrategy::Precise,
        })
        .unwrap();
        cases.push(CaseResult {
            name: "failure-infeasible-video".to_string(),
            category: "failure".to_string(),
            strategy: Some(CompressionStrategy::Precise),
            outcome: None,
            passed: !video_plan.feasible,
            expected: "planner rejects unusable video target".to_string(),
            detail: format!("feasible={}", video_plan.feasible),
            input_bytes: None,
            output_bytes: None,
            target_bytes: Some(80 * 1024),
            duration_ms: None,
            quality_score: None,
            width: None,
            height: None,
            batch: None,
        });
    }

    let cancelled_output = outputs.join("cancelled-must-not-exist.jpg");
    let cancellation_source_before = fs::read(&image).unwrap();
    let cancellation_analysis = analyze(image.to_string_lossy().as_ref()).unwrap();
    let cancellation_result = process_with_progress(
        &ProcessRequest {
            analysis: cancellation_analysis,
            target_bytes: 100 * 1024,
            output_path: cancelled_output.to_string_lossy().to_string(),
            strategy: CompressionStrategy::Precise,
        },
        |progress| progress.stage != "Finding the highest image quality",
    );
    let cancellation_error = cancellation_result.err();
    let cancellation_source_unchanged =
        fs::read(&image).ok().as_deref() == Some(cancellation_source_before.as_slice());
    let cancellation_clean = !contains_processing_residue(outputs);
    cases.push(CaseResult {
        name: "cancellation-removes-partial-output".to_string(),
        category: "failure".to_string(),
        strategy: Some(CompressionStrategy::Precise),
        outcome: None,
        passed: cancellation_error.as_deref() == Some(PROCESS_CANCELLED)
            && !cancelled_output.exists()
            && cancellation_source_unchanged
            && cancellation_clean,
        expected: "cancel returns the cancellation code and leaves no output".to_string(),
        detail: format!(
            "error={}; outputExists={}; sourceUnchanged={cancellation_source_unchanged}; clean={cancellation_clean}",
            cancellation_error.as_deref().unwrap_or("none"),
            cancelled_output.exists()
        ),
        input_bytes: None,
        output_bytes: None,
        target_bytes: None,
        duration_ms: None,
        quality_score: None,
        width: None,
        height: None,
        batch: None,
    });

    if video.exists() {
        let cancelled_video_output = outputs.join("cancelled-video-must-not-exist.mp4");
        let video_source_before = fs::read(&video).unwrap();
        let video_analysis = analyze(video.to_string_lossy().as_ref()).unwrap();
        let video_cancel_result = process_with_progress(
            &ProcessRequest {
                analysis: video_analysis,
                target_bytes: 300 * 1024,
                output_path: cancelled_video_output.to_string_lossy().to_string(),
                strategy: CompressionStrategy::Precise,
            },
            |progress| progress.stage != "Encoding pass 1 of 2",
        );
        let error = video_cancel_result.err();
        let source_unchanged =
            fs::read(&video).ok().as_deref() == Some(video_source_before.as_slice());
        let clean = !contains_processing_residue(outputs);
        cases.push(CaseResult {
            name: "video-cancellation-cleans-up".to_string(),
            category: "failure".to_string(),
            strategy: Some(CompressionStrategy::Precise),
            outcome: None,
            passed: error.as_deref() == Some(PROCESS_CANCELLED)
                && !cancelled_video_output.exists()
                && source_unchanged
                && clean,
            expected: "video cancellation leaves the source intact and no processing residue"
                .to_string(),
            detail: format!(
                "error={}; outputExists={}; sourceUnchanged={source_unchanged}; clean={clean}",
                error.as_deref().unwrap_or("none"),
                cancelled_video_output.exists()
            ),
            input_bytes: Some(video_source_before.len() as u64),
            output_bytes: None,
            target_bytes: Some(300 * 1024),
            duration_ms: None,
            quality_score: None,
            width: None,
            height: None,
            batch: None,
        });
    }
}

fn contains_nonempty_working_file(path: &Path) -> bool {
    fs::read_dir(path).ok().is_some_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains(".fitsend-working-")
                && entry.metadata().is_ok_and(|metadata| metadata.len() > 0)
        })
    })
}

fn run_aggregate_batch(
    inputs: &[PathBuf],
    outputs: &Path,
    ceiling_bytes: u64,
    strategy: CompressionStrategy,
) -> AggregateCaseResult {
    fs::create_dir_all(outputs).expect("aggregate output directory should be created");
    let original_hashes = inputs
        .iter()
        .map(|path| file_hash(path).expect("aggregate input should be readable"))
        .collect::<Vec<_>>();
    let analyses = inputs
        .iter()
        .map(|path| analyze(path.to_str().unwrap()).unwrap())
        .collect::<Vec<_>>();
    let ids = (0..analyses.len())
        .map(|index| format!("aggregate-{index}"))
        .collect::<Vec<_>>();
    let mut accepted: Vec<AcceptedBudgetItem> = Vec::new();
    let mut allocations =
        build_budget(&budget_request(&ids, &analyses, ceiling_bytes, &accepted)).unwrap();
    assert!(
        allocations.feasible,
        "aggregate fixture budget should be feasible"
    );
    let mut applied_allocations = Vec::new();
    let mut results = Vec::new();
    for (index, analysis) in analyses.iter().enumerate() {
        let id = &ids[index];
        let target = allocations
            .allocations
            .iter()
            .find(|entry| &entry.id == id)
            .unwrap()
            .target_bytes;
        applied_allocations.push(ItemAllocation {
            id: id.clone(),
            target_bytes: target,
        });
        let result = build(&PlanRequest {
            analysis: analysis.clone(),
            target_bytes: target,
            strategy,
        })
        .and_then(|plan| {
            if !plan.feasible {
                return Err(plan.warnings.join(" "));
            }
            process(&ProcessRequest {
                output_path: aggregate_output_path(outputs, id, analysis)
                    .to_string_lossy()
                    .to_string(),
                analysis: analysis.clone(),
                target_bytes: target,
                strategy,
            })
        });
        if let Ok(ref completed) = result {
            assert!(
                analyze(&completed.output_path).is_ok(),
                "every accepted aggregate output should open"
            );
            accepted.push(AcceptedBudgetItem {
                id: id.clone(),
                actual_bytes: completed.output_bytes,
            });
        }
        results.push(result);
        if index + 1 < analyses.len() {
            let request = remaining_budget_request(
                &ids,
                &analyses,
                &accepted,
                index + 1,
                ceiling_bytes,
                &allocations,
            );
            allocations = rebalance_budget(&request).unwrap();
            assert!(
                allocations.feasible,
                "forward aggregate fixture budget should remain feasible"
            );
        }
    }
    for (path, before) in inputs.iter().zip(original_hashes) {
        assert_eq!(
            file_hash(path).expect("aggregate input should remain readable"),
            before,
            "aggregate processing must not alter originals"
        );
    }
    AggregateCaseResult {
        accepted_bytes: accepted.iter().map(|entry| entry.actual_bytes).sum(),
        allocations: applied_allocations,
        results,
        ceiling_bytes,
    }
}

fn aggregate_output_path(outputs: &Path, id: &str, analysis: &MediaAnalysis) -> PathBuf {
    let extension = match &analysis.kind {
        MediaKind::Video => "mp4",
        MediaKind::Image if analysis.has_alpha => "png",
        MediaKind::Image => "jpg",
    };
    outputs.join(format!("{id}.{extension}"))
}

fn budget_request(
    ids: &[String],
    analyses: &[MediaAnalysis],
    ceiling_bytes: u64,
    accepted: &[AcceptedBudgetItem],
) -> BatchBudgetRequest {
    BatchBudgetRequest {
        scope: LimitScope::BatchTotal,
        ceiling_bytes,
        items: ids
            .iter()
            .zip(analyses)
            .map(|(id, analysis)| BudgetItemRequest {
                id: id.clone(),
                source_bytes: analysis.size_bytes,
                minimum_allocation_bytes: None,
            })
            .collect(),
        accepted: accepted.to_vec(),
    }
}

fn remaining_budget_request(
    ids: &[String],
    analyses: &[MediaAnalysis],
    accepted: &[AcceptedBudgetItem],
    start_index: usize,
    ceiling_bytes: u64,
    previous: &BatchBudget,
) -> BatchBudgetRequest {
    BatchBudgetRequest {
        scope: LimitScope::BatchTotal,
        ceiling_bytes,
        items: ids
            .iter()
            .zip(analyses)
            .enumerate()
            .skip(start_index)
            .map(|(_, (id, analysis))| BudgetItemRequest {
                id: id.clone(),
                source_bytes: analysis.size_bytes,
                minimum_allocation_bytes: Some(
                    previous
                        .allocations
                        .iter()
                        .find(|allocation| allocation.id == *id)
                        .expect("waiting item should retain its previous allocation")
                        .target_bytes,
                ),
            })
            .collect(),
        accepted: accepted.to_vec(),
    }
}

fn run_batch_cases(fixtures: &Path, outputs: &Path, cases: &mut Vec<CaseResult>) {
    const MIB: u64 = 1024 * 1024;
    const GMAIL_CEILING: u64 = 24 * MIB;

    let batch_fixtures = fixtures.join("batch");
    let batch_outputs = outputs.join("batch");
    fs::create_dir_all(&batch_fixtures).unwrap();
    fs::create_dir_all(&batch_outputs).unwrap();

    let under_inputs = [
        batch_fixtures.join("gmail-under-one.jpg"),
        batch_fixtures.join("gmail-under-two.png"),
    ];
    generate_image(&under_inputs[0], 640, 480, false, 301).unwrap();
    generate_image(&under_inputs[1], 720, 480, false, 302).unwrap();
    let under = run_aggregate_batch(
        &under_inputs,
        &batch_outputs.join("gmail-under"),
        GMAIL_CEILING,
        CompressionStrategy::Precise,
    );
    let under_ok = under.results.iter().all(Result::is_ok)
        && under.accepted_bytes <= under.ceiling_bytes
        && under.results.iter().all(|result| {
            result
                .as_ref()
                .is_ok_and(|completed| completed.outcome == ProcessOutcome::NoChange)
        });
    cases.push(batch_case(
        "Images already under Gmail total",
        under_ok,
        "all unchanged images are accepted under the Gmail aggregate ceiling",
        format!(
            "actual accepted bytes={} <= {}; every item stayed unchanged={}",
            under.accepted_bytes, under.ceiling_bytes, under_ok
        ),
        CompressionStrategy::Precise,
        LimitScope::BatchTotal,
        &under,
        0,
    ));

    let image_over_inputs = [
        batch_fixtures.join("gmail-over-one.jpg"),
        batch_fixtures.join("gmail-over-two.jpg"),
        batch_fixtures.join("gmail-over-three.jpg"),
    ];
    for (index, path) in image_over_inputs.iter().enumerate() {
        generate_image(path, 1280, 800, false, 311 + index as u32).unwrap();
        pad_file_to_size(path, 9 * MIB).unwrap();
    }
    let image_source_total = source_total(&image_over_inputs);
    let image_over = run_aggregate_batch(
        &image_over_inputs,
        &batch_outputs.join("gmail-image-over"),
        GMAIL_CEILING,
        CompressionStrategy::Precise,
    );
    let image_over_ok = image_source_total > GMAIL_CEILING
        && image_over.results.iter().all(Result::is_ok)
        && image_over.accepted_bytes <= GMAIL_CEILING;
    cases.push(batch_case(
        "Images over Gmail total",
        image_over_ok,
        "compressed image outputs use actual accepted bytes under the Gmail total",
        format!(
            "source total={image_source_total}; actual accepted total={}",
            image_over.accepted_bytes
        ),
        CompressionStrategy::Precise,
        LimitScope::BatchTotal,
        &image_over,
        0,
    ));

    let video_over_inputs = [
        batch_fixtures.join("gmail-video-one.mp4"),
        batch_fixtures.join("gmail-video-two.mp4"),
        batch_fixtures.join("gmail-video-three.mp4"),
    ];
    if fitsend_core::video_tools_available() {
        for (index, path) in video_over_inputs.iter().enumerate() {
            generate_video(path, 640 + index as u32 * 32, 360, 2.0, index != 1).unwrap();
            pad_file_to_size(path, 9 * MIB).unwrap();
        }
        let video_source_total = source_total(&video_over_inputs);
        let video_over = run_aggregate_batch(
            &video_over_inputs,
            &batch_outputs.join("gmail-video-over"),
            GMAIL_CEILING,
            CompressionStrategy::Precise,
        );
        let video_over_ok = video_source_total > GMAIL_CEILING
            && video_over.results.iter().all(Result::is_ok)
            && video_over.accepted_bytes <= GMAIL_CEILING;
        cases.push(batch_case(
            "Videos over Gmail total",
            video_over_ok,
            "compressed video outputs use actual accepted bytes under the Gmail total",
            format!(
                "source total={video_source_total}; actual accepted total={}",
                video_over.accepted_bytes
            ),
            CompressionStrategy::Precise,
            LimitScope::BatchTotal,
            &video_over,
            0,
        ));
    } else {
        cases.push(failed_batch_case(
            "Videos over Gmail total",
            GMAIL_CEILING,
            "bundled FFmpeg tools are required for aggregate video acceptance",
            3,
        ));
    }

    if fitsend_core::video_tools_available() {
        let mixed_inputs = [
            batch_fixtures.join("mixed-image.jpg"),
            batch_fixtures.join("mixed-video.mp4"),
        ];
        generate_image(&mixed_inputs[0], 1280, 800, false, 321).unwrap();
        generate_video(&mixed_inputs[1], 640, 360, 2.0, true).unwrap();
        for path in &mixed_inputs {
            pad_file_to_size(path, 2 * MIB).unwrap();
        }
        let mixed_ceiling = 3 * MIB;
        let mixed = run_aggregate_batch(
            &mixed_inputs,
            &batch_outputs.join("mixed"),
            mixed_ceiling,
            CompressionStrategy::Precise,
        );
        let kinds = mixed
            .results
            .iter()
            .filter_map(|result| result.as_ref().ok())
            .map(|result| analyze(&result.output_path).unwrap().kind)
            .collect::<Vec<_>>();
        let mixed_ok = source_total(&mixed_inputs) > mixed_ceiling
            && mixed.results.iter().all(Result::is_ok)
            && mixed.accepted_bytes <= mixed_ceiling
            && kinds.contains(&MediaKind::Image)
            && kinds.contains(&MediaKind::Video);
        cases.push(batch_case(
            "Mixed image/video total",
            mixed_ok,
            "mixed media outputs are accepted using their actual combined size",
            format!(
                "actual accepted total={}; output kinds={kinds:?}",
                mixed.accepted_bytes
            ),
            CompressionStrategy::Precise,
            LimitScope::BatchTotal,
            &mixed,
            0,
        ));
    } else {
        cases.push(failed_batch_case(
            "Mixed image/video total",
            3 * MIB,
            "bundled FFmpeg tools are required for mixed aggregate acceptance",
            2,
        ));
    }

    let cap_inputs = [
        batch_fixtures.join("source-cap-small.jpg"),
        batch_fixtures.join("source-cap-large-one.jpg"),
        batch_fixtures.join("source-cap-large-two.jpg"),
    ];
    generate_image(&cap_inputs[0], 48, 48, false, 331).unwrap();
    pad_file_to_size(&cap_inputs[0], MIN_ITEM_BUDGET_BYTES).unwrap();
    for (index, path) in cap_inputs[1..].iter().enumerate() {
        generate_image(path, 1280, 800, false, 332 + index as u32).unwrap();
        pad_file_to_size(path, 2 * MIB).unwrap();
    }
    let cap_analyses = analyze_paths(&cap_inputs);
    let cap_ids = aggregate_ids(cap_inputs.len());
    let cap_ceiling = 3 * MIB;
    let initial_cap =
        build_budget(&budget_request(&cap_ids, &cap_analyses, cap_ceiling, &[])).unwrap();
    let cap = run_aggregate_batch(
        &cap_inputs,
        &batch_outputs.join("source-cap"),
        cap_ceiling,
        CompressionStrategy::Precise,
    );
    let small_source = cap_analyses[0].size_bytes;
    let cap_ok = initial_cap.feasible
        && initial_cap.allocations[0].target_bytes == small_source
        && initial_cap
            .allocations
            .iter()
            .map(|allocation| allocation.target_bytes)
            .sum::<u64>()
            == cap_ceiling
        && cap.results.iter().all(Result::is_ok)
        && cap.accepted_bytes <= cap_ceiling;
    cases.push(batch_case(
        "Small source cap redistribution",
        cap_ok,
        "a small source is capped at its own size and unused budget moves to larger files",
        format!(
            "small source={small_source}; initial allocations={:?}; actual accepted={}",
            allocation_values(&initial_cap.allocations),
            cap.accepted_bytes
        ),
        CompressionStrategy::Precise,
        LimitScope::BatchTotal,
        &cap,
        0,
    ));

    let forward_inputs = [
        batch_fixtures.join("forward-one.jpg"),
        batch_fixtures.join("forward-two.jpg"),
        batch_fixtures.join("forward-three.jpg"),
    ];
    for (index, path) in forward_inputs.iter().enumerate() {
        generate_image(path, 1280, 800, false, 341 + index as u32).unwrap();
        pad_file_to_size(path, 2 * MIB).unwrap();
    }
    let forward_analyses = analyze_paths(&forward_inputs);
    let forward_ids = aggregate_ids(forward_inputs.len());
    let forward_ceiling = 3 * MIB;
    let initial_forward = build_budget(&budget_request(
        &forward_ids,
        &forward_analyses,
        forward_ceiling,
        &[],
    ))
    .unwrap();
    let forward = run_aggregate_batch(
        &forward_inputs,
        &batch_outputs.join("forward"),
        forward_ceiling,
        CompressionStrategy::Precise,
    );
    let forward_ok = forward.results.iter().all(Result::is_ok)
        && forward.accepted_bytes <= forward_ceiling
        && forward.results[0]
            .as_ref()
            .is_ok_and(|result| result.output_bytes < forward.allocations[0].target_bytes)
        && forward.allocations[1].target_bytes > initial_forward.allocations[1].target_bytes;
    cases.push(batch_case(
        "Actual-under-allocation forward redistribution",
        forward_ok,
        "an encoder result below its allocation increases a later file's target",
        format!(
            "initial second={}; applied second={}; first actual={}; actual accepted={}",
            initial_forward.allocations[1].target_bytes,
            forward.allocations[1].target_bytes,
            forward.results[0]
                .as_ref()
                .map(|result| result.output_bytes)
                .unwrap_or(0),
            forward.accepted_bytes
        ),
        CompressionStrategy::Precise,
        LimitScope::BatchTotal,
        &forward,
        0,
    ));

    let reserve_inputs = [
        batch_fixtures.join("reserve-one.jpg"),
        batch_fixtures.join("reserve-two.jpg"),
        batch_fixtures.join("reserve-three.jpg"),
    ];
    for (index, path) in reserve_inputs.iter().enumerate() {
        generate_image(path, 64, 64, false, 351 + index as u32).unwrap();
    }
    let reserve_analyses = analyze_paths(&reserve_inputs);
    let reserve_ids = aggregate_ids(reserve_inputs.len());
    let reserve_sum = reserve_analyses
        .iter()
        .map(|analysis| analysis.size_bytes.min(MIN_ITEM_BUDGET_BYTES))
        .sum::<u64>();
    let reserve_ceiling = reserve_sum - 1;
    let reserve_budget = build_budget(&budget_request(
        &reserve_ids,
        &reserve_analyses,
        reserve_ceiling,
        &[],
    ))
    .unwrap();
    let reserve_ok = !reserve_budget.feasible
        && reserve_budget.allocations.is_empty()
        && reserve_budget.reason.as_deref()
            == Some("This total limit is too small for the selected file count.");
    cases.push(batch_case_from_metrics(
        "Impossible sum of per-item reserves",
        reserve_ok,
        "the aggregate fails before processing when per-item reserves exceed the ceiling",
        format!(
            "reserve sum={reserve_sum}; ceiling={reserve_ceiling}; reason={}",
            reserve_budget.reason.as_deref().unwrap_or("none")
        ),
        CompressionStrategy::Precise,
        BatchCaseMetrics {
            scope: LimitScope::BatchTotal,
            ceiling_bytes: reserve_ceiling,
            allocation_bytes: Vec::new(),
            accepted_files: 0,
            attention_files: reserve_inputs.len(),
            accepted_bytes: 0,
        },
    ));

    let quality_inputs = [
        batch_fixtures.join("quality-floor-large.png"),
        batch_fixtures.join("quality-floor-small.jpg"),
    ];
    generate_image(&quality_inputs[0], 1200, 800, false, 361).unwrap();
    generate_image(&quality_inputs[1], 48, 48, false, 362).unwrap();
    pad_file_to_size(&quality_inputs[1], 16 * 1024).unwrap();
    let quality_large_bytes = fs::metadata(&quality_inputs[0]).unwrap().len();
    let quality_target = (quality_large_bytes.saturating_mul(20) / 100).max(80 * 1024);
    let quality_ceiling = quality_target + fs::metadata(&quality_inputs[1]).unwrap().len();
    let quality_output_dir = batch_outputs.join("quality-floor");
    let rejected_quality_output = aggregate_output_path(
        &quality_output_dir,
        "aggregate-0",
        &analyze(quality_inputs[0].to_string_lossy().as_ref()).unwrap(),
    );
    let quality = run_aggregate_batch(
        &quality_inputs,
        &quality_output_dir,
        quality_ceiling,
        CompressionStrategy::Balanced,
    );
    let quality_errors = quality
        .results
        .iter()
        .filter_map(|result| result.as_ref().err())
        .collect::<Vec<_>>();
    let quality_output_absent = !rejected_quality_output.exists();
    let quality_residue_absent = !contains_processing_residue(&quality_output_dir);
    let quality_ok = quality.results[0].is_err()
        && quality.results[1].is_ok()
        && quality_errors.len() == 1
        && quality_errors[0]
            .to_ascii_lowercase()
            .contains("quality floor")
        && quality_output_absent
        && quality_residue_absent
        && quality.accepted_bytes <= quality_ceiling;
    cases.push(batch_case(
        "One quality-floor failure",
        quality_ok,
        "one item fails safely at its quality floor while valid accepted bytes remain proved",
        format!(
            "error={}; rejectedOutputAbsent={quality_output_absent}; residueAbsent={quality_residue_absent}; actual accepted total={}",
            quality_errors.first().map_or("none", |error| error.as_str()),
            quality.accepted_bytes
        ),
        CompressionStrategy::Balanced,
        LimitScope::BatchTotal,
        &quality,
        0,
    ));

    let corrupt_inputs = [
        batch_fixtures.join("corrupt-good-one.jpg"),
        batch_fixtures.join("corrupt-selected.mp4"),
        batch_fixtures.join("corrupt-good-two.png"),
    ];
    generate_image(&corrupt_inputs[0], 640, 480, false, 371).unwrap();
    fs::write(&corrupt_inputs[1], b"not actually a video").unwrap();
    generate_image(&corrupt_inputs[2], 640, 480, false, 372).unwrap();
    let source_hashes = corrupt_inputs
        .iter()
        .map(|path| file_hash(path).unwrap())
        .collect::<Vec<_>>();
    let analyzed = corrupt_inputs
        .iter()
        .map(|path| analyze(path.to_string_lossy().as_ref()))
        .collect::<Vec<_>>();
    let corrupt_attention = analyzed.iter().filter(|result| result.is_err()).count();
    let valid_corrupt_inputs = corrupt_inputs
        .iter()
        .zip(&analyzed)
        .filter_map(|(path, result)| result.is_ok().then_some(path.clone()))
        .collect::<Vec<_>>();
    let corrupt = run_aggregate_batch(
        &valid_corrupt_inputs,
        &batch_outputs.join("corrupt"),
        GMAIL_CEILING,
        CompressionStrategy::Precise,
    );
    let corrupt_ok = corrupt_attention == 1
        && corrupt.results.iter().all(Result::is_ok)
        && corrupt.accepted_bytes <= GMAIL_CEILING
        && corrupt_inputs
            .iter()
            .zip(source_hashes)
            .all(|(path, before)| file_hash(path).ok() == Some(before));
    cases.push(batch_case(
        "One corrupt input",
        corrupt_ok,
        "corrupt analysis is attention and only valid media enters the aggregate queue",
        format!(
            "analysis attention={corrupt_attention}; valid accepted total={}",
            corrupt.accepted_bytes
        ),
        CompressionStrategy::Precise,
        LimitScope::BatchTotal,
        &corrupt,
        corrupt_attention,
    ));

    if fitsend_core::video_tools_available() {
        cases.push(run_aggregate_cancellation_case(
            &batch_fixtures,
            &batch_outputs,
        ));
    } else {
        cases.push(failed_batch_case(
            "Cancellation during aggregate video processing",
            3 * MIB,
            "bundled FFmpeg tools are required for aggregate cancellation acceptance",
            3,
        ));
    }

    let partial_inputs = [
        batch_fixtures.join("partial-quality-failure.png"),
        batch_fixtures.join("partial-good.jpg"),
    ];
    generate_image(&partial_inputs[0], 1200, 800, false, 381).unwrap();
    generate_image(&partial_inputs[1], 48, 48, false, 382).unwrap();
    pad_file_to_size(&partial_inputs[1], 16 * 1024).unwrap();
    let partial_failed_source = fs::metadata(&partial_inputs[0]).unwrap().len();
    let partial_target = (partial_failed_source.saturating_mul(20) / 100).max(80 * 1024);
    let partial_ceiling = fs::metadata(&partial_inputs[1]).unwrap().len() + partial_target;
    let partial_output_dir = batch_outputs.join("partial");
    let rejected_partial_output = aggregate_output_path(
        &partial_output_dir,
        "aggregate-0",
        &analyze(partial_inputs[0].to_string_lossy().as_ref()).unwrap(),
    );
    let partial = run_aggregate_batch(
        &partial_inputs,
        &partial_output_dir,
        partial_ceiling,
        CompressionStrategy::Balanced,
    );
    let successful_bytes = partial
        .results
        .iter()
        .filter_map(|result| result.as_ref().ok())
        .map(|result| result.output_bytes)
        .sum::<u64>();
    let partial_output_absent = !rejected_partial_output.exists();
    let partial_residue_absent = !contains_processing_residue(&partial_output_dir);
    let partial_ok = partial.results[0].is_err()
        && partial.results[1].is_ok()
        && partial_output_absent
        && partial_residue_absent
        && partial.accepted_bytes == successful_bytes
        && partial.accepted_bytes <= partial_ceiling
        && partial.accepted_bytes + partial_failed_source > partial.accepted_bytes;
    cases.push(batch_case(
        "Partial proof excluding failed bytes",
        partial_ok,
        "partial aggregate proof sums only accepted output bytes",
        format!(
            "accepted metric={}; successful output sum={successful_bytes}; excluded failed source={partial_failed_source}; rejectedOutputAbsent={partial_output_absent}; residueAbsent={partial_residue_absent}",
            partial.accepted_bytes
        ),
        CompressionStrategy::Balanced,
        LimitScope::BatchTotal,
        &partial,
        0,
    ));

    let per_file_inputs = [
        batch_fixtures.join("discord-per-file-one.jpg"),
        batch_fixtures.join("discord-per-file-two.jpg"),
    ];
    for (index, path) in per_file_inputs.iter().enumerate() {
        generate_image(path, 1280, 800, false, 391 + index as u32).unwrap();
        pad_file_to_size(path, 6 * MIB).unwrap();
    }
    cases.push(run_per_file_regression(
        &per_file_inputs,
        &batch_outputs.join("per-file"),
    ));
}

fn run_aggregate_cancellation_case(fixtures: &Path, outputs: &Path) -> CaseResult {
    const MIB: u64 = 1024 * 1024;
    let inputs = [
        fixtures.join("cancel-accepted-first.jpg"),
        fixtures.join("cancel-active-video.mp4"),
        fixtures.join("cancel-later-image.jpg"),
    ];
    generate_image(&inputs[0], 48, 48, false, 401).unwrap();
    pad_file_to_size(&inputs[0], 16 * 1024).unwrap();
    generate_video(&inputs[1], 960, 540, 4.0, true).unwrap();
    pad_file_to_size(&inputs[1], 4 * MIB).unwrap();
    generate_image(&inputs[2], 96, 96, false, 402).unwrap();
    pad_file_to_size(&inputs[2], 16 * 1024).unwrap();
    fs::create_dir_all(outputs.join("cancellation")).unwrap();

    let original_hashes = inputs
        .iter()
        .map(|path| file_hash(path).unwrap())
        .collect::<Vec<_>>();
    let analyses = analyze_paths(&inputs);
    let ids = aggregate_ids(inputs.len());
    let ceiling = 3 * MIB;
    let mut budget = build_budget(&budget_request(&ids, &analyses, ceiling, &[])).unwrap();
    let first_target = budget.allocations[0].target_bytes;
    let first = process(&ProcessRequest {
        analysis: analyses[0].clone(),
        target_bytes: first_target,
        output_path: aggregate_output_path(&outputs.join("cancellation"), &ids[0], &analyses[0])
            .to_string_lossy()
            .to_string(),
        strategy: CompressionStrategy::Precise,
    })
    .unwrap();
    assert!(analyze(&first.output_path).is_ok());
    let accepted = vec![AcceptedBudgetItem {
        id: ids[0].clone(),
        actual_bytes: first.output_bytes,
    }];
    budget = rebalance_budget(&remaining_budget_request(
        &ids, &analyses, &accepted, 1, ceiling, &budget,
    ))
    .unwrap();
    let active_target = budget.allocations[0].target_bytes;
    let later_target = budget.allocations[1].target_bytes;
    let active_output = aggregate_output_path(&outputs.join("cancellation"), &ids[1], &analyses[1]);
    let later_output = aggregate_output_path(&outputs.join("cancellation"), &ids[2], &analyses[2]);
    let cancellation_output_dir = outputs.join("cancellation");
    let mut positive_encoding_progress = false;
    let mut partial_output_seen = false;
    let cancelled = process_with_progress(
        &ProcessRequest {
            analysis: analyses[1].clone(),
            target_bytes: active_target,
            output_path: active_output.to_string_lossy().to_string(),
            strategy: CompressionStrategy::Precise,
        },
        |progress| {
            if progress.stage == "Encoding pass 2 of 2"
                && progress
                    .encoded_seconds
                    .is_some_and(|seconds| seconds > 0.0)
            {
                positive_encoding_progress = true;
                partial_output_seen = contains_nonempty_working_file(&cancellation_output_dir);
                return !partial_output_seen;
            }
            true
        },
    );
    let error = cancelled.err();
    let sources_unchanged = inputs
        .iter()
        .zip(original_hashes)
        .all(|(path, before)| file_hash(path).ok() == Some(before));
    let clean = !active_output.exists()
        && !later_output.exists()
        && !contains_processing_residue(&outputs.join("cancellation"));
    let passed = positive_encoding_progress
        && partial_output_seen
        && error.as_deref() == Some(PROCESS_CANCELLED)
        && sources_unchanged
        && clean
        && first.output_bytes <= ceiling;
    batch_case_from_metrics(
        "Cancellation during aggregate video processing",
        passed,
        "active video cancellation returns PROCESS_CANCELLED and later rows remain unprocessed",
        format!(
            "positiveEncodingProgress={positive_encoding_progress}; partialOutputSeen={partial_output_seen}; error={}; acceptedBeforeCancel={}; laterCancelled=1; sourcesUnchanged={sources_unchanged}; clean={clean}",
            error.as_deref().unwrap_or("none"),
            first.output_bytes
        ),
        CompressionStrategy::Precise,
        BatchCaseMetrics {
            scope: LimitScope::BatchTotal,
            ceiling_bytes: ceiling,
            allocation_bytes: vec![first_target, active_target, later_target],
            accepted_files: 1,
            attention_files: 2,
            accepted_bytes: first.output_bytes,
        },
    )
}

fn run_per_file_regression(inputs: &[PathBuf], outputs: &Path) -> CaseResult {
    const DISCORD_SAFE_CEILING: u64 = 10_276_044;
    fs::create_dir_all(outputs).unwrap();
    let original_hashes = inputs
        .iter()
        .map(|path| file_hash(path).unwrap())
        .collect::<Vec<_>>();
    let analyses = inputs
        .iter()
        .map(|path| analyze(path.to_string_lossy().as_ref()).unwrap())
        .collect::<Vec<_>>();
    let results = analyses
        .iter()
        .enumerate()
        .map(|(index, analysis)| {
            process(&ProcessRequest {
                analysis: analysis.clone(),
                target_bytes: DISCORD_SAFE_CEILING,
                output_path: aggregate_output_path(outputs, &format!("per-file-{index}"), analysis)
                    .to_string_lossy()
                    .to_string(),
                strategy: CompressionStrategy::Precise,
            })
        })
        .collect::<Vec<_>>();
    let accepted_bytes = results
        .iter()
        .filter_map(|result| result.as_ref().ok())
        .map(|result| result.output_bytes)
        .sum::<u64>();
    let originals_unchanged = inputs
        .iter()
        .zip(original_hashes)
        .all(|(path, before)| file_hash(path).ok() == Some(before));
    let passed = results.iter().all(|result| {
        result.as_ref().is_ok_and(|completed| {
            completed.verified
                && completed.output_bytes <= DISCORD_SAFE_CEILING
                && analyze(&completed.output_path).is_ok()
        })
    }) && accepted_bytes > DISCORD_SAFE_CEILING
        && originals_unchanged;
    batch_case_from_metrics(
        "Discord per-file regression",
        passed,
        "each accepted file independently fits Discord Safe even when their sum exceeds one ceiling",
        format!(
            "accepted total={accepted_bytes} > per-file ceiling={DISCORD_SAFE_CEILING}; originalsUnchanged={originals_unchanged}"
        ),
        CompressionStrategy::Precise,
        BatchCaseMetrics {
            scope: LimitScope::PerFile,
            ceiling_bytes: DISCORD_SAFE_CEILING,
            allocation_bytes: vec![DISCORD_SAFE_CEILING; inputs.len()],
            accepted_files: results.iter().filter(|result| result.is_ok()).count(),
            attention_files: results.iter().filter(|result| result.is_err()).count(),
            accepted_bytes,
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn batch_case(
    name: &str,
    passed: bool,
    expected: &str,
    detail: String,
    strategy: CompressionStrategy,
    scope: LimitScope,
    result: &AggregateCaseResult,
    extra_attention: usize,
) -> CaseResult {
    let accepted_files = result.results.iter().filter(|entry| entry.is_ok()).count();
    let attention_files = result.results.len() - accepted_files + extra_attention;
    batch_case_from_metrics(
        name,
        passed,
        expected,
        detail,
        strategy,
        BatchCaseMetrics {
            scope,
            ceiling_bytes: result.ceiling_bytes,
            allocation_bytes: allocation_values(&result.allocations),
            accepted_files,
            attention_files,
            accepted_bytes: result.accepted_bytes,
        },
    )
}

fn batch_case_from_metrics(
    name: &str,
    passed: bool,
    expected: &str,
    detail: String,
    strategy: CompressionStrategy,
    metrics: BatchCaseMetrics,
) -> CaseResult {
    let scope_proof =
        metrics.scope != LimitScope::BatchTotal || metrics.accepted_bytes <= metrics.ceiling_bytes;
    CaseResult {
        name: name.to_string(),
        category: "batch".to_string(),
        strategy: Some(strategy),
        outcome: None,
        passed: passed && scope_proof,
        expected: expected.to_string(),
        detail: format!("{detail}; scopeProof={scope_proof}"),
        input_bytes: None,
        output_bytes: None,
        target_bytes: None,
        duration_ms: None,
        quality_score: None,
        width: None,
        height: None,
        batch: Some(metrics),
    }
}

fn failed_batch_case(
    name: &str,
    ceiling_bytes: u64,
    detail: &str,
    attention_files: usize,
) -> CaseResult {
    batch_case_from_metrics(
        name,
        false,
        "batch fixture generation and processing succeed",
        detail.to_string(),
        CompressionStrategy::Precise,
        BatchCaseMetrics {
            scope: LimitScope::BatchTotal,
            ceiling_bytes,
            allocation_bytes: Vec::new(),
            accepted_files: 0,
            attention_files,
            accepted_bytes: 0,
        },
    )
}

fn analyze_paths<const N: usize>(paths: &[PathBuf; N]) -> Vec<MediaAnalysis> {
    paths
        .iter()
        .map(|path| analyze(path.to_string_lossy().as_ref()).unwrap())
        .collect()
}

fn aggregate_ids(count: usize) -> Vec<String> {
    (0..count)
        .map(|index| format!("aggregate-{index}"))
        .collect()
}

fn allocation_values(allocations: &[ItemAllocation]) -> Vec<u64> {
    allocations
        .iter()
        .map(|allocation| allocation.target_bytes)
        .collect()
}

fn source_total<const N: usize>(paths: &[PathBuf; N]) -> u64 {
    paths
        .iter()
        .map(|path| fs::metadata(path).unwrap().len())
        .sum()
}

fn pad_file_to_size(path: &Path, target_size: u64) -> Result<(), String> {
    let current = fs::metadata(path)
        .map_err(|error| format!("could not inspect padding target: {error}"))?
        .len();
    if current >= target_size {
        return Ok(());
    }
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| format!("could not open padding target: {error}"))?;
    let zeros = [0_u8; 64 * 1024];
    let mut remaining = target_size - current;
    while remaining > 0 {
        let length = remaining.min(zeros.len() as u64) as usize;
        file.write_all(&zeros[..length])
            .map_err(|error| format!("could not pad media fixture: {error}"))?;
        remaining -= length as u64;
    }
    Ok(())
}

fn file_hash(path: &Path) -> Result<u64, String> {
    let bytes = fs::read(path).map_err(|error| format!("could not hash source: {error}"))?;
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    Ok(hasher.finish())
}

#[allow(clippy::too_many_arguments)]
fn run_success_case(
    name: &str,
    category: &str,
    input: &Path,
    output: &Path,
    target_bytes: u64,
    expected_kind: MediaKind,
    strategy: CompressionStrategy,
    expect_no_change: Option<bool>,
) -> CaseResult {
    let analysis = match analyze(input.to_string_lossy().as_ref()) {
        Ok(analysis) => analysis,
        Err(error) => {
            return failed_case(
                name,
                category,
                target_bytes,
                Some(strategy),
                format!("analysis failed: {error}"),
            )
        }
    };
    let input_bytes = analysis.size_bytes;
    if analysis.kind != expected_kind {
        return failed_case(
            name,
            category,
            target_bytes,
            Some(strategy),
            "media kind did not match".to_string(),
        );
    }
    let plan = match build(&PlanRequest {
        analysis: analysis.clone(),
        target_bytes,
        strategy,
    }) {
        Ok(plan) => plan,
        Err(error) => {
            return failed_case(
                name,
                category,
                target_bytes,
                Some(strategy),
                format!("planning failed: {error}"),
            )
        }
    };
    if !plan.feasible || expect_no_change.is_some_and(|expected| plan.already_fits != expected) {
        return failed_case(
            name,
            category,
            target_bytes,
            Some(strategy),
            format!(
                "unexpected plan: feasible={}, alreadyFits={}",
                plan.feasible, plan.already_fits
            ),
        );
    }
    let result = match process(&ProcessRequest {
        analysis: analysis.clone(),
        target_bytes,
        output_path: output.to_string_lossy().to_string(),
        strategy,
    }) {
        Ok(result) => result,
        Err(error) => {
            return failed_case(
                name,
                category,
                target_bytes,
                Some(strategy),
                format!("processing failed: {error}"),
            )
        }
    };
    let result_path = Path::new(&result.output_path);
    let mut passed = result.verified && result.output_bytes <= target_bytes;
    match (expect_no_change, result.outcome) {
        (Some(true), ProcessOutcome::NoChange) | (None, ProcessOutcome::NoChange) => {
            passed &= result_path == input
                && !output.exists()
                && result.output_bytes == input_bytes
                && input_bytes <= target_bytes;
        }
        (Some(false), ProcessOutcome::Created) | (None, ProcessOutcome::Created) => {
            passed &= result_path.is_file();
        }
        _ => passed = false,
    }
    let mut detail = format!(
        "{strategy:?}/{:?}; {} -> {} bytes; {}×{}; quality={}; {} ms; {} attempt(s)",
        result.outcome,
        input_bytes,
        result.output_bytes,
        result.width,
        result.height,
        result
            .quality_score
            .map(|score| format!("{score:.4}"))
            .unwrap_or_else(|| "n/a".to_string()),
        result.duration_ms,
        result.attempts
    );

    if expected_kind == MediaKind::Video && result.outcome == ProcessOutcome::Created {
        match analyze(result.output_path.as_str()) {
            Ok(output_analysis) => {
                let compatible = output_analysis.video_codec.as_deref() == Some("h264")
                    && (!output_analysis.has_audio
                        || output_analysis.audio_codec.as_deref() == Some("aac"));
                let duration_preserved =
                    match (analysis.duration_seconds, output_analysis.duration_seconds) {
                        (Some(input), Some(output)) => {
                            (input - output).abs() <= input.mul_add(0.05, 0.15)
                        }
                        (None, None) => true,
                        _ => false,
                    };
                let media_preserved = output_analysis.has_audio == analysis.has_audio
                    && output_analysis.width == result.width
                    && output_analysis.height == result.height
                    && duration_preserved;
                passed &= compatible && media_preserved;
                detail.push_str(&format!(
                    "; codecs={}/{}; audioPreserved={}; durationPreserved={}; analyzedDimensions={}×{}",
                    output_analysis.video_codec.as_deref().unwrap_or("none"),
                    output_analysis.audio_codec.as_deref().unwrap_or("none"),
                    output_analysis.has_audio == analysis.has_audio,
                    duration_preserved,
                    output_analysis.width,
                    output_analysis.height,
                ));
            }
            Err(error) => {
                passed = false;
                detail.push_str(&format!("; output analysis failed: {error}"));
            }
        }
    }

    CaseResult {
        name: name.to_string(),
        category: category.to_string(),
        strategy: Some(strategy),
        outcome: Some(result.outcome),
        passed,
        expected: format!("verified {expected_kind:?} at or below {target_bytes} bytes"),
        detail,
        input_bytes: Some(input_bytes),
        output_bytes: Some(result.output_bytes),
        target_bytes: Some(target_bytes),
        duration_ms: Some(result.duration_ms),
        quality_score: result.quality_score,
        width: Some(result.width),
        height: Some(result.height),
        batch: None,
    }
}

fn run_expected_analysis_failure(name: &str, input: &Path, expected_text: &str) -> CaseResult {
    let result = analyze(input.to_string_lossy().as_ref());
    let detail = match &result {
        Ok(_) => "analysis unexpectedly succeeded".to_string(),
        Err(error) => error.clone(),
    };
    CaseResult {
        name: name.to_string(),
        category: "failure".to_string(),
        strategy: None,
        outcome: None,
        passed: result.is_err() && detail.contains(expected_text),
        expected: format!("analysis fails with '{expected_text}'"),
        detail,
        input_bytes: fs::metadata(input).ok().map(|metadata| metadata.len()),
        output_bytes: None,
        target_bytes: None,
        duration_ms: None,
        quality_score: None,
        width: None,
        height: None,
        batch: None,
    }
}

fn generate_image(
    path: &Path,
    width: u32,
    height: u32,
    alpha: bool,
    seed: u32,
) -> Result<(), String> {
    let pixels = ImageBuffer::from_fn(width, height, |x, y| {
        let mixed = x.wrapping_mul(31 + seed) ^ y.wrapping_mul(17 + seed) ^ x.wrapping_mul(y + 1);
        Rgba([
            (mixed % 251) as u8,
            ((mixed / 7 + x + seed) % 253) as u8,
            ((mixed / 11 + y + seed) % 255) as u8,
            if alpha && (x + y + seed).is_multiple_of(5) {
                150
            } else {
                255
            },
        ])
    });
    let format = match path.extension().and_then(|extension| extension.to_str()) {
        Some("jpg" | "jpeg") => ImageFormat::Jpeg,
        _ => ImageFormat::Png,
    };
    let image = DynamicImage::ImageRgba8(pixels);
    let image = if alpha {
        image
    } else {
        DynamicImage::ImageRgb8(image.to_rgb8())
    };
    image
        .save_with_format(path, format)
        .map_err(|error| format!("image generation failed: {error}"))
}

fn generate_video(
    path: &Path,
    width: u32,
    height: u32,
    duration: f64,
    with_audio: bool,
) -> Result<(), String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("mp4");
    let source = format!("testsrc2=size={width}x{height}:rate=30:duration={duration}");
    let tone = format!("sine=frequency=880:sample_rate=48000:duration={duration}");
    let ffmpeg = std::env::var_os("FITSEND_FFMPEG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("ffmpeg"));
    let mut command = Command::new(ffmpeg);
    command.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "lavfi",
        "-i",
        &source,
    ]);
    if with_audio {
        command.args(["-f", "lavfi", "-i", &tone]);
    }
    if extension == "webm" {
        command.args(["-c:v", "libvpx-vp9", "-deadline", "realtime", "-b:v", "3M"]);
        if with_audio {
            command.args(["-c:a", "libopus", "-b:a", "96k", "-shortest"]);
        }
    } else {
        command.args([
            "-c:v", "libx264", "-preset", "veryfast", "-b:v", "4M", "-pix_fmt", "yuv420p",
        ]);
        if with_audio {
            command.args(["-c:a", "aac", "-b:a", "128k", "-shortest"]);
        }
    }
    let output = command
        .arg(path)
        .output()
        .map_err(|error| format!("video generator could not start: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "video generation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn failed_generation(name: &str, category: &str, error: String) -> CaseResult {
    CaseResult {
        name: name.to_string(),
        category: category.to_string(),
        strategy: None,
        outcome: None,
        passed: false,
        expected: "fixture generation succeeds".to_string(),
        detail: error,
        input_bytes: None,
        output_bytes: None,
        target_bytes: None,
        duration_ms: None,
        quality_score: None,
        width: None,
        height: None,
        batch: None,
    }
}

fn generate_rotated_video(path: &Path) -> Result<(), String> {
    let source = path.with_file_name("rotated-source.mp4");
    generate_video(&source, 960, 540, 2.0, true)?;
    let ffmpeg = std::env::var_os("FITSEND_FFMPEG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("ffmpeg"));
    let output = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-display_rotation:v:0",
            "90",
            "-i",
        ])
        .arg(&source)
        .args(["-c", "copy"])
        .arg(path)
        .output()
        .map_err(|error| format!("rotated video generator could not start: {error}"))?;
    let _ = fs::remove_file(source);
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "rotated video generation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

fn failed_case(
    name: &str,
    category: &str,
    target_bytes: u64,
    strategy: Option<CompressionStrategy>,
    detail: String,
) -> CaseResult {
    CaseResult {
        name: name.to_string(),
        category: category.to_string(),
        strategy,
        outcome: None,
        passed: false,
        expected: format!("verified output at or below {target_bytes} bytes"),
        detail,
        input_bytes: None,
        output_bytes: None,
        target_bytes: Some(target_bytes),
        duration_ms: None,
        quality_score: None,
        width: None,
        height: None,
        batch: None,
    }
}

fn markdown_report(report: &AcceptanceReport) -> String {
    let mut markdown = format!(
        "# FitSend Acceptance Report\n\n- Total: {}\n- Passed: {}\n- Failed: {}\n\n| Case | Category | Strategy | Result | Outcome | Bytes (input → output / target) | Quality | Dimensions | Time | Scope | Ceiling | Allocations | Accepted | Attention | Accepted total | Detail |\n|---|---|---|---:|---|---|---:|---|---:|---|---:|---|---:|---:|---:|---|\n",
        report.total, report.passed, report.failed
    );
    for case in &report.cases {
        let scope = case
            .batch
            .as_ref()
            .map(|batch| format!("{:?}", batch.scope))
            .unwrap_or_else(|| "—".to_string());
        let ceiling = case
            .batch
            .as_ref()
            .map(|batch| batch.ceiling_bytes.to_string())
            .unwrap_or_else(|| "—".to_string());
        let allocations = case
            .batch
            .as_ref()
            .map(|batch| {
                batch
                    .allocation_bytes
                    .iter()
                    .map(u64::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "—".to_string());
        let accepted = case
            .batch
            .as_ref()
            .map(|batch| batch.accepted_files.to_string())
            .unwrap_or_else(|| "—".to_string());
        let attention = case
            .batch
            .as_ref()
            .map(|batch| batch.attention_files.to_string())
            .unwrap_or_else(|| "—".to_string());
        let accepted_total = case
            .batch
            .as_ref()
            .map(|batch| batch.accepted_bytes.to_string())
            .unwrap_or_else(|| "—".to_string());
        markdown.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} → {} / {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            case.name,
            case.category,
            case.strategy
                .map(|strategy| format!("{strategy:?}"))
                .unwrap_or_else(|| "—".to_string()),
            if case.passed { "PASS" } else { "FAIL" },
            case.outcome
                .map(|outcome| format!("{outcome:?}"))
                .unwrap_or_else(|| "—".to_string()),
            case.input_bytes
                .map(|value| value.to_string())
                .unwrap_or_else(|| "—".to_string()),
            case.output_bytes
                .map(|value| value.to_string())
                .unwrap_or_else(|| "—".to_string()),
            case.target_bytes
                .map(|value| value.to_string())
                .unwrap_or_else(|| "—".to_string()),
            case.quality_score
                .map(|value| format!("{value:.4}"))
                .unwrap_or_else(|| "—".to_string()),
            match (case.width, case.height) {
                (Some(width), Some(height)) => format!("{width}×{height}"),
                _ => "—".to_string(),
            },
            case.duration_ms
                .map(|value| format!("{value} ms"))
                .unwrap_or_else(|| "—".to_string()),
            scope,
            ceiling,
            allocations,
            accepted,
            attention,
            accepted_total,
            case.detail.replace('|', "\\|").replace('\n', " ")
        ));
    }
    markdown
}
