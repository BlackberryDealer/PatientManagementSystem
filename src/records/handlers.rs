use actix_web::{web, HttpResponse};
use tera::Context;

use crate::audit::services as audit;
use crate::auth::{require_doctor, AuthUser, Role};
use crate::errors::AppError;
use crate::records::models::{CreateRecordForm, PrescriptionForm};
use crate::records::services;

/// GET /records — list records filtered by role
pub async fn list_records(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let records = match user.role {
        Role::Patient => services::get_records_for_patient(pool.get_ref(), user.user_id).await?,
        // Doctors and admins (clinical staff) see the full register.
        Role::Doctor | Role::Admin => services::get_all_records(pool.get_ref()).await?,
    };

    let prescriptions = if user.role == Role::Patient {
        Some(services::get_prescriptions_for_patient(pool.get_ref(), user.user_id).await?)
    } else {
        // Staff see the full prescription register (Prescription Tracking module)
        Some(services::get_all_prescriptions(pool.get_ref()).await?)
    };

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("records", &records);
    ctx.insert("prescriptions", &prescriptions);
    ctx.insert("title", "Medical Records");
    let rendered = tera.render("records/list.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// GET /records/create — show create record form (doctor only)
pub async fn create_record_form(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;

    let patients = services::get_all_patients(pool.get_ref()).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("patients", &patients);
    ctx.insert("title", "Create Medical Record");
    let rendered = tera.render("records/create.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /records/create — create a new medical record (doctor only)
pub async fn create_record(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<CreateRecordForm>,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;

    let record = services::create_record(pool.get_ref(), user.user_id, &form).await?;
    audit::record(
        pool.get_ref(), &user, "record.created", "medical_record", Some(record.id),
        &format!("Medical record for patient #{}", record.patient_id),
    ).await;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/records/{}", record.id)))
        .finish())
}

// ============================================================
// Prescription Tracking (doctor writes prescriptions)
// ============================================================

/// GET /records/prescriptions/create — show prescription form (doctor only)
pub async fn prescription_form(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;

    let patients = services::get_all_patients(pool.get_ref()).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("patients", &patients);
    ctx.insert("title", "Write Prescription");
    let rendered = tera.render("records/prescription_form.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /records/prescriptions/create — write a prescription (doctor only)
pub async fn create_prescription(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<PrescriptionForm>,
) -> Result<HttpResponse, AppError> {
    require_doctor(&user)?;

    let rx = services::create_prescription(pool.get_ref(), user.user_id, &form).await?;
    audit::record(
        pool.get_ref(), &user, "prescription.created", "prescription", Some(rx.id),
        &format!("{} for patient #{}", rx.medication_name, rx.patient_id),
    ).await;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", "/records"))
        .finish())
}

// ============================================================
// Patient History Timeline (advanced feature, project spec)
// ============================================================

/// GET /records/timeline — chronological patient history.
/// Patients see their own; staff pass `?patient_id=N`.
pub async fn patient_timeline(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    let patient_id = if user.role == Role::Patient {
        crate::db::get_patient_id(pool.get_ref(), user.user_id).await?
    } else {
        query
            .get("patient_id")
            .and_then(|p| p.parse().ok())
            .ok_or_else(|| {
                AppError::BadRequest(
                    "Specify a patient: /records/timeline?patient_id=N".into(),
                )
            })?
    };

    let patient_name = services::get_patient_name(pool.get_ref(), patient_id).await?;
    let events = services::build_patient_timeline(pool.get_ref(), patient_id).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("patient_name", &patient_name);
    ctx.insert("events", &events);
    ctx.insert("title", &format!("History: {}", patient_name));
    let rendered = tera.render("records/timeline.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

// ============================================================
// Medical Report Generation (advanced feature, project spec)
// ============================================================

/// GET /records/{id}/report — printable medical report for a record.
/// Same ownership rule as the record itself.
pub async fn record_report(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let record_id = path.into_inner();
    let record =
        services::get_record_checked(pool.get_ref(), record_id, user.user_id, user.role)
            .await?;
    let report = services::build_record_report(pool.get_ref(), record).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("report", &report);
    ctx.insert("title", &format!("Medical Report #{}", record_id));
    let rendered = tera.render("records/report.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// GET /records/{id}/report.pdf — download the same medical report as a PDF.
///
/// Reuses the exact data bundle the HTML report renders from, then streams it
/// through the pure-Rust PDF generator. Same ownership rule as the record
/// itself (patients may only export their own).
pub async fn record_report_pdf(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let record_id = path.into_inner();
    let record =
        services::get_record_checked(pool.get_ref(), record_id, user.user_id, user.role)
            .await?;
    let report = services::build_record_report(pool.get_ref(), record).await?;

    let pdf_bytes = crate::records::pdf::render_record_report_pdf(&report)?;

    audit::record(
        pool.get_ref(), &user, "record.report_exported", "medical_record", Some(record_id),
        "Exported medical report as PDF",
    ).await;

    Ok(HttpResponse::Ok()
        .content_type("application/pdf")
        .append_header((
            "Content-Disposition",
            format!("attachment; filename=\"medical-report-{record_id}.pdf\""),
        ))
        .body(pdf_bytes))
}

/// GET /records/{id} — view a single medical record
pub async fn record_detail(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let record_id = path.into_inner();
    // Ownership rule (patients see only their own) lives in the service layer
    let record =
        services::get_record_checked(pool.get_ref(), record_id, user.user_id, user.role)
            .await?;

    // Resolve patient and doctor names for display (IDs alone are unfriendly)
    let patient_name: String = sqlx::query_as::<_, (String,)>(
        "SELECT u.full_name FROM patients p JOIN users u ON p.user_id = u.id WHERE p.id = ?"
    )
    .bind(record.patient_id)
    .fetch_optional(pool.get_ref())
    .await?
    .map(|(n,)| n)
    .unwrap_or_else(|| format!("Patient #{}", record.patient_id));

    let doctor_name: String = sqlx::query_as::<_, (String,)>(
        "SELECT u.full_name FROM doctors d JOIN users u ON d.user_id = u.id WHERE d.id = ?"
    )
    .bind(record.doctor_id)
    .fetch_optional(pool.get_ref())
    .await?
    .map(|(n,)| n)
    .unwrap_or_else(|| format!("Doctor #{}", record.doctor_id));

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("record", &record);
    ctx.insert("patient_name", &patient_name);
    ctx.insert("doctor_name", &doctor_name);
    ctx.insert("title", &format!("Medical Record #{}", record.id));
    let rendered = tera.render("records/detail.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}
