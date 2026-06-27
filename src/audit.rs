pub mod handlers;
pub mod models;
pub mod services;

use actix_web::web;

/// Mount the audit-trail routes under `/audit` (admin only).
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/audit")
            .route("", web::get().to(handlers::list_audit_log)),
    );
}
