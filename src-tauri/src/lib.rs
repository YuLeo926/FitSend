use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use fitsend_core::{
    BatchBudget, BatchBudgetRequest, CompressionPlan, MediaAnalysis, PlanRequest, ProcessProgress,
    ProcessRequest, ProcessResult,
};
use serde::Serialize;
use tauri::Emitter;

#[derive(Clone, Default)]
struct JobRegistry(Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JobProgress {
    job_id: String,
    percent: u8,
    stage: String,
    encoded_seconds: Option<f64>,
    attempt: u8,
}

impl JobProgress {
    fn from_progress(job_id: String, progress: ProcessProgress) -> Self {
        Self {
            job_id,
            percent: progress.percent,
            stage: progress.stage,
            encoded_seconds: progress.encoded_seconds,
            attempt: progress.attempt,
        }
    }
}

#[tauri::command]
fn analyze_media(path: String) -> Result<MediaAnalysis, String> {
    fitsend_core::analyze(&path)
}

#[tauri::command]
fn build_plan(request: PlanRequest) -> Result<CompressionPlan, String> {
    fitsend_core::build(&request)
}

#[tauri::command]
fn build_batch_budget(request: BatchBudgetRequest) -> Result<BatchBudget, String> {
    fitsend_core::build_budget(&request)
}

#[tauri::command]
fn rebalance_batch_budget(request: BatchBudgetRequest) -> Result<BatchBudget, String> {
    fitsend_core::rebalance_budget(&request)
}

#[tauri::command]
async fn process_media(
    app: tauri::AppHandle,
    jobs: tauri::State<'_, JobRegistry>,
    job_id: String,
    request: ProcessRequest,
) -> Result<ProcessResult, String> {
    let cancellation = Arc::new(AtomicBool::new(false));
    {
        let mut active = jobs
            .0
            .lock()
            .map_err(|_| "FitSend could not access the active job registry.".to_string())?;
        if let Some(previous) = active.insert(job_id.clone(), cancellation.clone()) {
            previous.store(true, Ordering::Relaxed);
        }
    }

    let registry = jobs.inner().clone();
    let worker_job_id = job_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        fitsend_core::process_with_progress(&request, |progress| {
            if cancellation.load(Ordering::Relaxed) {
                return false;
            }
            let _ = app.emit(
                "fitsend://process-progress",
                JobProgress::from_progress(worker_job_id.clone(), progress),
            );
            true
        })
    })
    .await
    .map_err(|error| format!("The processing task stopped unexpectedly: {error}"));

    if let Ok(mut active) = registry.0.lock() {
        active.remove(&job_id);
    }

    result?
}

#[tauri::command]
fn cancel_process(jobs: tauri::State<'_, JobRegistry>, job_id: String) -> Result<bool, String> {
    let active = jobs
        .0
        .lock()
        .map_err(|_| "FitSend could not access the active job registry.".to_string())?;
    if let Some(cancellation) = active.get(&job_id) {
        cancellation.store(true, Ordering::Relaxed);
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    fitsend_core::cleanup_stale_outputs();
    tauri::Builder::default()
        .manage(JobRegistry::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            analyze_media,
            build_plan,
            build_batch_budget,
            rebalance_batch_budget,
            process_media,
            cancel_process
        ])
        .run(tauri::generate_context!())
        .expect("error while running FitSend");
}
