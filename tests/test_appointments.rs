//! Integration tests for appointment pages.
mod common;
use common::*;
use actix_web::test;
use patient_management_system::appointments::services;

#[actix_web::test]
async fn test_booking_form_loads() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "apptpat", "patient");
        let req = auth_get("/appointments/book", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    });
}

#[actix_web::test]
async fn test_appointments_list_empty() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "emptylist", "patient");
        let req = auth_get("/appointments", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    });
}

// NOTE: suggest_slot_form tested in test_algorithms.rs directly.
// Integration test skipped â€” requires doctors seeded in the DB.

#[actix_web::test]
async fn test_waitlist_page_doctor() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = seed_and_login!(app, pool, "wldoc", "doctor");
        let req = auth_get("/appointments/waitlist", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    });
}

#[actix_web::test]
async fn test_waitlist_page_patient() {
    // Patients should see their own waitlist entries (not a forbidden/empty error)
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "wlpat", "patient");
        let req = auth_get("/appointments/waitlist", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    });
}

#[actix_web::test]
async fn test_promote_waitlist_patient_forbidden() {
    // A patient must not be able to promote a waitlist entry
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "promopatient", "patient");
        let req = test::TestRequest::post()
            .uri("/appointments/waitlist/1/promote")
            .insert_header(("Cookie", cookie))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    });
}

#[actix_web::test]
async fn test_suggest_slot_form_loads() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "suggestpat", "patient");
        let req = auth_get("/appointments/suggest", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    });
}

// ============================================================
// HTTP-level booking tests (full request â†’ response cycle)
// ============================================================

#[actix_web::test]
async fn test_book_appointment_http() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        // Need a doctor for the booking form to show, and as the target
        let _dcookie = seed_and_login!(app, pool, "bookdoc", "doctor");
        let cookie = register_and_login!(app, "bookpat", "patient");

        let req = auth_post("/appointments/book", &cookie, serde_json::json!({
            "doctor_id": 1,
            "appointment_date": "2027-06-15",
            "start_time": "10:00",
            "end_time": "10:30",
            "priority": 3,
        })).to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection(), "Booking should redirect to detail page");

        // Verify the appointment detail page loads
        let location = resp.headers().get("location").unwrap().to_str().unwrap();
        let detail_req = auth_get(location, &cookie).to_request();
        let detail_resp = test::call_service(&app, detail_req).await;
        assert!(detail_resp.status().is_success(), "Appointment detail page should load");
    });
}

#[actix_web::test]
async fn test_book_appointment_conflict_http() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _dcookie = seed_and_login!(app, pool, "conflictdoc", "doctor");
        let pat_cookie = register_and_login!(app, "conflictpat1", "patient");
        let pat2_cookie = register_and_login!(app, "conflictpat2", "patient");

        // First patient books a slot
        let req1 = auth_post("/appointments/book", &pat_cookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-15",
            "start_time": "14:00", "end_time": "14:30", "priority": 3,
        })).to_request();
        let resp1 = test::call_service(&app, req1).await;
        assert!(resp1.status().is_redirection(), "First booking should succeed");

        // Second patient tries to book the same slot â€” should fail
        let req2 = auth_post("/appointments/book", &pat2_cookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-15",
            "start_time": "14:00", "end_time": "14:30", "priority": 3,
        })).to_request();
        let resp2 = test::call_service(&app, req2).await;
        assert!(resp2.status().is_client_error(), "Double-booking should be rejected");
    });
}

#[actix_web::test]
async fn test_book_appointment_invalid_time_http() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _dcookie = seed_and_login!(app, pool, "invaliddoc", "doctor");
        let cookie = register_and_login!(app, "invalidpat", "patient");

        // Start time after end time â€” should be rejected
        let req = auth_post("/appointments/book", &cookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-15",
            "start_time": "15:00", "end_time": "14:00", "priority": 3,
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error(), "start > end should be rejected");
    });
}

#[actix_web::test]
async fn test_book_appointment_outside_clinic_hours_http() {
    // Server-side clinic-hours rule: the booking form only offers 08:00–17:00
    // slots, but a hand-crafted POST must be rejected too — even when the
    // doctor has no availability rows (an otherwise "open" schedule).
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _dcookie = seed_and_login!(app, pool, "hoursdoc", "doctor");
        let cookie = register_and_login!(app, "hourspat", "patient");

        // Grid-aligned but before opening time
        let req = auth_post("/appointments/book", &cookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-15",
            "start_time": "07:00", "end_time": "07:30", "priority": 3,
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error(), "booking before 08:00 must be rejected");

        // Starts inside hours but spills past closing time
        let req = auth_post("/appointments/book", &cookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-15",
            "start_time": "16:30", "end_time": "17:30", "priority": 3,
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error(), "booking past 17:00 must be rejected");

        // Middle of the night
        let req = auth_post("/appointments/book", &cookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-15",
            "start_time": "02:00", "end_time": "02:30", "priority": 3,
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error(), "booking at 02:00 must be rejected");

        // Sanity check: the same request inside clinic hours succeeds
        let req = auth_post("/appointments/book", &cookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-15",
            "start_time": "10:00", "end_time": "10:30", "priority": 3,
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection(), "in-hours booking still succeeds");
    });
}

#[actix_web::test]
async fn test_booking_error_is_styled_html_with_message() {
    // A rejected booking must land on the styled error page (not raw text)
    // and keep the specific domain message so the user knows what to fix.
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _dcookie = seed_and_login!(app, pool, "styledoc", "doctor");
        let pat1 = register_and_login!(app, "stylepat1", "patient");
        let pat2 = register_and_login!(app, "stylepat2", "patient");

        let slot = serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-15",
            "start_time": "14:00", "end_time": "14:30", "priority": 3,
        });
        let resp = test::call_service(&app,
            auth_post("/appointments/book", &pat1, slot.clone()).to_request()).await;
        assert!(resp.status().is_redirection());

        let resp = test::call_service(&app,
            auth_post("/appointments/book", &pat2, slot).to_request()).await;
        assert_eq!(resp.status().as_u16(), 400);
        let ct = resp.headers().get("content-type")
            .and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        assert!(ct.starts_with("text/html"), "error must render as HTML, got {ct}");

        let body = test::read_body(resp).await;
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("conflicts with an existing appointment"),
            "styled page must keep the specific error message");
        assert!(body.contains("</html>"), "error page must be a full HTML document");
    });
}

#[actix_web::test]
async fn test_staff_priority_booking_bumps_lower_priority() {
    // A doctor books an emergency patient into a slot held by a normal
    // booking — the single /appointments/book endpoint applies the bump.
    // The rest of the day is open, so the bumped patient is immediately
    // auto-rescheduled into the doctor's earliest free slot rather than
    // being left waiting (see test_bump_stays_waiting_when_day_is_full for
    // the full-day fallback case).
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let dcookie = seed_and_login!(app, pool, "priodoc", "doctor");
        let normal_cookie = register_and_login!(app, "prionormal", "patient");
        let _emerg_cookie = register_and_login!(app, "prioemerg", "patient");

        // Normal patient books a slot (patient row id 1)
        let req1 = auth_post("/appointments/book", &normal_cookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-16",
            "start_time": "09:00", "end_time": "09:30",
        })).to_request();
        let resp1 = test::call_service(&app, req1).await;
        assert!(resp1.status().is_redirection(), "Normal booking should succeed");

        // Doctor books the emergency patient (row id 2) into the same slot.
        // No doctor_id is sent — the appointment lands in the doctor's own
        // schedule. The lower-priority booking is bumped, then auto-rebooked.
        let req2 = auth_post("/appointments/book", &dcookie, serde_json::json!({
            "patient_id": 2, "appointment_date": "2027-06-16",
            "start_time": "09:00", "end_time": "09:30", "priority": 1,
        })).to_request();
        let resp2 = test::call_service(&app, req2).await;
        assert!(resp2.status().is_redirection(), "Emergency priority booking should succeed");

        // The bumped normal booking was auto-rescheduled and accepted, not
        // left waiting — the day's earliest free gap (08:00) precedes the
        // original 09:00 slot.
        let bumped: (String,) = sqlx::query_as(
            "SELECT status FROM waitlist WHERE patient_id = 1",
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(bumped.0, "accepted", "an open day must auto-rebook the bumped patient");

        let rebooked: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM appointments
             WHERE patient_id = 1 AND status = 'scheduled' AND start_time = '08:00'",
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(rebooked.0, 1, "the bumped patient must land the day's earliest free slot");
    });
}

#[actix_web::test]
async fn test_patient_priority_is_forced_to_normal() {
    // Triage is a clinical decision: even a hand-crafted POST claiming
    // Emergency must produce a Normal-priority appointment for a patient.
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _dcookie = seed_and_login!(app, pool, "clampdoc", "doctor");
        let pcookie = register_and_login!(app, "clamppat", "patient");

        let req = auth_post("/appointments/book", &pcookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-16",
            "start_time": "10:00", "end_time": "10:30", "priority": 1,
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection(), "booking itself should succeed");

        let pri: (i32,) = sqlx::query_as("SELECT priority FROM appointments WHERE id = 1")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(pri.0, 3, "a patient's self-reported priority must be clamped to Normal");
    });
}

#[actix_web::test]
async fn test_patient_cannot_bump_anothers_appointment() {
    // Because patient bookings are always Normal, a patient posting
    // priority=1 at an occupied slot gets a clean conflict, not a bump.
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _dcookie = seed_and_login!(app, pool, "bumpdoc", "doctor");
        let pat1 = register_and_login!(app, "bumppat1", "patient");
        let pat2 = register_and_login!(app, "bumppat2", "patient");

        let req = auth_post("/appointments/book", &pat1, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-16",
            "start_time": "11:00", "end_time": "11:30",
        })).to_request();
        assert!(test::call_service(&app, req).await.status().is_redirection());

        let req = auth_post("/appointments/book", &pat2, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-16",
            "start_time": "11:00", "end_time": "11:30", "priority": 1,
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 400, "a patient must never bump an existing booking");

        // The original appointment is untouched.
        let status: (String,) = sqlx::query_as("SELECT status FROM appointments WHERE id = 1")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(status.0, "scheduled");
    });
}

#[actix_web::test]
async fn test_doctor_books_into_own_schedule_only() {
    // A doctor's booking always lands in their own schedule: any submitted
    // doctor_id is ignored, and a missing patient selection is rejected.
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _d1 = seed_and_login!(app, pool, "owndoc1", "doctor");    // doctor row 1
        let d2cookie = seed_and_login!(app, pool, "owndoc2", "doctor"); // doctor row 2
        let _pat = register_and_login!(app, "ownpat", "patient");      // patient row 1

        // Doctor 2 tries to book "with doctor 1" — the appointment must be
        // created under doctor 2 regardless.
        let req = auth_post("/appointments/book", &d2cookie, serde_json::json!({
            "patient_id": 1, "doctor_id": 1, "appointment_date": "2027-06-16",
            "start_time": "09:00", "end_time": "09:30", "priority": 3,
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection(), "doctor booking for a patient should succeed");

        let doc: (i64,) = sqlx::query_as("SELECT doctor_id FROM appointments WHERE id = 1")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(doc.0, 2, "the appointment must belong to the booking doctor's own schedule");

        // Without a patient selected, a staff booking is a clean 400.
        let req = auth_post("/appointments/book", &d2cookie, serde_json::json!({
            "appointment_date": "2027-06-16",
            "start_time": "10:00", "end_time": "10:30", "priority": 3,
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 400, "staff booking without a patient must be rejected");
    });
}

#[actix_web::test]
async fn test_waitlist_join_is_patient_only_and_normal_priority() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let dcookie = seed_and_login!(app, pool, "wljoindoc", "doctor");
        let pcookie = register_and_login!(app, "wljoinpat", "patient");

        // Staff cannot join the waitlist (they have no patient profile).
        let req = auth_post("/appointments/waitlist/join", &dcookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-16",
            "requested_start": "09:00", "requested_end": "09:30", "priority": 3,
        })).to_request();
        assert_eq!(test::call_service(&app, req).await.status().as_u16(), 403);

        // A patient's claimed Emergency priority is filed as Normal.
        let req = auth_post("/appointments/waitlist/join", &pcookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-16",
            "requested_start": "09:00", "requested_end": "09:30", "priority": 1,
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection(), "patient join should succeed");

        let pri: (i32,) = sqlx::query_as("SELECT priority FROM waitlist WHERE id = 1")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(pri.0, 3, "patient-joined waitlist entries are always Normal priority");
    });
}

#[actix_web::test]
async fn test_cancel_appointment_http() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _dcookie = seed_and_login!(app, pool, "canceldoc", "doctor");
        let cookie = register_and_login!(app, "cancelpat", "patient");

        // Book
        let req = auth_post("/appointments/book", &cookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-17",
            "start_time": "10:00", "end_time": "10:30", "priority": 3,
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());

        // Cancel (appointment ID is 1 â€” first in fresh DB)
        let cancel_req = test::TestRequest::post()
            .uri("/appointments/1/cancel")
            .insert_header(("Cookie", cookie))
            .to_request();
        let cancel_resp = test::call_service(&app, cancel_req).await;
        assert!(cancel_resp.status().is_redirection(), "Cancel should redirect");
    });
}

#[actix_web::test]
async fn test_appointments_list_after_booking() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _dcookie = seed_and_login!(app, pool, "listdoc", "doctor");
        let cookie = register_and_login!(app, "listpat", "patient");

        // List should be empty first
        let req0 = auth_get("/appointments", &cookie).to_request();
        let resp0 = test::call_service(&app, req0).await;
        assert!(resp0.status().is_success());

        // Book an appointment
        let _ = test::call_service(&app, auth_post("/appointments/book", &cookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-18",
            "start_time": "11:00", "end_time": "11:30", "priority": 3,
        })).to_request()).await;

        // List should still load after booking
        let req2 = auth_get("/appointments", &cookie).to_request();
        let resp2 = test::call_service(&app, req2).await;
        assert!(resp2.status().is_success(), "Appointment list should load after booking");
    });
}

#[actix_web::test]
async fn test_patient_cannot_view_other_patients_appointment() {
    // IDOR guard: a patient must not view another patient's appointment by ID.
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _doc = seed_and_login!(app, pool, "idordoc", "doctor");   // doctor_id 1
        let pat1 = register_and_login!(app, "idorpat1", "patient");
        let pat2 = register_and_login!(app, "idorpat2", "patient");

        // patient #1 books an appointment
        let book = auth_post("/appointments/book", &pat1, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2026-07-15",
            "start_time": "10:00", "end_time": "10:30", "priority": 3,
        })).to_request();
        let resp = test::call_service(&app, book).await;
        assert!(resp.status().is_redirection(), "booking should succeed");
        let loc = resp.headers().get("location").unwrap().to_str().unwrap().to_string();

        // patient #2 tries to view patient #1's appointment -> rejected
        let spy = auth_get(&loc, &pat2).to_request();
        let resp2 = test::call_service(&app, spy).await;
        assert!(resp2.status().is_client_error(),
            "patient must not view another patient's appointment");

        // owner can still view their own
        let own = auth_get(&loc, &pat1).to_request();
        let resp3 = test::call_service(&app, own).await;
        assert!(resp3.status().is_success(), "owner can view their own appointment");
    });
}

// ============================================================
// Calendar view
// ============================================================

#[actix_web::test]
async fn test_calendar_view_loads() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let dcookie = seed_and_login!(app, pool, "caldoc", "doctor");

        // Calendar page should render for logged-in users
        let resp = test::call_service(&app, auth_get("/appointments/calendar", &dcookie).to_request()).await;
        assert!(resp.status().is_success(), "calendar page should render");
    });
}

#[actix_web::test]
async fn test_calendar_view_with_query_params() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "calpat", "patient");

        // Navigate to a specific month via query params
        let resp = test::call_service(&app,
            auth_get("/appointments/calendar?year=2027&month=6", &cookie).to_request()
        ).await;
        assert!(resp.status().is_success(), "calendar with query params should render");
    });
}

#[actix_web::test]
async fn test_calendar_requires_login() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let req = test::TestRequest::get().uri("/appointments/calendar").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error(), "calendar requires authentication");
    });
}

// ============================================================
// HTTP: appointment completion (staff-only lifecycle action)
// ============================================================

#[actix_web::test]
async fn test_complete_appointment_http() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let dcookie = seed_and_login!(app, pool, "compdoc", "doctor");
        let pcookie = register_and_login!(app, "comppat", "patient");

        let req = auth_post("/appointments/book", &pcookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-15",
            "start_time": "10:00", "end_time": "10:30", "priority": 3,
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());

        // The doctor closes out the visit.
        let req = test::TestRequest::post()
            .uri("/appointments/1/complete")
            .insert_header(("Cookie", dcookie))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection(), "completion should redirect to the detail page");

        let status: (String,) = sqlx::query_as("SELECT status FROM appointments WHERE id = 1")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(status.0, "completed");
    });
}

#[actix_web::test]
async fn test_complete_appointment_forbidden_for_patient() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _dcookie = seed_and_login!(app, pool, "compdoc2", "doctor");
        let pcookie = register_and_login!(app, "comppat2", "patient");

        let req = auth_post("/appointments/book", &pcookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-15",
            "start_time": "11:00", "end_time": "11:30", "priority": 3,
        })).to_request();
        assert!(test::call_service(&app, req).await.status().is_redirection());

        // Completion is a clinical action — the patient may not perform it.
        let req = test::TestRequest::post()
            .uri("/appointments/1/complete")
            .insert_header(("Cookie", pcookie))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 403);
    });
}

// ============================================================
// HTTP: staff room override (POST /appointments/{id}/assign-room)
// ============================================================

#[actix_web::test]
async fn test_assign_room_http() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let dcookie = seed_and_login!(app, pool, "roomdoc", "doctor");
        let pcookie = register_and_login!(app, "roompat", "patient");

        let req = auth_post("/appointments/book", &pcookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-15",
            "start_time": "10:00", "end_time": "10:30", "priority": 3,
        })).to_request();
        assert!(test::call_service(&app, req).await.status().is_redirection());

        // Move the visit to the procedure room (seeded room id 4).
        let req = auth_post("/appointments/1/assign-room", &dcookie,
            serde_json::json!({ "room_id": 4 })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection(), "room override should redirect to detail");

        // Appointment row AND its occupancy slots must both move.
        let room: (i64,) = sqlx::query_as("SELECT room_id FROM appointments WHERE id = 1")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(room.0, 4);
        let slot_room: (i64,) = sqlx::query_as("SELECT room_id FROM appointment_slots WHERE appointment_id = 1")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(slot_room.0, 4);
    });
}

#[actix_web::test]
async fn test_assign_room_forbidden_for_patient() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _dcookie = seed_and_login!(app, pool, "roomdoc2", "doctor");
        let pcookie = register_and_login!(app, "roompat2", "patient");

        let req = auth_post("/appointments/book", &pcookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-15",
            "start_time": "10:00", "end_time": "10:30", "priority": 3,
        })).to_request();
        assert!(test::call_service(&app, req).await.status().is_redirection());

        let req = auth_post("/appointments/1/assign-room", &pcookie,
            serde_json::json!({ "room_id": 4 })).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 403);
    });
}

#[actix_web::test]
async fn test_assign_room_conflict_rejected() {
    // Moving an appointment into a room that is occupied at the same slot
    // must be rejected by the room UNIQUE index (clean 400, not a 500).
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let dcookie = seed_and_login!(app, pool, "clashdoc1", "doctor");
        let _d2 = seed_and_login!(app, pool, "clashdoc2", "doctor");
        let pcookie = register_and_login!(app, "clashpat", "patient");

        // Same slot with both doctors — auto-allocation gives them
        // different rooms (appointments 1 and 2).
        for doctor_id in [1, 2] {
            let req = auth_post("/appointments/book", &pcookie, serde_json::json!({
                "doctor_id": doctor_id, "appointment_date": "2027-06-15",
                "start_time": "10:00", "end_time": "10:30", "priority": 3,
            })).to_request();
            assert!(test::call_service(&app, req).await.status().is_redirection());
        }
        let room1: (i64,) = sqlx::query_as("SELECT room_id FROM appointments WHERE id = 1")
            .fetch_one(&pool).await.unwrap();

        // Try to move appointment 2 into appointment 1's room at the same slot.
        let req = auth_post("/appointments/2/assign-room", &dcookie,
            serde_json::json!({ "room_id": room1.0 })).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 400, "same-slot room clash must be a clean 400");
    });
}

#[actix_web::test]
async fn test_conflict_detected_across_rooms() {
    // A doctor busy in one room must still conflict with a new booking that
    // resolves to a different room — doctor and room are independent
    // resources (regression: `doctor AND room` let this double-book).
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let dcookie = seed_and_login!(app, pool, "xroomdoc", "doctor");
        let pcookie = register_and_login!(app, "xroompat1", "patient");
        let p2cookie = register_and_login!(app, "xroompat2", "patient");

        let req = auth_post("/appointments/book", &pcookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-15",
            "start_time": "10:00", "end_time": "10:30",
        })).to_request();
        assert!(test::call_service(&app, req).await.status().is_redirection());

        // Move the visit out of the doctor's daily room (seeded room id 4),
        // so a second booking at the same time resolves a DIFFERENT room.
        let req = auth_post("/appointments/1/assign-room", &dcookie,
            serde_json::json!({ "room_id": 4 })).to_request();
        assert!(test::call_service(&app, req).await.status().is_redirection());

        let req = auth_post("/appointments/book", &p2cookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-15",
            "start_time": "10:00", "end_time": "10:30",
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 400, "doctor busy in another room must still conflict");
    });
}

// ============================================================
// HTTP: calendar day filter (GET /appointments?date=YYYY-MM-DD)
// ============================================================

#[actix_web::test]
async fn test_list_appointments_date_filter() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "datepat", "patient");
        seed_and_login!(app, pool, "datedoc", "doctor");

        // Two bookings on different days.
        for date in ["2027-06-15", "2027-06-16"] {
            let req = auth_post("/appointments/book", &cookie, serde_json::json!({
                "doctor_id": 1, "appointment_date": date,
                "start_time": "10:00", "end_time": "10:30",
            })).to_request();
            assert!(test::call_service(&app, req).await.status().is_redirection());
        }

        // The test harness stubs list.html.tera as "Apps: {{ appointments | length }}",
        // so assertions are count-based: filtered day → 1 of the 2 bookings.
        let req = auth_get("/appointments?date=2027-06-15", &cookie).to_request();
        let body = test::read_body(test::call_service(&app, req).await).await;
        assert_eq!(String::from_utf8_lossy(&body), "<html><body>Apps: 1</body></html>");

        // Garbage date is ignored — full list of 2, no error.
        let req = auth_get("/appointments?date=not-a-date", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        let body = test::read_body(resp).await;
        assert_eq!(String::from_utf8_lossy(&body), "<html><body>Apps: 2</body></html>");
    });
}

// ============================================================
// HTTP: staff re-triage (POST /appointments/{id}/priority)
// ============================================================

#[actix_web::test]
async fn test_update_priority_http() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let dcookie = seed_and_login!(app, pool, "pridoc", "doctor");
        let pcookie = register_and_login!(app, "pripat", "patient");

        let req = auth_post("/appointments/book", &pcookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-15",
            "start_time": "10:00", "end_time": "10:30",
        })).to_request();
        assert!(test::call_service(&app, req).await.status().is_redirection());

        // Doctor escalates the visit to Emergency.
        let req = auth_post("/appointments/1/priority", &dcookie,
            serde_json::json!({ "priority": 1 })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection(), "re-triage should redirect to detail");
        let pri: (i32,) = sqlx::query_as("SELECT priority FROM appointments WHERE id = 1")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(pri.0, 1);

        // Triage is a clinical decision — the patient may not perform it.
        let req = auth_post("/appointments/1/priority", &pcookie,
            serde_json::json!({ "priority": 2 })).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 403);

        // Out-of-range levels are rejected, not silently coerced to Normal.
        let req = auth_post("/appointments/1/priority", &dcookie,
            serde_json::json!({ "priority": 9 })).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 400);
    });
}

// ============================================================
// Waitlist lifecycle: expiry + auto re-slot
// ============================================================

#[actix_web::test]
async fn test_expire_stale_waitlist_sweep() {
    // A waiting entry whose date has already passed must be swept to
    // 'expired' and drop out of the doctor's (action-queue) view, while
    // remaining visible (as expired) on the patient's own view.
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _dcookie = seed_and_login!(app, pool, "expiredoc", "doctor");
        let _pcookie = register_and_login!(app, "expirepat", "patient");

        sqlx::query(
            "INSERT INTO waitlist (patient_id, doctor_id, room_id, appointment_date,
             requested_start, requested_end, priority, notes, status)
             VALUES (1, 1, NULL, '2020-01-01', '09:00', '09:30', 3, NULL, 'waiting')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let swept = services::expire_stale_waitlist(&pool).await.unwrap();
        assert_eq!(swept, 1, "the stale entry must be swept");

        let status: (String,) = sqlx::query_as("SELECT status FROM waitlist WHERE id = 1")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(status.0, "expired");

        let doctor_view = services::get_waitlist_for_doctor(&pool, 1).await.unwrap();
        assert!(doctor_view.is_empty(), "expired entries are noise on the doctor's action queue");

        let patient_view = services::get_waitlist_for_patient(&pool, 2).await.unwrap();
        assert_eq!(patient_view.len(), 1, "the patient must still see what happened to their request");
    });
}

#[actix_web::test]
async fn test_bump_auto_reschedules_into_free_slot() {
    // A bumped patient with an otherwise-open day is immediately rebooked
    // into the doctor's earliest free same-duration slot, not left waitlisted.
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let dcookie = seed_and_login!(app, pool, "rebookdoc", "doctor");
        let victim = register_and_login!(app, "rebookvictim", "patient");
        let _emerg = register_and_login!(app, "rebookemerg", "patient");

        // Victim (patient row 1) books 09:30-10:00 Normal, leaving 08:00-09:30
        // open so the earliest-gap search has somewhere unambiguous to land.
        let req = auth_post("/appointments/book", &victim, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-07-01",
            "start_time": "09:30", "end_time": "10:00", "priority": 3,
        })).to_request();
        assert!(test::call_service(&app, req).await.status().is_redirection());

        // Doctor books an Emergency for patient row 2 into the same slot —
        // the rest of the day is open, so the day's earliest gap (08:00)
        // must be found and taken.
        let req = auth_post("/appointments/book", &dcookie, serde_json::json!({
            "patient_id": 2, "appointment_date": "2027-07-01",
            "start_time": "09:30", "end_time": "10:00", "priority": 1,
        })).to_request();
        assert!(test::call_service(&app, req).await.status().is_redirection());

        let wl_status: (String,) = sqlx::query_as(
            "SELECT status FROM waitlist WHERE patient_id = 1",
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(wl_status.0, "accepted", "the bumped entry must be auto-rebooked, not left waiting");

        let rebooked: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM appointments
             WHERE patient_id = 1 AND status = 'scheduled' AND start_time = '08:00'",
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(rebooked.0, 1, "the victim must land a new appointment at the day's earliest free slot");
    });
}

#[actix_web::test]
async fn test_bump_stays_waiting_when_day_is_full() {
    // When the doctor's whole day is already booked solid, a bumped patient
    // has nowhere to auto-reschedule to and must fall back to waiting — the
    // pre-existing behaviour, preserved.
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let dcookie = seed_and_login!(app, pool, "fulldaydoc", "doctor");
        let filler = register_and_login!(app, "fullfiller", "patient");
        let _emerg = register_and_login!(app, "fullemerg", "patient");

        // Fill every 30-minute clinic slot (08:00-17:00) with the filler.
        for i in 0..18 {
            let start_min = 8 * 60 + i * 30;
            let end_min = start_min + 30;
            let start = format!("{:02}:{:02}", start_min / 60, start_min % 60);
            let end = format!("{:02}:{:02}", end_min / 60, end_min % 60);
            let req = auth_post("/appointments/book", &filler, serde_json::json!({
                "doctor_id": 1, "appointment_date": "2027-07-02",
                "start_time": start, "end_time": end, "priority": 3,
            })).to_request();
            assert!(test::call_service(&app, req).await.status().is_redirection(),
                "filler booking at {start} should succeed");
        }

        // Doctor overrides the 12:00 slot with an Emergency for patient row 2.
        let req = auth_post("/appointments/book", &dcookie, serde_json::json!({
            "patient_id": 2, "appointment_date": "2027-07-02",
            "start_time": "12:00", "end_time": "12:30", "priority": 1,
        })).to_request();
        assert!(test::call_service(&app, req).await.status().is_redirection());

        let wl_status: (String,) = sqlx::query_as(
            "SELECT status FROM waitlist WHERE patient_id = 1 AND requested_start = '12:00'",
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(wl_status.0, "waiting", "no gap exists, so the fallback to waitlist must hold");
    });
}

#[actix_web::test]
async fn test_promote_past_dated_entry_is_blocked() {
    // A stale Promote click must get a clear "already passed" notice, not a
    // confusing conflict error or a silent no-op.
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let dcookie = seed_and_login!(app, pool, "pastdoc", "doctor");
        let _pcookie = register_and_login!(app, "pastpat", "patient");

        sqlx::query(
            "INSERT INTO waitlist (patient_id, doctor_id, room_id, appointment_date,
             requested_start, requested_end, priority, notes, status)
             VALUES (1, 1, NULL, '2020-01-01', '09:00', '09:30', 3, NULL, 'waiting')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let req = test::TestRequest::post()
            .uri("/appointments/waitlist/1/promote")
            .insert_header(("Cookie", dcookie))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());
        let location = resp.headers().get("Location").unwrap().to_str().unwrap();
        assert!(location.contains("waitlist?error="), "must redirect back with a notice");
        assert!(location.contains("already"), "the notice must explain the time has passed");
    });
}

#[actix_web::test]
async fn test_cancel_still_restores_bumped_patient_when_day_was_full() {
    // Regression: when a bumped patient could not be auto-rescheduled (day
    // full) and stayed waiting, cancelling the overriding appointment must
    // still restore them via the pre-existing auto-promote-on-cancel flow.
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let dcookie = seed_and_login!(app, pool, "restoredoc", "doctor");
        let filler = register_and_login!(app, "restorefiller", "patient");
        let _emerg = register_and_login!(app, "restoreemerg", "patient");

        for i in 0..18 {
            let start_min = 8 * 60 + i * 30;
            let end_min = start_min + 30;
            let start = format!("{:02}:{:02}", start_min / 60, start_min % 60);
            let end = format!("{:02}:{:02}", end_min / 60, end_min % 60);
            let req = auth_post("/appointments/book", &filler, serde_json::json!({
                "doctor_id": 1, "appointment_date": "2027-07-03",
                "start_time": start, "end_time": end, "priority": 3,
            })).to_request();
            assert!(test::call_service(&app, req).await.status().is_redirection());
        }
        // 12:00-12:30 is the 9th booking (i = 8), so its appointment id is 9.
        let req = auth_post("/appointments/book", &dcookie, serde_json::json!({
            "patient_id": 2, "appointment_date": "2027-07-03",
            "start_time": "12:00", "end_time": "12:30", "priority": 1,
        })).to_request();
        assert!(test::call_service(&app, req).await.status().is_redirection());

        let wl_status: (String,) = sqlx::query_as(
            "SELECT status FROM waitlist WHERE patient_id = 1 AND requested_start = '12:00'",
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(wl_status.0, "waiting");

        // Cancel the Emergency appointment (id 19: 18 filler bookings + 1).
        let cancel_req = test::TestRequest::post()
            .uri("/appointments/19/cancel")
            .insert_header(("Cookie", dcookie.clone()))
            .to_request();
        assert!(test::call_service(&app, cancel_req).await.status().is_redirection());

        let wl_status: (String,) = sqlx::query_as(
            "SELECT status FROM waitlist WHERE patient_id = 1 AND requested_start = '12:00'",
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(wl_status.0, "accepted", "cancelling the override must auto-promote the waiter");

        let restored: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM appointments
             WHERE patient_id = 1 AND status = 'scheduled' AND start_time = '12:00'",
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(restored.0, 1, "the filler must get their original slot back");
    });
}
