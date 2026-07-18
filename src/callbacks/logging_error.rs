use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorInformation {
    pub error_type: String,
    pub message: String,
    pub trace: String,
}

pub fn error_information(error_type: &str, message: String) -> ErrorInformation {
    ErrorInformation {
        error_type: error_type.to_owned(),
        message,
        trace: std::backtrace::Backtrace::force_capture().to_string(),
    }
}
