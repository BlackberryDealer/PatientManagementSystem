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
            // POST only: logout mutates state, so it must not be reachable
            // via a plain link/prefetch (see handlers::logout).
            .route("/logout", web::post().to(handlers::logout))
            .route("", web::get().to(handlers::list_users))
            // `/new` must precede `/{id}` so it isn't captured as a dynamic id.
            .route("/new", web::get().to(handlers::create_staff_form))
            .route("/new", web::post().to(handlers::create_staff))
            .route("/{id}", web::get().to(handlers::user_profile))
            .route("/{id}/edit", web::get().to(handlers::edit_profile_form))
            .route("/{id}/edit", web::post().to(handlers::edit_profile)),
    );
}
