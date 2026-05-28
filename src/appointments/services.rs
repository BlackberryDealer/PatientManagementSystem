use crate::appointments::models::{Appointment, AppointmentView, BookAppointmentForm};
use crate::errors::AppError;
use sqlx::SqlitePool;

// ============================================================
// Conflict Detection (Core Feature)
// ============================================================

/// Check whether a proposed appointment time-slot conflicts with
/// any existing (non-cancelled) appointment for the same doctor.
///
/// ## Overlap logic
/// Two time intervals [A_start, A_end) and [B_start, B_end) overlap if:
///   A_start < B_end AND A_end > B_start
///
/// Returns `true` if a conflict exists.
pub async fn check_conflict(
    pool: &SqlitePool,
    doctor_id: i64,
    appointment_date: &str,
    start_time: &str,
    end_time: &str,
    exclude_appointment_id: Option<i64>, // for reschedule/edit support
) -> Result<bool, AppError> {
    let count: (i64,) = match exclude_appointment_id {
        Some(exclude_id) => {
            sqlx::query_as(
                "SELECT COUNT(*) FROM appointments
                 WHERE doctor_id = ?
                   AND appointment_date = ?
                   AND status != 'cancelled'
                   AND id != ?
                   AND start_time < ? AND end_time > ?",
            )
            .bind(doctor_id)
            .bind(appointment_date)
            .bind(exclude_id)
            .bind(end_time)
            .bind(start_time)
            .fetch_one(pool)
            .await?
        }
        None => {
            sqlx::query_as(
                "SELECT COUNT(*) FROM appointments
                 WHERE doctor_id = ?
                   AND appointment_date = ?
                   AND status != 'cancelled'
                   AND start_time < ? AND end_time > ?",
            )
            .bind(doctor_id)
            .bind(appointment_date)
            .bind(end_time)
            .bind(start_time)
            .fetch_one(pool)
            .await?
        }
    };

    Ok(count.0 > 0)
}

// ============================================================
// Booking
// ============================================================

/// Book a new appointment after verifying:
/// 1. The time slot is valid (start < end)
/// 2. No scheduling conflicts exist for the doctor
/// 3. The patient has a profile
///
/// ## Future enhancements (see code comments):
/// - Check against doctor_availability recurring/weekly slots
/// - Enforce a minimum notice period (e.g., 24h in advance)
/// - Implement a waiting-queue if slot is already taken
pub async fn book_appointment(
    pool: &SqlitePool,
    patient_user_id: i64,
    form: &BookAppointmentForm,
) -> Result<Appointment, AppError> {
    // --- Validate time ordering ---
    if form.start_time >= form.end_time {
        return Err(AppError::BadRequest(
            "Start time must be before end time".into(),
        ));
    }

    // --- Look up patient internal ID from user_id ---
    let patient = sqlx::query_as::<_, (i64,)>("SELECT id FROM patients WHERE user_id = ?")
        .bind(patient_user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::BadRequest("Patient profile not found".into()))?;

    let patient_id = patient.0;

    // --- Conflict check (core feature) ---
    // TODO: Also validate against doctor_availability here
    let has_conflict = check_conflict(
        pool,
        form.doctor_id,
        &form.appointment_date,
        &form.start_time,
        &form.end_time,
        None,
    )
    .await?;

    if has_conflict {
        return Err(AppError::BadRequest(
            "The requested time slot conflicts with an existing appointment. \
             Please choose a different time."
                .into(),
        ));
    }

    // --- Insert the appointment ---
    let appointment = sqlx::query_as::<_, Appointment>(
        "INSERT INTO appointments (patient_id, doctor_id, appointment_date, start_time, end_time, status, notes)
         VALUES (?, ?, ?, ?, ?, 'scheduled', ?)
         RETURNING id, patient_id, doctor_id, appointment_date, start_time, end_time, status, notes, created_at",
    )
    .bind(patient_id)
    .bind(form.doctor_id)
    .bind(&form.appointment_date)
    .bind(&form.start_time)
    .bind(&form.end_time)
    .bind(&form.notes)
    .fetch_one(pool)
    .await?;

    Ok(appointment)
}

// ============================================================
// Queries
// ============================================================

/// Get all appointments for a patient (by user ID).
pub async fn get_appointments_for_patient(
    pool: &SqlitePool,
    patient_user_id: i64,
) -> Result<Vec<AppointmentView>, AppError> {
    let rows = sqlx::query_as::<_, (i64, String, String, chrono::NaiveDate, String, String, String, Option<String>)>(
        "SELECT a.id, u_p.full_name AS patient_name, u_d.full_name AS doctor_name,
                a.appointment_date, a.start_time, a.end_time, a.status, a.notes
         FROM appointments a
         JOIN patients p ON a.patient_id = p.id
         JOIN users u_p ON p.user_id = u_p.id
         JOIN doctors d ON a.doctor_id = d.id
         JOIN users u_d ON d.user_id = u_d.id
         WHERE p.user_id = ?
         ORDER BY a.appointment_date DESC, a.start_time",
    )
    .bind(patient_user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, patient_name, doctor_name, appointment_date, start_time, end_time, status, notes)| {
            AppointmentView {
                id,
                patient_name,
                doctor_name,
                appointment_date,
                start_time,
                end_time,
                status,
                notes,
            }
        })
        .collect())
}

/// Get all appointments for a doctor (by user ID).
pub async fn get_appointments_for_doctor(
    pool: &SqlitePool,
    doctor_user_id: i64,
) -> Result<Vec<AppointmentView>, AppError> {
    let rows = sqlx::query_as::<_, (i64, String, String, chrono::NaiveDate, String, String, String, Option<String>)>(
        "SELECT a.id, u_p.full_name AS patient_name, u_d.full_name AS doctor_name,
                a.appointment_date, a.start_time, a.end_time, a.status, a.notes
         FROM appointments a
         JOIN patients p ON a.patient_id = p.id
         JOIN users u_p ON p.user_id = u_p.id
         JOIN doctors d ON a.doctor_id = d.id
         JOIN users u_d ON d.user_id = u_d.id
         WHERE u_d.id = ?
         ORDER BY a.appointment_date DESC, a.start_time",
    )
    .bind(doctor_user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, patient_name, doctor_name, appointment_date, start_time, end_time, status, notes)| {
            AppointmentView {
                id,
                patient_name,
                doctor_name,
                appointment_date,
                start_time,
                end_time,
                status,
                notes,
            }
        })
        .collect())
}

/// Get all appointments (admin view).
pub async fn get_all_appointments(
    pool: &SqlitePool,
) -> Result<Vec<AppointmentView>, AppError> {
    let rows = sqlx::query_as::<_, (i64, String, String, chrono::NaiveDate, String, String, String, Option<String>)>(
        "SELECT a.id, u_p.full_name AS patient_name, u_d.full_name AS doctor_name,
                a.appointment_date, a.start_time, a.end_time, a.status, a.notes
         FROM appointments a
         JOIN patients p ON a.patient_id = p.id
         JOIN users u_p ON p.user_id = u_p.id
         JOIN doctors d ON a.doctor_id = d.id
         JOIN users u_d ON d.user_id = u_d.id
         ORDER BY a.appointment_date DESC, a.start_time",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, patient_name, doctor_name, appointment_date, start_time, end_time, status, notes)| {
            AppointmentView {
                id,
                patient_name,
                doctor_name,
                appointment_date,
                start_time,
                end_time,
                status,
                notes,
            }
        })
        .collect())
}

/// Get a single appointment by ID with joined details.
pub async fn get_appointment_by_id(
    pool: &SqlitePool,
    appointment_id: i64,
) -> Result<AppointmentView, AppError> {
    let row = sqlx::query_as::<_, (i64, String, String, chrono::NaiveDate, String, String, String, Option<String>)>(
        "SELECT a.id, u_p.full_name AS patient_name, u_d.full_name AS doctor_name,
                a.appointment_date, a.start_time, a.end_time, a.status, a.notes
         FROM appointments a
         JOIN patients p ON a.patient_id = p.id
         JOIN users u_p ON p.user_id = u_p.id
         JOIN doctors d ON a.doctor_id = d.id
         JOIN users u_d ON d.user_id = u_d.id
         WHERE a.id = ?",
    )
    .bind(appointment_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Appointment not found".into()))?;

    Ok(AppointmentView {
        id: row.0,
        patient_name: row.1,
        doctor_name: row.2,
        appointment_date: row.3,
        start_time: row.4,
        end_time: row.5,
        status: row.6,
        notes: row.7,
    })
}

/// Cancel an appointment (set status to 'cancelled').
pub async fn cancel_appointment(
    pool: &SqlitePool,
    appointment_id: i64,
) -> Result<(), AppError> {
    let rows_affected = sqlx::query(
        "UPDATE appointments SET status = 'cancelled' WHERE id = ? AND status = 'scheduled'",
    )
    .bind(appointment_id)
    .execute(pool)
    .await?
    .rows_affected();

    if rows_affected == 0 {
        return Err(AppError::NotFound(
            "Appointment not found or already cancelled/completed".into(),
        ));
    }

    Ok(())
}

/// Get all doctors (for the booking form dropdown).
pub async fn get_all_doctors(pool: &SqlitePool) -> Result<Vec<(i64, String)>, AppError> {
    let rows =
        sqlx::query_as::<_, (i64, String)>(
            "SELECT d.id, u.full_name FROM doctors d JOIN users u ON d.user_id = u.id ORDER BY u.full_name",
        )
        .fetch_all(pool)
        .await?;

    Ok(rows)
}
