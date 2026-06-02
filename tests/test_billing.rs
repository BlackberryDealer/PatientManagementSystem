//! Integration tests for billing endpoints.
mod common;
use common::*;
use actix_web::test;

#[actix_web::test]
async fn test_billing_page_patient() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "billpat", "patient");
        let req = auth_get("/billing", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    });
}

#[actix_web::test]
async fn test_create_invoice_requires_admin() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "billpat2", "patient");
        let req = auth_get("/billing/create", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    });
}

#[actix_web::test]
async fn test_create_invoice_admin() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _pcookie = register_and_login!(app, "billpatient3", "patient");
        let cookie = register_and_login!(app, "billadmin", "admin");
        // items format: "Description|quantity|unit_price" per line
        let req = auth_post("/billing/create", &cookie, serde_json::json!({
            "patient_id": 1,
            "due_date": "2026-07-01",
            "items": "Consultation Fee|1|100.00",
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());
    });
}

#[actix_web::test]
async fn test_create_invoice_multi_item() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _pcookie = register_and_login!(app, "billpatient5", "patient");
        let cookie = register_and_login!(app, "billadmin3", "admin");
        let req = auth_post("/billing/create", &cookie, serde_json::json!({
            "patient_id": 1,
            "due_date": "2026-07-15",
            "items": "Consultation Fee|1|80.00\nX-Ray|2|45.00\nBlood Test|1|30.00",
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());
    });
}

#[actix_web::test]
async fn test_create_invoice_bad_items_rejected() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _pcookie = register_and_login!(app, "billpatient6", "patient");
        let cookie = register_and_login!(app, "billadmin4", "admin");
        let req = auth_post("/billing/create", &cookie, serde_json::json!({
            "patient_id": 1,
            "due_date": "2026-07-01",
            "items": "this is not valid",
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    });
}

#[actix_web::test]
async fn test_payment() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _pcookie = register_and_login!(app, "billpatient4", "patient");
        let cookie = register_and_login!(app, "billadmin2", "admin");
        let _ = test::call_service(&app, auth_post("/billing/create", &cookie, serde_json::json!({
            "patient_id": 1,
            "due_date": "2026-07-01",
            "items": "Consultation|1|50.00",
        })).to_request()).await;
        let resp = test::call_service(&app, auth_post("/billing/1/pay", &cookie, serde_json::json!({
            "amount": 50.0, "payment_method": "Cash", "transaction_ref": "T1",
        })).to_request()).await;
        assert!(resp.status().is_redirection());
    });
}
