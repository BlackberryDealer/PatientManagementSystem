use crate::appointments::models::{Appointment, BookAppointmentForm};
use crate::availability::services::ensure_doctor_available;
use crate::db;
use crate::errors::AppError;
use crate::time::{minutes_to_time, parse_slot};
use crate::traits::{Priority, StatusManaged};
use sqlx::SqlitePool;

use super::algorithms::check_conflict;
use super::helpers::{bump_to_waitlist, insert_appointment, insert_appointment_in_tx, NewAppointment};
use super::rooms::resolve_room;

// ============================================================
// Simple booking (no priority bumping)
// ============================================================

/// Book an appointment after checking conflicts.
/// Room is auto-assigned from the doctor's daily room allocation.
pub async fn book_appointment(
    pool: &SqlitePool,
    patient_user_id: i64,
    form: &BookAppointmentForm,
) -> Result<Appointment, AppError> {
    form.validate()?;
    // Canonical zero-padded "HH:MM": the UI's dropdowns always send padded
    // values, but a hand-crafted "9:00" parses fine while breaking the lexical
    // time comparisons in the conflict/availability checks — so everything
    // downstream uses the re-rendered canonical form, never the raw input.
    let (start_mins, end_mins) = parse_slot(&form.start_time, &form.end_time)?;
    let (start, end) = (minutes_to_time(start_mins), minutes_to_time(end_mins));

    ensure_doctor_available(pool, form.doctor_id, &form.appointment_date, &start, &end).await?;

    let patient_id = db::get_patient_id(pool, patient_user_id).await?;
    let priority = form.requested_priority() as i32;
    let room_id = resolve_room(pool, form.doctor_id, &form.appointment_date).await?;

    let has_conflict = check_conflict(
        pool, form.doctor_id, &form.appointment_date,
        &start, &end, room_id, None,
    ).await?;

    if has_conflict {
        return Err(AppError::BadRequest(
            "The requested time slot conflicts with an existing appointment.\
             \nPlease choose a different time, or use priority booking (Emergency/Urgent)."
                .into(),
        ));
    }

    insert_appointment(pool, &NewAppointment {
        patient_id,
        doctor_id: form.doctor_id,
        date: &form.appointment_date,
        start: &start,
        end: &end,
        priority,
        room_id,
        notes: &form.notes,
    }).await
}

// ============================================================
// Algorithm 3: Priority-Based Scheduling with BinaryHeap
// ============================================================

/// Priority-based booking: if the requested slot conflicts with a lower-priority
/// appointment, bump the lower-priority one to the waitlist and book the urgent one.
///
/// Only Emergency (1) and Urgent (2) can trigger priority override.
/// All operations run inside a database transaction for atomicity.
pub async fn book_with_priority(
    pool: &SqlitePool,
    patient_user_id: i64,
    form: &BookAppointmentForm,
) -> Result<Appointment, AppError> {
    form.validate()?;
    // Canonical zero-padded "HH:MM" — same normalisation as book_appointment.
    let (start_mins, end_mins) = parse_slot(&form.start_time, &form.end_time)?;
    let (start, end) = (minutes_to_time(start_mins), minutes_to_time(end_mins));
    ensure_doctor_available(pool, form.doctor_id, &form.appointment_date, &start, &end).await?;

    let new_priority = form.requested_priority();

    if !new_priority.can_override() {
        return Err(AppError::BadRequest(
            "Priority override is only available for Emergency or Urgent appointments.\
             \nUse standard booking for Normal or Follow-up visits."
                .into(),
        ));
    }

    let patient_id = db::get_patient_id(pool, patient_user_id).await?;
    let room_id = resolve_room(pool, form.doctor_id, &form.appointment_date).await?;

    let has_conflict = check_conflict(
        pool, form.doctor_id, &form.appointment_date,
        &start, &end, room_id, None,
    ).await?;

    if !has_conflict {
        return insert_appointment(pool, &NewAppointment {
            patient_id,
            doctor_id: form.doctor_id,
            date: &form.appointment_date,
            start: &start,
            end: &end,
            priority: new_priority as i32,
            room_id,
            notes: &form.notes,
        }).await;
    }

    let conflicts = sqlx::query_as::<_, (i64, i32, String, String, String)>(
        "SELECT id, priority, start_time, end_time, notes FROM appointments
         WHERE doctor_id = ? AND appointment_date = ? AND status = 'scheduled'
           AND start_time < ? AND end_time > ?",
    )
    .bind(form.doctor_id)
    .bind(&form.appointment_date)
    .bind(&end)
    .bind(&start)
    .fetch_all(pool)
    .await?;

    if conflicts.is_empty() {
        return Err(AppError::BadRequest(
            "This time slot is occupied by a completed appointment and cannot be overridden.\
             \nPlease choose a different time."
                .into(),
        ));
    }

    let can_bump = conflicts
        .iter()
        .all(|(_, pri, _, _, _)| new_priority.outranks(Priority::from_i32(*pri)));

    if !can_bump {
        return Err(AppError::BadRequest(
            "This time slot is occupied by an appointment with equal or higher priority.\
             \nUse the suggestion feature to find an available slot, or join the waitlist."
                .into(),
        ));
    }

    let mut tx = pool.begin().await?;

    for (conflict_id, _, _, _, c_notes) in &conflicts {
        bump_to_waitlist(&mut tx, *conflict_id, c_notes).await?;

        let mut bumped = sqlx::query_as::<_, Appointment>(
            "SELECT id, patient_id, doctor_id, appointment_date, start_time, end_time,
                    status, notes, created_at, room_id, priority
             FROM appointments WHERE id = ?",
        )
        .bind(conflict_id)
        .fetch_one(&mut *tx)
        .await?;
        bumped.cancel()?;
        sqlx::query("UPDATE appointments SET status = ? WHERE id = ?")
            .bind(bumped.current_status())
            .bind(bumped.id)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM appointment_slots WHERE appointment_id = ?")
            .bind(conflict_id)
            .execute(&mut *tx)
            .await?;
    }

    let appointment = insert_appointment_in_tx(
        &mut tx,
        &NewAppointment {
            patient_id,
            doctor_id: form.doctor_id,
            date: &form.appointment_date,
            start: &start,
            end: &end,
            priority: new_priority as i32,
            room_id,
            notes: &form.notes,
        },
        start_mins,
        end_mins,
    )
    .await?;

    tx.commit().await?;
    Ok(appointment)
}
