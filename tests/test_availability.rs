//! Integration tests for availability endpoints: the two-mode set form
//! (multi-day recurring / date-derived one-off), edit, delete, ownership,
//! and the stranded-appointment guard.
mod common;
use common::*;
use actix_web::test;

/// Row shape used by assertions below.
type SlotRow = (i64, i32, String, String, i32, Option<String>, i32);

async fn slots_for_doctor(pool: &sqlx::SqlitePool, doctor_id: i64) -> Vec<SlotRow> {
    sqlx::query_as(
        "SELECT id, day_of_week, start_time, end_time, is_recurring, specific_date, is_blocked
         FROM doctor_availability WHERE doctor_id = ? ORDER BY day_of_week, start_time",
    )
    .bind(doctor_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn doctor_id_of(pool: &sqlx::SqlitePool, username: &str) -> i64 {
    let (id,): (i64,) = sqlx::query_as(
        "SELECT d.id FROM doctors d JOIN users u ON d.user_id = u.id WHERE u.username = ?",
    )
    .bind(username)
    .fetch_one(pool)
    .await
    .unwrap();
    id
}

#[actix_web::test]
async fn test_availability_page() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = seed_and_login!(app, pool, "adoc", "doctor");
        let req = auth_get("/availability", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    });
}

#[actix_web::test]
async fn test_set_availability_form() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = seed_and_login!(app, pool, "asetdoc", "doctor");
        let req = auth_get("/availability/set", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    });
}

// One recurring submit with several weekdays ticked creates one weekly
// rule per day — "Mon–Fri 9 to 5" is a single form post.
#[actix_web::test]
async fn test_recurring_submit_creates_one_row_per_day() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = seed_and_login!(app, pool, "asubdoc", "doctor");
        let did = doctor_id_of(&pool, "asubdoc").await;
        // Replace the default full-week test schedule with what the form sends.
        sqlx::query("DELETE FROM doctor_availability WHERE doctor_id = ?")
            .bind(did).execute(&pool).await.unwrap();

        let req = auth_post("/availability/set", &cookie, serde_json::json!({
            "mode": "recurring",
            "day_1": "on", "day_2": "on", "day_3": "on", "day_4": "on", "day_5": "on",
            "start_time": "09:00", "end_time": "17:00",
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());

        let rows = slots_for_doctor(&pool, did).await;
        assert_eq!(rows.len(), 5, "five ticked weekdays create five weekly rules");
        let days: Vec<i32> = rows.iter().map(|r| r.1).collect();
        assert_eq!(days, vec![1, 2, 3, 4, 5]);
        assert!(rows.iter().all(|r| r.4 == 1 && r.5.is_none() && r.6 == 0));
    });
}

// A recurring submit with no day ticked is a user mistake, not a no-op.
#[actix_web::test]
async fn test_recurring_submit_requires_a_day() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = seed_and_login!(app, pool, "anoday", "doctor");
        let req = auth_post("/availability/set", &cookie, serde_json::json!({
            "mode": "recurring",
            "start_time": "09:00", "end_time": "17:00",
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 400);
    });
}

// One-off entries take only a date; the weekday is derived from it on the
// server, so the stored day can never contradict the picked date.
#[actix_web::test]
async fn test_oneoff_derives_weekday_from_date() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = seed_and_login!(app, pool, "aoneoff", "doctor");
        let did = doctor_id_of(&pool, "aoneoff").await;

        // 2027-06-07 is a Monday (day 1 in the 0=Sunday convention).
        let req = auth_post("/availability/set", &cookie, serde_json::json!({
            "mode": "oneoff",
            "specific_date": "2027-06-07",
            "start_time": "10:00", "end_time": "14:00",
            "is_blocked": "on",
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());

        let rows = slots_for_doctor(&pool, did).await;
        let oneoff = rows.iter().find(|r| r.5.is_some()).expect("one-off row created");
        assert_eq!(oneoff.1, 1, "weekday must be derived from the date (Monday)");
        assert_eq!(oneoff.5.as_deref(), Some("2027-06-07"));
        assert_eq!(oneoff.6, 1, "blocked flag preserved");
    });
}

// A one-off entry without a date, or with a past date, is rejected.
#[actix_web::test]
async fn test_oneoff_requires_valid_future_date() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = seed_and_login!(app, pool, "abaddate", "doctor");
        for body in [
            serde_json::json!({"mode": "oneoff", "start_time": "10:00", "end_time": "14:00"}),
            serde_json::json!({"mode": "oneoff", "specific_date": "2020-01-01", "start_time": "10:00", "end_time": "14:00"}),
        ] {
            let req = auth_post("/availability/set", &cookie, body).to_request();
            let resp = test::call_service(&app, req).await;
            assert_eq!(resp.status().as_u16(), 400);
        }
    });
}

// Overlapping same-kind windows are duplicates, not extra availability.
#[actix_web::test]
async fn test_overlapping_recurring_window_rejected() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = seed_and_login!(app, pool, "adup", "doctor");
        // The default test schedule already covers Monday 08:00–17:00.
        let req = auth_post("/availability/set", &cookie, serde_json::json!({
            "mode": "recurring", "day_1": "on",
            "start_time": "09:00", "end_time": "12:00",
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status().as_u16(), 400, "overlap with an existing Monday window");
    });
}

#[actix_web::test]
async fn test_edit_availability_slot() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = seed_and_login!(app, pool, "aedit", "doctor");
        let did = doctor_id_of(&pool, "aedit").await;
        let slot_id = slots_for_doctor(&pool, did).await[0].0;

        let form_page = auth_get(&format!("/availability/{slot_id}/edit"), &cookie).to_request();
        let resp = test::call_service(&app, form_page).await;
        assert!(resp.status().is_success());

        // Narrow Sunday (day 0, the first row) to 10:00–12:00.
        let req = auth_post(&format!("/availability/{slot_id}/edit"), &cookie, serde_json::json!({
            "day_of_week": 0,
            "start_time": "10:00", "end_time": "12:00",
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());

        let row = slots_for_doctor(&pool, did).await.into_iter()
            .find(|r| r.0 == slot_id).unwrap();
        assert_eq!((row.2.as_str(), row.3.as_str()), ("10:00", "12:00"));
    });
}

#[actix_web::test]
async fn test_delete_availability_slot() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = seed_and_login!(app, pool, "adel", "doctor");
        let did = doctor_id_of(&pool, "adel").await;
        let before = slots_for_doctor(&pool, did).await;
        let slot_id = before[0].0;

        let req = auth_post(&format!("/availability/{slot_id}/delete"), &cookie,
            serde_json::json!({})).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection());

        let after = slots_for_doctor(&pool, did).await;
        assert_eq!(after.len(), before.len() - 1);
        assert!(after.iter().all(|r| r.0 != slot_id));
    });
}

// A doctor must not be able to touch a colleague's schedule.
#[actix_web::test]
async fn test_cannot_manage_other_doctors_slot() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _owner = seed_and_login!(app, pool, "aowner", "doctor");
        let intruder = seed_and_login!(app, pool, "aintruder", "doctor");
        let owner_did = doctor_id_of(&pool, "aowner").await;
        let slot_id = slots_for_doctor(&pool, owner_did).await[0].0;

        let del = auth_post(&format!("/availability/{slot_id}/delete"), &intruder,
            serde_json::json!({})).to_request();
        let resp = test::call_service(&app, del).await;
        assert_eq!(resp.status().as_u16(), 403);

        let edit = auth_post(&format!("/availability/{slot_id}/edit"), &intruder, serde_json::json!({
            "day_of_week": 1, "start_time": "10:00", "end_time": "12:00",
        })).to_request();
        let resp = test::call_service(&app, edit).await;
        assert_eq!(resp.status().as_u16(), 403);

        assert!(slots_for_doctor(&pool, owner_did).await.iter().any(|r| r.0 == slot_id));
    });
}

// The stranded-appointment guard: no availability change may leave an
// upcoming booked appointment outside the doctor's working hours.
#[actix_web::test]
async fn test_change_stranding_appointment_rejected() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let dcookie = seed_and_login!(app, pool, "aguard", "doctor");
        let pcookie = register_and_login!(app, "aguardpat", "patient");
        let did = doctor_id_of(&pool, "aguard").await;

        // Patient books 2027-06-07 (Monday) 10:00 with the doctor.
        let req = auth_post("/appointments/book", &pcookie, serde_json::json!({
            "doctor_id": did,
            "appointment_date": "2027-06-07",
            "start_time": "10:00", "end_time": "10:30",
        })).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_redirection(), "booking should succeed");

        // Deleting the Monday window would strand that appointment — rejected.
        let monday_slot = slots_for_doctor(&pool, did).await.into_iter()
            .find(|r| r.1 == 1).unwrap().0;
        let del = auth_post(&format!("/availability/{monday_slot}/delete"), &dcookie,
            serde_json::json!({})).to_request();
        let resp = test::call_service(&app, del).await;
        assert_eq!(resp.status().as_u16(), 400, "delete would strand a booked appointment");

        // Blocking that whole date is rejected for the same reason.
        let block = auth_post("/availability/set", &dcookie, serde_json::json!({
            "mode": "oneoff", "specific_date": "2027-06-07",
            "start_time": "08:00", "end_time": "17:00", "is_blocked": "on",
        })).to_request();
        let resp = test::call_service(&app, block).await;
        assert_eq!(resp.status().as_u16(), 400, "leave over a booked appointment is rejected");

        // Narrowing the Monday window to the afternoon is rejected too...
        let edit = auth_post(&format!("/availability/{monday_slot}/edit"), &dcookie, serde_json::json!({
            "day_of_week": 1, "start_time": "13:00", "end_time": "17:00",
        })).to_request();
        let resp = test::call_service(&app, edit).await;
        assert_eq!(resp.status().as_u16(), 400, "edit would strand the 10:00 appointment");

        // ...but a narrowing that still covers the appointment is fine.
        let edit_ok = auth_post(&format!("/availability/{monday_slot}/edit"), &dcookie, serde_json::json!({
            "day_of_week": 1, "start_time": "09:00", "end_time": "13:00",
        })).to_request();
        let resp = test::call_service(&app, edit_ok).await;
        assert!(resp.status().is_redirection(), "covering window edit is allowed");
    });
}

// Closed-by-default, end to end over HTTP: with no rules for a day, the
// live slot lookup offers nothing and a direct booking attempt fails.
#[actix_web::test]
async fn test_unpublished_day_not_bookable_http() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let _doc = seed_and_login!(app, pool, "aclosed", "doctor");
        let pcookie = register_and_login!(app, "aclosedpat", "patient");
        let did = doctor_id_of(&pool, "aclosed").await;
        sqlx::query("DELETE FROM doctor_availability WHERE doctor_id = ?")
            .bind(did).execute(&pool).await.unwrap();

        let req = auth_get(
            &format!("/appointments/availability?doctor_id={did}&date=2027-06-07"),
            &pcookie,
        ).to_request();
        let resp = test::call_service(&app, req).await;
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["slots"].as_array().map(|a| a.len()), Some(0));

        let book = auth_post("/appointments/book", &pcookie, serde_json::json!({
            "doctor_id": did,
            "appointment_date": "2027-06-07",
            "start_time": "10:00", "end_time": "10:30",
        })).to_request();
        let resp = test::call_service(&app, book).await;
        assert_eq!(resp.status().as_u16(), 400, "unpublished day must not be bookable");
    });
}
