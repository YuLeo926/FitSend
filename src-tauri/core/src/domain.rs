use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum MediaKind {
    Image,
    Video,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CompressionStrategy {
    Precise,
    Balanced,
    Smallest,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProcessOutcome {
    Created,
    NoChange,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LimitScope {
    PerFile,
    BatchTotal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetItemRequest {
    pub id: String,
    pub source_bytes: u64,
    pub minimum_allocation_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcceptedBudgetItem {
    pub id: String,
    pub actual_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchBudgetRequest {
    pub scope: LimitScope,
    pub ceiling_bytes: u64,
    pub items: Vec<BudgetItemRequest>,
    pub accepted: Vec<AcceptedBudgetItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ItemAllocation {
    pub id: String,
    pub target_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchBudget {
    pub scope: LimitScope,
    pub ceiling_bytes: u64,
    pub accepted_bytes: u64,
    pub remaining_bytes: u64,
    pub allocations: Vec<ItemAllocation>,
    pub feasible: bool,
    pub reason: Option<String>,
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
    pub frame_rate: Option<f64>,
    pub rotation_degrees: i32,
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
    pub strategy: CompressionStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRequest {
    pub analysis: MediaAnalysis,
    pub target_bytes: u64,
    pub output_path: String,
    pub strategy: CompressionStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressionPlan {
    pub strategy: CompressionStrategy,
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
    pub outcome: ProcessOutcome,
    pub reason: String,
    pub quality_score: Option<f64>,
}
