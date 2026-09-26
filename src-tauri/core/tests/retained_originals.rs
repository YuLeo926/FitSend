use fitsend_core::{
    analyze, build, build_budget, process, process_with_progress, BatchBudgetRequest,
    BudgetItemRequest, CompressionStrategy, LimitScope, PlanRequest, ProcessOutcome,
    ProcessRequest, PROCESS_CANCELLED,
};
use std::{fs, io::Write, path::Path};

const STRATEGIES: [CompressionStrategy; 3] = [
    CompressionStrategy::Balanced,
    CompressionStrategy::Smallest,
    CompressionStrategy::Precise,
];

fn fixture(path: &Path, video: bool) {
    if video {
        let tool = std::env::var_os("FITSEND_FFMPEG_PATH").unwrap_or_else(|| "ffmpeg".into());
        let status = std::process::Command::new(tool)
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=black:size=64x64:rate=10:duration=1",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(path)
            .status()
            .unwrap();
        assert!(status.success());
    } else {
        image::RgbaImage::from_pixel(16, 16, image::Rgba([20, 40, 80, 180]))
            .save(path)
            .unwrap();
    }
}

#[test]
fn tiny_sources_allocate_plan_and_process_for_every_strategy() {
    let dir = tempfile::tempdir().unwrap();
    for video in [false, true] {
        let path = dir.path().join(if video { "tiny.mp4" } else { "tiny.png" });
        fixture(&path, video);
        let before = fs::read(&path).unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        let analysis = analyze(path.to_str().unwrap()).unwrap();
        assert!(analysis.size_bytes < 8192);
        let budget = build_budget(&BatchBudgetRequest {
            scope: LimitScope::BatchTotal,
            ceiling_bytes: 24 * 1024 * 1024,
            items: vec![BudgetItemRequest {
                id: "tiny".into(),
                source_bytes: analysis.size_bytes,
                minimum_allocation_bytes: None,
            }],
            accepted: vec![],
        })
        .unwrap();
        let target_bytes = budget.allocations[0].target_bytes;
        assert_eq!(target_bytes, analysis.size_bytes);
        for strategy in STRATEGIES {
            let plan = build(&PlanRequest {
                analysis: analysis.clone(),
                target_bytes,
                strategy,
            })
            .unwrap();
            assert!(plan.feasible);
            let output = dir.path().join("unused-output");
            let request = ProcessRequest {
                analysis: analysis.clone(),
                target_bytes,
                strategy,
                output_path: output.to_string_lossy().into(),
            };
            let result = process(&request).unwrap();
            assert_eq!(result.outcome, ProcessOutcome::NoChange);
            assert_eq!(result.output_bytes, before.len() as u64);
            assert!(result.verified);
            assert!(!output.exists());
            assert_eq!(fs::read(&path).unwrap(), before);
            assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
            let cancelled = process_with_progress(&request, |progress| progress.percent < 20);
            assert_eq!(cancelled.unwrap_err(), PROCESS_CANCELLED);
            let too_small = ProcessRequest {
                target_bytes: analysis.size_bytes - 1,
                ..request
            };
            assert!(build(&PlanRequest {
                analysis: analysis.clone(),
                target_bytes: too_small.target_bytes,
                strategy
            })
            .is_err());
            assert!(process(&too_small).is_err());
        }
    }
}

#[test]
fn retained_sources_are_remeasured_and_corrupt_changes_rejected() {
    let dir = tempfile::tempdir().unwrap();
    for video in [false, true] {
        let path = dir
            .path()
            .join(if video { "changed.mp4" } else { "changed.png" });
        for strategy in STRATEGIES {
            for mutation in ["under", "over", "corrupt"] {
                fixture(&path, video);
                let analysis = analyze(path.to_str().unwrap()).unwrap();
                let target_bytes = 64 * 1024;
                let plan = build(&PlanRequest {
                    analysis: analysis.clone(),
                    target_bytes,
                    strategy,
                })
                .unwrap();
                assert!(plan.feasible);
                assert_eq!(plan.already_fits, strategy == CompressionStrategy::Precise);
                if mutation == "corrupt" {
                    fs::write(&path, b"no longer a readable media file").unwrap();
                } else {
                    let padding = if mutation == "over" {
                        target_bytes
                    } else {
                        100
                    };
                    fs::OpenOptions::new()
                        .append(true)
                        .open(&path)
                        .unwrap()
                        .write_all(&vec![0; padding as usize])
                        .unwrap();
                }
                let current = fs::read(&path).unwrap();
                let output = dir.path().join(if video {
                    "must-not-exist.mp4"
                } else {
                    "must-not-exist.png"
                });
                let result = process(&ProcessRequest {
                    analysis,
                    target_bytes,
                    strategy,
                    output_path: output.to_string_lossy().into(),
                });
                if mutation == "under" {
                    let kept = result.unwrap();
                    assert_eq!(kept.outcome, ProcessOutcome::NoChange);
                    assert_eq!(kept.output_bytes, current.len() as u64);
                    assert!(kept.verified);
                } else {
                    assert!(
                        result.is_err(),
                        "{video:?} {strategy:?} {mutation}: {result:?}"
                    );
                }
                assert_eq!(fs::read(&path).unwrap(), current);
                assert!(!output.exists());
            }
        }
    }
}

#[test]
fn oversized_sources_still_obey_encoding_floors() {
    let dir = tempfile::tempdir().unwrap();
    for video in [false, true] {
        let path = dir.path().join(if video {
            "oversized.mp4"
        } else {
            "oversized.png"
        });
        fixture(&path, video);
        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&vec![0; 64 * 1024])
            .unwrap();
        let analysis = analyze(path.to_str().unwrap()).unwrap();
        for strategy in STRATEGIES {
            let target_bytes = 10 * 1024;
            let plan = build(&PlanRequest {
                analysis: analysis.clone(),
                target_bytes,
                strategy,
            })
            .unwrap();
            assert!(!plan.feasible);
            assert!(process(&ProcessRequest {
                analysis: analysis.clone(),
                target_bytes,
                strategy,
                output_path: dir.path().join("absent").to_string_lossy().into()
            })
            .is_err());
        }
    }
}
