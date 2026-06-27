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
async fn test_register_as_doctor_rejected() {
    // Security: public sign-up cannot grant a staff role. Doctor accounts are
    // created by an administrator / the seed script, not self-registered.
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let req = test::TestRequest::post().uri("/users/register")
            .set_form(&serde_json::json!({
                "username": "doctor1", "email": "d1@test.com",
                "password": "password123", "full_name": "Doctor One", "role": "doctor",
            })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error(), "doctor self-registration must be rejected");
    });
}

#[actix_web::test]
async fn test_register_as_admin_rejected() {
    // Security: self-registering as admin would be a full privilege escalation.
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let req = test::TestRequest::post().uri("/users/register")
            .set_form(&serde_json::json!({
                "username": "admin1", "email": "a1@test.com",
                "password": "password123", "full_name": "Admin One", "role": "admin",
            })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error(), "admin self-registration must be rejected");
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
        // A duplicate username is a user mistake → 400, not a server error
        assert!(resp.status().is_client_error());
    });
}

#[actix_web::test]
async fn test_login_success() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _cookie = register_and_login!(app, "logintest", "patient");
        let req = test::TestRequest::post().uri("/users/login")
            .set_form(&serde_json::json!({"login": "logintest", "password": "password123"})).to_request();
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
            .set_form(&serde_json::json!({"login": "badpw", "password": "wrong"})).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    });
}

#[actix_web::test]
async fn test_login_nonexistent() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let req = test::TestRequest::post().uri("/users/login")
            .set_form(&serde_json::json!({"login": "nobody", "password": "x"})).to_request();
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
        let cookie = seed_and_login!(app, pool, "adminuser", "admin");
        let req = auth_get("/users", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    });
}

// ============================================================
// Profile access control (regression test for PII-disclosure fix)
// ============================================================

#[actix_web::test]
async fn test_patient_cannot_view_other_users_profile() {
    // A patient must not be able to read another user's profile page, which
    // exposes personal/medical details (DOB, address, blood group...).
    // Safe fail: they are redirected away rather than shown an error.
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        // First registered user => id 1, second => id 2.
        let p1 = register_and_login!(app, "patalpha", "patient");
        let _p2 = register_and_login!(app, "patbeta", "patient");

        // patient #1 tries to view patient #2's profile (id 2) -> redirected away
        let req = auth_get("/users/2", &p1).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection(), "patient must be redirected away from another profile");

        // Anti-enumeration: a NON-existent id behaves identically (redirect, not 404),
        // so a brute-forcer cannot distinguish "exists but forbidden" from "doesn't exist".
        let req_ne = auth_get("/users/9999", &p1).to_request();
        let resp_ne = test::call_service(&app, req_ne).await;
        assert!(resp_ne.status().is_redirection(), "non-existent id must not be distinguishable");

        // patient #1 viewing their OWN profile (id 1) is allowed
        let req_self = auth_get("/users/1", &p1).to_request();
        let resp_self = test::call_service(&app, req_self).await;
        assert!(resp_self.status().is_success(), "patient may view own profile");
    });
}

#[actix_web::test]
async fn test_doctor_can_view_patient_profile() {
    // Clinical staff (doctor/admin) may view any profile.
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _patient = register_and_login!(app, "patgamma", "patient"); // id 1
        let doctor = seed_and_login!(app, pool, "drgamma", "doctor");     // id 2

        let req = auth_get("/users/1", &doctor).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success(), "doctor may view a patient profile");
    });
}

// ============================================================
// Admin-only "Add Staff" feature
// ============================================================

#[actix_web::test]
async fn test_admin_can_create_staff() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let admin = seed_and_login!(app, pool, "bossadmin", "admin");

        // Admin can open the form
        let form = test::call_service(&app, auth_get("/users/new", &admin).to_request()).await;
        assert!(form.status().is_success(), "admin can open the add-staff form");

        // Admin creates a doctor account
        let create = auth_post("/users/new", &admin, serde_json::json!({
            "username": "newdoc", "email": "newdoc@clinic.com", "password": "password123",
            "full_name": "Dr New", "role": "doctor",
            "specialization": "Cardiology", "license_number": "LIC-9",
        })).to_request();
        let resp = test::call_service(&app, create).await;
        assert!(resp.status().is_redirection(), "creating a doctor should redirect");

        // The newly created doctor can log in
        let login = test::TestRequest::post().uri("/users/login")
            .set_form(&serde_json::json!({"login": "newdoc", "password": "password123"}))
            .to_request();
        let resp2 = test::call_service(&app, login).await;
        assert!(resp2.status().is_redirection(), "the new doctor can log in");
    });
}

#[actix_web::test]
async fn test_non_admin_cannot_create_staff() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let patient = register_and_login!(app, "plainpat", "patient");

        let form = test::call_service(&app, auth_get("/users/new", &patient).to_request()).await;
        assert!(form.status().is_client_error(), "patients cannot open the add-staff form");

        // Even posting directly must be rejected (no privilege escalation).
        let create = auth_post("/users/new", &patient, serde_json::json!({
            "username": "sneaky", "email": "s@t.com", "password": "password123",
            "full_name": "Sneaky", "role": "admin",
        })).to_request();
        let resp = test::call_service(&app, create).await;
        assert!(resp.status().is_client_error(), "patients cannot create staff accounts");
    });
}
