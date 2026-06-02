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
// Integration test skipped — requires doctors seeded in the DB.

#[actix_web::test]
async fn test_waitlist_page() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "wldoc", "doctor");
        let req = auth_get("/appointments/waitlist", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    });
}
