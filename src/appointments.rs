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
            .route("/suggest", web::get().to(handlers::suggest_slot_form))
            .route("/suggest", web::post().to(handlers::suggest_slot))
            // JSON slot lookups for the booking form's live dropdown: free-only
            // for patients, every slot (marked free/occupied) for staff overrides.
            .route("/availability", web::get().to(handlers::available_slots_api))
            .route("/all-slots", web::get().to(handlers::all_slots_api))
            // Batch reassignment (Algorithm 5) — registered before `/{id}` so
            // the static path is not captured by the dynamic id route.
            .route("/reassign-day", web::get().to(handlers::reassign_day_form))
            .route("/reassign-day", web::post().to(handlers::reassign_day_preview))
            .route("/reassign-day/apply", web::post().to(handlers::reassign_day_apply))
            .route("/waitlist", web::get().to(handlers::list_waitlist))
            .route("/waitlist/join", web::post().to(handlers::join_waitlist))
            .route("/waitlist/{id}/promote", web::post().to(handlers::promote_waitlist))
            .route("/calendar", web::get().to(handlers::calendar_view))
            .route("/{id}", web::get().to(handlers::appointment_detail))
            .route("/{id}/reschedule", web::get().to(handlers::reschedule_form))
            .route("/{id}/reschedule", web::post().to(handlers::reschedule_appointment))
            .route("/{id}/cancel", web::post().to(handlers::cancel_appointment))
            .route("/{id}/complete", web::post().to(handlers::complete_appointment))
            .route("/{id}/assign-room", web::post().to(handlers::assign_room))
            .route("/{id}/priority", web::post().to(handlers::update_priority))
            .route("/{id}/reassign", web::post().to(handlers::reassign_appointment)),
    );
}
