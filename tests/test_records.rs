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
        let cookie = seed_and_login!(app, pool, "recdoc", "doctor");
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
        let cookie = seed_and_login!(app, pool, "recsubdoc", "doctor");

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

#[actix_web::test]
async fn test_record_report_pdf_download() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        // A patient (patient_id = 1) and a doctor to author the record.
        let _pcookie = register_and_login!(app, "pdfpat", "patient");
        let cookie = seed_and_login!(app, pool, "pdfdoc", "doctor");

        // Create a medical record over HTTP.
        let create_req = auth_post("/records/create", &cookie, serde_json::json!({
            "patient_id": 1,
            "diagnosis": "Seasonal influenza",
            "treatment": "Antivirals and rest",
            "notes": "Review in one week",
        })).to_request();
        let create_resp = test::call_service(&app, create_req).await;
        assert!(create_resp.status().is_redirection());

        // Download the PDF export of that record (record id = 1).
        let pdf_req = auth_get("/records/1/report.pdf", &cookie).to_request();
        let pdf_resp = test::call_service(&app, pdf_req).await;
        assert!(pdf_resp.status().is_success(), "PDF route should return 200");

        let content_type = pdf_resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(content_type, "application/pdf", "must be served as a PDF");

        // The response body must be a genuine PDF (magic-number check).
        let body = test::call_and_read_body(&app, auth_get("/records/1/report.pdf", &cookie).to_request()).await;
        assert!(body.starts_with(b"%PDF"), "body must start with the %PDF header");
    });
}

#[actix_web::test]
async fn test_record_report_pdf_requires_ownership() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        // Patient A (id 1) and a doctor create a record for patient A.
        let _a = register_and_login!(app, "pdfowner", "patient");
        let doc = seed_and_login!(app, pool, "pdfdoc2", "doctor");
        let create_req = auth_post("/records/create", &doc, serde_json::json!({
            "patient_id": 1,
            "diagnosis": "Migraine",
            "treatment": "Analgesics",
        })).to_request();
        assert!(test::call_service(&app, create_req).await.status().is_redirection());

        // Patient B must NOT be able to export patient A's report.
        let b = register_and_login!(app, "pdfstranger", "patient");
        let req = auth_get("/records/1/report.pdf", &b).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error(), "a stranger must be refused (4xx)");
    });
}
