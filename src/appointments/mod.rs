pub mod handlers;
pub mod models;
pub mod services;

use actix_web::web;

/// Mount all appointment-related routes under `/appointments`.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/appointments")
            .route("", web::get().to(handlers::list_appointments))
            .route("/book", web::get().to(handlers::book_form))
            .route("/book", web::post().to(handlers::book_appointment))
            .route("/{id}", web::get().to(handlers::appointment_detail))
            .route("/{id}/cancel", web::post().to(handlers::cancel_appointment)),
    );
}
