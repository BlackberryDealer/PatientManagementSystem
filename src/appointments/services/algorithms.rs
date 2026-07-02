use crate::appointments::models::{Appointment, DaySchedule, SuggestSlotForm};
use crate::availability::services::ensure_doctor_available;
use crate::errors::AppError;
use crate::traits::Prioritized;
use crate::time::{minutes_to_time, parse_slot, time_to_minutes, CLINIC_CLOSE_MINUTES, CLINIC_OPEN_MINUTES, SLOT_MINUTES};
use sqlx::SqlitePool;
use std::collections::BinaryHeap;

use super::helpers::insert_slots;
use super::rooms::resolve_room;

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
/// 3. Walk through the gaps; for each gap ≥ duration, check the doctor's
///    availability rules (leave, blocked breaks, declared working windows —
///    the same 3-rule gate booking enforces)
/// 4. Return the first gap that passes both; `None` if nothing fits
///
/// Consulting `ensure_doctor_available` here keeps the suggestion feature
/// consistent with booking: a suggested slot is always one the booking
/// path would actually accept.
pub async fn find_earliest_slot(
    pool: &SqlitePool,
    form: &SuggestSlotForm,
) -> Result<Option<String>, AppError> {
    form.validate()?;

    let existing = sqlx::query_as::<_, (String, String)>(
        "SELECT start_time, end_time FROM appointments
         WHERE doctor_id = ? AND appointment_date = ? AND status != 'cancelled'
         ORDER BY start_time",
    )
    .bind(form.doctor_id)
    .bind(&form.appointment_date)
    .fetch_all(pool)
    .await?;

    let busy: Vec<(i32, i32)> = existing
        .iter()
        .filter_map(|(start, end)| Some((time_to_minutes(start)?, time_to_minutes(end)?)))
        .collect();

    let schedule = DaySchedule::new(busy);
    let mut open = CLINIC_OPEN_MINUTES;
    while let Some(start) = schedule.earliest_gap(form.duration_minutes, open, CLINIC_CLOSE_MINUTES) {
        let end = start + form.duration_minutes;
        let available = ensure_doctor_available(
            pool, form.doctor_id, &form.appointment_date,
            &minutes_to_time(start), &minutes_to_time(end),
        )
        .await
        .is_ok();
        if available {
            return Ok(Some(minutes_to_time(start)));
        }
        // Doctor is blocked or outside their declared hours at this gap —
        // resume the search from the next grid-aligned start time.
        open = start + SLOT_MINUTES;
    }
    Ok(None)
}

// ============================================================
// Algorithm 3 support: Priority Queue (BinaryHeap)
// ============================================================

/// A waitlist item ordered by priority for the BinaryHeap.
/// Lower priority number = higher urgency (flipped for max-heap behaviour).
#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct PriorityItem {
    pub(super) waitlist_id: i64,
    pub(super) patient_id: i64,
    pub(super) priority: i32,
    pub(super) requested_start: String,
    pub(super) requested_end: String,
    pub(super) created_at: chrono::NaiveDateTime,
}

/// PriorityItem joins the same `Prioritized` family as Appointment and
/// WaitlistEntry, so the heap ordering below reuses the shared urgency
/// comparison instead of re-encoding "lower number wins" a second time.
impl Prioritized for PriorityItem {
    fn priority_level(&self) -> i32 { self.priority }
}

// BinaryHeap is a max-heap; `pop` yields the GREATEST element. We want the
// most urgent (lowest priority number), then the oldest, to pop first — so the
// winner must compare as the greatest. BOTH keys are reversed to achieve that.
impl Ord for PriorityItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        if self.is_higher_priority_than(other) {
            Ordering::Greater
        } else if other.is_higher_priority_than(self) {
            Ordering::Less
        } else {
            // Equal urgency: FIFO — the older entry must pop first.
            other.created_at.cmp(&self.created_at)
        }
    }
}
impl PartialOrd for PriorityItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Build a priority queue from waitlist entries for a doctor on a date.
pub(super) async fn build_priority_queue(
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
/// The first candidate that is both available and conflict-free is chosen.
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
/// doctor. The appointment row and its occupancy slots move atomically.
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
