use patient_management_system as pms;
use pms::auth::OptionalAuthUser;
use pms::{appointments, audit, availability, billing, dashboard, db, records, users};

use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::dev::ServiceResponse;
use actix_web::http::StatusCode;
use actix_web::middleware::{
    Compress, ErrorHandlerResponse, ErrorHandlers, Logger, NormalizePath, TrailingSlash,
};
use actix_web::{cookie::Key, web, App, HttpResponse, HttpServer};
use log::{info, warn};
use std::fs;
use std::io::Write;

// ============================================================
// Template loader: loads all .tera files from modules' templates/
// ============================================================

fn load_module_templates(tera: &mut tera::Tera) -> Result<(), tera::Error> {
    let modules = ["users", "appointments", "availability", "records", "billing", "audit", "dashboard"];
    for module in &modules {
        let dir_path = format!("src/{}/templates", module);
        if let Ok(entries) = fs::read_dir(&dir_path) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.extension().map_or(false, |ext| ext == "tera") {
                        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                            let content = fs::read_to_string(&path).map_err(|e| {
                                tera::Error::chain(
                                    format!("Failed to read template: {}", path.display()),
                                    e,
                                )
                            })?;
                            let template_name = format!("{}/{}", module, file_name);
                            tera.add_raw_template(&template_name, &content)?;
                            log::debug!("Loaded template: {}", template_name);
                        }
                    }
                }
            }
        } else {
            log::warn!("Template directory not found: {}", dir_path);
        }
    }
    Ok(())
}

// ============================================================
// Root / handlers
// ============================================================

async fn index(user: OptionalAuthUser) -> HttpResponse {
    match user.0 {
        Some(_) => HttpResponse::SeeOther()
            .append_header(("Location", "/appointments"))
            .finish(),
        None => HttpResponse::SeeOther()
            .append_header(("Location", "/users/login"))
            .finish(),
    }
}

// ============================================================
// Persistent session key (survives server restarts)
// ============================================================

/// Get the session signing key from `SESSION_SECRET` env var,
/// or generate a new one and append it to `.env` for persistence.
fn get_or_create_secret_key() -> Key {
    // Re-read .env in case it was updated (dotenv already called in main)
    if let Ok(secret) = std::env::var("SESSION_SECRET") {
        if secret.len() >= 64 {
            return Key::from(secret.as_bytes());
        }
        warn!("SESSION_SECRET too short (< 64 chars), generating a new one.");
    }

    // Generate 64 random bytes → 128 hex chars
    let mut bytes = [0u8; 64];
    getrandom::getrandom(&mut bytes).expect("Failed to generate random bytes for session key");
    let new_secret: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();

    // Append to .env file for persistence across restarts
    if let Ok(mut file) = fs::OpenOptions::new().append(true).create(true).open(".env") {
        let _ = writeln!(file, "\n# Auto-generated session signing key\nSESSION_SECRET={}", new_secret);
        info!("Generated and saved new SESSION_SECRET to .env");
    }

    Key::from(new_secret.as_bytes())
}

// ============================================================
// Custom error pages (ErrorHandlers middleware)
// ============================================================

/// Replace an error response body with a friendly, SSR-rendered HTML page.
/// Renders `error.html.tera`; falls back to inline HTML if Tera is unavailable.
fn render_error_page<B>(
    res: ServiceResponse<B>,
    title: &str,
    message: &str,
) -> Result<ErrorHandlerResponse<B>, actix_web::Error> {
    let status = res.status();

    let html = res
        .request()
        .app_data::<web::Data<tera::Tera>>()
        .and_then(|tera| {
            let mut ctx = tera::Context::new();
            ctx.insert("status_code", &status.as_u16());
            ctx.insert("error_title", title);
            ctx.insert("error_message", message);
            tera.render("error.html.tera", &ctx).ok()
        })
        .unwrap_or_else(|| {
            let code = status.as_u16();
            format!(
                "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"UTF-8\">\
                 <title>{code} {title}</title></head>\
                 <body><h1>{code} {title}</h1><p>{message}</p>\
                 <a href=\"/\">Return home</a></body></html>"
            )
        });

    let new_response = HttpResponse::build(status)
        .content_type("text/html; charset=utf-8")
        .body(html);

    Ok(ErrorHandlerResponse::Response(
        res.into_response(new_response).map_into_right_body(),
    ))
}

/// 404 handler — styled "page not found" screen.
fn handle_not_found<B>(res: ServiceResponse<B>) -> Result<ErrorHandlerResponse<B>, actix_web::Error> {
    render_error_page(
        res,
        "Page Not Found",
        "The page you are looking for does not exist or may have been moved.",
    )
}

/// 500 handler — styled "internal server error" screen (hides internal details).
fn handle_internal_error<B>(
    res: ServiceResponse<B>,
) -> Result<ErrorHandlerResponse<B>, actix_web::Error> {
    render_error_page(
        res,
        "Internal Server Error",
        "Something went wrong on our end. Please try again later.",
    )
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load .env file (silently ignore if missing)
    dotenv::dotenv().ok();

    // Initialize logger (controlled by RUST_LOG env var)
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    // Database setup
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:patient_management.db?mode=rwc".to_string());

    let pool = db::create_pool(&database_url).await;
    db::run_migrations(&pool).await;

    // Template engine setup
    let mut tera = match tera::Tera::new("templates/**/*.tera") {
        Ok(t) => t,
        Err(e) => {
            log::error!("Failed to load root templates: {}", e);
            log::error!(
                "Make sure the 'templates/' directory exists with at least base.html.tera"
            );
            std::process::exit(1);
        }
    };

    load_module_templates(&mut tera).expect("Failed to load module templates");

    // Auto-register tera in debug mode for easy template debugging
    // Enable auto-escaping for all .tera templates to prevent XSS.
    // Use the `| safe` filter in templates only for trusted, pre-escaped HTML.
    tera.autoescape_on(vec![".tera"]);

    // Session encryption key (persisted in .env across restarts)
    let secret_key = get_or_create_secret_key();

    info!("============================================");
    info!("  Patient Management System");
    info!("  University of Glasgow — CSC1106");
    info!("  Server: http://0.0.0.0:8080");
    info!("============================================");

    HttpServer::new(move || {
        App::new()
            // Shared state
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(tera.clone()))
            // --- Middleware (registered first = innermost = processes response first) ---
            // ErrorHandlers: replace 404/500 bodies with friendly SSR pages.
            // Registered first so the new body is produced before Compress runs.
            .wrap(
                ErrorHandlers::new()
                    .handler(StatusCode::NOT_FOUND, handle_not_found)
                    .handler(StatusCode::INTERNAL_SERVER_ERROR, handle_internal_error),
            )
            // Logger: records method, path, status, and response time for every request.
            .wrap(Logger::default())
            // Compress: gzip/brotli-compresses responses based on Accept-Encoding.
            .wrap(Compress::default())
            // NormalizePath: treats "/appointments" and "/appointments/" as identical.
            .wrap(NormalizePath::new(TrailingSlash::Trim))
            // Session middleware (signed cookies)
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), secret_key.clone())
                    .cookie_secure(false) // allow HTTP in development
                    .build(),
            )
            // Routes
            .route("/", web::get().to(index))
            .configure(users::configure)
            .configure(appointments::configure)
            .configure(availability::configure)
            .configure(records::configure)
            .configure(billing::configure)
            .configure(audit::configure)
            .configure(dashboard::configure)
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
