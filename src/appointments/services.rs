use crate::appointments::models::{
    Appointment, AppointmentView, BookAppointmentForm, Room,
    SuggestSlotForm, WaitlistEntry, WaitlistForm,
};
use crate::errors::AppError;
use crate::traits::{Prioritized, TimeSlotted};
use sqlx::SqlitePool;
use std::collections::BinaryHeap;

// ============================================================
// Helper: time helpers
// ============================================================

/// Convert "HH:MM" to minutes since midnight for comparison.
fn time_to_minutes(t: &str) -> Option<i32> {
    let parts: Vec<&str> = t.split(':').collect();
    if parts.len() != 2 { return None; }
    let h: i32 = parts[0].parse().ok()?;
    let m: i32 = parts[1].parse().ok()?;
    Some(h * 60 + m)
}

/// Convert minutes since midnight back to "HH:MM".
fn minutes_to_time(mins: i32) -> String {
    format!("{:02}:{:02}", mins / 60, mins % 60)
}

/// Parse "HH:MM" strings, returning (start_mins, end_mins).
fn parse_slot(start: &str, end: &str) -> Result<(i32, i32), AppError> {
    let s = time_to_minutes(start)
        .ok_or_else(|| AppError::BadRequest("Invalid start time format".into()))?;
    let e = time_to_minutes(end)
        .ok_or_else(|| AppError::BadRequest("Invalid end time format".into()))?;
    if s >= e {
        return Err(AppError::BadRequest("Start time must be before end time".into()));
    }
    Ok((s, e))
}

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
/// If `room_id` is provided, also checks room conflicts.
/// Returns `true` if a conflict exists.
pub async fn check_conflict(
    pool: &SqlitePool,
    doctor_id: i64,
    appointment_date: &str,
    start_time: &str,
    end_time: &str,
    room_id: Option<i64>,
    exclude_appointment_id: Option<i64>,
) -> Result<bool, AppError> {
    let mut sql = String::from(
        "SELECT COUNT(*) FROM appointments
         WHERE doctor_id = ?
           AND appointment_date = ?
           AND status != 'cancelled'
           AND start_time < ? AND end_time > ?",
    );

    if room_id.is_some() {
        sql.push_str(" AND room_id = ?");
    }
    if exclude_appointment_id.is_some() {
        sql.push_str(" AND id != ?");
    }

    let mut query = sqlx::query_as::<_, (i64,)>(&sql)
        .bind(doctor_id)
        .bind(appointment_date)
        .bind(end_time)   // A_start < B_end → start_time < end_time (new end)
        .bind(start_time); // A_end > B_start → end_time > start_time (new start)

    if let Some(rid) = room_id {
        query = query.bind(rid);
    }
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
    let duration = form.duration_minutes;
    if duration <= 0 || duration > 480 {
        return Err(AppError::BadRequest("Duration must be 1–480 minutes".into()));
    }

    // Fetch existing appointments for the doctor on that date, sorted by start time.
    // Also include any room conflicts if a room was specified.
    let existing = if let Some(rid) = form.room_id {
        sqlx::query_as::<_, (String, String)>(
            "SELECT start_time, end_time FROM appointments
             WHERE (doctor_id = ? OR room_id = ?) AND appointment_date = ?
               AND status != 'cancelled'
             ORDER BY start_time",
        )
        .bind(form.doctor_id)
        .bind(rid)
        .bind(&form.appointment_date)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, (String, String)>(
            "SELECT start_time, end_time FROM appointments
             WHERE doctor_id = ? AND appointment_date = ? AND status != 'cancelled'
             ORDER BY start_time",
        )
        .bind(form.doctor_id)
        .bind(&form.appointment_date)
        .fetch_all(pool)
        .await?
    };

    // Work hours: 08:00 to 17:00
    let day_start = 8 * 60;   // 480 mins
    let day_end = 17 * 60;    // 1020 mins

    let mut cursor = day_start;

    for (start, end) in &existing {
        let s = time_to_minutes(start).unwrap_or(0);
        let e = time_to_minutes(end).unwrap_or(0);

        // Gap before this appointment
        if cursor + duration <= s {
            return Ok(Some(minutes_to_time(cursor)));
        }
        // Move cursor past this appointment
        if e > cursor {
            cursor = e;
        }
    }

    // Gap at the end of the day
    if cursor + duration <= day_end {
        return Ok(Some(minutes_to_time(cursor)));
    }

    // No slot found
    Ok(None)
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

// BinaryHeap is a max-heap; we want the most urgent (lowest priority number)
// to come out first, so we reverse the comparison.
impl Ord for PriorityItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other.priority.cmp(&self.priority) // reversed: lower priority # wins
            .then_with(|| self.created_at.cmp(&other.created_at)) // tie-break: oldest first
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
pub async fn build_priority_queue(
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
    let (_, _) = parse_slot(&form.start_time, &form.end_time)?;

    let new_priority = form.priority.unwrap_or(3);

    // Only Emergency (1) or Urgent (2) can bump other appointments
    if new_priority > 2 {
        return Err(AppError::BadRequest(
            "Priority override is only available for Emergency or Urgent appointments.\
             \nUse standard booking for Normal or Follow-up visits."
                .into(),
        ));
    }

    // Look up patient
    let patient = sqlx::query_as::<_, (i64,)>("SELECT id FROM patients WHERE user_id = ?")
        .bind(patient_user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::BadRequest("Patient profile not found".into()))?;
    let patient_id = patient.0;

    // Check for conflicts
    let has_conflict = check_conflict(
        pool, form.doctor_id, &form.appointment_date,
        &form.start_time, &form.end_time, form.room_id, None,
    ).await?;

    if !has_conflict {
        return insert_appointment(
            pool, patient_id, form.doctor_id, &form.appointment_date,
            &form.start_time, &form.end_time, new_priority,
            form.room_id, &form.notes,
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

    // Verify new appointment has higher priority than ALL conflicting ones
    let can_bump = conflicts.iter().all(|(_, pri, _, _, _)| new_priority < *pri);

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
        sqlx::query(
            "INSERT INTO waitlist (patient_id, doctor_id, room_id, appointment_date,
             requested_start, requested_end, priority, notes, status)
             SELECT patient_id, doctor_id, room_id, appointment_date,
                    start_time, end_time, priority, ?, 'waiting'
             FROM appointments WHERE id = ?",
        )
        .bind(c_notes)
        .bind(conflict_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query("UPDATE appointments SET status = 'cancelled' WHERE id = ?")
            .bind(conflict_id)
            .execute(&mut *tx)
            .await?;
    }

    // Book the new appointment inside the same transaction
    let appointment = sqlx::query_as::<_, Appointment>(
        "INSERT INTO appointments (patient_id, doctor_id, appointment_date,
         start_time, end_time, status, priority, room_id, notes)
         VALUES (?, ?, ?, ?, ?, 'scheduled', ?, ?, ?)
         RETURNING id, patient_id, doctor_id, appointment_date,
                   start_time, end_time, status, notes, created_at,
                   room_id, priority",
    )
    .bind(patient_id)
    .bind(form.doctor_id)
    .bind(&form.appointment_date)
    .bind(&form.start_time)
    .bind(&form.end_time)
    .bind(new_priority)
    .bind(form.room_id)
    .bind(&form.notes)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(appointment)
}

// ============================================================
// Simple booking (no priority bumping)
// ============================================================

/// Book an appointment after checking conflicts.
pub async fn book_appointment(
    pool: &SqlitePool,
    patient_user_id: i64,
    form: &BookAppointmentForm,
) -> Result<Appointment, AppError> {
    let (_, _) = parse_slot(&form.start_time, &form.end_time)?;

    let patient = sqlx::query_as::<_, (i64,)>("SELECT id FROM patients WHERE user_id = ?")
        .bind(patient_user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::BadRequest("Patient profile not found".into()))?;
    let patient_id = patient.0;

    let priority = form.priority.unwrap_or(3);

    let has_conflict = check_conflict(
        pool, form.doctor_id, &form.appointment_date,
        &form.start_time, &form.end_time, form.room_id, None,
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
        form.room_id, &form.notes,
    ).await
}

/// Raw INSERT helper used by both booking paths.
async fn insert_appointment(
    pool: &SqlitePool,
    patient_id: i64,
    doctor_id: i64,
    date: &str,
    start: &str,
    end: &str,
    priority: i32,
    room_id: Option<i64>,
    notes: &Option<String>,
) -> Result<Appointment, AppError> {
    Ok(sqlx::query_as::<_, Appointment>(
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
    .fetch_one(pool)
    .await?)
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
    let patient = sqlx::query_as::<_, (i64,)>("SELECT id FROM patients WHERE user_id = ?")
        .bind(patient_user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::BadRequest("Patient profile not found".into()))?;

    Ok(sqlx::query_as::<_, WaitlistEntry>(
        "INSERT INTO waitlist (patient_id, doctor_id, room_id, appointment_date,
         requested_start, requested_end, priority, notes, status)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'waiting')
         RETURNING id, patient_id, doctor_id, room_id, appointment_date,
                   requested_start, requested_end, priority, notes, status, created_at",
    )
    .bind(patient.0)
    .bind(form.doctor_id)
    .bind(form.room_id)
    .bind(&form.appointment_date)
    .bind(&form.requested_start)
    .bind(&form.requested_end)
    .bind(form.priority)
    .bind(&form.notes)
    .fetch_one(pool)
    .await?)
}

/// Get waitlist for a doctor.
pub async fn get_waitlist_for_doctor(
    pool: &SqlitePool,
    doctor_id: i64,
) -> Result<Vec<WaitlistEntry>, AppError> {
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
    let heap = build_priority_queue(pool, doctor_id, appointment_date).await?;

    for item in heap.into_sorted_vec() {
        let entry = sqlx::query_as::<_, WaitlistEntry>(
            "SELECT * FROM waitlist WHERE id = ? AND status = 'waiting'",
        )
        .bind(item.waitlist_id)
        .fetch_optional(pool)
        .await?;

        if let Some(entry) = entry {
            // Use TimeSlotted trait to check conflict before querying DB
            let conflict = check_conflict(
                pool, entry.doctor_id,
                appointment_date,
                entry.start_time(), // from TimeSlotted trait
                entry.end_time(),   // from TimeSlotted trait
                entry.room_id, None,
            ).await?;

            if !conflict {
                let appt = insert_appointment(
                    pool, entry.patient_id, entry.doctor_id,
                    appointment_date,
                    entry.start_time(), entry.end_time(),
                    entry.priority_level(), // from Prioritized trait
                    entry.room_id, &entry.notes,
                ).await?;

                sqlx::query("UPDATE waitlist SET status = 'accepted' WHERE id = ?")
                    .bind(item.waitlist_id)
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
    let entry = sqlx::query_as::<_, WaitlistEntry>(
        "SELECT * FROM waitlist WHERE id = ? AND status = 'waiting'"
    )
    .bind(waitlist_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Waitlist entry not found".into()))?;

    // Check if slot is now free
    let date_str = entry.appointment_date.format("%Y-%m-%d").to_string();
    let conflict = check_conflict(
        pool, entry.doctor_id,
        &date_str,
        &entry.requested_start, &entry.requested_end,
        entry.room_id, None,
    ).await?;

    if conflict {
        return Ok(None); // slot still taken
    }

    // Book it
    let appt = insert_appointment(
        pool, entry.patient_id, entry.doctor_id,
        &date_str,
        &entry.requested_start, &entry.requested_end,
        entry.priority, entry.room_id, &entry.notes,
    ).await?;

    // Mark waitlist entry as accepted
    sqlx::query("UPDATE waitlist SET status = 'accepted' WHERE id = ?")
        .bind(waitlist_id)
        .execute(pool)
        .await?;

    Ok(Some(appt))
}

// ============================================================
// Queries (updated for rooms + priority)
// ============================================================

/// Get all appointments for a patient.
pub async fn get_appointments_for_patient(
    pool: &SqlitePool,
    patient_user_id: i64,
) -> Result<Vec<AppointmentView>, AppError> {
    let rows = sqlx::query_as::<_, (i64, String, String, chrono::NaiveDate, String, String, String, Option<String>, Option<String>, i32)>(
        "SELECT a.id, u_p.full_name AS patient_name, u_d.full_name AS doctor_name,
                a.appointment_date, a.start_time, a.end_time, a.status, a.notes,
                r.name AS room_name, a.priority
         FROM appointments a
         JOIN patients p ON a.patient_id = p.id
         JOIN users u_p ON p.user_id = u_p.id
         JOIN doctors d ON a.doctor_id = d.id
         JOIN users u_d ON d.user_id = u_d.id
         LEFT JOIN rooms r ON a.room_id = r.id
         WHERE p.user_id = ?
         ORDER BY a.appointment_date DESC, a.start_time",
    )
    .bind(patient_user_id)
    .fetch_all(pool)
    .await?;

    Ok(map_to_views(rows))
}

/// Get all appointments for a doctor.
pub async fn get_appointments_for_doctor(
    pool: &SqlitePool,
    doctor_user_id: i64,
) -> Result<Vec<AppointmentView>, AppError> {
    let rows = sqlx::query_as::<_, (i64, String, String, chrono::NaiveDate, String, String, String, Option<String>, Option<String>, i32)>(
        "SELECT a.id, u_p.full_name AS patient_name, u_d.full_name AS doctor_name,
                a.appointment_date, a.start_time, a.end_time, a.status, a.notes,
                r.name AS room_name, a.priority
         FROM appointments a
         JOIN patients p ON a.patient_id = p.id
         JOIN users u_p ON p.user_id = u_p.id
         JOIN doctors d ON a.doctor_id = d.id
         JOIN users u_d ON d.user_id = u_d.id
         LEFT JOIN rooms r ON a.room_id = r.id
         WHERE u_d.id = ?
         ORDER BY a.appointment_date DESC, a.start_time",
    )
    .bind(doctor_user_id)
    .fetch_all(pool)
    .await?;

    Ok(map_to_views(rows))
}

/// Get all appointments (admin).
pub async fn get_all_appointments(pool: &SqlitePool) -> Result<Vec<AppointmentView>, AppError> {
    let rows = sqlx::query_as::<_, (i64, String, String, chrono::NaiveDate, String, String, String, Option<String>, Option<String>, i32)>(
        "SELECT a.id, u_p.full_name AS patient_name, u_d.full_name AS doctor_name,
                a.appointment_date, a.start_time, a.end_time, a.status, a.notes,
                r.name AS room_name, a.priority
         FROM appointments a
         JOIN patients p ON a.patient_id = p.id
         JOIN users u_p ON p.user_id = u_p.id
         JOIN doctors d ON a.doctor_id = d.id
         JOIN users u_d ON d.user_id = u_d.id
         LEFT JOIN rooms r ON a.room_id = r.id
         ORDER BY a.appointment_date DESC, a.start_time",
    )
    .fetch_all(pool)
    .await?;

    Ok(map_to_views(rows))
}

/// Get a single appointment by ID.
pub async fn get_appointment_by_id(
    pool: &SqlitePool,
    appointment_id: i64,
) -> Result<AppointmentView, AppError> {
    let row = sqlx::query_as::<_, (i64, String, String, chrono::NaiveDate, String, String, String, Option<String>, Option<String>, i32)>(
        "SELECT a.id, u_p.full_name AS patient_name, u_d.full_name AS doctor_name,
                a.appointment_date, a.start_time, a.end_time, a.status, a.notes,
                r.name AS room_name, a.priority
         FROM appointments a
         JOIN patients p ON a.patient_id = p.id
         JOIN users u_p ON p.user_id = u_p.id
         JOIN doctors d ON a.doctor_id = d.id
         JOIN users u_d ON d.user_id = u_d.id
         LEFT JOIN rooms r ON a.room_id = r.id
         WHERE a.id = ?",
    )
    .bind(appointment_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Appointment not found".into()))?;

    Ok(map_to_views(vec![row]).pop().unwrap())
}

fn map_to_views(
    rows: Vec<(i64, String, String, chrono::NaiveDate, String, String, String, Option<String>, Option<String>, i32)>,
) -> Vec<AppointmentView> {
    rows.into_iter()
        .map(|(id, pn, dn, ad, st, et, status, notes, room, pri)| AppointmentView {
            id, patient_name: pn, doctor_name: dn,
            appointment_date: ad, start_time: st, end_time: et,
            status, notes, room_name: room, priority: pri,
        })
        .collect()
}

/// Cancel an appointment.
/// After cancellation, automatically attempts to promote the highest-priority
/// waitlist entry into the freed slot using the BinaryHeap priority queue.
pub async fn cancel_appointment(pool: &SqlitePool, appointment_id: i64) -> Result<(), AppError> {
    // Fetch the appointment first so we can use its date/doctor for waitlist promotion
    let appt = sqlx::query_as::<_, (i64, String, chrono::NaiveDate)>(
        "SELECT doctor_id, status, appointment_date FROM appointments WHERE id = ?",
    )
    .bind(appointment_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Appointment not found".into()))?;

    if appt.1 != "scheduled" {
        return Err(AppError::BadRequest("Appointment is already cancelled or completed".into()));
    }

    sqlx::query("UPDATE appointments SET status = 'cancelled' WHERE id = ?")
        .bind(appointment_id)
        .execute(pool)
        .await?;

    // Auto-promote the most urgent waitlist entry for the freed slot
    let date_str = appt.2.format("%Y-%m-%d").to_string();
    let _ = auto_promote_waitlist(pool, appt.0, &date_str).await;

    Ok(())
}

/// Cancel an appointment, enforcing ownership for patients.
/// Patients may only cancel their own appointments; doctors and admins can cancel any.
pub async fn cancel_appointment_checked(
    pool: &SqlitePool,
    appointment_id: i64,
    user_id: i64,
    role: &str,
) -> Result<(), AppError> {
    if role == "patient" {
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

/// Get all doctors for dropdowns.
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
