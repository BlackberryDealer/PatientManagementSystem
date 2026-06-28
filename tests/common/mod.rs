//! Shared test utilities: in-memory database, app factory, auth helpers.
//!
//! Uses a macro to avoid complex `Service` trait type annotations
//! that conflict across actix-web / actix-http versions.

// Many imports and helpers here are consumed exclusively by the exported macros
// (`with_test_app!`, `register_and_login!`, `seed_and_login!`) which expand at
// their call-sites in other crates. The compiler can't see those uses from
// this module, so we suppress the false-positive warnings.
#![allow(unused_imports)]
#![allow(dead_code)]

use actix_session::{storage::CookieSessionStore, SessionMiddleware};
use actix_web::{cookie::Key, test, web, App};
use patient_management_system::appointments;
use patient_management_system::availability;
use patient_management_system::billing;
use patient_management_system::db;
use patient_management_system::records;
use patient_management_system::users;
use sqlx::SqlitePool;
use tera::Tera;

pub async fn test_db_pool() -> SqlitePool {
    let pool = db::create_pool("sqlite::memory:").await;
    db::run_migrations(&pool).await;
    pool
}

pub fn test_tera() -> Tera {
    let mut tera = Tera::default();
    let pages = [
        ("users/login.html.tera", "<html><body>Login: {{ title }}</body></html>"),
        ("users/register.html.tera", "<html><body>Register: {{ title }}</body></html>"),
        ("users/list.html.tera", "<html><body>Users: {{ users | length }}</body></html>"),
        ("users/profile.html.tera", "<html><body>Profile: {{ profile_user.full_name }}</body></html>"),
        ("users/new.html.tera", "<html><body>Add Staff</body></html>"),
        ("users/edit.html.tera", "<html><body>Edit: {{ profile_user.full_name }}</body></html>"),
        ("appointments/list.html.tera", "<html><body>Apps: {{ appointments | length }}</body></html>"),
        ("appointments/book.html.tera", "<html><body>Book form</body></html>"),
        ("appointments/detail.html.tera", "<html><body>Appt #{{ appointment.id }}</body></html>"),
        ("appointments/suggest.html.tera", "<html><body>Suggest: {{ suggested_slot }}</body></html>"),
        ("appointments/waitlist.html.tera", "<html><body>Waitlist: {{ waitlist | length }}</body></html>"),
        ("appointments/calendar.html.tera", "<html><body>Calendar: {{ month_name }} {{ year }}</body></html>"),
        ("availability/list.html.tera", "<html><body>Slots: {{ slots | length }}</body></html>"),
        ("availability/set.html.tera", "<html><body>Set avail</body></html>"),
        ("records/list.html.tera", "<html><body>Records: {{ records | length }}</body></html>"),
        ("records/create.html.tera", "<html><body>Create record</body></html>"),
        ("records/detail.html.tera", "<html><body>Record #{{ record.id }}</body></html>"),
        ("billing/list.html.tera", "<html><body>Invoices: {{ invoices | length }}</body></html>"),
        ("billing/create.html.tera", "<html><body>Create invoice</body></html>"),
        ("billing/detail.html.tera", "<html><body>Invoice #{{ invoice.id }}</body></html>"),
        ("records/timeline.html.tera", "<html><body>Timeline: {{ events | length }}</body></html>"),
        ("records/report.html.tera", "<html><body>Report #{{ report.record.id }}</body></html>"),
        ("records/prescription_form.html.tera", "<html><body>Prescribe: {{ patients | length }}</body></html>"),
        ("audit/list.html.tera", "<html><body>Audit: {{ entries | length }}</body></html>"),
        ("dashboard/index.html.tera", "<html><body>Dashboard: {{ stats.total_patients }}</body></html>"),
    ];
    for (name, content) in &pages {
        tera.add_raw_template(name, content).expect("add test template");
    }
    tera.add_raw_template("base.html.tera",
        "<!DOCTYPE html><html><body>{% block content %}{% endblock %}</body></html>"
    ).ok();
    tera
}

// ============================================================
// Macro: creates a test app and runs a closure against it.
// Avoids all `Service<Request, Response, Error>` type annotations.
// ============================================================

#[macro_export]
macro_rules! with_test_app {
    ($pool:expr, $app:ident, $body:block) => {
        {
            let tera = $crate::common::test_tera();
            let secret_key = actix_web::cookie::Key::generate();
            let $app = actix_web::test::init_service(
                actix_web::App::new()
                    // Clone into app state so the original pool stays usable in
                    // the test body (e.g. for `seed_and_login!`).
                    .app_data(actix_web::web::Data::new($pool.clone()))
                    .app_data(actix_web::web::Data::new(tera))
                    .wrap(
                        actix_session::SessionMiddleware::builder(
                            actix_session::storage::CookieSessionStore::default(),
                            secret_key,
                        )
                        .cookie_secure(false)
                        .build(),
                    )
                    .configure(patient_management_system::users::configure)
                    .configure(patient_management_system::appointments::configure)
                    .configure(patient_management_system::availability::configure)
                    .configure(patient_management_system::records::configure)
                    .configure(patient_management_system::billing::configure)
                    .configure(patient_management_system::audit::configure)
                    .configure(patient_management_system::dashboard::configure),
            )
            .await;
            $body
        }
    };
}

// ============================================================
// Macro: register a user and return the session cookie
// ============================================================

#[macro_export]
macro_rules! register_and_login {
    ($app:expr, $username:expr, $role:expr) => {{
        let req = actix_web::test::TestRequest::post()
            .uri("/users/register")
            .set_form(&serde_json::json!({
                "username": $username,
                "email": format!("{}@test.com", $username),
                "password": "password123",
                "full_name": format!("Test {}", $username),
                "role": $role,
            }))
            .to_request();
        let resp = actix_web::test::call_service(&$app, req).await;
        assert!(resp.status().is_redirection());
        resp.headers()
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    }};
}

// ============================================================
// Macro: create a STAFF (or any) user directly in the DB, then log in.
//
// Public registration is intentionally patient-only, so doctors/admins are
// created the way production does it (seed script / admin), i.e. inserted
// straight into the database. Returns the session cookie.
// ============================================================

#[macro_export]
macro_rules! seed_and_login {
    ($app:expr, $pool:expr, $username:expr, $role:expr) => {{
        let hash = bcrypt::hash("password123", 4).unwrap();
        sqlx::query(
            "INSERT INTO users (username, email, password_hash, role, full_name)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind($username)
        .bind(format!("{}@test.com", $username))
        .bind(&hash)
        .bind($role)
        .bind(format!("Test {}", $username))
        .execute(&$pool)
        .await
        .unwrap();

        let uid: (i64,) = sqlx::query_as("SELECT id FROM users WHERE username = ?")
            .bind($username)
            .fetch_one(&$pool)
            .await
            .unwrap();

        // Create the matching profile row so id-lookups work.
        match $role {
            "doctor" => {
                sqlx::query("INSERT INTO doctors (user_id, specialization, license_number) VALUES (?, 'General Practice', 'LIC')")
                    .bind(uid.0).execute(&$pool).await.unwrap();
            }
            "patient" => {
                sqlx::query("INSERT INTO patients (user_id) VALUES (?)")
                    .bind(uid.0).execute(&$pool).await.unwrap();
            }
            _ => {} // admin has no extension row
        }

        // Log in through the real endpoint to obtain a session cookie.
        let req = actix_web::test::TestRequest::post()
            .uri("/users/login")
            .set_form(&serde_json::json!({"login": $username, "password": "password123"}))
            .to_request();
        let resp = actix_web::test::call_service(&$app, req).await;
        resp.headers()
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string()
    }};
}

pub fn auth_get(uri: &str, cookie: &str) -> test::TestRequest {
    test::TestRequest::get()
        .uri(uri)
        .insert_header(("Cookie", cookie.to_string()))
}

pub fn auth_post(uri: &str, cookie: &str, body: serde_json::Value) -> test::TestRequest {
    test::TestRequest::post()
        .uri(uri)
        .insert_header(("Cookie", cookie.to_string()))
        .set_form(&body)
}
