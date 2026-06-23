//! Integration tests for appointment pages.
mod common;
use common::*;
use actix_web::test;

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
async fn test_priority_booking_http() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _dcookie = seed_and_login!(app, pool, "priodoc", "doctor");
        let normal_cookie = register_and_login!(app, "prionormal", "patient");
        let emerg_cookie = register_and_login!(app, "prioemerg", "patient");

        // Normal patient books a slot
        let req1 = auth_post("/appointments/book", &normal_cookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-16",
            "start_time": "09:00", "end_time": "09:30", "priority": 3,
        })).to_request();
        let resp1 = test::call_service(&app, req1).await;
        assert!(resp1.status().is_redirection(), "Normal booking should succeed");

        // Emergency patient uses priority override â€” should bump the normal one
        let req2 = auth_post("/appointments/book/priority", &emerg_cookie, serde_json::json!({
            "doctor_id": 1, "appointment_date": "2027-06-16",
            "start_time": "09:00", "end_time": "09:30", "priority": 1,
        })).to_request();
        let resp2 = test::call_service(&app, req2).await;
        assert!(resp2.status().is_redirection(), "Emergency priority booking should succeed");
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
