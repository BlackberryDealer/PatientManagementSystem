// Algorithm 4: Doctor Reassignment (greedy, load-balanced). Moves a single
// scheduled appointment to the best available colleague. The whole-day,
// optimal counterpart is Algorithm 5 in batch_reassign.

use crate::appointments::models::Appointment;
use crate::availability::services::ensure_doctor_available;
use crate::errors::AppError;
use crate::time::parse_slot;
use sqlx::SqlitePool;

use super::super::helpers::{insert_slots, load_appointment};
use super::super::rooms::resolve_room;
use super::check_conflict;

/// Find the best alternative doctor for a given slot. Candidates are every
/// other doctor, ranked by same specialization first (continuity of care),
/// then fewest scheduled appointments on that date (load balancing). The
/// first candidate that is both available and conflict-free is chosen.
///
/// Private to this module: the only caller is `reassign_appointment` below.
/// The batch equivalent (Algorithm 5) doesn't reuse it, it scores every
/// colleague at once in a cost matrix rather than picking the first feasible one.
async fn find_alternative_doctor(
    pool: &SqlitePool,
    exclude_doctor_id: i64,
    appointment_date: &str,
    start_time: &str,
    end_time: &str,
    room_id: i64,
    exclude_appointment_id: i64,
) -> Result<Option<(i64, String)>, AppError> {
    let candidates = sqlx::query_as::<_, (i64, String)>(
        "SELECT d.id, u.full_name
         FROM doctors d
         JOIN users u ON d.user_id = u.id
         LEFT JOIN appointments a
                ON a.doctor_id = d.id
               AND a.appointment_date = ?
               AND a.status = 'scheduled'
         WHERE d.id != ?
         GROUP BY d.id, u.full_name
         ORDER BY (d.specialization =
                     (SELECT specialization FROM doctors WHERE id = ?)) DESC,
                  COUNT(a.id) ASC,
                  d.id ASC",
    )
    .bind(appointment_date)
    .bind(exclude_doctor_id)
    .bind(exclude_doctor_id)
    .fetch_all(pool)
    .await?;

    for (candidate_id, candidate_name) in candidates {
        let available = ensure_doctor_available(
            pool, candidate_id, appointment_date, start_time, end_time,
        ).await.is_ok();
        if !available {
            continue;
        }
        // Exclude the appointment being moved: it still occupies its current
        // room here, and check_conflict now flags a shared room, so without this
        // it would count itself as a conflict against every candidate.
        let conflict = check_conflict(
            pool, candidate_id, appointment_date, start_time, end_time, room_id,
            Some(exclude_appointment_id),
        ).await?;
        if !conflict {
            return Ok(Some((candidate_id, candidate_name)));
        }
    }
    Ok(None)
}

/// Reassign a scheduled appointment to the best available alternative
/// doctor. The appointment row and its occupancy slots move atomically.
pub async fn reassign_appointment(
    pool: &SqlitePool,
    appointment_id: i64,
) -> Result<(Appointment, String), AppError> {
    let mut appt = load_appointment(pool, appointment_id).await?;

    let date_str = appt.appointment_date.format("%Y-%m-%d").to_string();

    let room_id = match appt.room_id {
        Some(rid) => rid,
        None => resolve_room(pool, appt.doctor_id(), &date_str).await?,
    };

    let (new_doctor_id, new_doctor_name) = find_alternative_doctor(
        pool, appt.doctor_id(), &date_str, &appt.start_time, &appt.end_time, room_id,
        appointment_id,
    )
    .await?
    .ok_or_else(|| {
        AppError::BadRequest(
            "No alternative doctor is available for this time slot.\
             \nTry a different time, or cancel and rebook.".into(),
        )
    })?;

    let (start_mins, end_mins) = parse_slot(&appt.start_time, &appt.end_time)?;

    appt.reassign_to(new_doctor_id)?;

    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE appointments SET doctor_id = ? WHERE id = ?")
        .bind(appt.doctor_id())
        .bind(appt.id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM appointment_slots WHERE appointment_id = ?")
        .bind(appt.id)
        .execute(&mut *tx)
        .await?;
    insert_slots(&mut tx, appt.id, appt.doctor_id(), &date_str, start_mins, end_mins, room_id)
        .await?;
    tx.commit().await?;

    Ok((appt, new_doctor_name))
}
