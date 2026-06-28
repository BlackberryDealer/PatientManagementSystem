use crate::appointments::models::Appointment;
use crate::errors::AppError;
use crate::time::{minutes_to_time, parse_slot, SLOT_MINUTES};
use sqlx::SqlitePool;

/// Translate a slot-insert failure: a UNIQUE-index violation means another
/// booking grabbed the slot first (a race we lost), surfaced as a clean 400.
pub(super) fn map_slot_conflict(e: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db) = &e {
        if db.is_unique_violation() {
            return AppError::BadRequest(
                "That time slot has just been taken. Please choose another slot.".into(),
            );
        }
    }
    AppError::DatabaseError(e)
}

/// Copy a bumped appointment onto the waitlist (preserving its slot, room, and
/// priority) inside the booking transaction.
pub(super) async fn bump_to_waitlist(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    appointment_id: i64,
    notes: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO waitlist (patient_id, doctor_id, room_id, appointment_date,
         requested_start, requested_end, priority, notes, status)
         SELECT patient_id, doctor_id, room_id, appointment_date,
                start_time, end_time, priority, ?, 'waiting'
         FROM appointments WHERE id = ?",
    )
    .bind(notes)
    .bind(appointment_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Write one occupancy row per 30-minute slot in `[start_mins, end_mins)`
/// inside the given transaction. The `appointment_slots` UNIQUE index is the
/// authoritative double-booking guard.
pub(super) async fn insert_slots(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    appointment_id: i64,
    doctor_id: i64,
    date: &str,
    start_mins: i32,
    end_mins: i32,
    room_id: i64,
) -> Result<(), AppError> {
    let mut m = start_mins;
    while m < end_mins {
        sqlx::query(
            "INSERT INTO appointment_slots
             (appointment_id, doctor_id, appointment_date, slot_time, room_id)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(appointment_id)
        .bind(doctor_id)
        .bind(date)
        .bind(minutes_to_time(m))
        .bind(room_id)
        .execute(&mut **tx)
        .await
        .map_err(map_slot_conflict)?;
        m += SLOT_MINUTES;
    }
    Ok(())
}

/// Insert the appointment row plus its occupancy slots inside an existing
/// transaction. Used by both the standard and priority booking paths.
pub(super) async fn insert_appointment_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    patient_id: i64,
    doctor_id: i64,
    date: &str,
    start: &str,
    end: &str,
    priority: i32,
    room_id: i64,
    notes: &Option<String>,
    start_mins: i32,
    end_mins: i32,
) -> Result<Appointment, AppError> {
    let appt = sqlx::query_as::<_, Appointment>(
        "INSERT INTO appointments (patient_id, doctor_id, appointment_date,
         start_time, end_time, status, priority, room_id, notes)
         VALUES (?, ?, ?, ?, ?, 'scheduled', ?, ?, ?)
         RETURNING id, patient_id, doctor_id, appointment_date,
                   start_time, end_time, status, notes, created_at,
                   room_id, priority",
    )
    .bind(patient_id)
    .bind(doctor_id)
    .bind(date)
    .bind(start)
    .bind(end)
    .bind(priority)
    .bind(room_id)
    .bind(notes)
    .fetch_one(&mut **tx)
    .await?;

    insert_slots(tx, appt.id, doctor_id, date, start_mins, end_mins, room_id).await?;
    Ok(appt)
}

/// Book an appointment and its 30-minute occupancy slots atomically.
/// Opens its own transaction so the appointment row and every slot row commit
/// together; if any slot is already taken the whole booking rolls back.
pub(super) async fn insert_appointment(
    pool: &SqlitePool,
    patient_id: i64,
    doctor_id: i64,
    date: &str,
    start: &str,
    end: &str,
    priority: i32,
    room_id: i64,
    notes: &Option<String>,
) -> Result<Appointment, AppError> {
    let (start_mins, end_mins) = parse_slot(start, end)?;

    let mut tx = pool.begin().await?;
    let appt = insert_appointment_in_tx(
        &mut tx, patient_id, doctor_id, date, start, end,
        priority, room_id, notes, start_mins, end_mins,
    )
    .await?;
    tx.commit().await?;
    Ok(appt)
}
