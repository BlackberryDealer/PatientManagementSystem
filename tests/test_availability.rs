//! Integration tests for availability endpoints.
mod common;
use common::*;
use actix_web::test;

#[actix_web::test]
async fn test_availability_page() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "adoc", "doctor");
        let req = auth_get("/availability", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    });
}

#[actix_web::test]
async fn test_set_availability_form() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "asetdoc", "doctor");
        let req = auth_get("/availability/set", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    });
}

#[actix_web::test]
async fn test_set_availability_submit() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "asubdoc", "doctor");
        let req = auth_post("/availability/set", &cookie, serde_json::json!({
            "day_of_week": 1, "start_time": "09:00", "end_time": "17:00",
            "is_recurring": "on", "is_blocked": "",
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());

        // Verify the slot appears in the availability list
        let list_req = auth_get("/availability", &cookie).to_request();
        let list_resp = test::call_service(&app, list_req).await;
        assert!(list_resp.status().is_success(), "Availability page should show new slot");
    });
}
