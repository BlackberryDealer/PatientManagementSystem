//! Tests for the spec-driven extended features: availability enforcement,
//! doctor reassignment, patient timeline, prescriptions, medical reports,
//! audit logging, and the analytics dashboard.
mod common;
use common::*;
use actix_web::test;
use patient_management_system::appointments::models::BookAppointmentForm;
use patient_management_system::appointments::services as appt;
use patient_management_system::billing::models::CreateInvoiceForm;
use patient_management_system::billing::services as billing;
use patient_management_system::dashboard::services as dashboard;
use patient_management_system::records::models::{CreateRecordForm, PrescriptionForm};
use patient_management_system::records::services as records;
use sqlx::SqlitePool;

// 2027-06-07 is a Monday (day_of_week = 1 in the 0=Sunday convention).
const MONDAY: &str = "2027-06-07";

async fn seed_patient(pool: &SqlitePool, uid: i64, name: &str) {
    sqlx::query("INSERT INTO users (id, username, email, password_hash, role, full_name) VALUES (?,?,?,?,'patient',?)")
        .bind(uid).bind(name).bind(format!("{}@t", name)).bind("$2b$10$abc").bind(format!("P {}", name))
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO patients (user_id) VALUES (?)").bind(uid).execute(pool).await.unwrap();
}

async fn seed_doctor(pool: &SqlitePool, uid: i64, name: &str) {
    sqlx::query("INSERT INTO users (id, username, email, password_hash, role, full_name) VALUES (?,?,?,?,'doctor',?)")
        .bind(uid).bind(name).bind(format!("{}@t", name)).bind("$2b$10$abc").bind(format!("Dr {}", name))
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO doctors (user_id, specialization, license_number) VALUES (?,?,?)")
        .bind(uid).bind("GP").bind("LIC").execute(pool).await.unwrap();
}

async fn seed_availability(
    pool: &SqlitePool, doctor_id: i64, day_of_week: i32,
    start: &str, end: &str, recurring: bool, specific_date: Option<&str>, blocked: bool,
) {
    sqlx::query(
        "INSERT INTO doctor_availability
         (doctor_id, day_of_week, start_time, end_time, is_recurring, specific_date, is_blocked)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(doctor_id).bind(day_of_week).bind(start).bind(end)
    .bind(recurring as i32).bind(specific_date).bind(blocked as i32)
    .execute(pool).await.unwrap();
}

fn booking(doctor_id: i64, date: &str, start: &str, end: &str) -> BookAppointmentForm {
    BookAppointmentForm {
        doctor_id,
        appointment_date: date.into(),
        start_time: start.into(),
        end_time: end.into(),
        room_id: None,
        priority: Some(3),
        notes: None,
    }
}

// ============================================================
// Availability enforcement in the booking workflow
// ============================================================

#[actix_web::test]
async fn test_blocked_doctor_rejects_booking() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "avp1").await;
    seed_doctor(&pool, 2, "avd1").await;
    // Doctor 1 is on leave the whole Monday
    seed_availability(&pool, 1, 1, "00:00", "23:59", false, Some(MONDAY), true).await;

    let result = appt::book_appointment(&pool, 1, &booking(1, MONDAY, "10:00", "10:30")).await;
    assert!(result.is_err(), "booking on a blocked date must be rejected");
}

#[actix_web::test]
async fn test_booking_outside_working_hours_rejected() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "avp2").await;
    seed_doctor(&pool, 2, "avd2").await;
    // Doctor only works Monday mornings 09:00–12:00
    seed_availability(&pool, 1, 1, "09:00", "12:00", true, None, false).await;

    // Inside the window — accepted
    let inside = appt::book_appointment(&pool, 1, &booking(1, MONDAY, "09:00", "09:30")).await;
    assert!(inside.is_ok(), "booking inside the declared window should succeed");

    // Outside the window — rejected
    let outside = appt::book_appointment(&pool, 1, &booking(1, MONDAY, "14:00", "14:30")).await;
    assert!(outside.is_err(), "booking outside working hours must be rejected");
}

#[actix_web::test]
async fn test_no_availability_rows_means_open_schedule() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "avp3").await;
    seed_doctor(&pool, 2, "avd3").await;

    let result = appt::book_appointment(&pool, 1, &booking(1, MONDAY, "10:00", "10:30")).await;
    assert!(result.is_ok(), "doctor with no availability rules follows the open default");
}

#[actix_web::test]
async fn test_past_or_malformed_date_rejected() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "avp4").await;
    seed_doctor(&pool, 2, "avd4").await;

    let past = appt::book_appointment(&pool, 1, &booking(1, "2020-01-01", "10:00", "10:30")).await;
    assert!(past.is_err(), "past dates must be rejected");

    let malformed = appt::book_appointment(&pool, 1, &booking(1, "not-a-date", "10:00", "10:30")).await;
    assert!(malformed.is_err(), "malformed dates must be rejected");
}

// ============================================================
// Doctor Reassignment (Algorithm 4)
// ============================================================

#[actix_web::test]
async fn test_reassignment_moves_to_free_doctor() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "rap1").await;
    seed_doctor(&pool, 2, "rad1").await; // doctors row id 1
    seed_doctor(&pool, 3, "rad2").await; // doctors row id 2

    let appt_row = appt::book_appointment(&pool, 1, &booking(1, MONDAY, "10:00", "10:30"))
        .await
        .unwrap();

    let (updated, new_doctor) = appt::reassign_appointment(&pool, appt_row.id).await.unwrap();
    assert_eq!(updated.doctor_id(), 2, "appointment should move to the alternative doctor");
    assert_eq!(new_doctor, "Dr rad2");

    // Occupancy slots must follow the appointment to the new doctor
    let slots: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM appointment_slots WHERE appointment_id = ? AND doctor_id = 2",
    )
    .bind(updated.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(slots.0, 1, "slot rows should be re-issued under the new doctor");
}

#[actix_web::test]
async fn test_reassignment_fails_without_alternative() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "rap2").await;
    seed_doctor(&pool, 2, "rad3").await; // the only doctor

    let appt_row = appt::book_appointment(&pool, 1, &booking(1, MONDAY, "10:00", "10:30"))
        .await
        .unwrap();

    assert!(
        appt::reassign_appointment(&pool, appt_row.id).await.is_err(),
        "reassignment must fail when no other doctor exists"
    );
}

#[actix_web::test]
async fn test_reassignment_skips_busy_and_unavailable_doctors() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "rap3").await;
    seed_patient(&pool, 5, "rap4").await;
    seed_doctor(&pool, 2, "rad4").await; // doctors row 1 — original
    seed_doctor(&pool, 3, "rad5").await; // doctors row 2 — will be busy
    seed_doctor(&pool, 4, "rad6").await; // doctors row 3 — on leave

    // Doctor 2 is busy in the same slot; doctor 3 is blocked all day
    appt::book_appointment(&pool, 5, &booking(2, MONDAY, "10:00", "10:30")).await.unwrap();
    seed_availability(&pool, 3, 1, "00:00", "23:59", false, Some(MONDAY), true).await;

    let appt_row = appt::book_appointment(&pool, 1, &booking(1, MONDAY, "10:00", "10:30"))
        .await
        .unwrap();

    assert!(
        appt::reassign_appointment(&pool, appt_row.id).await.is_err(),
        "no candidate is free: one is double-booked, the other is on leave"
    );
}

// ============================================================
// Patient History Timeline
// ============================================================

#[actix_web::test]
async fn test_timeline_merges_all_entity_types() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "tlp1").await;
    seed_doctor(&pool, 2, "tld1").await;

    appt::book_appointment(&pool, 1, &booking(1, MONDAY, "10:00", "10:30")).await.unwrap();
    records::create_record(&pool, 2, &CreateRecordForm {
        patient_id: 1, appointment_id: None,
        diagnosis: "Flu".into(), treatment: "Rest".into(), notes: None,
    }).await.unwrap();
    records::create_prescription(&pool, 2, &PrescriptionForm {
        patient_id: 1, appointment_id: None,
        medication_name: "Paracetamol".into(), dosage: "500mg".into(),
        frequency: "Twice daily".into(), duration: Some("5 days".into()), notes: None,
    }).await.unwrap();
    billing::create_invoice(&pool, &CreateInvoiceForm {
        patient_id: 1, due_date: "2027-07-01".into(),
        items: "Consultation|1|50.00".into(),
    }).await.unwrap();

    let events = records::build_patient_timeline(&pool, 1).await.unwrap();
    assert_eq!(events.len(), 4, "appointment, record, prescription, and invoice expected");

    let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    for expected in ["appointment", "record", "prescription", "invoice"] {
        assert!(types.contains(&expected), "timeline missing event type {expected}");
    }
}

#[actix_web::test]
async fn test_timeline_http_patient_sees_own() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "tlweb", "patient");
        let resp = test::call_service(&app, auth_get("/records/timeline", &cookie).to_request()).await;
        assert!(resp.status().is_success(), "patient timeline page should render");
    });
}

// ============================================================
// Prescription Tracking
// ============================================================

#[actix_web::test]
async fn test_prescription_create_http() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _p = register_and_login!(app, "rxpat", "patient");
        let cookie = seed_and_login!(app, pool, "rxdoc", "doctor");

        let resp = test::call_service(&app, auth_post("/records/prescriptions/create", &cookie, serde_json::json!({
            "patient_id": 1,
            "medication_name": "Amoxicillin",
            "dosage": "500mg",
            "frequency": "3 times daily",
            "duration": "7 days",
        })).to_request()).await;
        assert!(resp.status().is_redirection(), "doctor should be able to issue a prescription");
    });
}

#[actix_web::test]
async fn test_prescription_create_forbidden_for_patient() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "rxpat2", "patient");
        let resp = test::call_service(&app, auth_post("/records/prescriptions/create", &cookie, serde_json::json!({
            "patient_id": 1, "medication_name": "X", "dosage": "1", "frequency": "daily",
        })).to_request()).await;
        assert!(resp.status().is_client_error(), "patients must not issue prescriptions");
    });
}

#[actix_web::test]
async fn test_prescription_requires_medication_fields() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "rxv1").await;
    seed_doctor(&pool, 2, "rxv2").await;

    let result = records::create_prescription(&pool, 2, &PrescriptionForm {
        patient_id: 1, appointment_id: None,
        medication_name: "  ".into(), dosage: "500mg".into(),
        frequency: "daily".into(), duration: None, notes: None,
    }).await;
    assert!(result.is_err(), "blank medication name must be rejected");
}

// ============================================================
// Medical Report Generation
// ============================================================

#[actix_web::test]
async fn test_record_report_http() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _p = register_and_login!(app, "reppat", "patient");
        let cookie = seed_and_login!(app, pool, "repdoc", "doctor");

        let create = test::call_service(&app, auth_post("/records/create", &cookie, serde_json::json!({
            "patient_id": 1, "diagnosis": "Sprained ankle", "treatment": "RICE protocol",
        })).to_request()).await;
        assert!(create.status().is_redirection());

        let resp = test::call_service(&app, auth_get("/records/1/report", &cookie).to_request()).await;
        assert!(resp.status().is_success(), "report page should render for the record");
    });
}

// ============================================================
// Audit logging
// ============================================================

#[actix_web::test]
async fn test_audit_log_written_on_booking() {
    let pool = test_db_pool().await;
    let pool_check = pool.clone();
    with_test_app!(pool, app, {
        let _d = seed_and_login!(app, pool, "auddoc", "doctor");
        let cookie = register_and_login!(app, "audpat", "patient");

        let resp = test::call_service(&app, auth_post("/appointments/book", &cookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-15",
            "start_time": "10:00", "end_time": "10:30", "priority": 3,
        })).to_request()).await;
        assert!(resp.status().is_redirection());

        let booked: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'appointment.booked'",
        ).fetch_one(&pool_check).await.unwrap();
        assert!(booked.0 >= 1, "booking must leave an audit trail entry");

        // Only the patient self-registers through the public endpoint (staff are
        // seeded directly), so exactly that registration is audited.
        let registered: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM audit_log WHERE action = 'user.registered'",
        ).fetch_one(&pool_check).await.unwrap();
        assert!(registered.0 >= 1, "the patient registration must be audited");
    });
}

#[actix_web::test]
async fn test_audit_page_admin_only() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let patient_cookie = register_and_login!(app, "audview1", "patient");
        let resp = test::call_service(&app, auth_get("/audit", &patient_cookie).to_request()).await;
        assert!(resp.status().is_client_error(), "audit trail is admin-only");

        let admin_cookie = seed_and_login!(app, pool, "audview2", "admin");
        let resp = test::call_service(&app, auth_get("/audit", &admin_cookie).to_request()).await;
        assert!(resp.status().is_success(), "admin should see the audit trail");
    });
}

// ============================================================
// Analytics dashboard
// ============================================================

#[actix_web::test]
async fn test_dashboard_stats_and_metrics() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "dsp1").await;
    seed_doctor(&pool, 2, "dsd1").await;
    appt::book_appointment(&pool, 1, &booking(1, MONDAY, "10:00", "10:30")).await.unwrap();
    billing::create_invoice(&pool, &CreateInvoiceForm {
        patient_id: 1, due_date: "2027-07-01".into(),
        items: "Consultation|1|100.00".into(),
    }).await.unwrap();

    let stats = dashboard::gather_stats(&pool).await.unwrap();
    assert_eq!(stats.total_patients, 1);
    assert_eq!(stats.total_doctors, 1);
    assert_eq!(stats.scheduled_count, 1);
    assert_eq!(stats.pending_invoices, 1);
    assert!((stats.billed_total - 100.0).abs() < f64::EPSILON);
    // Derived metrics live on the struct, not in templates
    assert!((stats.outstanding_amount() - 100.0).abs() < f64::EPSILON);
    assert!(stats.cancellation_rate() < f64::EPSILON);
}

#[actix_web::test]
async fn test_dashboard_admin_only() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let patient_cookie = register_and_login!(app, "dashpat", "patient");
        let resp = test::call_service(&app, auth_get("/dashboard", &patient_cookie).to_request()).await;
        assert!(resp.status().is_client_error(), "dashboard is admin-only");

        let admin_cookie = seed_and_login!(app, pool, "dashadm", "admin");
        let resp = test::call_service(&app, auth_get("/dashboard", &admin_cookie).to_request()).await;
        assert!(resp.status().is_success(), "admin should see the dashboard");
    });
}
