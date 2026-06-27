//! Unit tests for the three scheduling algorithms.
mod common;
use common::*;
use actix_web::test;
use patient_management_system::appointments::models::{
    BookAppointmentForm, SuggestSlotForm, WaitlistForm,
};
use patient_management_system::appointments::services;
use patient_management_system::auth::Role;
use patient_management_system::traits::StatusManaged;
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
    let r = services::check_conflict(&pool, 1, "2027-06-01", "10:00", "10:30", None, None).await.unwrap();
    assert!(!r);
}

#[actix_web::test]
async fn test_conflict_detected() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "p1").await;
    seed_doctor(&pool, 2, "d1").await;
    let f = BookAppointmentForm { doctor_id: 1, appointment_date: "2027-06-01".into(), start_time: "10:00".into(), end_time: "10:30".into(), room_id: None, priority: Some(3), notes: None };
    services::book_appointment(&pool, 1, &f).await.unwrap();
    assert!(services::check_conflict(&pool, 1, "2027-06-01", "10:15", "10:45", None, None).await.unwrap());
    assert!(!services::check_conflict(&pool, 1, "2027-06-01", "10:30", "11:00", None, None).await.unwrap());
    assert!(!services::check_conflict(&pool, 1, "2027-06-02", "10:00", "10:30", None, None).await.unwrap());
}

#[actix_web::test]
async fn test_cancelled_excluded() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "p2").await;
    seed_doctor(&pool, 2, "d2").await;
    let f = BookAppointmentForm { doctor_id: 1, appointment_date: "2027-06-01".into(), start_time: "10:00".into(), end_time: "10:30".into(), room_id: None, priority: Some(3), notes: None };
    let a = services::book_appointment(&pool, 1, &f).await.unwrap();
    services::cancel_appointment(&pool, a.id).await.unwrap();
    assert!(!services::check_conflict(&pool, 1, "2027-06-01", "10:00", "10:30", None, None).await.unwrap());
}

#[actix_web::test]
async fn test_earliest_slot_empty() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "p3").await; seed_doctor(&pool, 2, "d3").await;
    let f = SuggestSlotForm { doctor_id: 1, appointment_date: "2027-06-01".into(), duration_minutes: 30, room_id: None };
    assert_eq!(services::find_earliest_slot(&pool, &f).await.unwrap(), Some("08:00".into()));
}

#[actix_web::test]
async fn test_earliest_slot_after_existing() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "p4").await; seed_doctor(&pool, 2, "d4").await;
    let f = BookAppointmentForm { doctor_id: 1, appointment_date: "2027-06-01".into(), start_time: "08:00".into(), end_time: "08:30".into(), room_id: None, priority: Some(3), notes: None };
    services::book_appointment(&pool, 1, &f).await.unwrap();
    let s = SuggestSlotForm { doctor_id: 1, appointment_date: "2027-06-01".into(), duration_minutes: 60, room_id: None };
    assert_eq!(services::find_earliest_slot(&pool, &s).await.unwrap(), Some("08:30".into()));
}

#[actix_web::test]
async fn test_earliest_slot_full() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "p5").await; seed_doctor(&pool, 2, "d5").await;
    let f = BookAppointmentForm { doctor_id: 1, appointment_date: "2027-06-01".into(), start_time: "08:00".into(), end_time: "17:00".into(), room_id: None, priority: Some(3), notes: None };
    services::book_appointment(&pool, 1, &f).await.unwrap();
    assert_eq!(services::find_earliest_slot(&pool, &SuggestSlotForm { doctor_id: 1, appointment_date: "2027-06-01".into(), duration_minutes: 30, room_id: None }).await.unwrap(), None);
}

#[actix_web::test]
async fn test_priority_bump() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "plow").await; seed_patient(&pool, 3, "phigh").await; seed_doctor(&pool, 2, "dprio").await;
    let f1 = BookAppointmentForm { doctor_id: 1, appointment_date: "2027-06-01".into(), start_time: "10:00".into(), end_time: "10:30".into(), room_id: None, priority: Some(3), notes: None };
    services::book_appointment(&pool, 1, &f1).await.unwrap();
    let f2 = BookAppointmentForm { doctor_id: 1, appointment_date: "2027-06-01".into(), start_time: "10:00".into(), end_time: "10:30".into(), room_id: None, priority: Some(1), notes: None };
    assert!(services::book_with_priority(&pool, 3, &f2).await.is_ok());
}

#[actix_web::test]
async fn test_priority_equal_rejected() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pa").await; seed_patient(&pool, 3, "pb").await; seed_doctor(&pool, 2, "deq").await;
    let f = BookAppointmentForm { doctor_id: 1, appointment_date: "2027-06-01".into(), start_time: "10:00".into(), end_time: "10:30".into(), room_id: None, priority: Some(1), notes: None };
    services::book_appointment(&pool, 1, &f).await.unwrap();
    let f2 = BookAppointmentForm { doctor_id: 1, appointment_date: "2027-06-01".into(), start_time: "10:00".into(), end_time: "10:30".into(), room_id: None, priority: Some(1), notes: None };
    assert!(services::book_with_priority(&pool, 3, &f2).await.is_err());
}

#[actix_web::test]
async fn test_normal_cannot_override() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pn").await; seed_patient(&pool, 3, "pn2").await; seed_doctor(&pool, 2, "dn").await;
    let f = BookAppointmentForm { doctor_id: 1, appointment_date: "2027-06-01".into(), start_time: "10:00".into(), end_time: "10:30".into(), room_id: None, priority: Some(3), notes: None };
    services::book_appointment(&pool, 1, &f).await.unwrap();
    let f2 = BookAppointmentForm { doctor_id: 1, appointment_date: "2027-06-01".into(), start_time: "10:00".into(), end_time: "10:30".into(), room_id: None, priority: Some(3), notes: None };
    assert!(services::book_with_priority(&pool, 3, &f2).await.is_err());
}

#[actix_web::test]
async fn test_waitlist_add() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pwl").await; seed_doctor(&pool, 2, "dwl").await;
    let f = WaitlistForm { doctor_id: 1, appointment_date: "2027-06-01".into(), requested_start: "10:00".into(), requested_end: "10:30".into(), priority: 2, room_id: None, notes: None };
    let e = services::add_to_waitlist(&pool, 1, &f).await.unwrap();
    assert_eq!(e.current_status(), "waiting");
}

#[actix_web::test]
async fn test_waitlist_promote() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pwp").await; seed_doctor(&pool, 2, "dwp").await;
    let f = WaitlistForm { doctor_id: 1, appointment_date: "2027-06-01".into(), requested_start: "10:00".into(), requested_end: "10:30".into(), priority: 2, room_id: None, notes: None };
    let e = services::add_to_waitlist(&pool, 1, &f).await.unwrap();
    let r = services::promote_from_waitlist(&pool, e.id).await.unwrap();
    assert!(r.is_some());
}

#[actix_web::test]
async fn test_cancel_triggers_waitlist_promotion() {
    // Cancel an appointment â†’ the system auto-promotes the highest-priority waitlist entry
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pcancel").await;
    seed_patient(&pool, 3, "pwlpat").await;
    seed_doctor(&pool, 2, "dcancel").await;

    // Book the slot
    let booked = services::book_appointment(&pool, 1, &BookAppointmentForm {
        doctor_id: 1, appointment_date: "2027-06-10".into(),
        start_time: "10:00".into(), end_time: "10:30".into(),
        room_id: None, priority: Some(3), notes: None,
    }).await.unwrap();

    // Add second patient to waitlist for same slot
    services::add_to_waitlist(&pool, 3, &WaitlistForm {
        doctor_id: 1, appointment_date: "2027-06-10".into(),
        requested_start: "10:00".into(), requested_end: "10:30".into(),
        priority: 2, room_id: None, notes: None,
    }).await.unwrap();

    // Cancel triggers auto-promotion â€” slot is now free and taken by waitlisted patient
    services::cancel_appointment(&pool, booked.id).await.unwrap();

    // Verify the slot is booked again (for the waitlisted patient)
    assert!(services::check_conflict(&pool, 1, "2027-06-10", "10:00", "10:30", None, None).await.unwrap());
}

#[actix_web::test]
async fn test_ownership_check_blocks_other_patient() {
    // A patient should NOT be able to cancel another patient's appointment
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pown1").await;
    seed_patient(&pool, 3, "pown2").await;
    seed_doctor(&pool, 2, "down").await;

    let booked = services::book_appointment(&pool, 1, &BookAppointmentForm {
        doctor_id: 1, appointment_date: "2027-06-15".into(),
        start_time: "09:00".into(), end_time: "09:30".into(),
        room_id: None, priority: Some(3), notes: None,
    }).await.unwrap();

    // Patient 2 (user_id=3) tries to cancel patient 1's appointment â†’ Forbidden
    let result = services::cancel_appointment_checked(&pool, booked.id, 3, Role::Patient).await;
    assert!(result.is_err());
}

#[actix_web::test]
async fn test_ownership_check_allows_own_appointment() {
    // A patient should be able to cancel their own appointment
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pown3").await;
    seed_doctor(&pool, 2, "down2").await;

    let booked = services::book_appointment(&pool, 1, &BookAppointmentForm {
        doctor_id: 1, appointment_date: "2027-06-15".into(),
        start_time: "11:00".into(), end_time: "11:30".into(),
        room_id: None, priority: Some(3), notes: None,
    }).await.unwrap();

    let result = services::cancel_appointment_checked(&pool, booked.id, 1, Role::Patient).await;
    assert!(result.is_ok());
}

#[actix_web::test]
async fn test_multi_gap_earliest_slot() {
    // Schedule: 08:00â€“09:00, 09:30â€“10:30, 11:00â€“17:00
    // A 30-min request should find the gap 09:00â€“09:30
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pgap").await;
    seed_doctor(&pool, 2, "dgap").await;

    for (s, e) in [("08:00","09:00"),("09:30","10:30"),("11:00","17:00")] {
        services::book_appointment(&pool, 1, &BookAppointmentForm {
            doctor_id: 1, appointment_date: "2027-07-01".into(),
            start_time: s.into(), end_time: e.into(),
            room_id: None, priority: Some(3), notes: None,
        }).await.unwrap();
    }

    let slot = services::find_earliest_slot(&pool, &SuggestSlotForm {
        doctor_id: 1, appointment_date: "2027-07-01".into(),
        duration_minutes: 30, room_id: None,
    }).await.unwrap();
    assert_eq!(slot, Some("09:00".into()));
}

// ============================================================
// Edge case tests
// ============================================================

#[actix_web::test]
async fn test_book_invalid_time_rejected() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pinv").await;
    seed_doctor(&pool, 2, "dinv").await;

    // start > end
    let f = BookAppointmentForm {
        doctor_id: 1, appointment_date: "2027-06-01".into(),
        start_time: "15:00".into(), end_time: "14:00".into(),
        room_id: None, priority: Some(3), notes: None,
    };
    assert!(services::book_appointment(&pool, 1, &f).await.is_err());

    // start == end
    let f2 = BookAppointmentForm {
        doctor_id: 1, appointment_date: "2027-06-01".into(),
        start_time: "10:00".into(), end_time: "10:00".into(),
        room_id: None, priority: Some(3), notes: None,
    };
    assert!(services::book_appointment(&pool, 1, &f2).await.is_err());
}

#[actix_web::test]
async fn test_suggest_slot_invalid_duration_rejected() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pdur").await;
    seed_doctor(&pool, 2, "ddur").await;

    // 0 minutes
    assert!(services::find_earliest_slot(&pool, &SuggestSlotForm {
        doctor_id: 1, appointment_date: "2027-06-01".into(),
        duration_minutes: 0, room_id: None,
    }).await.is_err());

    // Negative
    assert!(services::find_earliest_slot(&pool, &SuggestSlotForm {
        doctor_id: 1, appointment_date: "2027-06-01".into(),
        duration_minutes: -5, room_id: None,
    }).await.is_err());

    // Over 480 (more than a workday)
    assert!(services::find_earliest_slot(&pool, &SuggestSlotForm {
        doctor_id: 1, appointment_date: "2027-06-01".into(),
        duration_minutes: 500, room_id: None,
    }).await.is_err());
}

#[actix_web::test]
async fn test_room_conflict_detected() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "prm1").await;
    seed_doctor(&pool, 2, "drm").await;

    // Book doctor 1 in room 1 at 10:00-10:30
    services::book_appointment(&pool, 1, &BookAppointmentForm {
        doctor_id: 1, appointment_date: "2027-06-01".into(),
        start_time: "10:00".into(), end_time: "10:30".into(),
        room_id: Some(1), priority: Some(3), notes: None,
    }).await.unwrap();

    // Same doctor, same room, overlapping time â€” should conflict
    assert!(services::check_conflict(&pool, 1, "2027-06-01", "10:15", "10:45", Some(1), None).await.unwrap());

    // Same doctor, same time, DIFFERENT room â€” should NOT conflict
    assert!(!services::check_conflict(&pool, 1, "2027-06-01", "10:15", "10:45", Some(2), None).await.unwrap());

    // Same doctor, same time, no room filter â€” SHOULD conflict (doctor is busy regardless of room)
    assert!(services::check_conflict(&pool, 1, "2027-06-01", "10:00", "10:30", None, None).await.unwrap());

    // Different doctor (2), same room, same time â€” no conflict (different doctors can use same room at different times, and doctor 2 isn't booked)
    assert!(!services::check_conflict(&pool, 2, "2027-06-01", "10:00", "10:30", Some(1), None).await.unwrap());
}

#[actix_web::test]
async fn test_suggest_slot_respects_existing() {
    // Suggest slot should return the gap, not overlap existing appointments
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "presp").await;
    seed_doctor(&pool, 2, "dresp").await;

    // Book 09:00-10:00, 10:30-11:00, 13:00-14:00
    for (s, e) in [("09:00","10:00"),("10:30","11:00"),("13:00","14:00")] {
        services::book_appointment(&pool, 1, &BookAppointmentForm {
            doctor_id: 1, appointment_date: "2027-07-15".into(),
            start_time: s.into(), end_time: e.into(),
            room_id: None, priority: Some(3), notes: None,
        }).await.unwrap();
    }

    // A 25-minute slot should land at 08:00 (before 09:00)
    let slot = services::find_earliest_slot(&pool, &SuggestSlotForm {
        doctor_id: 1, appointment_date: "2027-07-15".into(),
        duration_minutes: 25, room_id: None,
    }).await.unwrap();
    assert_eq!(slot, Some("08:00".into()));

    // A 90-minute slot should land at 11:00 (between 11:00 and 13:00 â€” the only gap big enough)
    let slot2 = services::find_earliest_slot(&pool, &SuggestSlotForm {
        doctor_id: 1, appointment_date: "2027-07-15".into(),
        duration_minutes: 90, room_id: None,
    }).await.unwrap();
    assert_eq!(slot2, Some("11:00".into()));
}

// ============================================================
// 30-minute slot grid + occupancy-table (double-booking) tests
// ============================================================

/// Helper: count occupancy rows for an appointment.
async fn slot_count(pool: &SqlitePool, appointment_id: i64) -> i64 {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM appointment_slots WHERE appointment_id = ?")
        .bind(appointment_id)
        .fetch_one(pool)
        .await
        .unwrap();
    row.0
}

#[actix_web::test]
async fn test_unaligned_slot_rejected() {
    // Booking must land on the 30-minute grid; 10:15â€“10:45 is off-grid.
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "palign").await;
    seed_doctor(&pool, 2, "dalign").await;
    let f = BookAppointmentForm {
        doctor_id: 1, appointment_date: "2027-06-01".into(),
        start_time: "10:15".into(), end_time: "10:45".into(),
        room_id: None, priority: Some(3), notes: None,
    };
    assert!(services::book_appointment(&pool, 1, &f).await.is_err());
}

#[actix_web::test]
async fn test_aligned_30min_creates_one_slot() {
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "p1slot").await;
    seed_doctor(&pool, 2, "d1slot").await;
    let appt = services::book_appointment(&pool, 1, &BookAppointmentForm {
        doctor_id: 1, appointment_date: "2027-06-01".into(),
        start_time: "10:00".into(), end_time: "10:30".into(),
        room_id: None, priority: Some(3), notes: None,
    }).await.unwrap();
    // One 30-minute appointment occupies exactly one slot row.
    assert_eq!(slot_count(&pool, appt.id).await, 1);
}

#[actix_web::test]
async fn test_multi_slot_creates_one_row_per_slot() {
    // A 90-minute appointment (10:00â€“11:30) occupies three 30-minute slots.
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pmulti").await;
    seed_doctor(&pool, 2, "dmulti").await;
    let appt = services::book_appointment(&pool, 1, &BookAppointmentForm {
        doctor_id: 1, appointment_date: "2027-06-01".into(),
        start_time: "10:00".into(), end_time: "11:30".into(),
        room_id: None, priority: Some(3), notes: None,
    }).await.unwrap();
    assert_eq!(slot_count(&pool, appt.id).await, 3); // 10:00, 10:30, 11:00
}

#[actix_web::test]
async fn test_multi_slot_blocks_overlapping_booking() {
    // After a 10:00â€“11:00 booking (slots 10:00, 10:30), a 10:30â€“11:00 booking
    // must be rejected because it collides on the 10:30 slot.
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pova").await;
    seed_patient(&pool, 3, "povb").await;
    seed_doctor(&pool, 2, "dov").await;

    services::book_appointment(&pool, 1, &BookAppointmentForm {
        doctor_id: 1, appointment_date: "2027-06-01".into(),
        start_time: "10:00".into(), end_time: "11:00".into(),
        room_id: None, priority: Some(3), notes: None,
    }).await.unwrap();

    let clash = services::book_appointment(&pool, 3, &BookAppointmentForm {
        doctor_id: 1, appointment_date: "2027-06-01".into(),
        start_time: "10:30".into(), end_time: "11:00".into(),
        room_id: None, priority: Some(3), notes: None,
    }).await;
    assert!(clash.is_err(), "overlapping slot must be rejected");

    // The non-overlapping 11:00â€“11:30 slot is still bookable.
    let ok = services::book_appointment(&pool, 3, &BookAppointmentForm {
        doctor_id: 1, appointment_date: "2027-06-01".into(),
        start_time: "11:00".into(), end_time: "11:30".into(),
        room_id: None, priority: Some(3), notes: None,
    }).await;
    assert!(ok.is_ok(), "adjacent free slot should book");
}

#[actix_web::test]
async fn test_db_unique_index_is_the_backstop() {
    // Prove the database itself prevents double-booking, independent of the
    // application-level check_conflict. We book normally, then attempt a RAW
    // duplicate slot insert (simulating a lost race that slipped past the
    // app check). The UNIQUE index must reject it.
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "praw").await;
    seed_doctor(&pool, 2, "draw").await;

    services::book_appointment(&pool, 1, &BookAppointmentForm {
        doctor_id: 1, appointment_date: "2027-06-01".into(),
        start_time: "10:00".into(), end_time: "10:30".into(),
        room_id: None, priority: Some(3), notes: None,
    }).await.unwrap();

    // Direct insert of the same (doctor_id, date, slot_time) â€” bypasses all
    // service-layer checks. The DB UNIQUE index is the last line of defence.
    let raw = sqlx::query(
        "INSERT INTO appointment_slots (appointment_id, doctor_id, appointment_date, slot_time)
         VALUES (999, 1, '2027-06-01', '10:00')",
    )
    .execute(&pool)
    .await;

    assert!(raw.is_err(), "DB UNIQUE index must reject a duplicate slot");
    let err = raw.unwrap_err();
    assert!(
        err.as_database_error().map_or(false, |e| e.is_unique_violation()),
        "error should be a UNIQUE violation, got: {err}"
    );
}

#[actix_web::test]
async fn test_cancel_releases_slots_for_rebooking() {
    // Cancelling deletes the occupancy rows so the slot can be rebooked.
    let pool = test_db_pool().await;
    seed_patient(&pool, 1, "pfree").await;
    seed_doctor(&pool, 2, "dfree").await;

    let appt = services::book_appointment(&pool, 1, &BookAppointmentForm {
        doctor_id: 1, appointment_date: "2027-06-20".into(),
        start_time: "10:00".into(), end_time: "10:30".into(),
        room_id: None, priority: Some(3), notes: None,
    }).await.unwrap();
    assert_eq!(slot_count(&pool, appt.id).await, 1);

    services::cancel_appointment(&pool, appt.id).await.unwrap();
    assert_eq!(slot_count(&pool, appt.id).await, 0, "slots freed on cancel");

    // The slot is available again.
    let rebook = services::book_appointment(&pool, 1, &BookAppointmentForm {
        doctor_id: 1, appointment_date: "2027-06-20".into(),
        start_time: "10:00".into(), end_time: "10:30".into(),
        room_id: None, priority: Some(3), notes: None,
    }).await;
    assert!(rebook.is_ok(), "freed slot should be rebookable");
}
