use crate::appointments::models::Appointment;
use crate::errors::AppError;
use crate::time::{minutes_to_time, parse_slot, SLOT_MINUTES};
use crate::traits::{Priority, StatusManaged};
use sqlx::SqlitePool;

/// The full column list for loading an `Appointment` row. Every mutation
/// (cancel, complete, reschedule, reassign, batch-reassign, ...) re-reads the
/// row it is about to change, so the SELECT lives here once instead of being
/// copied into each service.
pub(super) async fn load_appointment(
    pool: &SqlitePool,
    appointment_id: i64,
) -> Result<Appointment, AppError> {
    sqlx::query_as::<_, Appointment>(
        "SELECT id, patient_id, doctor_id, appointment_date, start_time, end_time,
                status, notes, created_at, room_id, priority
         FROM appointments WHERE id = ?",
    )
    .bind(appointment_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Appointment not found".into()))
}

/// Persist an appointment's current status. Shared by every call site that
/// just drove `Appointment` through a guarded transition (`cancel()`,
/// `complete()`) and now needs the new value written. The SQL is identical
/// everywhere, only the executor (a pool or an already-open transaction)
/// differs, so this is generic over `sqlx::Executor`.
pub(super) async fn persist_status<'e, E>(executor: E, appt: &Appointment) -> Result<(), AppError>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query("UPDATE appointments SET status = ? WHERE id = ?")
        .bind(appt.current_status())
        .bind(appt.id)
        .execute(executor)
        .await?;
    Ok(())
}

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
) -> Result<i64, AppError> {
    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO waitlist (patient_id, doctor_id, room_id, appointment_date,
         requested_start, requested_end, priority, notes, status)
         SELECT patient_id, doctor_id, room_id, appointment_date,
                start_time, end_time, priority, ?, 'waiting'
         FROM appointments WHERE id = ?
         RETURNING id",
    )
    .bind(notes)
    .bind(appointment_id)
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

/// Evict one lower-priority appointment from its slot inside a booking
/// transaction so a more urgent booking can take its place: copy it onto the
/// waitlist, cancel it, and release its occupancy slots. The single source of
/// truth for "bump the occupant", shared by priority booking
/// (`book_with_priority`) and priority-override waitlist promotion
/// (`promote_from_waitlist`).
pub(super) async fn bump_conflict(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    conflict_id: i64,
    notes: &str,
) -> Result<i64, AppError> {
    let waitlist_id = bump_to_waitlist(tx, conflict_id, notes).await?;

    let mut bumped = sqlx::query_as::<_, Appointment>(
        "SELECT id, patient_id, doctor_id, appointment_date, start_time, end_time,
                status, notes, created_at, room_id, priority
         FROM appointments WHERE id = ?",
    )
    .bind(conflict_id)
    .fetch_one(&mut **tx)
    .await?;
    bumped.cancel()?;
    persist_status(&mut **tx, &bumped).await?;

    sqlx::query("DELETE FROM appointment_slots WHERE appointment_id = ?")
        .bind(conflict_id)
        .execute(&mut **tx)
        .await?;
    Ok(waitlist_id)
}

/// Fetch every *scheduled* appointment occupying `doctor_id`'s or `room_id`'s
/// calendar that overlaps `[start, end)` on `date`, the same doctor-OR-room
/// criteria `check_conflict` uses. A priority override can only bump what is
/// actually holding the slot, so this must see every resource the new booking
/// would collide with, not just the doctor's own calendar (a doctor-only
/// filter would miss a same-room, different-doctor occupant and let two
/// appointments land in the same room). Shared by `book_with_priority` and
/// `promote_from_waitlist`.
pub(super) async fn fetch_scheduled_conflicts(
    pool: &SqlitePool,
    doctor_id: i64,
    room_id: i64,
    date: &str,
    start: &str,
    end: &str,
) -> Result<Vec<(i64, i32, String, String, String)>, AppError> {
    Ok(sqlx::query_as::<_, (i64, i32, String, String, String)>(
        "SELECT id, priority, start_time, end_time, notes FROM appointments
         WHERE (doctor_id = ? OR room_id = ?) AND appointment_date = ? AND status = 'scheduled'
           AND start_time < ? AND end_time > ?",
    )
    .bind(doctor_id)
    .bind(room_id)
    .bind(date)
    .bind(end)
    .bind(start)
    .fetch_all(pool)
    .await?)
}

/// Does `new_priority` strictly outrank every occupant in `conflicts`? Only
/// then may a priority override bump them; a tie or a higher-priority
/// occupant blocks the override entirely. Shared by `book_with_priority` and
/// `promote_from_waitlist`.
pub(super) fn all_outranked(
    new_priority: Priority,
    conflicts: &[(i64, i32, String, String, String)],
) -> bool {
    conflicts
        .iter()
        .all(|(_, pri, _, _, _)| new_priority.outranks(Priority::from_i32(*pri)))
}

/// Bump every conflicting occupant onto the waitlist inside `tx`, returning
/// their new waitlist ids for the caller's post-commit rebook attempt. Shared
/// by both priority-override booking paths.
pub(super) async fn bump_all_conflicts(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    conflicts: &[(i64, i32, String, String, String)],
) -> Result<Vec<i64>, AppError> {
    let mut bumped_waitlist_ids = Vec::new();
    for (conflict_id, _, _, _, c_notes) in conflicts {
        bumped_waitlist_ids.push(bump_conflict(tx, *conflict_id, c_notes).await?);
    }
    Ok(bumped_waitlist_ids)
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

/// The full contents of a new appointment row: one named bundle instead of
/// nine loose positional arguments, shared by every path that books an
/// appointment (standard, priority override, and waitlist promotion). Field
/// names make the call sites self-documenting and impossible to transpose.
pub(super) struct NewAppointment<'a> {
    pub(super) patient_id: i64,
    pub(super) doctor_id: i64,
    pub(super) date: &'a str,
    pub(super) start: &'a str,
    pub(super) end: &'a str,
    pub(super) priority: i32,
    pub(super) room_id: i64,
    pub(super) notes: &'a Option<String>,
}

/// Insert the appointment row plus its occupancy slots inside an existing
/// transaction. Used by both the standard and priority booking paths.
pub(super) async fn insert_appointment_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    new: &NewAppointment<'_>,
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
    .bind(new.patient_id)
    .bind(new.doctor_id)
    .bind(new.date)
    .bind(new.start)
    .bind(new.end)
    .bind(new.priority)
    .bind(new.room_id)
    .bind(new.notes)
    .fetch_one(&mut **tx)
    .await?;

    insert_slots(tx, appt.id, new.doctor_id, new.date, start_mins, end_mins, new.room_id).await?;
    Ok(appt)
}

/// Book an appointment and its 30-minute occupancy slots atomically.
/// Opens its own transaction so the appointment row and every slot row commit
/// together; if any slot is already taken the whole booking rolls back.
pub(super) async fn insert_appointment(
    pool: &SqlitePool,
    new: &NewAppointment<'_>,
) -> Result<Appointment, AppError> {
    let (start_mins, end_mins) = parse_slot(new.start, new.end)?;

    let mut tx = pool.begin().await?;
    let appt = insert_appointment_in_tx(&mut tx, new, start_mins, end_mins).await?;
    tx.commit().await?;
    Ok(appt)
}
