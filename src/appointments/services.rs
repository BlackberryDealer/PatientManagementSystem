use crate::appointments::models::{
    Appointment, AppointmentView, BookAppointmentForm, DaySchedule, Room,
    SuggestSlotForm, WaitlistEntry, WaitlistForm,
};
use crate::auth::Role;
use crate::db;
use crate::errors::AppError;
use crate::availability::services::ensure_doctor_available;
use crate::time::{
    minutes_to_time, parse_slot, time_to_minutes, CLINIC_CLOSE_MINUTES,
    CLINIC_OPEN_MINUTES, SLOT_MINUTES,
};
use crate::traits::{Prioritized, Priority, StatusManaged, TimeSlotted};
use sqlx::SqlitePool;
use std::collections::BinaryHeap;

// ============================================================
// Algorithm 1: Time Interval Overlap Detection
// ============================================================

/// Check whether a proposed time-slot conflicts with any existing
/// (non-cancelled) appointment for the same doctor AND room.
///
/// ## Overlap logic
/// Two intervals [A_start, A_end) and [B_start, B_end) overlap iff:
///   A_start < B_end AND A_end > B_start
///
/// Room is always checked — every appointment has an auto-assigned room.
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
         WHERE doctor_id = ?
           AND appointment_date = ?
           AND status != 'cancelled'
           AND start_time < ? AND end_time > ?
           AND room_id = ?",
    );

    if exclude_appointment_id.is_some() {
        sql.push_str(" AND id != ?");
    }

    let mut query = sqlx::query_as::<_, (i64,)>(&sql)
        .bind(doctor_id)
        .bind(appointment_date)
        .bind(end_time)   // A_start < B_end → start_time < end_time (new end)
        .bind(start_time) // A_end > B_start → end_time > start_time (new start)
        .bind(room_id);

    if let Some(eid) = exclude_appointment_id {
        query = query.bind(eid);
    }

    let count = query.fetch_one(pool).await?;
    Ok(count.0 > 0)
}

// ============================================================
// Algorithm 2: Earliest Available Slot
// ============================================================

/// Given a doctor, date, and desired duration, find the earliest
/// available start time. Returns `None` if the day is fully booked.
///
/// ## Steps:
/// 1. Fetch all existing appointments for the doctor on that date
/// 2. Sort by start_time
/// 3. Walk through the gaps, return the first gap ≥ duration
/// 4. If no gap found, return `None`
pub async fn find_earliest_slot(
    pool: &SqlitePool,
    form: &SuggestSlotForm,
) -> Result<Option<String>, AppError> {
    // Validation owned by the form, not stranded in the algorithm.
    form.validate()?;

    // Fetch existing appointments for the doctor on that date, sorted by start time.
    // Room is auto-assigned per doctor/day so we check doctor-level conflicts only.
    let existing = sqlx::query_as::<_, (String, String)>(
        "SELECT start_time, end_time FROM appointments
         WHERE doctor_id = ? AND appointment_date = ? AND status != 'cancelled'
         ORDER BY start_time",
    )
    .bind(form.doctor_id)
    .bind(&form.appointment_date)
    .fetch_all(pool)
    .await?;

    // Map the booked rows to minute intervals, then delegate the gap-finding
    // math to the DaySchedule domain object (pure + unit-testable). Rows are
    // always valid "HH:MM" (enforced by parse_slot on insert), so any
    // unparseable row is simply dropped rather than coerced.
    let busy: Vec<(i32, i32)> = existing
        .iter()
        .filter_map(|(start, end)| Some((time_to_minutes(start)?, time_to_minutes(end)?)))
        .collect();

    let slot = DaySchedule::new(busy)
        .earliest_gap(form.duration_minutes, CLINIC_OPEN_MINUTES, CLINIC_CLOSE_MINUTES)
        .map(minutes_to_time);

    Ok(slot)
}

// ============================================================
// Algorithm 3: Priority-Based Scheduling with BinaryHeap
// ============================================================

/// A waitlist item ordered by priority for the BinaryHeap.
/// Lower priority number = higher urgency (flipped for max-heap behaviour).
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PriorityItem {
    waitlist_id: i64,
    patient_id: i64,
    priority: i32,       // lower = more urgent
    requested_start: String,
    requested_end: String,
    created_at: chrono::NaiveDateTime,
}

// BinaryHeap is a max-heap; `pop` yields the GREATEST element. We want the
// most urgent (lowest priority number), then the oldest, to pop first — so the
// winner must compare as the greatest. BOTH keys are reversed to achieve that:
// a smaller priority number and an earlier created_at each make an item
// "greater". (Reversing only the priority, as before, made `into_sorted_vec`
// surface the LEAST urgent entry first — the auto-promotion bug.)
impl Ord for PriorityItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.priority.cmp(&self.priority) // lower priority # = more urgent = greater
            .then_with(|| other.created_at.cmp(&self.created_at)) // older = greater (pops first)
    }
}
impl PartialOrd for PriorityItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Build a priority queue from waitlist entries for a doctor on a date.
/// Uses `std::collections::BinaryHeap` with a reversed Ord so that the
/// most urgent (lowest priority number) is always at the top.
pub(crate) async fn build_priority_queue(
    pool: &SqlitePool,
    doctor_id: i64,
    appointment_date: &str,
) -> Result<BinaryHeap<PriorityItem>, AppError> {
    let entries = sqlx::query_as::<_, (i64, i64, i32, String, String, chrono::NaiveDateTime)>(
        "SELECT id, patient_id, priority, requested_start, requested_end, created_at
         FROM waitlist
         WHERE doctor_id = ? AND appointment_date = ? AND status = 'waiting'
         ORDER BY priority ASC, created_at ASC",
    )
    .bind(doctor_id)
    .bind(appointment_date)
    .fetch_all(pool)
    .await?;

    let heap: BinaryHeap<PriorityItem> = entries
        .into_iter()
        .map(|(id, pid, pri, start, end, created)| PriorityItem {
            waitlist_id: id,
            patient_id: pid,
            priority: pri,
            requested_start: start,
            requested_end: end,
            created_at: created,
        })
        .collect();

    Ok(heap)
}

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
    // Validation layer: the form owns its date + slot-grid rules; the service
    // still enforces the doctor's availability (a domain rule that needs the DB).
    form.validate()?;
    let (start_mins, end_mins) = parse_slot(&form.start_time, &form.end_time)?;
    ensure_doctor_available(
        pool, form.doctor_id, &form.appointment_date, &form.start_time, &form.end_time,
    ).await?;

    let new_priority = form.requested_priority();

    // Triage rule lives on the Priority enum, not as a magic number here.
    if !new_priority.can_override() {
        return Err(AppError::BadRequest(
            "Priority override is only available for Emergency or Urgent appointments.\
             \nUse standard booking for Normal or Follow-up visits."
                .into(),
        ));
    }

    // Look up patient
    let patient_id = db::get_patient_id(pool, patient_user_id).await?;

    // Auto-assign room from the doctor's daily allocation
    let room_id = resolve_room(pool, form.doctor_id, &form.appointment_date).await?;

    // Check for conflicts
    let has_conflict = check_conflict(
        pool, form.doctor_id, &form.appointment_date,
        &form.start_time, &form.end_time, room_id, None,
    ).await?;

    if !has_conflict {
        return insert_appointment(
            pool, patient_id, form.doctor_id, &form.appointment_date,
            &form.start_time, &form.end_time, new_priority as i32,
            room_id, &form.notes,
        ).await;
    }

    // Conflict exists — find conflicting appointments
    let conflicts = sqlx::query_as::<_, (i64, i32, String, String, String)>(
        "SELECT id, priority, start_time, end_time, notes FROM appointments
         WHERE doctor_id = ? AND appointment_date = ? AND status = 'scheduled'
           AND start_time < ? AND end_time > ?",
    )
    .bind(form.doctor_id)
    .bind(&form.appointment_date)
    .bind(&form.end_time)
    .bind(&form.start_time)
    .fetch_all(pool)
    .await?;

    // `check_conflict` counts any non-cancelled appointment, but only a
    // *scheduled* one can be bumped. If the overlap is entirely with a
    // completed appointment, there is nothing to bump — say so clearly instead
    // of falling through to a confusing slot-collision error on insert.
    if conflicts.is_empty() {
        return Err(AppError::BadRequest(
            "This time slot is occupied by a completed appointment and cannot be overridden.\
             \nPlease choose a different time."
                .into(),
        ));
    }

    // Verify new appointment outranks ALL conflicting ones. The triage
    // precedence rule lives on the Priority type (outranks), not as an
    // inline comparison here.
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

    // --- Run all mutations inside a transaction ---
    let mut tx = pool.begin().await?;

    for (conflict_id, _, _, _, c_notes) in &conflicts {
        // Move the bumped appointment to the waitlist...
        bump_to_waitlist(&mut tx, *conflict_id, c_notes).await?;

        // ...cancel it through the domain method so the "only a scheduled
        // appointment may be cancelled" rule lives in exactly one place
        // (Appointment::cancel), instead of being duplicated as a raw SQL
        // status write here.
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

        // Free its slots so the urgent booking can take them.
        sqlx::query("DELETE FROM appointment_slots WHERE appointment_id = ?")
            .bind(conflict_id)
            .execute(&mut *tx)
            .await?;
    }

    // Book the new appointment + its slots inside the same transaction.
    // Slots are freed above before this runs, so the UNIQUE index is satisfied.
    let appointment = insert_appointment_in_tx(
        &mut tx,
        patient_id,
        form.doctor_id,
        &form.appointment_date,
        &form.start_time,
        &form.end_time,
        new_priority as i32,
        room_id,
        &form.notes,
        start_mins,
        end_mins,
    )
    .await?;

    tx.commit().await?;

    Ok(appointment)
}

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
    // Validation layer: the form owns its date + slot-grid rules; the service
    // still enforces the doctor's availability (a domain rule that needs the DB).
    form.validate()?;
    ensure_doctor_available(
        pool, form.doctor_id, &form.appointment_date, &form.start_time, &form.end_time,
    ).await?;

    let patient_id = db::get_patient_id(pool, patient_user_id).await?;
    let priority = form.requested_priority() as i32;

    // Auto-assign room from the doctor's daily allocation
    let room_id = resolve_room(pool, form.doctor_id, &form.appointment_date).await?;

    let has_conflict = check_conflict(
        pool, form.doctor_id, &form.appointment_date,
        &form.start_time, &form.end_time, room_id, None,
    ).await?;

    if has_conflict {
        return Err(AppError::BadRequest(
            "The requested time slot conflicts with an existing appointment.\
             \nPlease choose a different time, or use priority booking (Emergency/Urgent)."
                .into(),
        ));
    }

    insert_appointment(
        pool, patient_id, form.doctor_id, &form.appointment_date,
        &form.start_time, &form.end_time, priority,
        room_id, &form.notes,
    ).await
}

/// Translate a slot-insert failure: a UNIQUE-index violation means another
/// booking grabbed the slot first (a race we lost), surfaced as a clean 400.
fn map_slot_conflict(e: sqlx::Error) -> AppError {
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
/// priority, with an overridden `notes`) inside the booking transaction. Used
/// when a higher-priority booking overrides lower-priority ones, so the
/// displaced patients keep their place in the queue.
async fn bump_to_waitlist(
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
/// authoritative double-booking guard — if any slot is already taken, the
/// INSERT fails and the whole transaction rolls back.
async fn insert_slots(
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
/// transaction. Used by both the standard and priority booking paths so the
/// appointment and all its slots commit atomically (all-or-nothing).
async fn insert_appointment_in_tx(
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
async fn insert_appointment(
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

// ============================================================
// Waitlist operations
// ============================================================

/// Add a patient to the waitlist.
pub async fn add_to_waitlist(
    pool: &SqlitePool,
    patient_user_id: i64,
    form: &WaitlistForm,
) -> Result<WaitlistEntry, AppError> {
    // Validation owned by the form, like every sibling write path
    // (Route -> Validation -> Business Logic -> DB).
    form.validate()?;

    let patient_id = db::get_patient_id(pool, patient_user_id).await?;

    // Auto-assign room from the doctor's daily allocation
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
    .bind(&form.requested_start)
    .bind(&form.requested_end)
    .bind(form.priority)
    .bind(&form.notes)
    .fetch_one(pool)
    .await?)
}

/// Get the pending waitlist for a doctor (by user_id, like the other
/// per-role queries — the doctor row ID is resolved internally).
pub async fn get_waitlist_for_doctor(
    pool: &SqlitePool,
    doctor_user_id: i64,
) -> Result<Vec<WaitlistEntry>, AppError> {
    let doctor_id = crate::db::get_doctor_id(pool, doctor_user_id).await?;
    Ok(sqlx::query_as::<_, WaitlistEntry>(
        "SELECT * FROM waitlist WHERE doctor_id = ? AND status = 'waiting'
         ORDER BY priority ASC, created_at ASC",
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
        "SELECT w.* FROM waitlist w
         JOIN patients p ON w.patient_id = p.id
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
        "SELECT * FROM waitlist WHERE status = 'waiting' ORDER BY priority ASC, created_at ASC",
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

    // `pop` drains the queue most-urgent-first (oldest first on ties), so the
    // most urgent eligible patient claims the freed slot.
    while let Some(item) = heap.pop() {
        let entry = sqlx::query_as::<_, WaitlistEntry>(
            "SELECT * FROM waitlist WHERE id = ? AND status = 'waiting'",
        )
        .bind(item.waitlist_id)
        .fetch_optional(pool)
        .await?;

        if let Some(mut entry) = entry {
            // Doctor must still be available (not on leave) for the stored slot
            let available = ensure_doctor_available(
                pool, entry.doctor_id, appointment_date,
                entry.start_time(), entry.end_time(),
            ).await.is_ok();

            // Resolve room: use the stored one from the original booking, or auto-assign
            let room_id = match entry.room_id {
                Some(rid) => rid,
                None => resolve_room(pool, entry.doctor_id, appointment_date).await?,
            };

            // Use TimeSlotted trait to check conflict before querying DB
            let conflict = check_conflict(
                pool, entry.doctor_id,
                appointment_date,
                entry.start_time(), // from TimeSlotted trait
                entry.end_time(),   // from TimeSlotted trait
                room_id, None,
            ).await?;

            if available && !conflict {
                let appt = insert_appointment(
                    pool, entry.patient_id, entry.doctor_id,
                    appointment_date,
                    entry.start_time(), entry.end_time(),
                    entry.priority_level(), // from Prioritized trait
                    room_id, &entry.notes,
                ).await?;

                entry.accept()?; // encapsulated state transition (&mut self)
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

    // Check if slot is now free AND the doctor is still available for it
    let date_str = entry.appointment_date.format("%Y-%m-%d").to_string();
    let available = ensure_doctor_available(
        pool, entry.doctor_id, &date_str,
        &entry.requested_start, &entry.requested_end,
    ).await.is_ok();

    // Resolve room: use the stored one from the original booking, or auto-assign
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
        return Ok(None); // slot still taken or doctor no longer available
    }

    // Book it
    let appt = insert_appointment(
        pool, entry.patient_id, entry.doctor_id,
        &date_str,
        &entry.requested_start, &entry.requested_end,
        entry.priority_level(), room_id, &entry.notes,
    ).await?;

    // Encapsulated state transition, then persist the entry's new status
    entry.accept()?;
    sqlx::query("UPDATE waitlist SET status = ? WHERE id = ?")
        .bind(entry.current_status())
        .bind(entry.id)
        .execute(pool)
        .await?;

    Ok(Some(appt))
}

// ============================================================
// Queries (updated for rooms + priority)
// ============================================================

/// Shared SELECT for the joined appointment view. Each query appends its own
/// `WHERE` / `ORDER BY`. Columns are aliased to match `AppointmentView` field
/// names so rows deserialize directly via `FromRow` — no manual tuple mapping.
/// Only static SQL is interpolated; all user values are bound as parameters.
const APPOINTMENT_VIEW_SELECT: &str = "\
    SELECT a.id, u_p.full_name AS patient_name, u_d.full_name AS doctor_name,
           a.appointment_date, a.start_time, a.end_time, a.status, a.notes,
           r.name AS room_name, a.priority
    FROM appointments a
    JOIN patients p ON a.patient_id = p.id
    JOIN users u_p ON p.user_id = u_p.id
    JOIN doctors d ON a.doctor_id = d.id
    JOIN users u_d ON d.user_id = u_d.id
    LEFT JOIN rooms r ON a.room_id = r.id";

/// Get all appointments for a patient.
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

/// Get all appointments for a doctor.
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

/// Get all appointments (admin).
pub async fn get_all_appointments(pool: &SqlitePool) -> Result<Vec<AppointmentView>, AppError> {
    Ok(sqlx::query_as::<_, AppointmentView>(&format!(
        "{APPOINTMENT_VIEW_SELECT} ORDER BY a.appointment_date DESC, a.start_time"
    ))
    .fetch_all(pool)
    .await?)
}

/// Get a single appointment by ID.
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

/// Get a single appointment, enforcing that a patient may only view their OWN.
/// Doctors and admins (clinical staff) may view any appointment. Mirrors the
/// ownership pattern used by `get_record_checked` / `get_invoice_checked`.
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

/// Cancel an appointment.
/// After cancellation, automatically attempts to promote the highest-priority
/// waitlist entry into the freed slot using the BinaryHeap priority queue.
pub async fn cancel_appointment(pool: &SqlitePool, appointment_id: i64) -> Result<(), AppError> {
    // Load the full domain object so the cancellation rule runs on it
    let mut appt = sqlx::query_as::<_, Appointment>(
        "SELECT id, patient_id, doctor_id, appointment_date, start_time, end_time,
                status, notes, created_at, room_id, priority
         FROM appointments WHERE id = ?",
    )
    .bind(appointment_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Appointment not found".into()))?;

    // Business rule + state change live on the Appointment itself
    appt.cancel()?;

    // Persist the new status AND release its occupancy slots in one transaction,
    // so the freed slots become immediately bookable again.
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

    // Auto-promote the most urgent waitlist entry for the freed slot
    let date_str = appt.appointment_date.format("%Y-%m-%d").to_string();
    let _ = auto_promote_waitlist(pool, appt.doctor_id(), &date_str).await;

    Ok(())
}

/// Cancel an appointment, enforcing ownership for patients.
/// Patients may only cancel their own appointments; doctors and admins can cancel any.
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

/// Per-day appointment counts for a patient. Returns a map of "YYYY-MM-DD" → count.
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

/// Per-day appointment counts for a doctor. Returns a map of "YYYY-MM-DD" → count.
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

/// Per-day appointment counts across all patients (admin view).
/// Returns a map of "YYYY-MM-DD" → count.
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

/// Room auto-assignment: each doctor gets one room per day.
/// Returns the existing assignment if one exists, otherwise claims
/// the first available active room for that doctor+date and returns it.
///
/// "At the start of each day, a doctor is allocated a room."
/// The first booking of the day triggers lazy auto-assignment.
async fn resolve_room(
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

/// Get all active rooms for dropdowns.
pub async fn get_all_rooms(pool: &SqlitePool) -> Result<Vec<Room>, AppError> {
    Ok(sqlx::query_as::<_, Room>(
        "SELECT * FROM rooms WHERE is_active = 1 ORDER BY name",
    )
    .fetch_all(pool)
    .await?)
}

// ============================================================
// Algorithm 4: Doctor Reassignment (greedy, load-balanced)
// ============================================================

/// Find the best alternative doctor for a given slot.
///
/// ## Greedy selection strategy
/// Candidates are every other doctor, ranked by:
/// 1. Same specialization as the current doctor first (continuity of care)
/// 2. Fewest scheduled appointments on that date (load balancing)
///
/// The first candidate that is both available (per their availability
/// rules) and conflict-free for the slot is chosen. Returns the doctor's
/// row ID and display name, or `None` if every doctor is busy.
pub async fn find_alternative_doctor(
    pool: &SqlitePool,
    exclude_doctor_id: i64,
    appointment_date: &str,
    start_time: &str,
    end_time: &str,
    room_id: i64,
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
        let conflict = check_conflict(
            pool, candidate_id, appointment_date, start_time, end_time, room_id, None,
        ).await?;
        if !conflict {
            return Ok(Some((candidate_id, candidate_name)));
        }
    }
    Ok(None)
}

/// Reassign a scheduled appointment to the best available alternative
/// doctor (e.g. when the original doctor goes on leave). The appointment
/// row and its occupancy slots move atomically; the slot-table UNIQUE
/// index backstops any race.
pub async fn reassign_appointment(
    pool: &SqlitePool,
    appointment_id: i64,
) -> Result<(Appointment, String), AppError> {
    let mut appt = sqlx::query_as::<_, Appointment>(
        "SELECT id, patient_id, doctor_id, appointment_date, start_time, end_time,
                status, notes, created_at, room_id, priority
         FROM appointments WHERE id = ?",
    )
    .bind(appointment_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Appointment not found".into()))?;

    let date_str = appt.appointment_date.format("%Y-%m-%d").to_string();

    // Resolve room: use the stored one, or auto-assign for the original doctor
    let room_id = match appt.room_id {
        Some(rid) => rid,
        None => resolve_room(pool, appt.doctor_id(), &date_str).await?,
    };

    let (new_doctor_id, new_doctor_name) = find_alternative_doctor(
        pool, appt.doctor_id(), &date_str, &appt.start_time, &appt.end_time, room_id,
    )
    .await?
    .ok_or_else(|| {
        AppError::BadRequest(
            "No alternative doctor is available for this time slot.\
             \nTry a different time, or cancel and rebook.".into(),
        )
    })?;

    let (start_mins, end_mins) = parse_slot(&appt.start_time, &appt.end_time)?;

    // Business rule + state change live on the Appointment itself (mirrors
    // cancel()): only a scheduled appointment can be reassigned. The "only
    // scheduled" guard and the doctor change no longer live as a raw SQL
    // status/field write in this service layer.
    appt.reassign_to(new_doctor_id)?;

    // Persist the new doctor and move its occupancy slots atomically.
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

// ============================================================
// Grid slot options for the booking UI
// ============================================================
// These power the start/end dropdowns so the user can ONLY pick grid-aligned
// times — making an off-grid booking impossible to express in the interface.
// The server still validates alignment in `parse_slot` as defence in depth.

/// Bookable 30-minute START times within clinic working hours (08:00–16:30).
pub fn start_time_slots() -> Vec<String> {
    (CLINIC_OPEN_MINUTES..CLINIC_CLOSE_MINUTES)
        .step_by(SLOT_MINUTES as usize)
        .map(minutes_to_time)
        .collect()
}

/// Valid END times (08:30–17:00) — one slot after the earliest start, up to close.
pub fn end_time_slots() -> Vec<String> {
    ((CLINIC_OPEN_MINUTES + SLOT_MINUTES)..=CLINIC_CLOSE_MINUTES)
        .step_by(SLOT_MINUTES as usize)
        .map(minutes_to_time)
        .collect()
}
