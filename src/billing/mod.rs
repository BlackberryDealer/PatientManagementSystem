pub mod handlers;
pub mod models;
pub mod services;

use actix_web::web;

/// Mount all billing-related routes under `/billing`.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/billing")
            .route("", web::get().to(handlers::list_invoices))
            .route("/create", web::get().to(handlers::create_invoice_form))
            .route("/create", web::post().to(handlers::create_invoice))
            .route("/{id}", web::get().to(handlers::invoice_detail))
            .route("/{id}/pay", web::post().to(handlers::record_payment)),
    );
}
