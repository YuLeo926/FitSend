use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use fitsend_core::{
    analyze, build, process, process_with_progress, CompressionStrategy, MediaKind, PlanRequest,
    ProcessOutcome, ProcessRequest, PROCESS_CANCELLED,
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
        });
    }
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
    }
}

fn markdown_report(report: &AcceptanceReport) -> String {
    let mut markdown = format!(
        "# FitSend Acceptance Report\n\n- Total: {}\n- Passed: {}\n- Failed: {}\n\n| Case | Category | Strategy | Result | Outcome | Bytes (input → output / target) | Quality | Dimensions | Time | Detail |\n|---|---|---|---:|---|---|---:|---|---:|---|\n",
        report.total, report.passed, report.failed
    );
    for case in &report.cases {
        markdown.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} → {} / {} | {} | {} | {} | {} |\n",
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
            case.detail.replace('|', "\\|").replace('\n', " ")
        ));
    }
    markdown
}
