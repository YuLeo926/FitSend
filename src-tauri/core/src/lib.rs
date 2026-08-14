mod analyzer;
mod domain;
mod image_processor;
mod output;
mod planner;
mod processor;
mod progress;
pub mod quality;
mod toolchain;

pub use analyzer::analyze;
pub use domain::{
    CompressionPlan, CompressionStrategy, MediaAnalysis, MediaKind, PlanRequest, ProcessOutcome,
    ProcessRequest, ProcessResult,
};
pub use planner::build;
pub use processor::{process, process_with_progress};
pub use progress::{ProcessProgress, PROCESS_CANCELLED};

pub fn video_tools_available() -> bool {
    toolchain::available("ffmpeg") && toolchain::available("ffprobe")
}

pub fn cleanup_stale_outputs() {
    output::cleanup_stale_outputs();
}
