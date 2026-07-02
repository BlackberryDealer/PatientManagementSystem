use crate::appointments::models::{Appointment, WaitlistEntry, WaitlistForm};
use crate::availability::services::ensure_doctor_available;
use crate::db;
use crate::errors::AppError;
use crate::traits::{Prioritized, StatusManaged, TimeSlotted};
use sqlx::SqlitePool;

use super::algorithms::{build_priority_queue, check_conflict};
use super::helpers::{insert_appointment, NewAppointment};
use super::rooms::resolve_room;

/// Add a patient to the waitlist.
pub async fn add_to_waitlist(
    pool: &SqlitePool,
    patient_user_id: i64,
    form: &WaitlistForm,
) -> Result<WaitlistEntry, AppError> {
    form.validate()?;
    // Store the canonical zero-padded "HH:MM" form, not the raw input —
    // promotion later compares and re-books these strings verbatim.
    let (start_mins, end_mins) = crate::time::parse_slot(&form.requested_start, &form.requested_end)?;

    let patient_id = db::get_patient_id(pool, patient_user_id).await?;
    let room_id = resolve_room(pool, form.doctor_id, &form.appointment_date).await?;

    Ok(sqlx::query_as::<_, WaitlistEntry>(
        "INSERT INTO waitlist (patient_id, doctor_id, room_id, appointment_date,
         requested_start, requested_end, priority, notes, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'waiting')
         RETURNING id, patient_id, doctor_id, room_id, appointment_date,
                   requested_start, requested_end, priority, notes, status, created_at",
    )
    .bind(patient_id)
    .bind(form.doctor_id)
    .bind(room_id)
    .bind(&form.appointment_date)
    .bind(crate::time::minutes_to_time(start_mins))
    .bind(crate::time::minutes_to_time(end_mins))
    .bind(form.priority)
    .bind(&form.notes)
    .fetch_one(pool)
    .await?)
}

/// Get the pending waitlist for a doctor (by user_id).
pub async fn get_waitlist_for_doctor(
    pool: &SqlitePool,
    doctor_user_id: i64,
) -> Result<Vec<WaitlistEntry>, AppError> {
    let doctor_id = crate::db::get_doctor_id(pool, doctor_user_id).await?;
    Ok(sqlx::query_as::<_, WaitlistEntry>(
        "SELECT w.*, COALESCE(pu.full_name, 'Patient #' || w.patient_id) AS patient_name,
                COALESCE(du.full_name, 'Doctor #' || w.doctor_id) AS doctor_name
         FROM waitlist w
         JOIN patients p ON w.patient_id = p.id
         JOIN users pu ON p.user_id = pu.id
         JOIN doctors d ON w.doctor_id = d.id
         JOIN users du ON d.user_id = du.id
         WHERE w.doctor_id = ? AND w.status = 'waiting'
         ORDER BY w.priority ASC, w.created_at ASC",
    )
    .bind(doctor_id)
    .fetch_all(pool)
    .await?)
}

/// Get all waitlist entries for a specific patient (by user_id).
pub async fn get_waitlist_for_patient(
    pool: &SqlitePool,
    patient_user_id: i64,
) -> Result<Vec<WaitlistEntry>, AppError> {
    Ok(sqlx::query_as::<_, WaitlistEntry>(
        "SELECT w.*, COALESCE(pu.full_name, 'Patient #' || w.patient_id) AS patient_name,
                COALESCE(du.full_name, 'Doctor #' || w.doctor_id) AS doctor_name
         FROM waitlist w
         JOIN patients p ON w.patient_id = p.id
         JOIN users pu ON p.user_id = pu.id
         JOIN doctors d ON w.doctor_id = d.id
         JOIN users du ON d.user_id = du.id
         WHERE p.user_id = ? AND w.status = 'waiting'
         ORDER BY w.priority ASC, w.created_at ASC",
    )
    .bind(patient_user_id)
    .fetch_all(pool)
    .await?)
}

/// Get all pending waitlist entries (admin view).
pub async fn get_all_waitlist(pool: &SqlitePool) -> Result<Vec<WaitlistEntry>, AppError> {
    Ok(sqlx::query_as::<_, WaitlistEntry>(
        "SELECT w.*, COALESCE(pu.full_name, 'Patient #' || w.patient_id) AS patient_name,
                COALESCE(du.full_name, 'Doctor #' || w.doctor_id) AS doctor_name
         FROM waitlist w
         JOIN patients p ON w.patient_id = p.id
         JOIN users pu ON p.user_id = pu.id
         JOIN doctors d ON w.doctor_id = d.id
         JOIN users du ON d.user_id = du.id
         WHERE w.status = 'waiting'
         ORDER BY w.priority ASC, w.created_at ASC",
    )
    .fetch_all(pool)
    .await?)
}

/// Automatically promote the highest-priority waitlist entry for a freed slot.
/// Called after an appointment is cancelled to fill the gap immediately.
///
/// Uses Algorithm 3 (BinaryHeap priority queue) to select the most urgent
/// waiting patient, then promotes them if the slot is now conflict-free.
pub async fn auto_promote_waitlist(
    pool: &SqlitePool,
    doctor_id: i64,
    appointment_date: &str,
) -> Result<Option<Appointment>, AppError> {
    let mut heap = build_priority_queue(pool, doctor_id, appointment_date).await?;

    while let Some(item) = heap.pop() {
        let entry = sqlx::query_as::<_, WaitlistEntry>(
            "SELECT * FROM waitlist WHERE id = ? AND status = 'waiting'",
        )
        .bind(item.waitlist_id)
        .fetch_optional(pool)
        .await?;

        if let Some(mut entry) = entry {
            let available = ensure_doctor_available(
                pool, entry.doctor_id, appointment_date,
                entry.start_time(), entry.end_time(),
            ).await.is_ok();

            let room_id = match entry.room_id {
                Some(rid) => rid,
                None => resolve_room(pool, entry.doctor_id, appointment_date).await?,
            };

            let conflict = check_conflict(
                pool, entry.doctor_id,
                appointment_date,
                entry.start_time(),
                entry.end_time(),
                room_id, None,
            ).await?;

            if available && !conflict {
                let appt = insert_appointment(pool, &NewAppointment {
                    patient_id: entry.patient_id,
                    doctor_id: entry.doctor_id,
                    date: appointment_date,
                    start: entry.start_time(),
                    end: entry.end_time(),
                    priority: entry.priority_level(),
                    room_id,
                    notes: &entry.notes,
                }).await?;

                entry.accept()?;
                sqlx::query("UPDATE waitlist SET status = ? WHERE id = ?")
                    .bind(entry.current_status())
                    .bind(entry.id)
                    .execute(pool)
                    .await?;

                return Ok(Some(appt));
            }
        }
    }

    Ok(None)
}

/// Promote a waitlist entry: if its slot is now free, book it.
pub async fn promote_from_waitlist(
    pool: &SqlitePool,
    waitlist_id: i64,
) -> Result<Option<Appointment>, AppError> {
    let mut entry = sqlx::query_as::<_, WaitlistEntry>(
        "SELECT * FROM waitlist WHERE id = ? AND status = 'waiting'"
    )
    .bind(waitlist_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Waitlist entry not found".into()))?;

    let date_str = entry.appointment_date.format("%Y-%m-%d").to_string();
    let available = ensure_doctor_available(
        pool, entry.doctor_id, &date_str,
        &entry.requested_start, &entry.requested_end,
    ).await.is_ok();

    let room_id = match entry.room_id {
        Some(rid) => rid,
        None => resolve_room(pool, entry.doctor_id, &date_str).await?,
    };

    let conflict = check_conflict(
        pool, entry.doctor_id,
        &date_str,
        &entry.requested_start, &entry.requested_end,
        room_id, None,
    ).await?;

    if conflict || !available {
        return Ok(None);
    }

    let appt = insert_appointment(pool, &NewAppointment {
        patient_id: entry.patient_id,
        doctor_id: entry.doctor_id,
        date: &date_str,
        start: &entry.requested_start,
        end: &entry.requested_end,
        priority: entry.priority_level(),
        room_id,
        notes: &entry.notes,
    }).await?;

    entry.accept()?;
    sqlx::query("UPDATE waitlist SET status = ? WHERE id = ?")
        .bind(entry.current_status())
        .bind(entry.id)
        .execute(pool)
        .await?;

    Ok(Some(appt))
}
