use actix_web::{HttpResponse, ResponseError};
use std::fmt;

/// Unified error type for the entire application.
/// Implements `actix_web::ResponseError` so it can be returned directly from handlers.
#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Internal(String),
    DatabaseError(sqlx::Error),
    TemplateError(tera::Error),
    BcryptError(bcrypt::BcryptError),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::BadRequest(msg) => write!(f, "Bad Request: {}", msg),
            AppError::Unauthorized(msg) => write!(f, "Unauthorized: {}", msg),
            AppError::Forbidden(msg) => write!(f, "Forbidden: {}", msg),
            AppError::NotFound(msg) => write!(f, "Not Found: {}", msg),
            AppError::Internal(msg) => write!(f, "Internal Server Error: {}", msg),
            AppError::DatabaseError(e) => write!(f, "Database Error: {}", e),
            AppError::TemplateError(e) => write!(f, "Template Error: {}", e),
            AppError::BcryptError(e) => write!(f, "Bcrypt Error: {}", e),
        }
    }
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        match self {
            AppError::BadRequest(msg) => HttpResponse::BadRequest().body(msg.clone()),
            AppError::Unauthorized(msg) => HttpResponse::Unauthorized().body(msg.clone()),
            AppError::Forbidden(msg) => HttpResponse::Forbidden().body(msg.clone()),
            AppError::NotFound(msg) => HttpResponse::NotFound().body(msg.clone()),
            AppError::Internal(_) => {
                HttpResponse::InternalServerError().body("Internal Server Error")
            }
            AppError::DatabaseError(_) => {
                HttpResponse::InternalServerError().body("A database error occurred")
            }
            AppError::TemplateError(_) => {
                HttpResponse::InternalServerError().body("A template rendering error occurred")
            }
            AppError::BcryptError(_) => {
                HttpResponse::InternalServerError().body("Internal Server Error")
            }
        }
    }
}

// -- From impls so we can use `?` with these error types --

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::DatabaseError(e)
    }
}

impl From<tera::Error> for AppError {
    fn from(e: tera::Error) -> Self {
        AppError::TemplateError(e)
    }
}

impl From<bcrypt::BcryptError> for AppError {
    fn from(e: bcrypt::BcryptError) -> Self {
        AppError::BcryptError(e)
    }
}

impl From<actix_web::Error> for AppError {
    fn from(e: actix_web::Error) -> Self {
        AppError::Internal(format!("Actix error: {}", e))
    }
}

impl From<actix_session::SessionInsertError> for AppError {
    fn from(e: actix_session::SessionInsertError) -> Self {
        AppError::Internal(format!("Session insert error: {}", e))
    }
}

impl From<actix_session::SessionGetError> for AppError {
    fn from(e: actix_session::SessionGetError) -> Self {
        AppError::Internal(format!("Session get error: {}", e))
    }
}
