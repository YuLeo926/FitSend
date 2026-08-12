use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use fitsend_core::{
    analyze, build, process, process_with_progress, MediaKind, PlanRequest, ProcessRequest,
    PROCESS_CANCELLED,
};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CaseResult {
    name: String,
    category: String,
    passed: bool,
    expected: String,
    detail: String,
    input_bytes: Option<u64>,
    output_bytes: Option<u64>,
    target_bytes: Option<u64>,
    duration_ms: Option<u64>,
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
            110,
        ),
        ("image-square-png", "square.png", 1000, 1000, false, 100),
        ("image-unicode-path", "旅行 照片.png", 1280, 720, true, 105),
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
            45,
        ),
        ("image-alpha-dark", "alpha dark.png", 1100, 700, true, 75),
        (
            "image-large-noise",
            "large-noise.jpg",
            2200,
            1400,
            false,
            170,
        ),
        ("image-compact", "compact.png", 640, 480, false, 32),
    ];

    for (index, (name, filename, width, height, alpha, target_kb)) in
        specifications.into_iter().enumerate()
    {
        let input = fixtures.join(filename);
        let output = outputs.join(format!("{name}.jpg"));
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
            false,
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
            true,
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
            false,
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
                true,
            ));
        }
        Err(error) => cases.push(failed_generation("video-already-fits", "video", error)),
    }
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
    })
    .unwrap();
    cases.push(CaseResult {
        name: "failure-infeasible-image".to_string(),
        category: "failure".to_string(),
        passed: !image_plan.feasible,
        expected: "planner rejects unusable image target".to_string(),
        detail: format!("feasible={}", image_plan.feasible),
        input_bytes: None,
        output_bytes: None,
        target_bytes: Some(10 * 1024),
        duration_ms: None,
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
        })
        .unwrap();
        cases.push(CaseResult {
            name: "failure-infeasible-video".to_string(),
            category: "failure".to_string(),
            passed: !video_plan.feasible,
            expected: "planner rejects unusable video target".to_string(),
            detail: format!("feasible={}", video_plan.feasible),
            input_bytes: None,
            output_bytes: None,
            target_bytes: Some(80 * 1024),
            duration_ms: None,
        });
    }

    let cancelled_output = outputs.join("cancelled-must-not-exist.jpg");
    let cancellation_analysis = analyze(image.to_string_lossy().as_ref()).unwrap();
    let cancellation_result = process_with_progress(
        &ProcessRequest {
            analysis: cancellation_analysis,
            target_bytes: 100 * 1024,
            output_path: cancelled_output.to_string_lossy().to_string(),
        },
        |progress| progress.stage != "Finding the best image quality",
    );
    let cancellation_error = cancellation_result.err();
    cases.push(CaseResult {
        name: "cancellation-removes-partial-output".to_string(),
        category: "failure".to_string(),
        passed: cancellation_error.as_deref() == Some(PROCESS_CANCELLED)
            && !cancelled_output.exists(),
        expected: "cancel returns the cancellation code and leaves no output".to_string(),
        detail: format!(
            "error={}; outputExists={}",
            cancellation_error.as_deref().unwrap_or("none"),
            cancelled_output.exists()
        ),
        input_bytes: None,
        output_bytes: None,
        target_bytes: None,
        duration_ms: None,
    });
}

fn run_success_case(
    name: &str,
    category: &str,
    input: &Path,
    output: &Path,
    target_bytes: u64,
    expected_kind: MediaKind,
    expect_copy: bool,
) -> CaseResult {
    let analysis = match analyze(input.to_string_lossy().as_ref()) {
        Ok(analysis) => analysis,
        Err(error) => {
            return failed_case(
                name,
                category,
                target_bytes,
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
            "media kind did not match".to_string(),
        );
    }
    let plan = match build(&PlanRequest {
        analysis: analysis.clone(),
        target_bytes,
    }) {
        Ok(plan) => plan,
        Err(error) => {
            return failed_case(
                name,
                category,
                target_bytes,
                format!("planning failed: {error}"),
            )
        }
    };
    if !plan.feasible || plan.already_fits != expect_copy {
        return failed_case(
            name,
            category,
            target_bytes,
            format!(
                "unexpected plan: feasible={}, alreadyFits={}",
                plan.feasible, plan.already_fits
            ),
        );
    }
    let result = match process(&ProcessRequest {
        analysis,
        target_bytes,
        output_path: output.to_string_lossy().to_string(),
    }) {
        Ok(result) => result,
        Err(error) => {
            return failed_case(
                name,
                category,
                target_bytes,
                format!("processing failed: {error}"),
            )
        }
    };
    let mut passed = result.verified && result.output_bytes <= target_bytes && output.is_file();
    let mut detail = format!(
        "{} -> {} bytes; {}×{}; {} ms; {} attempt(s)",
        input_bytes,
        result.output_bytes,
        result.width,
        result.height,
        result.duration_ms,
        result.attempts
    );

    if expected_kind == MediaKind::Video && !expect_copy {
        match analyze(result.output_path.as_str()) {
            Ok(output_analysis) => {
                let compatible = output_analysis.video_codec.as_deref() == Some("h264")
                    && (!output_analysis.has_audio
                        || output_analysis.audio_codec.as_deref() == Some("aac"));
                passed &= compatible;
                detail.push_str(&format!(
                    "; codecs={}/{}",
                    output_analysis.video_codec.as_deref().unwrap_or("none"),
                    output_analysis.audio_codec.as_deref().unwrap_or("none")
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
        passed,
        expected: format!("verified {expected_kind:?} at or below {target_bytes} bytes"),
        detail,
        input_bytes: Some(input_bytes),
        output_bytes: Some(result.output_bytes),
        target_bytes: Some(target_bytes),
        duration_ms: Some(result.duration_ms),
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
        passed: result.is_err() && detail.contains(expected_text),
        expected: format!("analysis fails with '{expected_text}'"),
        detail,
        input_bytes: fs::metadata(input).ok().map(|metadata| metadata.len()),
        output_bytes: None,
        target_bytes: None,
        duration_ms: None,
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
    DynamicImage::ImageRgba8(pixels)
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
        passed: false,
        expected: "fixture generation succeeds".to_string(),
        detail: error,
        input_bytes: None,
        output_bytes: None,
        target_bytes: None,
        duration_ms: None,
    }
}

fn failed_case(name: &str, category: &str, target_bytes: u64, detail: String) -> CaseResult {
    CaseResult {
        name: name.to_string(),
        category: category.to_string(),
        passed: false,
        expected: format!("verified output at or below {target_bytes} bytes"),
        detail,
        input_bytes: None,
        output_bytes: None,
        target_bytes: Some(target_bytes),
        duration_ms: None,
    }
}

fn markdown_report(report: &AcceptanceReport) -> String {
    let mut markdown = format!(
        "# FitSend Acceptance Report\n\n- Total: {}\n- Passed: {}\n- Failed: {}\n\n| Case | Category | Result | Detail |\n|---|---|---:|---|\n",
        report.total, report.passed, report.failed
    );
    for case in &report.cases {
        markdown.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            case.name,
            case.category,
            if case.passed { "PASS" } else { "FAIL" },
            case.detail.replace('|', "\\|").replace('\n', " ")
        ));
    }
    markdown
}
