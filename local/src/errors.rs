//! 错误类型 + OpenAI/Anthropic 兼容错误响应（移植自 tokenharbor2api 架构）

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, Clone)]
pub enum ApiError {
    BadRequest(String),
    Unauthorized(String),
    Upstream(String),
    NotFound(String),
    RateLimited(String),
    Internal(String),
}

impl ApiError {
    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::BadRequest(msg.into())
    }
    pub fn unauthorized(msg: impl Into<String>) -> Self {
        Self::Unauthorized(msg.into())
    }
    pub fn upstream(msg: impl Into<String>) -> Self {
        Self::Upstream(msg.into())
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }
    pub fn rate_limited(msg: impl Into<String>) -> Self {
        Self::RateLimited(msg.into())
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Upstream(_) => StatusCode::BAD_GATEWAY,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::RateLimited(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::BadRequest(m)
            | Self::Unauthorized(m)
            | Self::Upstream(m)
            | Self::NotFound(m)
            | Self::RateLimited(m)
            | Self::Internal(m) => m,
        }
    }

    pub fn err_type(&self) -> &'static str {
        match self {
            Self::BadRequest(_) => "invalid_request_error",
            Self::Unauthorized(_) => "authentication_error",
            Self::Upstream(_) => "upstream_error",
            Self::NotFound(_) => "not_found_error",
            Self::RateLimited(_) => "rate_limit_error",
            Self::Internal(_) => "internal_error",
        }
    }

    pub fn openai_json(&self) -> axum::Json<serde_json::Value> {
        axum::Json(serde_json::json!({
            "error": { "message": self.message(), "type": self.err_type(), "code": null }
        }))
    }

    pub fn anthropic_json(&self) -> axum::Json<serde_json::Value> {
        axum::Json(serde_json::json!({
            "type": "error",
            "error": { "type": self.err_type(), "message": self.message() }
        }))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = self.openai_json();
        (status, body).into_response()
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for ApiError {}
