//! Integration tests for medical records.
mod common;
use common::*;
use actix_web::test;

#[actix_web::test]
async fn test_records_page() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "recpat", "patient");
        let req = auth_get("/records", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    });
}

#[actix_web::test]
async fn test_create_record_form_doctor() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "recdoc", "doctor");
        let req = auth_get("/records/create", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    });
}

#[actix_web::test]
async fn test_create_record_requires_doctor() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "recpat2", "patient");
        let req = auth_get("/records/create", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    });
}
