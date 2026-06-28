use hiptty_core::{AdapterError, ErrorCode};
use serde::Serialize;

pub const SCHEMA_VERSION: u32 = 1;

/// Exit codes for agent / script consumers.
pub mod exit {
    pub const SUCCESS: i32 = 0;
    pub const BUSINESS_ERROR: i32 = 1;
    pub const USAGE_ERROR: i32 = 2;
    pub const NETWORK_ERROR: i32 = 3;
}

#[derive(Debug, Serialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Serialize)]
pub struct Response<T: Serialize> {
    pub schema_version: u32,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorBody>,
}

impl<T: Serialize> Response<T> {
    pub fn success(data: T) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            ok: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn failure(error: AdapterError) -> Response<()> {
        Response {
            schema_version: SCHEMA_VERSION,
            ok: false,
            data: None,
            error: Some(ErrorBody {
                code: error.code(),
                message: error.to_string(),
                retryable: error.code().retryable(),
            }),
        }
    }
}

pub fn exit_code_for_error(error: &AdapterError) -> i32 {
    match error {
        AdapterError::Network(_) => exit::NETWORK_ERROR,
        AdapterError::InvalidInput(_) => exit::USAGE_ERROR,
        _ => exit::BUSINESS_ERROR,
    }
}

pub fn print_json<T: Serialize>(value: &T) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).expect("serialize response")
    );
}

pub fn print_human_ok(message: &str) {
    println!("{message}");
}

pub fn print_human_error(error: &AdapterError) {
    eprintln!("error [{:?}]: {error}", error.code());
}
