use crate::appointments::models::Room;
use crate::errors::AppError;
use sqlx::SqlitePool;

/// Room auto-assignment: each doctor gets one room per day.
/// Returns the existing assignment if one exists, otherwise claims
/// the first available active room for that doctor+date and returns it.
///
/// "At the start of each day, a doctor is allocated a room."
/// The first booking of the day triggers lazy auto-assignment.
pub(super) async fn resolve_room(
    pool: &SqlitePool,
    doctor_id: i64,
    appointment_date: &str,
) -> Result<i64, AppError> {
    // 1. Already assigned for this date?
    if let Some((room_id,)) = sqlx::query_as::<_, (i64,)>(
        "SELECT room_id FROM doctor_room_assignments
         WHERE doctor_id = ? AND assignment_date = ?",
    )
    .bind(doctor_id)
    .bind(appointment_date)
    .fetch_optional(pool)
    .await?
    {
        return Ok(room_id);
    }

    // 2. Find an active room NOT already assigned to another doctor on this date
    let free_room = sqlx::query_as::<_, (i64, String)>(
        "SELECT r.id, r.name FROM rooms r
         WHERE r.is_active = 1
           AND r.id NOT IN (
               SELECT room_id FROM doctor_room_assignments
               WHERE assignment_date = ? AND doctor_id != ?
           )
         LIMIT 1",
    )
    .bind(appointment_date)
    .bind(doctor_id)
    .fetch_optional(pool)
    .await?;

    let room_id = match free_room {
        Some((id, _)) => id,
        None => {
            // 3. Fallback: any active room (doctors may share if needed;
            //    the appointment_slots UNIQUE index prevents double-booking)
            let (id,) = sqlx::query_as::<_, (i64,)>(
                "SELECT id FROM rooms WHERE is_active = 1 LIMIT 1",
            )
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::Internal(
                "No active rooms available in the system.".into(),
            ))?;
            id
        }
    };

    // 4. Persist the assignment so subsequent bookings reuse it
    sqlx::query(
        "INSERT OR IGNORE INTO doctor_room_assignments (doctor_id, room_id, assignment_date)
         VALUES (?, ?, ?)",
    )
    .bind(doctor_id)
    .bind(room_id)
    .bind(appointment_date)
    .execute(pool)
    .await?;

    Ok(room_id)
}

pub async fn get_all_doctors(pool: &SqlitePool) -> Result<Vec<(i64, String)>, AppError> {
    Ok(sqlx::query_as::<_, (i64, String)>(
        "SELECT d.id, u.full_name FROM doctors d JOIN users u ON d.user_id = u.id ORDER BY u.full_name",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn get_all_rooms(pool: &SqlitePool) -> Result<Vec<Room>, AppError> {
    Ok(sqlx::query_as::<_, Room>(
        "SELECT * FROM rooms WHERE is_active = 1 ORDER BY name",
    )
    .fetch_all(pool)
    .await?)
}
