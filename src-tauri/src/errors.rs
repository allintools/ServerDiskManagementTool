use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
    pub retryable: bool,
}

impl AppError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into(), detail: None, retryable: false }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn retryable(mut self) -> Self {
        self.retryable = true;
        self
    }

    pub fn ssh(message: impl Into<String>) -> Self {
        Self::new("SSH_ERROR", message)
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("INVALID_INPUT", message)
    }

    pub fn db(message: impl Into<String>) -> Self {
        Self::new("DB_ERROR", message)
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AppError {}

impl From<AppError> for String {
    fn from(value: AppError) -> Self {
        value.message
    }
}

impl From<String> for AppError {
    fn from(value: String) -> Self {
        Self::new("UNKNOWN", value)
    }
}

impl From<&str> for AppError {
    fn from(value: &str) -> Self {
        Self::new("UNKNOWN", value)
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(value: rusqlite::Error) -> Self {
        Self::new("DB_ERROR", format!("数据库操作失败: {value}"))
    }
}

pub type AppResult<T> = Result<T, AppError>;
