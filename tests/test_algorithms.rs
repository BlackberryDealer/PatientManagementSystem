//! Unit tests for the three scheduling algorithms.
mod common;
use common::*;
use actix_web::test;
use patient_management_system::appointments::models::{
    BookAppointmentForm, SuggestSlotForm, WaitlistForm,
};
use patient_management_system::appointments::services;
use sqlx::SqlitePool;

async fn seed_patient(pool: &SqlitePool, uid: i64, name: &str) {
    sqlx::query("INSERT INTO users (id, username, email, password_hash, role, full_name) VALUES (?,?,?,?,'patient',?)")
        .bind(uid).bind(name).bind(format!("{}@t", name)).bind("$2b$10$abc").bind(format!("P {}", name))
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO patients (user_id) VALUES (?)").bind(uid).execute(pool).await.unwrap();
}
async fn seed_doctor(pool: &SqlitePool, uid: i64, name: &str) {
    sqlx::query("INSERT INTO users (id, username, email, password_hash, role, full_name) VALUES (?,?,?,?,'doctor',?)")
        .bind(uid).bind(name).bind(format!("{}@t", name)).bind("$2b$10$abc").bind(format!("Dr {}", name))
        .execute(pool).await.unwrap();
    sqlx::query("INSERT INTO doctors (user_id, specialization, license_number) VALUES (?,?,?)")
        .bind(uid).bind("GP").bind("LIC").execute(pool).await.unwrap();
}

#[actix_web::test]
async fn test_empty_schedule_no_conflict() {
    let pool = test_db_pool().await;
    let r = services::check_conflict(&pool, 1, "2026-06-01", "10:00", "10:30", None, None).await.unwrap();
    assert!(!r);
}

#[actix_web::test]
async fn test_conflict_detected() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "p1").await;
    seed_doctor(&pool, 2, "d1").await;
    let f = BookAppointmentForm { doctor_id: 1, appointment_date: "2026-06-01".into(), start_time: "10:00".into(), end_time: "10:30".into(), room_id: None, priority: Some(3), notes: None };
    services::book_appointment(&pool, 1, &f).await.unwrap();
    assert!(services::check_conflict(&pool, 1, "2026-06-01", "10:15", "10:45", None, None).await.unwrap());
    assert!(!services::check_conflict(&pool, 1, "2026-06-01", "10:30", "11:00", None, None).await.unwrap());
    assert!(!services::check_conflict(&pool, 1, "2026-06-02", "10:00", "10:30", None, None).await.unwrap());
}

#[actix_web::test]
async fn test_cancelled_excluded() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "p2").await;
    seed_doctor(&pool, 2, "d2").await;
    let f = BookAppointmentForm { doctor_id: 1, appointment_date: "2026-06-01".into(), start_time: "10:00".into(), end_time: "10:30".into(), room_id: None, priority: Some(3), notes: None };
    let a = services::book_appointment(&pool, 1, &f).await.unwrap();
    services::cancel_appointment(&pool, a.id).await.unwrap();
    assert!(!services::check_conflict(&pool, 1, "2026-06-01", "10:00", "10:30", None, None).await.unwrap());
}

#[actix_web::test]
async fn test_earliest_slot_empty() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "p3").await; seed_doctor(&pool, 2, "d3").await;
    let f = SuggestSlotForm { doctor_id: 1, appointment_date: "2026-06-01".into(), duration_minutes: 30, room_id: None };
    assert_eq!(services::find_earliest_slot(&pool, &f).await.unwrap(), Some("08:00".into()));
}

#[actix_web::test]
async fn test_earliest_slot_after_existing() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "p4").await; seed_doctor(&pool, 2, "d4").await;
    let f = BookAppointmentForm { doctor_id: 1, appointment_date: "2026-06-01".into(), start_time: "08:00".into(), end_time: "08:30".into(), room_id: None, priority: Some(3), notes: None };
    services::book_appointment(&pool, 1, &f).await.unwrap();
    let s = SuggestSlotForm { doctor_id: 1, appointment_date: "2026-06-01".into(), duration_minutes: 60, room_id: None };
    assert_eq!(services::find_earliest_slot(&pool, &s).await.unwrap(), Some("08:30".into()));
}

#[actix_web::test]
async fn test_earliest_slot_full() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "p5").await; seed_doctor(&pool, 2, "d5").await;
    let f = BookAppointmentForm { doctor_id: 1, appointment_date: "2026-06-01".into(), start_time: "08:00".into(), end_time: "17:00".into(), room_id: None, priority: Some(3), notes: None };
    services::book_appointment(&pool, 1, &f).await.unwrap();
    assert_eq!(services::find_earliest_slot(&pool, &SuggestSlotForm { doctor_id: 1, appointment_date: "2026-06-01".into(), duration_minutes: 30, room_id: None }).await.unwrap(), None);
}

#[actix_web::test]
async fn test_priority_bump() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "plow").await; seed_patient(&pool, 3, "phigh").await; seed_doctor(&pool, 2, "dprio").await;
    let f1 = BookAppointmentForm { doctor_id: 1, appointment_date: "2026-06-01".into(), start_time: "10:00".into(), end_time: "10:30".into(), room_id: None, priority: Some(3), notes: None };
    services::book_appointment(&pool, 1, &f1).await.unwrap();
    let f2 = BookAppointmentForm { doctor_id: 1, appointment_date: "2026-06-01".into(), start_time: "10:00".into(), end_time: "10:30".into(), room_id: None, priority: Some(1), notes: None };
    assert!(services::book_with_priority(&pool, 3, &f2).await.is_ok());
}

#[actix_web::test]
async fn test_priority_equal_rejected() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pa").await; seed_patient(&pool, 3, "pb").await; seed_doctor(&pool, 2, "deq").await;
    let f = BookAppointmentForm { doctor_id: 1, appointment_date: "2026-06-01".into(), start_time: "10:00".into(), end_time: "10:30".into(), room_id: None, priority: Some(1), notes: None };
    services::book_appointment(&pool, 1, &f).await.unwrap();
    let f2 = BookAppointmentForm { doctor_id: 1, appointment_date: "2026-06-01".into(), start_time: "10:00".into(), end_time: "10:30".into(), room_id: None, priority: Some(1), notes: None };
    assert!(services::book_with_priority(&pool, 3, &f2).await.is_err());
}

#[actix_web::test]
async fn test_normal_cannot_override() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pn").await; seed_patient(&pool, 3, "pn2").await; seed_doctor(&pool, 2, "dn").await;
    let f = BookAppointmentForm { doctor_id: 1, appointment_date: "2026-06-01".into(), start_time: "10:00".into(), end_time: "10:30".into(), room_id: None, priority: Some(3), notes: None };
    services::book_appointment(&pool, 1, &f).await.unwrap();
    let f2 = BookAppointmentForm { doctor_id: 1, appointment_date: "2026-06-01".into(), start_time: "10:00".into(), end_time: "10:30".into(), room_id: None, priority: Some(3), notes: None };
    assert!(services::book_with_priority(&pool, 3, &f2).await.is_err());
}

#[actix_web::test]
async fn test_waitlist_add() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pwl").await; seed_doctor(&pool, 2, "dwl").await;
    let f = WaitlistForm { doctor_id: 1, appointment_date: "2026-06-01".into(), requested_start: "10:00".into(), requested_end: "10:30".into(), priority: 2, room_id: None, notes: None };
    let e = services::add_to_waitlist(&pool, 1, &f).await.unwrap();
    assert_eq!(e.status, "waiting");
}

#[actix_web::test]
async fn test_waitlist_promote() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pwp").await; seed_doctor(&pool, 2, "dwp").await;
    let f = WaitlistForm { doctor_id: 1, appointment_date: "2026-06-01".into(), requested_start: "10:00".into(), requested_end: "10:30".into(), priority: 2, room_id: None, notes: None };
    let e = services::add_to_waitlist(&pool, 1, &f).await.unwrap();
    let r = services::promote_from_waitlist(&pool, e.id).await.unwrap();
    assert!(r.is_some());
}

#[actix_web::test]
async fn test_cancel_triggers_waitlist_promotion() {
    // Cancel an appointment → the system auto-promotes the highest-priority waitlist entry
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pcancel").await;
    seed_patient(&pool, 3, "pwlpat").await;
    seed_doctor(&pool, 2, "dcancel").await;

    // Book the slot
    let booked = services::book_appointment(&pool, 1, &BookAppointmentForm {
        doctor_id: 1, appointment_date: "2026-06-10".into(),
        start_time: "10:00".into(), end_time: "10:30".into(),
        room_id: None, priority: Some(3), notes: None,
    }).await.unwrap();

    // Add second patient to waitlist for same slot
    services::add_to_waitlist(&pool, 3, &WaitlistForm {
        doctor_id: 1, appointment_date: "2026-06-10".into(),
        requested_start: "10:00".into(), requested_end: "10:30".into(),
        priority: 2, room_id: None, notes: None,
    }).await.unwrap();

    // Cancel triggers auto-promotion — slot is now free and taken by waitlisted patient
    services::cancel_appointment(&pool, booked.id).await.unwrap();

    // Verify the slot is booked again (for the waitlisted patient)
    assert!(services::check_conflict(&pool, 1, "2026-06-10", "10:00", "10:30", None, None).await.unwrap());
}

#[actix_web::test]
async fn test_ownership_check_blocks_other_patient() {
    // A patient should NOT be able to cancel another patient's appointment
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pown1").await;
    seed_patient(&pool, 3, "pown2").await;
    seed_doctor(&pool, 2, "down").await;

    let booked = services::book_appointment(&pool, 1, &BookAppointmentForm {
        doctor_id: 1, appointment_date: "2026-06-15".into(),
        start_time: "09:00".into(), end_time: "09:30".into(),
        room_id: None, priority: Some(3), notes: None,
    }).await.unwrap();

    // Patient 2 (user_id=3) tries to cancel patient 1's appointment → Forbidden
    let result = services::cancel_appointment_checked(&pool, booked.id, 3, "patient").await;
    assert!(result.is_err());
}

#[actix_web::test]
async fn test_ownership_check_allows_own_appointment() {
    // A patient should be able to cancel their own appointment
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pown3").await;
    seed_doctor(&pool, 2, "down2").await;

    let booked = services::book_appointment(&pool, 1, &BookAppointmentForm {
        doctor_id: 1, appointment_date: "2026-06-15".into(),
        start_time: "11:00".into(), end_time: "11:30".into(),
        room_id: None, priority: Some(3), notes: None,
    }).await.unwrap();

    let result = services::cancel_appointment_checked(&pool, booked.id, 1, "patient").await;
    assert!(result.is_ok());
}

#[actix_web::test]
async fn test_multi_gap_earliest_slot() {
    // Schedule: 08:00–09:00, 09:30–10:30, 11:00–17:00
    // A 30-min request should find the gap 09:00–09:30
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pgap").await;
    seed_doctor(&pool, 2, "dgap").await;

    for (s, e) in [("08:00","09:00"),("09:30","10:30"),("11:00","17:00")] {
        services::book_appointment(&pool, 1, &BookAppointmentForm {
            doctor_id: 1, appointment_date: "2026-07-01".into(),
            start_time: s.into(), end_time: e.into(),
            room_id: None, priority: Some(3), notes: None,
        }).await.unwrap();
    }

    let slot = services::find_earliest_slot(&pool, &SuggestSlotForm {
        doctor_id: 1, appointment_date: "2026-07-01".into(),
        duration_minutes: 30, room_id: None,
    }).await.unwrap();
    assert_eq!(slot, Some("09:00".into()));
}
