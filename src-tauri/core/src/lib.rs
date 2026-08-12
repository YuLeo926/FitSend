mod analyzer;
mod domain;
mod planner;
mod processor;

pub use analyzer::analyze;
pub use domain::{
    CompressionPlan, MediaAnalysis, MediaKind, PlanRequest, ProcessRequest, ProcessResult,
};
pub use planner::build;
pub use processor::process;
