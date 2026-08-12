use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MediaKind {
    Image,
    Video,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaAnalysis {
    pub path: String,
    pub name: String,
    pub extension: String,
    pub kind: MediaKind,
    pub size_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub duration_seconds: Option<f64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub has_audio: bool,
    pub has_alpha: bool,
    pub ffmpeg_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRequest {
    pub analysis: MediaAnalysis,
    pub target_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRequest {
    pub analysis: MediaAnalysis,
    pub target_bytes: u64,
    pub output_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressionPlan {
    pub already_fits: bool,
    pub feasible: bool,
    pub target_bytes: u64,
    pub estimated_bytes: u64,
    pub operation: String,
    pub summary: String,
    pub quality_label: String,
    pub warnings: Vec<String>,
    pub output_extension: String,
    pub video_bitrate_kbps: Option<u64>,
    pub audio_bitrate_kbps: Option<u64>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessResult {
    pub output_path: String,
    pub output_bytes: u64,
    pub target_bytes: u64,
    pub verified: bool,
    pub attempts: u8,
    pub width: u32,
    pub height: u32,
    pub duration_ms: u64,
}
