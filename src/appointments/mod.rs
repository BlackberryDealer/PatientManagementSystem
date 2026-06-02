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
            .route("/book/priority", web::post().to(handlers::book_with_priority))
            .route("/suggest", web::get().to(handlers::suggest_slot_form))
            .route("/suggest", web::post().to(handlers::suggest_slot))
            .route("/waitlist", web::get().to(handlers::list_waitlist))
            .route("/waitlist/join", web::post().to(handlers::join_waitlist))
            .route("/waitlist/{id}/promote", web::post().to(handlers::promote_waitlist))
            .route("/{id}", web::get().to(handlers::appointment_detail))
            .route("/{id}/cancel", web::post().to(handlers::cancel_appointment)),
    );
}
