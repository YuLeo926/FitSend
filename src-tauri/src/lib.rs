use fitsend_core::{CompressionPlan, MediaAnalysis, PlanRequest, ProcessRequest, ProcessResult};

#[tauri::command]
fn analyze_media(path: String) -> Result<MediaAnalysis, String> {
    fitsend_core::analyze(&path)
}

#[tauri::command]
fn build_plan(request: PlanRequest) -> Result<CompressionPlan, String> {
    fitsend_core::build(&request)
}

#[tauri::command]
async fn process_media(request: ProcessRequest) -> Result<ProcessResult, String> {
    tauri::async_runtime::spawn_blocking(move || fitsend_core::process(&request))
        .await
        .map_err(|error| format!("The processing task stopped unexpectedly: {error}"))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            analyze_media,
            build_plan,
            process_media
        ])
        .run(tauri::generate_context!())
        .expect("error while running FitSend");
}
