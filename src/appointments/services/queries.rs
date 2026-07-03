use crate::appointments::models::{Appointment, AppointmentView, AssignRoomForm, RescheduleForm, SetPriorityForm};
use crate::auth::Role;
use crate::availability::services::ensure_doctor_available;
use crate::db;
use crate::errors::AppError;
use crate::time::{minutes_to_time, parse_slot};
use crate::traits::StatusManaged;
use sqlx::SqlitePool;

use super::algorithms::check_conflict;
use super::helpers::insert_slots;
use super::rooms::resolve_room;
use super::waitlist::auto_promote_waitlist;

/// Shared SELECT for the joined appointment view. Each query appends its own
/// `WHERE` / `ORDER BY`. Columns are aliased to match `AppointmentView` field
/// names so rows deserialize directly via `FromRow`.
const APPOINTMENT_VIEW_SELECT: &str = "\
    SELECT a.id, u_p.full_name AS patient_name, u_d.full_name AS doctor_name,
           a.appointment_date, a.start_time, a.end_time, a.status, a.notes,
           r.name AS room_name, a.priority,
           a.patient_id, a.doctor_id
    FROM appointments a
    JOIN patients p ON a.patient_id = p.id
    JOIN users u_p ON p.user_id = u_p.id
    JOIN doctors d ON a.doctor_id = d.id
    JOIN users u_d ON d.user_id = u_d.id
    LEFT JOIN rooms r ON a.room_id = r.id";

// ============================================================
// Read queries
// ============================================================

/// Fetch all appointments for a patient by their users table user_id.
/// Returns `AppointmentView` rows with patient/doctor names and room name.
pub async fn get_appointments_for_patient(
    pool: &SqlitePool,
    patient_user_id: i64,
) -> Result<Vec<AppointmentView>, AppError> {
    Ok(sqlx::query_as::<_, AppointmentView>(&format!(
        "{APPOINTMENT_VIEW_SELECT} WHERE p.user_id = ? \
         ORDER BY a.appointment_date DESC, a.start_time"
    ))
    .bind(patient_user_id)
    .fetch_all(pool)
    .await?)
}

/// Fetch all appointments for a doctor by their users table user_id.
pub async fn get_appointments_for_doctor(
    pool: &SqlitePool,
    doctor_user_id: i64,
) -> Result<Vec<AppointmentView>, AppError> {
    Ok(sqlx::query_as::<_, AppointmentView>(&format!(
        "{APPOINTMENT_VIEW_SELECT} WHERE u_d.id = ? \
         ORDER BY a.appointment_date DESC, a.start_time"
    ))
    .bind(doctor_user_id)
    .fetch_all(pool)
    .await?)
}

/// Fetch every appointment in the system (admin view).
pub async fn get_all_appointments(pool: &SqlitePool) -> Result<Vec<AppointmentView>, AppError> {
    Ok(sqlx::query_as::<_, AppointmentView>(&format!(
        "{APPOINTMENT_VIEW_SELECT} ORDER BY a.appointment_date DESC, a.start_time"
    ))
    .fetch_all(pool)
    .await?)
}

/// Fetch a single appointment by its primary key.
/// Returns `NotFound` if no row matches.
pub async fn get_appointment_by_id(
    pool: &SqlitePool,
    appointment_id: i64,
) -> Result<AppointmentView, AppError> {
    sqlx::query_as::<_, AppointmentView>(&format!(
        "{APPOINTMENT_VIEW_SELECT} WHERE a.id = ?"
    ))
    .bind(appointment_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Appointment not found".into()))
}

/// Get a single appointment, enforcing that a patient may only view their own.
pub async fn get_appointment_by_id_checked(
    pool: &SqlitePool,
    appointment_id: i64,
    user_id: i64,
    role: Role,
) -> Result<AppointmentView, AppError> {
    let appointment = get_appointment_by_id(pool, appointment_id).await?;
    if role == Role::Patient {
        let patient_id = db::get_patient_id(pool, user_id).await?;
        let owns: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM appointments WHERE id = ? AND patient_id = ?",
        )
        .bind(appointment_id)
        .bind(patient_id)
        .fetch_one(pool)
        .await?;
        if owns.0 == 0 {
            return Err(AppError::Forbidden(
                "You can only view your own appointments.".into(),
            ));
        }
    }
    Ok(appointment)
}

/// Daily appointment counts for a patient (used by the calendar).
pub async fn get_appointment_counts_for_patient(
    pool: &SqlitePool,
    patient_user_id: i64,
    from: &str,
    to: &str,
) -> Result<std::collections::HashMap<String, usize>, AppError> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT a.appointment_date, COUNT(*) FROM appointments a
         JOIN patients p ON a.patient_id = p.id
         WHERE p.user_id = ? AND a.appointment_date >= ? AND a.appointment_date <= ?
           AND a.status != 'cancelled'
         GROUP BY a.appointment_date",
    )
    .bind(patient_user_id).bind(from).bind(to)
    .fetch_all(pool).await?;
    Ok(rows.into_iter().map(|(d, c)| (d, c as usize)).collect())
}

/// Daily appointment counts for a doctor (used by the calendar).
pub async fn get_appointment_counts_for_doctor(
    pool: &SqlitePool,
    doctor_user_id: i64,
    from: &str,
    to: &str,
) -> Result<std::collections::HashMap<String, usize>, AppError> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT a.appointment_date, COUNT(*) FROM appointments a
         JOIN doctors d ON a.doctor_id = d.id
         WHERE d.user_id = ? AND a.appointment_date >= ? AND a.appointment_date <= ?
           AND a.status != 'cancelled'
         GROUP BY a.appointment_date",
    )
    .bind(doctor_user_id).bind(from).bind(to)
    .fetch_all(pool).await?;
    Ok(rows.into_iter().map(|(d, c)| (d, c as usize)).collect())
}

/// Daily appointment counts system-wide (admin calendar).
pub async fn get_all_appointment_counts(
    pool: &SqlitePool,
    from: &str,
    to: &str,
) -> Result<std::collections::HashMap<String, usize>, AppError> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT appointment_date, COUNT(*) FROM appointments
         WHERE appointment_date >= ? AND appointment_date <= ?
           AND status != 'cancelled'
         GROUP BY appointment_date",
    )
    .bind(from).bind(to)
    .fetch_all(pool).await?;
    Ok(rows.into_iter().map(|(d, c)| (d, c as usize)).collect())
}

// ============================================================
// Mutations: cancel, reschedule, assign room
// ============================================================

/// Cancel an appointment. After cancellation, automatically attempts to
/// promote the highest-priority waitlist entry into the freed slot.
pub async fn cancel_appointment(pool: &SqlitePool, appointment_id: i64) -> Result<(), AppError> {
    let mut appt = sqlx::query_as::<_, Appointment>(
        "SELECT id, patient_id, doctor_id, appointment_date, start_time, end_time,
                status, notes, created_at, room_id, priority
         FROM appointments WHERE id = ?",
    )
    .bind(appointment_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Appointment not found".into()))?;

    appt.cancel()?;

    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE appointments SET status = ? WHERE id = ?")
        .bind(appt.current_status())
        .bind(appt.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM appointment_slots WHERE appointment_id = ?")
        .bind(appt.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    let date_str = appt.appointment_date.format("%Y-%m-%d").to_string();
    let _ = auto_promote_waitlist(pool, appt.doctor_id(), &date_str).await;

    Ok(())
}

/// Cancel an appointment, enforcing ownership for patients.
pub async fn cancel_appointment_checked(
    pool: &SqlitePool,
    appointment_id: i64,
    user_id: i64,
    role: Role,
) -> Result<(), AppError> {
    if role == Role::Patient {
        let patient_id = crate::db::get_patient_id(pool, user_id).await?;
        let owns: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM appointments WHERE id = ? AND patient_id = ?",
        )
        .bind(appointment_id)
        .bind(patient_id)
        .fetch_one(pool)
        .await?;

        if owns.0 == 0 {
            return Err(AppError::Forbidden(
                "You can only cancel your own appointments.".into(),
            ));
        }
    }
    cancel_appointment(pool, appointment_id).await
}

/// Mark an appointment as completed (staff action after the visit).
///
/// The occupancy slots are deliberately kept: the time was used, and both
/// `check_conflict` and the slot ledger treat completed visits as occupying
/// their window — freeing the rows would let the two disagree.
pub async fn complete_appointment(
    pool: &SqlitePool,
    appointment_id: i64,
) -> Result<Appointment, AppError> {
    let mut appt = sqlx::query_as::<_, Appointment>(
        "SELECT id, patient_id, doctor_id, appointment_date, start_time, end_time,
                status, notes, created_at, room_id, priority
         FROM appointments WHERE id = ?",
    )
    .bind(appointment_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Appointment not found".into()))?;

    appt.complete()?;

    sqlx::query("UPDATE appointments SET status = ? WHERE id = ?")
        .bind(appt.current_status())
        .bind(appt.id)
        .execute(pool)
        .await?;

    Ok(appt)
}

/// Reschedule a scheduled appointment to a new date/time, keeping its doctor and room.
pub async fn reschedule_appointment(
    pool: &SqlitePool,
    appointment_id: i64,
    form: &RescheduleForm,
) -> Result<Appointment, AppError> {
    let new_date = form.validate()?;
    let date_str = &form.appointment_date;

    // Canonical zero-padded "HH:MM" strings. The form's dropdowns always send
    // padded values, but a hand-crafted "9:00" parses fine while breaking the
    // lexical time comparisons below — so every comparison and write uses the
    // re-rendered canonical form instead of the raw input.
    let (start_mins, end_mins) = parse_slot(&form.start_time, &form.end_time)?;
    let (start, end) = (minutes_to_time(start_mins), minutes_to_time(end_mins));

    let mut appt = sqlx::query_as::<_, Appointment>(
        "SELECT id, patient_id, doctor_id, appointment_date, start_time, end_time,
                status, notes, created_at, room_id, priority
         FROM appointments WHERE id = ?",
    )
    .bind(appointment_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Appointment not found".into()))?;

    ensure_doctor_available(pool, appt.doctor_id(), date_str, &start, &end).await?;

    // The appointment keeps its room on reschedule; a legacy row without one
    // gets the doctor's daily room for the new date (same as reassignment).
    let room_id = match appt.room_id {
        Some(rid) => rid,
        None => resolve_room(pool, appt.doctor_id(), date_str).await?,
    };
    let conflict = check_conflict(
        pool, appt.doctor_id(), date_str,
        &start, &end, room_id, Some(appointment_id),
    ).await?;
    if conflict {
        return Err(AppError::BadRequest(
            "The new time slot conflicts with an existing appointment.\
             \nPlease choose a different time.".into(),
        ));
    }

    appt.reschedule_to(new_date, &start, &end)?;

    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE appointments SET appointment_date = ?, start_time = ?, end_time = ? WHERE id = ?",
    )
    .bind(appt.appointment_date)
    .bind(&appt.start_time)
    .bind(&appt.end_time)
    .bind(appt.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM appointment_slots WHERE appointment_id = ?")
        .bind(appt.id)
        .execute(&mut *tx)
        .await?;
    insert_slots(&mut tx, appt.id, appt.doctor_id(), date_str, start_mins, end_mins, room_id)
        .await?;
    tx.commit().await?;

    Ok(appt)
}

/// Reschedule with patient ownership enforcement.
pub async fn reschedule_appointment_checked(
    pool: &SqlitePool,
    appointment_id: i64,
    user_id: i64,
    role: Role,
    form: &RescheduleForm,
) -> Result<Appointment, AppError> {
    if role == Role::Patient {
        let patient_id = crate::db::get_patient_id(pool, user_id).await?;
        let owns: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM appointments WHERE id = ? AND patient_id = ?",
        )
        .bind(appointment_id)
        .bind(patient_id)
        .fetch_one(pool)
        .await?;
        if owns.0 == 0 {
            return Err(AppError::Forbidden(
                "You can only reschedule your own appointments.".into(),
            ));
        }
    }
    reschedule_appointment(pool, appointment_id, form).await
}

/// Assign (or change) the consultation room for an appointment.
pub async fn assign_room(
    pool: &SqlitePool,
    appointment_id: i64,
    form: &AssignRoomForm,
) -> Result<Appointment, AppError> {
    let mut appt = sqlx::query_as::<_, Appointment>(
        "SELECT id, patient_id, doctor_id, appointment_date, start_time, end_time,
                status, notes, created_at, room_id, priority
         FROM appointments WHERE id = ?",
    )
    .bind(appointment_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Appointment not found".into()))?;

    let room_active: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM rooms WHERE id = ? AND is_active = 1")
            .bind(form.room_id)
            .fetch_one(pool)
            .await?;
    if room_active.0 == 0 {
        return Err(AppError::BadRequest(
            "That room does not exist or is no longer active.".into(),
        ));
    }

    appt.assign_room(form.room_id)?;

    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE appointments SET room_id = ? WHERE id = ?")
        .bind(form.room_id)
        .bind(appt.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE appointment_slots SET room_id = ? WHERE appointment_id = ?")
        .bind(form.room_id)
        .bind(appt.id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db) = &e {
                if db.is_unique_violation() {
                    return AppError::BadRequest(
                        "That room is already booked for this time slot.\
                         \nPlease choose a different room.".into(),
                    );
                }
            }
            AppError::DatabaseError(e)
        })?;
    tx.commit().await?;

    Ok(appt)
}

/// Re-triage an appointment's priority (staff override).
/// The domain method enforces the rules (scheduled only, 1–4 range);
/// slots carry no priority, so a single UPDATE is enough.
pub async fn set_priority(
    pool: &SqlitePool,
    appointment_id: i64,
    form: &SetPriorityForm,
) -> Result<Appointment, AppError> {
    let mut appt = sqlx::query_as::<_, Appointment>(
        "SELECT id, patient_id, doctor_id, appointment_date, start_time, end_time,
                status, notes, created_at, room_id, priority
         FROM appointments WHERE id = ?",
    )
    .bind(appointment_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Appointment not found".into()))?;

    appt.set_priority(form.priority)?;

    sqlx::query("UPDATE appointments SET priority = ? WHERE id = ?")
        .bind(form.priority)
        .bind(appt.id)
        .execute(pool)
        .await?;

    Ok(appt)
}
