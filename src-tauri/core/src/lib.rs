mod analyzer;
mod batch_budget;
mod domain;
mod image_processor;
mod output;
mod planner;
mod processor;
mod progress;
pub mod quality;
mod toolchain;
mod video_processor;

pub use analyzer::analyze;
pub use batch_budget::{build_budget, rebalance_budget, MIN_ITEM_BUDGET_BYTES};
pub use domain::{
    AcceptedBudgetItem, BatchBudget, BatchBudgetRequest, BudgetItemRequest, CompressionPlan,
    CompressionStrategy, ItemAllocation, LimitScope, MediaAnalysis, MediaKind, PlanRequest,
    ProcessOutcome, ProcessRequest, ProcessResult,
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
