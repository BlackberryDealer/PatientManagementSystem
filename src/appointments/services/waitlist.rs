use crate::appointments::models::{Appointment, SuggestSlotForm, WaitlistEntry, WaitlistForm};
use crate::availability::services::ensure_doctor_available;
use crate::db;
use crate::errors::AppError;
use crate::time::{minutes_to_time, parse_slot, time_to_minutes};
use crate::traits::{Prioritized, StatusManaged, TimeSlotted};
use sqlx::SqlitePool;

use super::algorithms::{build_priority_queue, check_conflict, find_earliest_slot};
use super::helpers::{
    all_outranked, bump_all_conflicts, fetch_scheduled_conflicts, insert_appointment,
    insert_appointment_in_tx, NewAppointment,
};
use super::rooms::resolve_room;

/// Shared SELECT for the joined waitlist view. Each query appends its own
/// `WHERE` / `ORDER BY`. Only static SQL is interpolated; all user values are
/// bound as parameters.
const WAITLIST_VIEW_SELECT: &str = "\
    SELECT w.*, COALESCE(pu.full_name, 'Patient #' || w.patient_id) AS patient_name,
           COALESCE(du.full_name, 'Doctor #' || w.doctor_id) AS doctor_name
    FROM waitlist w
    JOIN patients p ON w.patient_id = p.id
    JOIN users pu ON p.user_id = pu.id
    JOIN doctors d ON w.doctor_id = d.id
    JOIN users du ON d.user_id = du.id";

/// Outcome of a staff-initiated waitlist promotion.
///
/// Distinguishes the two non-error results the handler must render differently:
/// a successful booking (redirect to the new appointment) versus a promotion
/// that could not proceed under the triage rules (redirect back with a reason
/// the user sees). Genuine failures (DB errors, a missing entry) stay in the
/// `Err` arm as `AppError`.
pub enum PromotionOutcome {
    /// The entry was booked into a real appointment (slot was free, or an
    /// override bumped a lower-priority occupant). The `Vec` holds any
    /// bumped occupants that were immediately auto-rescheduled into the
    /// doctor's next free same-day slot (empty when the slot was free or
    /// when no gap was found for a bumped occupant).
    Promoted(Appointment, Vec<Appointment>),
    /// The promotion was rejected; the string is a user-facing explanation.
    Blocked(String),
}

/// Add a patient to the waitlist.
pub async fn add_to_waitlist(
    pool: &SqlitePool,
    patient_user_id: i64,
    form: &WaitlistForm,
) -> Result<WaitlistEntry, AppError> {
    form.validate()?;
    // Store the canonical zero-padded "HH:MM" form, not the raw input:
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
    Ok(sqlx::query_as::<_, WaitlistEntry>(&format!(
        "{WAITLIST_VIEW_SELECT} WHERE w.doctor_id = ? AND w.status = 'waiting'
         ORDER BY w.priority ASC, w.created_at ASC"
    ))
    .bind(doctor_id)
    .fetch_all(pool)
    .await?)
}

/// Get all waitlist entries for a specific patient (by user_id).
pub async fn get_waitlist_for_patient(
    pool: &SqlitePool,
    patient_user_id: i64,
) -> Result<Vec<WaitlistEntry>, AppError> {
    Ok(sqlx::query_as::<_, WaitlistEntry>(&format!(
        "{WAITLIST_VIEW_SELECT} WHERE p.user_id = ? AND w.status IN ('waiting', 'expired')
         ORDER BY w.priority ASC, w.created_at ASC"
    ))
    .bind(patient_user_id)
    .fetch_all(pool)
    .await?)
}

/// Expire every waiting entry whose requested window has already passed:
/// any past date, or today once requested_end is behind the current time.
/// A set-based UPDATE for efficiency; the single-row `WaitlistEntry::expire`
/// still owns the domain rule, and the `status = 'waiting'` clause here
/// enforces the same transition guard at the database level.
pub async fn expire_stale_waitlist(pool: &SqlitePool) -> Result<u64, AppError> {
    let now = chrono::Utc::now();
    let today = now.date_naive().format("%Y-%m-%d").to_string();
    let time_now = now.format("%H:%M").to_string();

    let result = sqlx::query(
        "UPDATE waitlist SET status = 'expired'
         WHERE status = 'waiting'
           AND (appointment_date < ?
                OR (appointment_date = ? AND requested_end <= ?))",
    )
    .bind(&today)
    .bind(&today)
    .bind(&time_now)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Get all pending waitlist entries (admin view).
pub async fn get_all_waitlist(pool: &SqlitePool) -> Result<Vec<WaitlistEntry>, AppError> {
    Ok(sqlx::query_as::<_, WaitlistEntry>(&format!(
        "{WAITLIST_VIEW_SELECT} WHERE w.status = 'waiting'
         ORDER BY w.priority ASC, w.created_at ASC"
    ))
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
    expire_stale_waitlist(pool).await?;
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

                mark_entry_accepted(pool, &mut entry).await?;

                return Ok(Some(appt));
            }
        }
    }

    Ok(None)
}

/// Promote a waitlist entry into a real appointment (staff-initiated).
///
/// Two cases:
/// * **Slot free**, book it directly (the original behaviour, also used when a
///   cancellation just opened the slot up).
/// * **Slot occupied**, apply the same triage rule as `book_with_priority`:
///   if the waiting entry strictly outranks *every* appointment holding the
///   slot, bump those lower-priority occupants to the waitlist and book the
///   promoted entry in their place, all inside one transaction. Otherwise the
///   promotion is `Blocked` with a reason the caller surfaces to the user,
///   never a silent no-op.
pub async fn promote_from_waitlist(
    pool: &SqlitePool,
    waitlist_id: i64,
) -> Result<PromotionOutcome, AppError> {
    // Sweep everything else stale first (system-wide hygiene), then load this
    // entry regardless of status so a past-due click gets the friendly
    // "already passed" message below instead of a bare NotFound.
    expire_stale_waitlist(pool).await?;

    let mut entry = sqlx::query_as::<_, WaitlistEntry>("SELECT * FROM waitlist WHERE id = ?")
        .bind(waitlist_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Waitlist entry not found".into()))?;

    let date_str = entry.appointment_date.format("%Y-%m-%d").to_string();

    let now = chrono::Utc::now();
    let is_past = date_str < now.date_naive().format("%Y-%m-%d").to_string()
        || (date_str == now.date_naive().format("%Y-%m-%d").to_string()
            && entry.requested_end <= now.format("%H:%M").to_string());
    if is_past {
        return Ok(PromotionOutcome::Blocked(
            "Could not promote: the requested time has already passed.".into(),
        ));
    }
    if entry.current_status() != "waiting" {
        return Err(AppError::NotFound("Waitlist entry not found".into()));
    }

    // The doctor's availability rules gate every booking, priority or not, so a
    // slot the doctor has blocked can never be promoted into.
    let available = ensure_doctor_available(
        pool, entry.doctor_id, &date_str,
        &entry.requested_start, &entry.requested_end,
    ).await.is_ok();
    if !available {
        return Ok(PromotionOutcome::Blocked(
            "Could not promote: the doctor is not available for that time on that date.".into(),
        ));
    }

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

    // Slot is free, the simple path: book it as-is.
    if !conflict {
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
        mark_entry_accepted(pool, &mut entry).await?;
        return Ok(PromotionOutcome::Promoted(appt, Vec::new()));
    }

    // Slot is occupied, a promotion may still proceed as a priority override,
    // under the same triage rule as book_with_priority.
    let new_priority = entry.priority();

    let conflicts = fetch_scheduled_conflicts(
        pool, entry.doctor_id, room_id, &date_str, &entry.requested_start, &entry.requested_end,
    )
    .await?;

    // Occupied, but not by any *scheduled* appointment, a completed visit holds
    // the slot and completed appointments are immutable history, so no override.
    if conflicts.is_empty() {
        return Ok(PromotionOutcome::Blocked(
            "Could not promote: the slot is held by a completed appointment, \
             which cannot be overridden."
                .into(),
        ));
    }

    if !all_outranked(new_priority, &conflicts) {
        return Ok(PromotionOutcome::Blocked(
            "Could not promote: the slot is held by an appointment of equal or \
             higher priority. Reschedule the current holder, or choose another slot."
                .into(),
        ));
    }

    // Every occupant is strictly lower priority: bump them and book the entry.
    let (start_mins, end_mins) = parse_slot(&entry.requested_start, &entry.requested_end)?;
    let mut tx = pool.begin().await?;

    let bumped_waitlist_ids = bump_all_conflicts(&mut tx, &conflicts).await?;

    let appt = insert_appointment_in_tx(
        &mut tx,
        &NewAppointment {
            patient_id: entry.patient_id,
            doctor_id: entry.doctor_id,
            date: &date_str,
            start: &entry.requested_start,
            end: &entry.requested_end,
            priority: new_priority as i32,
            room_id,
            notes: &entry.notes,
        },
        start_mins,
        end_mins,
    )
    .await?;

    entry.accept()?;
    sqlx::query("UPDATE waitlist SET status = ? WHERE id = ?")
        .bind(entry.current_status())
        .bind(entry.id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    let mut rescheduled = Vec::new();
    for wl_id in bumped_waitlist_ids {
        if let Some(rebooked) = try_rebook_bumped(pool, wl_id).await? {
            rescheduled.push(rebooked);
        }
    }

    Ok(PromotionOutcome::Promoted(appt, rescheduled))
}

/// Try to rebook a just-bumped waitlist entry into the doctor's earliest free
/// same-duration gap on the same day. On success the entry is booked and
/// marked accepted; the caller (which has the acting AuthUser) is responsible
/// for the audit record. Returns `None` when the day is full, the entry then
/// simply stays on the waitlist, the existing fallback behaviour.
///
/// Reuses Algorithm 2 (`find_earliest_slot`, which already walks
/// `DaySchedule` gaps and checks `ensure_doctor_available`) rather than
/// re-deriving the gap search, and books through the same `insert_appointment`
/// helper every other booking path uses, so the `appointment_slots` UNIQUE
/// index remains the single source of truth arbitrating any race.
///
/// Runs after the caller's override transaction has committed: the override
/// itself (bump + urgent booking) must stay atomic, but rebooking is
/// best-effort, so a rebooking failure can never roll back an emergency booking.
pub async fn try_rebook_bumped(
    pool: &SqlitePool,
    waitlist_id: i64,
) -> Result<Option<Appointment>, AppError> {
    let Some(mut entry) = sqlx::query_as::<_, WaitlistEntry>(
        "SELECT * FROM waitlist WHERE id = ? AND status = 'waiting'",
    )
    .bind(waitlist_id)
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };

    let (start_mins, end_mins) = parse_slot(&entry.requested_start, &entry.requested_end)?;
    let duration = end_mins - start_mins;
    let date_str = entry.appointment_date.format("%Y-%m-%d").to_string();

    let Some(new_start) = find_earliest_slot(
        pool,
        &SuggestSlotForm {
            doctor_id: entry.doctor_id,
            appointment_date: date_str.clone(),
            duration_minutes: duration,
        },
    )
    .await?
    else {
        return Ok(None);
    };

    let new_start_mins = time_to_minutes(&new_start).ok_or_else(|| {
        AppError::Internal(format!("find_earliest_slot returned an unparseable time: {new_start}"))
    })?;
    let new_end = minutes_to_time(new_start_mins + duration);
    let room_id = match entry.room_id {
        Some(rid) => rid,
        None => resolve_room(pool, entry.doctor_id, &date_str).await?,
    };
    let notes = Some(format!(
        "{}auto-rescheduled from {} after a priority override",
        entry
            .notes
            .as_deref()
            .map(|n| format!("{n}, "))
            .unwrap_or_default(),
        entry.requested_start,
    ));

    match insert_appointment(
        pool,
        &NewAppointment {
            patient_id: entry.patient_id,
            doctor_id: entry.doctor_id,
            date: &date_str,
            start: &new_start,
            end: &new_end,
            priority: entry.priority_level(),
            room_id,
            notes: &notes,
        },
    )
    .await
    {
        Ok(appt) => {
            mark_entry_accepted(pool, &mut entry).await?;
            Ok(Some(appt))
        }
        // Another booking grabbed it between find_earliest_slot's check and
        // this insert (a real race, not a bug), leave the entry waiting.
        Err(AppError::SlotConflict(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Mark a waiting entry as accepted after it has been booked (non-transactional
/// path, used when the slot was free). Enforces the "only a waiting entry can be
/// promoted" domain rule via `WaitlistEntry::accept`.
async fn mark_entry_accepted(
    pool: &SqlitePool,
    entry: &mut WaitlistEntry,
) -> Result<(), AppError> {
    entry.accept()?;
    sqlx::query("UPDATE waitlist SET status = ? WHERE id = ?")
        .bind(entry.current_status())
        .bind(entry.id)
        .execute(pool)
        .await?;
    Ok(())
}
