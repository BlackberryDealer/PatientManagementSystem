mod auth;
mod db;
mod errors;

mod users;
mod appointments;
mod availability;
mod records;
mod billing;

use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{cookie::Key, web, App, HttpResponse, HttpServer};
use log::{info, warn};
use std::fs;
use std::io::Write;

use crate::auth::OptionalAuthUser;

// ============================================================
// Template loader: loads all .tera files from modules' templates/
// ============================================================

fn load_module_templates(tera: &mut tera::Tera) -> Result<(), tera::Error> {
    let modules = ["users", "appointments", "availability", "records", "billing"];
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
    tera.autoescape_on(vec![]); // disable auto-escaping for .tera files (they are HTML)

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
    })
    .bind("0.0.0.0:8080")?
    .run()
    .await
}
