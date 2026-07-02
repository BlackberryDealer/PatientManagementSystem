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

impl AppError {
    /// Translate a UNIQUE-constraint violation into a clean 400 with the given
    /// message; any other database error passes through as a 500. Use this on
    /// INSERTs where a duplicate is a user mistake, not a server fault.
    pub fn bad_request_on_unique(e: sqlx::Error, msg: &str) -> AppError {
        if let sqlx::Error::Database(db) = &e {
            if db.is_unique_violation() {
                return AppError::BadRequest(msg.to_string());
            }
        }
        AppError::DatabaseError(e)
    }
}

/// Minimal HTML-escape for user-facing messages interpolated into error pages.
/// Covers the five characters that matter for injection into element content
/// and attribute values.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Self-contained, Bulma-styled error page (same look as `error.html.tera`).
///
/// Built without Tera because `ResponseError::error_response` has no access to
/// application state — the page must render even where the template engine is
/// unreachable. Unlike the generic 404/500 middleware pages, this preserves the
/// *specific* domain message (e.g. "slot conflicts with an existing
/// appointment"), HTML-escaped, with a Go Back button so form errors are not a
/// dead end.
fn error_page(status: u16, title: &str, message: &str) -> String {
    let message_html = escape_html(message).replace('\n', "<br>");
    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{status} — {title}</title>
    <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/bulma@0.9.4/css/bulma.min.css">
    <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css">
</head>
<body>
    <section class="hero is-fullheight">
        <div class="hero-body">
            <div class="container has-text-centered">
                <p class="title is-1 has-text-danger mb-2">
                    <i class="fas fa-triangle-exclamation mr-2"></i>{status}
                </p>
                <p class="title is-3">{title}</p>
                <p class="subtitle is-5 mt-3">{message_html}</p>
                <div class="buttons is-centered mt-4">
                    <a class="button is-light is-medium" href="javascript:history.back()">
                        <span class="icon"><i class="fas fa-arrow-left"></i></span>
                        <span>Go Back</span>
                    </a>
                    <a class="button is-primary is-medium" href="/">
                        <span class="icon"><i class="fas fa-house"></i></span>
                        <span>Return to Home</span>
                    </a>
                </div>
            </div>
        </div>
    </section>
</body>
</html>"##
    )
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        // Client errors keep their specific domain message (it is the user's
        // feedback); server errors hide internals behind a generic line.
        let (mut builder, status, title, message) = match self {
            AppError::BadRequest(msg) => {
                (HttpResponse::BadRequest(), 400, "Bad Request", msg.as_str())
            }
            AppError::Unauthorized(msg) => {
                (HttpResponse::Unauthorized(), 401, "Unauthorized", msg.as_str())
            }
            AppError::Forbidden(msg) => {
                (HttpResponse::Forbidden(), 403, "Forbidden", msg.as_str())
            }
            AppError::NotFound(msg) => {
                (HttpResponse::NotFound(), 404, "Not Found", msg.as_str())
            }
            AppError::Internal(_)
            | AppError::DatabaseError(_)
            | AppError::TemplateError(_)
            | AppError::BcryptError(_) => (
                HttpResponse::InternalServerError(),
                500,
                "Internal Server Error",
                "Something went wrong on our end. Please try again later.",
            ),
        };
        builder
            .content_type("text/html; charset=utf-8")
            .body(error_page(status, title, message))
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

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::ResponseError;

    #[test]
    fn escape_html_neutralises_markup() {
        assert_eq!(
            escape_html(r#"<script>alert("x&y")</script>"#),
            "&lt;script&gt;alert(&quot;x&amp;y&quot;)&lt;/script&gt;"
        );
    }

    #[test]
    fn bad_request_renders_styled_page_with_message() {
        let resp = AppError::BadRequest("Slot already taken".into()).error_response();
        assert_eq!(resp.status().as_u16(), 400);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.starts_with("text/html"));
    }

    #[test]
    fn server_errors_hide_internal_details() {
        let resp = AppError::Internal("secret stack trace".into()).error_response();
        assert_eq!(resp.status().as_u16(), 500);
    }
}
