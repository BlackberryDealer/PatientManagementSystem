//! Algorithm 1: Time Interval Overlap Detection.

use crate::errors::AppError;
use sqlx::SqlitePool;

/// Check whether a proposed time-slot conflicts with any existing
/// (non-cancelled) appointment, on either resource involved.
///
/// A conflict exists when an overlapping appointment shares *either* the
/// doctor *or* the room: a doctor cannot be in two places at once, and a
/// room cannot hold two appointments at once. In routine operation a doctor
/// is auto-assigned a single room per day, so both move together and the two
/// clauses coincide; the OR only diverges when a caller checks a doctor
/// against another doctor's room (reassignment) or vice versa.
///
/// ## Overlap logic
/// Two intervals [A_start, A_end) and [B_start, B_end) overlap iff:
///   A_start < B_end AND A_end > B_start
///
/// Returns `true` if a conflict exists.
pub async fn check_conflict(
    pool: &SqlitePool,
    doctor_id: i64,
    appointment_date: &str,
    start_time: &str,
    end_time: &str,
    room_id: i64,
    exclude_appointment_id: Option<i64>,
) -> Result<bool, AppError> {
    let mut sql = String::from(
        "SELECT COUNT(*) FROM appointments
         WHERE (doctor_id = ? OR room_id = ?)
           AND appointment_date = ?
           AND status != 'cancelled'
           AND start_time < ? AND end_time > ?",
    );

    if exclude_appointment_id.is_some() {
        sql.push_str(" AND id != ?");
    }

    let mut query = sqlx::query_as::<_, (i64,)>(&sql)
        .bind(doctor_id)
        .bind(room_id)
        .bind(appointment_date)
        .bind(end_time)   // A_start < B_end → start_time < end_time (new end)
        .bind(start_time); // A_end > B_start → end_time > start_time (new start)

    if let Some(eid) = exclude_appointment_id {
        query = query.bind(eid);
    }

    let count = query.fetch_one(pool).await?;
    Ok(count.0 > 0)
}
