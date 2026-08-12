mod analyzer;
mod domain;
mod planner;
mod processor;
mod progress;
mod toolchain;

pub use analyzer::analyze;
pub use domain::{
    CompressionPlan, MediaAnalysis, MediaKind, PlanRequest, ProcessRequest, ProcessResult,
};
pub use planner::build;
pub use processor::{process, process_with_progress};
pub use progress::{ProcessProgress, PROCESS_CANCELLED};

pub fn video_tools_available() -> bool {
    toolchain::available("ffmpeg") && toolchain::available("ffprobe")
}
