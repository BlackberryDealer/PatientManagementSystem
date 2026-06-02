//! Tests for user registration, login, sessions, and role-based access.

mod common;
use common::*;
use actix_web::test;

#[actix_web::test]
async fn test_register_patient() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let req = test::TestRequest::post()
            .uri("/users/register")
            .set_form(&serde_json::json!({
                "username": "patient1", "email": "p1@test.com",
                "password": "password123", "full_name": "Patient One", "role": "patient",
            })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());
    });
}

#[actix_web::test]
async fn test_register_doctor() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let req = test::TestRequest::post().uri("/users/register")
            .set_form(&serde_json::json!({
                "username": "doctor1", "email": "d1@test.com",
                "password": "password123", "full_name": "Doctor One", "role": "doctor",
            })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());
    });
}

#[actix_web::test]
async fn test_register_admin() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let req = test::TestRequest::post().uri("/users/register")
            .set_form(&serde_json::json!({
                "username": "admin1", "email": "a1@test.com",
                "password": "password123", "full_name": "Admin One", "role": "admin",
            })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());
    });
}

#[actix_web::test]
async fn test_register_duplicate_fails() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let form = serde_json::json!({
            "username": "dupe", "email": "first@test.com",
            "password": "password123", "full_name": "First", "role": "patient",
        });
        let _ = test::call_service(&app, test::TestRequest::post().uri("/users/register").set_form(&form).to_request()).await;
        let resp = test::call_service(&app, test::TestRequest::post().uri("/users/register")
            .set_form(&serde_json::json!({
                "username": "dupe", "email": "second@test.com",
                "password": "password123", "full_name": "Second", "role": "patient",
            })).to_request()).await;
        assert!(resp.status().is_server_error());
    });
}

#[actix_web::test]
async fn test_login_success() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _cookie = register_and_login!(app, "logintest", "patient");
        let req = test::TestRequest::post().uri("/users/login")
            .set_form(&serde_json::json!({"username": "logintest", "password": "password123"})).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());
    });
}

#[actix_web::test]
async fn test_login_wrong_password() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _cookie = register_and_login!(app, "badpw", "patient");
        let req = test::TestRequest::post().uri("/users/login")
            .set_form(&serde_json::json!({"username": "badpw", "password": "wrong"})).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    });
}

#[actix_web::test]
async fn test_login_nonexistent() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let req = test::TestRequest::post().uri("/users/login")
            .set_form(&serde_json::json!({"username": "nobody", "password": "x"})).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    });
}

#[actix_web::test]
async fn test_logout() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "logouttest", "patient");
        let req = auth_get("/users/logout", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());
    });
}

#[actix_web::test]
async fn test_appointments_requires_login() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let req = test::TestRequest::get().uri("/appointments").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    });
}

#[actix_web::test]
async fn test_admin_page_requires_admin() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "regular", "patient");
        let req = auth_get("/users", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    });
}

#[actix_web::test]
async fn test_admin_can_access_user_list() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "adminuser", "admin");
        let req = auth_get("/users", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    });
}
