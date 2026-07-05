pub mod handlers;
pub mod models;
pub mod services;

use actix_web::web;

/// Mount all availability-related routes under `/availability`.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/availability")
            .route("", web::get().to(handlers::list_availability))
            .route("/set", web::get().to(handlers::set_availability_form))
            .route("/set", web::post().to(handlers::set_availability))
            .route("/{id}/edit", web::get().to(handlers::edit_availability_form))
            .route("/{id}/edit", web::post().to(handlers::edit_availability))
            .route("/{id}/delete", web::post().to(handlers::delete_availability)),
    );
}
