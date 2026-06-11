pub mod handlers;
pub mod models;
pub mod services;

use actix_web::web;

/// Mount the analytics dashboard under `/dashboard` (admin only).
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/dashboard")
            .route("", web::get().to(handlers::show_dashboard)),
    );
}
