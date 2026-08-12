use serde::{Deserialize, Serialize};

pub const PROCESS_CANCELLED: &str = "PROCESS_CANCELLED";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProcessProgress {
    pub percent: u8,
    pub stage: String,
    pub encoded_seconds: Option<f64>,
    pub attempt: u8,
}

pub(crate) fn report(
    callback: &mut dyn FnMut(ProcessProgress) -> bool,
    percent: u8,
    stage: impl Into<String>,
    encoded_seconds: Option<f64>,
    attempt: u8,
) -> Result<(), String> {
    let event = ProcessProgress {
        percent: percent.min(100),
        stage: stage.into(),
        encoded_seconds,
        attempt: attempt.max(1),
    };
    if callback(event) {
        Ok(())
    } else {
        Err(PROCESS_CANCELLED.to_string())
    }
}
