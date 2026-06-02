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

#[actix_web::test]
async fn test_create_record_submit_http() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        // Need a patient first
        let _pcookie = register_and_login!(app, "recsubpat", "patient");
        let cookie = register_and_login!(app, "recsubdoc", "doctor");

        let req = auth_post("/records/create", &cookie, serde_json::json!({
            "patient_id": 1,
            "diagnosis": "Common cold",
            "treatment": "Rest and fluids",
            "notes": "Follow up if symptoms persist",
        })).to_request();

        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection(), "Record creation should redirect to detail");

        // Verify the record detail page loads
        let location = resp.headers().get("location").unwrap().to_str().unwrap();
        let detail_req = auth_get(location, &cookie).to_request();
        let detail_resp = test::call_service(&app, detail_req).await;
        assert!(detail_resp.status().is_success(), "Record detail page should load");
    });
}
