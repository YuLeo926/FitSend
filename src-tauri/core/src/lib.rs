mod analyzer;
mod domain;
mod planner;
mod processor;
mod toolchain;

pub use analyzer::analyze;
pub use domain::{
    CompressionPlan, MediaAnalysis, MediaKind, PlanRequest, ProcessRequest, ProcessResult,
};
pub use planner::build;
pub use processor::process;

pub fn video_tools_available() -> bool {
    toolchain::available("ffmpeg") && toolchain::available("ffprobe")
}
