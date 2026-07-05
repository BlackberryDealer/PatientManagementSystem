use crate::appointments::models::Room;
use crate::errors::AppError;
use sqlx::SqlitePool;

/// Upper bound on claim retries. Each retry means another request claimed
/// our candidate room between the SELECT and the INSERT, with 6 seeded
/// rooms this can only happen a handful of times before every room is
/// taken and the sharing fallback applies anyway.
const MAX_ROOM_CLAIM_ATTEMPTS: usize = 8;

/// Room auto-assignment: each doctor gets one room per day.
/// Returns the existing assignment if one exists, otherwise claims
/// the first available active room for that doctor+date and returns it.
///
/// "At the start of each day, a doctor is allocated a room."
/// The first booking of the day triggers lazy auto-assignment.
///
/// Race-safe: the claim is a plain INSERT guarded by two UNIQUE indexes
/// (one room per doctor per day, one doctor per room per day, migrations
/// 005/006). Losing a race surfaces as a unique violation, and the loop
/// simply retries with the next free room instead of double-assigning.
pub(super) async fn resolve_room(
    pool: &SqlitePool,
    doctor_id: i64,
    appointment_date: &str,
) -> Result<i64, AppError> {
    for _ in 0..MAX_ROOM_CLAIM_ATTEMPTS {
        // 1. Already assigned for this date? (Also covers losing a race to a
        //    concurrent request for the SAME doctor, retry lands here.)
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

        // 2. Find an active room not yet assigned to anyone on this date.
        let free_room = sqlx::query_as::<_, (i64,)>(
            "SELECT r.id FROM rooms r
             WHERE r.is_active = 1
               AND r.id NOT IN (
                   SELECT room_id FROM doctor_room_assignments
                   WHERE assignment_date = ?
               )
             LIMIT 1",
        )
        .bind(appointment_date)
        .fetch_optional(pool)
        .await?;

        let Some((room_id,)) = free_room else {
            break; // every room is assigned today, fall through to sharing
        };

        // 3. Claim it. A unique violation on either index means a concurrent
        //    request won the race, loop and re-resolve.
        let claim = sqlx::query(
            "INSERT INTO doctor_room_assignments (doctor_id, room_id, assignment_date)
             VALUES (?, ?, ?)",
        )
        .bind(doctor_id)
        .bind(room_id)
        .bind(appointment_date)
        .execute(pool)
        .await;

        match claim {
            Ok(_) => return Ok(room_id),
            Err(sqlx::Error::Database(db)) if db.is_unique_violation() => continue,
            Err(e) => return Err(e.into()),
        }
    }

    // Fallback: more doctors than rooms today, so doctors share. The share is
    // deliberately NOT persisted (the room's daily assignment stays with its
    // first claimant); same-slot clashes inside the shared room are still
    // blocked by the appointment_slots room UNIQUE index.
    let (room_id,) = sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM rooms WHERE is_active = 1 ORDER BY id LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::Internal("No active rooms available in the system.".into()))?;
    Ok(room_id)
}

/// List of (doctor_id, full_name) pairs for dropdowns and display.
pub async fn get_all_doctors(pool: &SqlitePool) -> Result<Vec<(i64, String)>, AppError> {
    Ok(sqlx::query_as::<_, (i64, String)>(
        "SELECT d.id, u.full_name FROM doctors d JOIN users u ON d.user_id = u.id ORDER BY u.full_name",
    )
    .fetch_all(pool)
    .await?)
}

/// List of active rooms for display and auto-assignment.
pub async fn get_all_rooms(pool: &SqlitePool) -> Result<Vec<Room>, AppError> {
    Ok(sqlx::query_as::<_, Room>(
        "SELECT * FROM rooms WHERE is_active = 1 ORDER BY name",
    )
    .fetch_all(pool)
    .await?)
}
