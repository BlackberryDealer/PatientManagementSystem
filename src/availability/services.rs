use crate::auth::{AuthUser, Role};
use crate::availability::models::{
    AvailabilityListItem, DoctorAvailability, EditAvailabilityForm, SetAvailabilityForm,
};
use crate::db;
use crate::errors::AppError;
use crate::traits::{any_conflict, TimeSlotted, TimeWindow};
use chrono::Datelike;
use sqlx::SqlitePool;

// ============================================================
// Availability enforcement, integrates this module into the
// appointment-booking workflow (scheduling + conflict resolution)
// ============================================================

/// Enforce a doctor's availability rules for a requested time slot.
///
/// Rules, in order:
/// 1. **Blocked entries win.** If any blocked entry (one-off leave on that
///    exact date, or a recurring weekly block like a lunch break) overlaps
///    the requested window, the booking is rejected.
/// 2. **Closed by default.** A day with no declared working windows
///    (recurring weekly, or a one-off date entry) is not bookable at all:
///    a doctor who never published hours cannot receive appointments.
/// 3. **Defined windows constrain.** Where windows exist, the requested
///    slot must fall entirely inside one of them.
pub async fn ensure_doctor_available(
    pool: &SqlitePool,
    doctor_id: i64,
    appointment_date: &str,
    start_time: &str,
    end_time: &str,
) -> Result<(), AppError> {
    let rules = fetch_rules_for_day(pool, doctor_id, appointment_date).await?;

    // Rule 1: any overlapping blocked entry rejects the booking. The requested
    // window is lifted into the TimeSlotted world (`TimeWindow`) so the shared
    // polymorphic `any_conflict` check performs the overlap comparison.
    let requested = TimeWindow::new(start_time, end_time);
    if any_conflict(&requested, rules.iter().filter(|r| r.blocked())) {
        return Err(AppError::BadRequest(
            "The doctor is unavailable at that time (on leave or blocked).\
             \nPlease pick a different time or doctor, or use the suggestion feature."
                .into(),
        ));
    }

    // Rule 2: closed by default, no declared working windows for this day
    // means the day is simply not bookable, no matter the requested time.
    let windows: Vec<&DoctorAvailability> = rules.iter().filter(|r| !r.blocked()).collect();
    if windows.is_empty() {
        return Err(AppError::BadRequest(
            "The doctor has no working hours published for that day.\
             \nPlease pick a different day or doctor, or use the suggestion feature."
                .into(),
        ));
    }

    // Rule 3: the slot must sit entirely inside one of the declared windows.
    // Containment is a TimeSlotted domain method (sibling of overlaps_with
    // used by Rule 1 above), so the service never compares raw fields by hand.
    if !windows.iter().any(|w| w.contains(start_time, end_time)) {
        return Err(AppError::BadRequest(
            "The requested time is outside the doctor's working hours for that day.\
             \nCheck the doctor's availability and choose a time within their schedule."
                .into(),
        ));
    }

    Ok(())
}

/// Load every availability rule that applies to `appointment_date` for a
/// doctor: recurring weekly entries matching that weekday, plus any one-off
/// entries pinned to that exact date. Split out of `ensure_doctor_available`
/// so callers that check many slots for the same day (the free-slot API and
/// the batch-reassignment feasibility matrix) fetch the rules once and then
/// evaluate them in memory with `slot_allowed_by_rules`.
pub async fn fetch_rules_for_day(
    pool: &SqlitePool,
    doctor_id: i64,
    appointment_date: &str,
) -> Result<Vec<DoctorAvailability>, AppError> {
    let date = chrono::NaiveDate::parse_from_str(appointment_date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid appointment date".into()))?;
    // Schema convention: 0 = Sunday .. 6 = Saturday
    let day_of_week = date.weekday().num_days_from_sunday() as i32;

    Ok(sqlx::query_as::<_, DoctorAvailability>(
        "SELECT id, doctor_id, day_of_week, start_time, end_time, is_recurring, specific_date, is_blocked
         FROM doctor_availability
         WHERE doctor_id = ?
           AND ((is_recurring = 1 AND day_of_week = ?) OR specific_date = ?)",
    )
    .bind(doctor_id)
    .bind(day_of_week)
    .bind(appointment_date)
    .fetch_all(pool)
    .await?)
}

/// Pure, in-memory version of the availability gate: does the window
/// `[start_time, end_time)` satisfy this doctor's rules for the day?
///
/// Same three rules `ensure_doctor_available` enforces, minus the specific
/// error messages (callers here only need a yes/no):
/// 1. no overlapping blocked entry,
/// 2. at least one working window is declared for the day (closed by
///    default), and
/// 3. the slot sits entirely inside one of the declared windows.
///
/// Reuses the polymorphic `TimeSlotted` helpers (`any_conflict`, `contains`)
/// so the overlap and containment logic is never re-encoded by hand.
pub fn slot_allowed_by_rules(
    rules: &[DoctorAvailability],
    start_time: &str,
    end_time: &str,
) -> bool {
    let requested = TimeWindow::new(start_time, end_time);
    if any_conflict(&requested, rules.iter().filter(|r| r.blocked())) {
        return false;
    }
    let windows: Vec<&DoctorAvailability> = rules.iter().filter(|r| !r.blocked()).collect();
    !windows.is_empty() && windows.iter().any(|w| w.contains(start_time, end_time))
}

/// Shared SELECT for the availability list: a slot joined to its doctor's
/// display name, so the list page never falls back to showing a raw id.
const AVAILABILITY_LIST_SELECT: &str = "
    SELECT da.id, da.doctor_id, da.day_of_week, da.start_time, da.end_time,
           da.is_recurring, da.specific_date, da.is_blocked, u.full_name AS doctor_name
    FROM doctor_availability da
    JOIN doctors d ON da.doctor_id = d.id
    JOIN users u ON d.user_id = u.id";

/// List all availability slots for a doctor.
pub async fn get_availability_for_doctor(
    pool: &SqlitePool,
    doctor_user_id: i64,
) -> Result<Vec<AvailabilityListItem>, AppError> {
    let doctor_id = db::get_doctor_id(pool, doctor_user_id).await?;

    Ok(sqlx::query_as::<_, AvailabilityListItem>(&format!(
        "{AVAILABILITY_LIST_SELECT} WHERE da.doctor_id = ? ORDER BY da.day_of_week, da.start_time"
    ))
    .bind(doctor_id)
    .fetch_all(pool)
    .await?)
}

/// List all availability slots (admin view).
pub async fn get_all_availability(
    pool: &SqlitePool,
) -> Result<Vec<AvailabilityListItem>, AppError> {
    Ok(sqlx::query_as::<_, AvailabilityListItem>(&format!(
        "{AVAILABILITY_LIST_SELECT} ORDER BY da.doctor_id, da.day_of_week, da.start_time"
    ))
    .fetch_all(pool)
    .await?)
}

const AVAILABILITY_COLUMNS: &str =
    "id, doctor_id, day_of_week, start_time, end_time, is_recurring, specific_date, is_blocked";

/// Load one availability slot by id.
pub async fn get_availability_slot(
    pool: &SqlitePool,
    id: i64,
) -> Result<DoctorAvailability, AppError> {
    sqlx::query_as::<_, DoctorAvailability>(&format!(
        "SELECT {AVAILABILITY_COLUMNS} FROM doctor_availability WHERE id = ?"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Availability slot not found".into()))
}

/// Load one availability slot and verify the acting user may manage it:
/// admins manage any slot, a doctor only their own. The shared entry point
/// for the edit and delete flows.
pub async fn get_owned_slot(
    pool: &SqlitePool,
    user: &AuthUser,
    id: i64,
) -> Result<DoctorAvailability, AppError> {
    let slot = get_availability_slot(pool, id).await?;
    match user.role {
        Role::Admin => Ok(slot),
        Role::Doctor => {
            if db::get_doctor_id(pool, user.user_id).await? == slot.doctor_id {
                Ok(slot)
            } else {
                Err(AppError::Forbidden(
                    "You can only manage your own availability.".into(),
                ))
            }
        }
        Role::Patient => Err(AppError::Forbidden(
            "Availability is managed by doctors and admins.".into(),
        )),
    }
}

/// Every availability rule a doctor has, recurring and one-off alike, the
/// full rule set the mutation guards below evaluate hypothetical changes
/// against.
async fn get_rules_for_doctor(
    pool: &SqlitePool,
    doctor_id: i64,
) -> Result<Vec<DoctorAvailability>, AppError> {
    Ok(sqlx::query_as::<_, DoctorAvailability>(&format!(
        "SELECT {AVAILABILITY_COLUMNS} FROM doctor_availability WHERE doctor_id = ?"
    ))
    .bind(doctor_id)
    .fetch_all(pool)
    .await?)
}

/// Reject a new/edited window that overlaps an existing entry of the same
/// kind: recurring vs. recurring on the same weekday, or one-off vs. one-off
/// on the same date, with the same blocked flag. (A blocked entry *may*
/// overlap an available one, that is exactly how a lunch break inside a
/// working window is expressed.)
fn ensure_no_duplicate_window(
    existing: &[DoctorAvailability],
    day_of_week: i32,
    specific_date: Option<chrono::NaiveDate>,
    blocked: bool,
    start: &str,
    end: &str,
    exclude_id: Option<i64>,
) -> Result<(), AppError> {
    let clash = existing.iter().find(|r| {
        Some(r.id) != exclude_id
            && r.blocked() == blocked
            && match specific_date {
                None => r.recurring() && r.day_of_week == day_of_week,
                Some(date) => !r.recurring() && r.specific_date == Some(date),
            }
            && r.overlaps_with(start, end)
    });
    if let Some(r) = clash {
        return Err(AppError::BadRequest(format!(
            "This window overlaps your existing {} entry {}–{} for that {}.\
             \nEdit or delete that entry instead of adding an overlapping one.",
            if blocked { "blocked" } else { "available" },
            r.start_time,
            r.end_time,
            if specific_date.is_some() { "date" } else { "day" },
        )));
    }
    Ok(())
}

/// The core safety guard for every availability mutation: given the rule set
/// as it *would* look after the change, verify that no upcoming scheduled
/// appointment falls outside it. Without this, narrowing hours, deleting a
/// window, or adding leave would silently strand already-booked patients.
///
/// Reads `appointments` directly instead of calling into
/// `appointments::services`: this guard needs the live (date, start, end)
/// tuples for one doctor to replay against a hypothetical rule set, not any
/// entity-level operation that module exposes, and it must run inside the
/// same availability-mutation request as the write it is guarding.
async fn ensure_no_stranded_appointments(
    pool: &SqlitePool,
    doctor_id: i64,
    new_rules: &[DoctorAvailability],
) -> Result<(), AppError> {
    let today = chrono::Utc::now().date_naive();
    let upcoming: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT appointment_date, start_time, end_time FROM appointments
         WHERE doctor_id = ? AND status = 'scheduled' AND appointment_date >= ?
         ORDER BY appointment_date, start_time",
    )
    .bind(doctor_id)
    .bind(today.to_string())
    .fetch_all(pool)
    .await?;

    let mut stranded: Vec<(String, String, String)> = Vec::new();
    for (date_str, start, end) in upcoming {
        let Ok(date) = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d") else {
            continue;
        };
        let applicable: Vec<DoctorAvailability> = new_rules
            .iter()
            .filter(|r| r.applies_on(date))
            .map(|r| {
                DoctorAvailability::draft(
                    r.doctor_id, r.day_of_week, &r.start_time, &r.end_time,
                    r.recurring(), r.specific_date, r.blocked(),
                )
            })
            .collect();
        if !slot_allowed_by_rules(&applicable, &start, &end) {
            stranded.push((date_str, start, end));
        }
    }

    if let Some((date, start, end)) = stranded.first() {
        return Err(AppError::BadRequest(format!(
            "This change would leave {} upcoming appointment(s) outside your working hours \
             (first: {} {}–{}).\
             \nReschedule, reassign (use Batch Reassign to clear a whole day), or cancel \
             them first, then apply the change.",
            stranded.len(),
            date, start, end,
        )));
    }
    Ok(())
}

/// Add availability for a doctor. One submit may create several rows: the
/// recurring mode inserts one weekly rule per selected weekday, atomically.
/// Flow: validate → duplicate-window check → stranded-appointment guard →
/// persist inside a transaction.
pub async fn add_availability(
    pool: &SqlitePool,
    doctor_user_id: i64,
    form: &SetAvailabilityForm,
) -> Result<Vec<DoctorAvailability>, AppError> {
    // Validation, nothing touches the database before this passes
    let entries = form.entries()?;
    // Store the canonical zero-padded "HH:MM" form: the availability gate
    // compares these strings lexically against requested booking windows.
    let (start_mins, end_mins) = crate::time::parse_time_range(&form.start_time, &form.end_time)?;
    let (start, end) = (
        crate::time::minutes_to_time(start_mins),
        crate::time::minutes_to_time(end_mins),
    );
    let blocked = form.blocked();

    let doctor_id = db::get_doctor_id(pool, doctor_user_id).await?;
    let existing = get_rules_for_doctor(pool, doctor_id).await?;

    let mut hypothetical = existing;
    for e in &entries {
        ensure_no_duplicate_window(
            &hypothetical, e.day_of_week, e.specific_date, blocked, &start, &end, None,
        )?;
        hypothetical.push(DoctorAvailability::draft(
            doctor_id, e.day_of_week, &start, &end, e.recurring(), e.specific_date, blocked,
        ));
    }
    ensure_no_stranded_appointments(pool, doctor_id, &hypothetical).await?;

    let mut tx = pool.begin().await?;
    let mut created = Vec::with_capacity(entries.len());
    for e in &entries {
        created.push(
            sqlx::query_as::<_, DoctorAvailability>(&format!(
                "INSERT INTO doctor_availability
                 (doctor_id, day_of_week, start_time, end_time, is_recurring, specific_date, is_blocked)
                 VALUES (?, ?, ?, ?, ?, ?, ?)
                 RETURNING {AVAILABILITY_COLUMNS}"
            ))
            .bind(doctor_id)
            .bind(e.day_of_week)
            .bind(&start)
            .bind(&end)
            .bind(e.recurring() as i32) // SQLite stores BOOLEAN as INTEGER 0/1
            .bind(e.specific_date)
            .bind(blocked as i32)
            .fetch_one(&mut *tx)
            .await?,
        );
    }
    tx.commit().await?;
    Ok(created)
}

/// Update one availability slot (times, blocked flag, and its weekday or
/// date, the slot keeps its recurring/one-off kind). Guarded the same way
/// as adding: no duplicate window, and no upcoming appointment may be left
/// outside the resulting schedule.
pub async fn update_availability(
    pool: &SqlitePool,
    user: &AuthUser,
    id: i64,
    form: &EditAvailabilityForm,
) -> Result<DoctorAvailability, AppError> {
    let slot = get_owned_slot(pool, user, id).await?;
    let (day_of_week, specific_date) = form.validated_target(slot.recurring())?;
    let (start_mins, end_mins) = crate::time::parse_time_range(&form.start_time, &form.end_time)?;
    let (start, end) = (
        crate::time::minutes_to_time(start_mins),
        crate::time::minutes_to_time(end_mins),
    );
    let blocked = form.blocked();

    let existing = get_rules_for_doctor(pool, slot.doctor_id).await?;
    ensure_no_duplicate_window(
        &existing, day_of_week, specific_date, blocked, &start, &end, Some(slot.id),
    )?;

    // Simulate the rule set with this row replaced by its edited version.
    let mut hypothetical: Vec<DoctorAvailability> =
        existing.into_iter().filter(|r| r.id != slot.id).collect();
    hypothetical.push(DoctorAvailability::draft(
        slot.doctor_id, day_of_week, &start, &end, slot.recurring(), specific_date, blocked,
    ));
    ensure_no_stranded_appointments(pool, slot.doctor_id, &hypothetical).await?;

    Ok(sqlx::query_as::<_, DoctorAvailability>(&format!(
        "UPDATE doctor_availability
         SET day_of_week = ?, start_time = ?, end_time = ?, specific_date = ?, is_blocked = ?
         WHERE id = ?
         RETURNING {AVAILABILITY_COLUMNS}"
    ))
    .bind(day_of_week)
    .bind(&start)
    .bind(&end)
    .bind(specific_date)
    .bind(blocked as i32)
    .bind(slot.id)
    .fetch_one(pool)
    .await?)
}

/// Delete one availability slot. Removing a *working window* narrows the
/// schedule, so the stranded-appointment guard runs against the remaining
/// rules; removing a *blocked* entry only ever widens it and passes
/// trivially.
pub async fn delete_availability(
    pool: &SqlitePool,
    user: &AuthUser,
    id: i64,
) -> Result<(), AppError> {
    let slot = get_owned_slot(pool, user, id).await?;

    let remaining: Vec<DoctorAvailability> = get_rules_for_doctor(pool, slot.doctor_id)
        .await?
        .into_iter()
        .filter(|r| r.id != slot.id)
        .collect();
    ensure_no_stranded_appointments(pool, slot.doctor_id, &remaining).await?;

    sqlx::query("DELETE FROM doctor_availability WHERE id = ?")
        .bind(slot.id)
        .execute(pool)
        .await?;
    Ok(())
}
