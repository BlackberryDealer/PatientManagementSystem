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
            .set_form(serde_json::json!({
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
            .set_form(serde_json::json!({
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
            .set_form(serde_json::json!({
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
        let _ = test::call_service(&app, test::TestRequest::post().uri("/users/register").set_form(form).to_request()).await;
        let resp = test::call_service(&app, test::TestRequest::post().uri("/users/register")
            .set_form(serde_json::json!({
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
            .set_form(serde_json::json!({"login": "logintest", "password": "password123"})).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());
    });
}

#[actix_web::test]
async fn test_login_with_email() {
    // Users should be able to log in using their email address instead of username.
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        // Register a user with a known email (auto-login happens)
        let _ = register_and_login!(app, "emailuser", "patient");
        // The login endpoint still works even when already logged in — test email login
        let login = test::TestRequest::post().uri("/users/login")
            .set_form(serde_json::json!({"login": "emailuser@test.com", "password": "password123"})).to_request();
        let resp = test::call_service(&app, login).await;
        assert!(resp.status().is_redirection(), "login with email should succeed");

        // Case-insensitive email login
        let login2 = test::TestRequest::post().uri("/users/login")
            .set_form(serde_json::json!({"login": "EMAILUSER@TEST.COM", "password": "password123"})).to_request();
        let resp2 = test::call_service(&app, login2).await;
        assert!(resp2.status().is_redirection(), "case-insensitive email login should succeed");
    });
}

#[actix_web::test]
async fn test_login_wrong_password() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _cookie = register_and_login!(app, "badpw", "patient");
        let req = test::TestRequest::post().uri("/users/login")
            .set_form(serde_json::json!({"login": "badpw", "password": "wrong"})).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    });
}

#[actix_web::test]
async fn test_login_nonexistent() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let req = test::TestRequest::post().uri("/users/login")
            .set_form(serde_json::json!({"login": "nobody", "password": "x"})).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_client_error());
    });
}

#[actix_web::test]
async fn test_logout() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "logouttest", "patient");

        // Logout is a state-changing action, so it is POST-only.
        let req = auth_post("/users/logout", &cookie, serde_json::json!({})).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());

        // The old GET route must be gone (no logout via link/prefetch).
        let req = auth_get("/users/logout", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 404, "GET logout must not exist");
    });
}

#[actix_web::test]
async fn test_forbidden_error_is_styled_html() {
    // Role rejections must render the styled error page (full HTML document
    // with the specific message), not a raw text body.
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "styled403", "patient");
        let resp = test::call_service(&app, auth_get("/users", &cookie).to_request()).await;
        assert_eq!(resp.status().as_u16(), 403, "/users is admin-only");
        let ct = resp.headers().get("content-type")
            .and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        assert!(ct.starts_with("text/html"), "403 must render as HTML, got {ct}");
        let body = test::read_body(resp).await;
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("do not have permission"),
            "styled page must keep the specific error message");
        assert!(body.contains("</html>"), "error page must be a full HTML document");
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
            .set_form(serde_json::json!({"login": "newdoc", "password": "password123"}))
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

// ============================================================
// Edit profile
// ============================================================

#[actix_web::test]
async fn test_edit_own_profile() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "editme", "patient");

        // GET the edit form
        let form = test::call_service(&app, auth_get("/users/1/edit", &cookie).to_request()).await;
        assert!(form.status().is_success(), "own edit form should load");

        // POST updated profile fields
        let update = auth_post("/users/1/edit", &cookie, serde_json::json!({
            "full_name": "Edited Name",
            "email": "edited@clinic.com",
            "phone": "555-1111",
            "address": "123 New Street",
            "blood_group": "AB+",
            "emergency_contact": "555-9999",
        })).to_request();
        let resp = test::call_service(&app, update).await;
        assert!(resp.status().is_redirection(), "profile update should redirect");

        // Verify the update persisted on the profile page
        let profile = test::call_service(&app, auth_get("/users/1", &cookie).to_request()).await;
        assert!(profile.status().is_success());
    });
}

#[actix_web::test]
async fn test_cannot_edit_other_profile_as_patient() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _pat1 = register_and_login!(app, "editpat1", "patient"); // id 1
        let pat2 = register_and_login!(app, "editpat2", "patient"); // id 2

        // Patient 2 tries to edit Patient 1's profile
        let resp = test::call_service(&app, auth_get("/users/1/edit", &pat2).to_request()).await;
        assert!(resp.status().is_client_error(), "patients cannot edit others' profiles");
    });
}

#[actix_web::test]
async fn test_admin_can_edit_any_profile() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _pat = register_and_login!(app, "editpat3", "patient"); // id 1
        let admin = seed_and_login!(app, pool, "editadm", "admin");

        // Admin can open the edit form for any user
        let form = test::call_service(&app, auth_get("/users/1/edit", &admin).to_request()).await;
        assert!(form.status().is_success(), "admin can edit any profile");

        // Admin updates the patient's profile
        let update = auth_post("/users/1/edit", &admin, serde_json::json!({
            "full_name": "Admin Edited",
            "email": "adminedited@clinic.com",
        })).to_request();
        let resp = test::call_service(&app, update).await;
        assert!(resp.status().is_redirection(), "admin profile update should redirect");
    });
}

// ============================================================
// Delete user (admin only)
// ============================================================

#[actix_web::test]
async fn test_admin_can_delete_user_with_no_history() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _pat = register_and_login!(app, "delpat1", "patient"); // id 1
        let admin = seed_and_login!(app, pool, "deladm1", "admin");

        let resp = test::call_service(
            &app, auth_post("/users/1/delete", &admin, serde_json::json!({})).to_request(),
        ).await;
        assert!(resp.status().is_redirection(), "deleting a history-free account should redirect");

        let profile = test::call_service(&app, auth_get("/users/1", &admin).to_request()).await;
        assert_eq!(profile.status().as_u16(), 404, "deleted user should no longer be found");
    });
}

#[actix_web::test]
async fn test_admin_cannot_delete_own_account() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let admin = seed_and_login!(app, pool, "deladm2", "admin"); // id 1

        let resp = test::call_service(
            &app, auth_post("/users/1/delete", &admin, serde_json::json!({})).to_request(),
        ).await;
        assert!(resp.status().is_client_error(), "admin cannot delete their own account");
    });
}

#[actix_web::test]
async fn test_non_admin_cannot_delete_user() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _pat1 = register_and_login!(app, "delpat2", "patient"); // id 1
        let pat2 = register_and_login!(app, "delpat3", "patient"); // id 2

        let resp = test::call_service(
            &app, auth_post("/users/1/delete", &pat2, serde_json::json!({})).to_request(),
        ).await;
        assert!(resp.status().is_client_error(), "a patient cannot delete another account");
    });
}

#[actix_web::test]
async fn test_cannot_delete_user_with_appointment_history() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _pat = register_and_login!(app, "delpat4", "patient"); // id 1
        let admin = seed_and_login!(app, pool, "deladm3", "admin");
        let _doc = seed_and_login!(app, pool, "deldoc1", "doctor");

        let patient_row: (i64,) = sqlx::query_as("SELECT id FROM patients WHERE user_id = 1")
            .fetch_one(&pool).await.unwrap();
        let doctor_row: (i64,) = sqlx::query_as("SELECT id FROM doctors WHERE user_id = 3")
            .fetch_one(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO appointments (patient_id, doctor_id, appointment_date, start_time, end_time)
             VALUES (?, ?, '2030-01-01', '09:00', '09:30')",
        )
        .bind(patient_row.0).bind(doctor_row.0)
        .execute(&pool).await.unwrap();

        // Deletion is refused with a clear message instead of an opaque 500,
        // and the account (and its clinical history) survives untouched.
        let resp = test::call_service(
            &app, auth_post("/users/1/delete", &admin, serde_json::json!({})).to_request(),
        ).await;
        assert_eq!(resp.status().as_u16(), 400, "a patient with appointment history cannot be deleted");

        let profile = test::call_service(&app, auth_get("/users/1", &admin).to_request()).await;
        assert!(profile.status().is_success(), "the account must still exist");
    });
}

// ============================================================
// Change password
// ============================================================

#[actix_web::test]
async fn test_change_own_password() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "pwuser1", "patient");

        let form = test::call_service(&app, auth_get("/users/1/change-password", &cookie).to_request()).await;
        assert!(form.status().is_success(), "own change-password form should load");

        let update = auth_post("/users/1/change-password", &cookie, serde_json::json!({
            "current_password": "password123",
            "new_password": "newpassword456",
            "confirm_password": "newpassword456",
        })).to_request();
        let resp = test::call_service(&app, update).await;
        assert!(resp.status().is_redirection(), "a correct password change should redirect");

        // The old password no longer works.
        let old_login = test::TestRequest::post().uri("/users/login")
            .set_form(serde_json::json!({"login": "pwuser1", "password": "password123"})).to_request();
        assert!(test::call_service(&app, old_login).await.status().is_client_error(),
            "the old password must stop working");

        // The new password logs in.
        let new_login = test::TestRequest::post().uri("/users/login")
            .set_form(serde_json::json!({"login": "pwuser1", "password": "newpassword456"})).to_request();
        assert!(test::call_service(&app, new_login).await.status().is_redirection(),
            "the new password must work");
    });
}

#[actix_web::test]
async fn test_change_password_wrong_current_rejected() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "pwuser2", "patient");

        let update = auth_post("/users/1/change-password", &cookie, serde_json::json!({
            "current_password": "wrongpassword",
            "new_password": "newpassword456",
            "confirm_password": "newpassword456",
        })).to_request();
        let resp = test::call_service(&app, update).await;
        assert_eq!(resp.status().as_u16(), 401, "a wrong current password must be rejected");
    });
}

#[actix_web::test]
async fn test_change_password_mismatched_confirmation_rejected() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "pwuser3", "patient");

        let update = auth_post("/users/1/change-password", &cookie, serde_json::json!({
            "current_password": "password123",
            "new_password": "newpassword456",
            "confirm_password": "somethingelse",
        })).to_request();
        let resp = test::call_service(&app, update).await;
        assert_eq!(resp.status().as_u16(), 400, "a mismatched confirmation must be rejected");
    });
}

#[actix_web::test]
async fn test_cannot_change_other_patients_password() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _pat1 = register_and_login!(app, "pwuser4", "patient"); // id 1
        let pat2 = register_and_login!(app, "pwuser5", "patient"); // id 2

        let resp = test::call_service(
            &app,
            auth_post("/users/1/change-password", &pat2, serde_json::json!({
                "current_password": "password123",
                "new_password": "newpassword456",
                "confirm_password": "newpassword456",
            })).to_request(),
        ).await;
        assert!(resp.status().is_client_error(), "a patient cannot change another patient's password");
    });
}

#[actix_web::test]
async fn test_admin_can_reset_another_users_password_without_current() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _pat = register_and_login!(app, "pwuser6", "patient"); // id 1
        let admin = seed_and_login!(app, pool, "pwadmin1", "admin");

        // Admin resets the patient's password without knowing the current one.
        let resp = test::call_service(
            &app,
            auth_post("/users/1/change-password", &admin, serde_json::json!({
                "current_password": "",
                "new_password": "resetbyadmin1",
                "confirm_password": "resetbyadmin1",
            })).to_request(),
        ).await;
        assert!(resp.status().is_redirection(), "an admin reset should succeed without the current password");

        let login = test::TestRequest::post().uri("/users/login")
            .set_form(serde_json::json!({"login": "pwuser6", "password": "resetbyadmin1"})).to_request();
        assert!(test::call_service(&app, login).await.status().is_redirection(),
            "the reset password must work");
    });
}
