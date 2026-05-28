pub mod handlers;
pub mod models;
pub mod services;

use actix_web::web;

/// Mount all user-related routes under `/users`.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/users")
            .route("/register", web::get().to(handlers::register_form))
            .route("/register", web::post().to(handlers::register))
            .route("/login", web::get().to(handlers::login_form))
            .route("/login", web::post().to(handlers::login))
            .route("/logout", web::get().to(handlers::logout))
            .route("", web::get().to(handlers::list_users))
            .route("/{id}", web::get().to(handlers::user_profile)),
    );
}
