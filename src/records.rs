pub mod handlers;
pub mod models;
pub mod pdf;
pub mod services;

use actix_web::web;

/// Mount all medical-record routes under `/records`.
pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/records")
            .route("", web::get().to(handlers::list_records))
            .route("/create", web::get().to(handlers::create_record_form))
            .route("/create", web::post().to(handlers::create_record))
            .route("/timeline", web::get().to(handlers::patient_timeline))
            .route("/prescriptions/create", web::get().to(handlers::prescription_form))
            .route("/prescriptions/create", web::post().to(handlers::create_prescription))
            .route("/{id}", web::get().to(handlers::record_detail))
            .route("/{id}/report", web::get().to(handlers::record_report))
            .route("/{id}/report.pdf", web::get().to(handlers::record_report_pdf)),
    );
}
