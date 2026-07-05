use crate::traits::TimeSlotted;
use serde::{Deserialize, Serialize};

/// Represents a doctor's availability slot, either a recurring
/// weekly time window or a one-off blocked/leave date.
///
/// `is_recurring` and `is_blocked` are private so callers use the
/// `recurring()` / `blocked()` accessors which return proper Rust
/// `bool` values instead of leaking SQLite's INTEGER 0/1 convention.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct DoctorAvailability {
    pub id: i64,
    pub doctor_id: i64,
    pub day_of_week: i32,       // 0=Sun .. 6=Sat
    pub start_time: String,     // HH:MM
    pub end_time: String,       // HH:MM
    is_recurring: i32,          // 1 = recurring weekly, 0 = one-off (private: use recurring())
    pub specific_date: Option<chrono::NaiveDate>,
    is_blocked: i32,            // 1 = blocked/unavailable, 0 = available (private: use blocked())
}

impl DoctorAvailability {
    /// SQLite stores BOOLEAN as INTEGER 0/1; expose proper bools so
    /// callers never compare against raw integers.
    pub fn recurring(&self) -> bool { self.is_recurring != 0 }
    pub fn blocked(&self) -> bool { self.is_blocked != 0 }

    /// An unsaved rule (id 0) used to evaluate "what would the schedule look
    /// like if this entry existed?" before writing anything. The
    /// stranded-appointment guard simulates the post-change rule set in
    /// memory with these.
    pub fn draft(
        doctor_id: i64,
        day_of_week: i32,
        start_time: &str,
        end_time: &str,
        recurring: bool,
        specific_date: Option<chrono::NaiveDate>,
        blocked: bool,
    ) -> Self {
        Self {
            id: 0,
            doctor_id,
            day_of_week,
            start_time: start_time.to_string(),
            end_time: end_time.to_string(),
            is_recurring: recurring as i32,
            specific_date,
            is_blocked: blocked as i32,
        }
    }

    /// Does this rule apply on the given calendar date? Recurring rules match
    /// by weekday; one-off rules match by exact date. This is the in-memory
    /// twin of the SQL predicate in `fetch_rules_for_day`, used when a caller
    /// already holds the doctor's full rule set.
    pub fn applies_on(&self, date: chrono::NaiveDate) -> bool {
        use chrono::Datelike;
        if self.recurring() {
            self.day_of_week == date.weekday().num_days_from_sunday() as i32
        } else {
            self.specific_date == Some(date)
        }
    }
}

impl TimeSlotted for DoctorAvailability {
    fn start_time(&self) -> &str { &self.start_time }
    fn end_time(&self) -> &str { &self.end_time }
}

/// One row of the availability list page: a slot plus its doctor's display
/// name, resolved via a join. The booking-conflict machinery keeps using the
/// bare `DoctorAvailability` (it never needs the name); this shape exists so
/// the list page never falls back to showing a raw doctor id.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AvailabilityListItem {
    pub id: i64,
    pub doctor_id: i64,
    pub day_of_week: i32,
    pub start_time: String,
    pub end_time: String,
    pub is_recurring: i32,
    pub specific_date: Option<chrono::NaiveDate>,
    pub is_blocked: i32,
    pub doctor_name: String,
}

/// One normalised entry produced from a validated availability form:
/// either a weekly recurring rule for `day_of_week`, or a one-off rule
/// pinned to `specific_date` (whose `day_of_week` is *derived from the
/// date*, never user-supplied, so the two can never disagree).
#[derive(Debug)]
pub struct NewAvailabilityEntry {
    pub day_of_week: i32,
    pub specific_date: Option<chrono::NaiveDate>,
}

impl NewAvailabilityEntry {
    pub fn recurring(&self) -> bool {
        self.specific_date.is_none()
    }
}

/// Form submitted by a doctor to set their availability.
///
/// Two mutually exclusive modes, selected by a radio button:
/// * `recurring`, one or more weekday checkboxes (`day_0`..`day_6`); a
///   single submit creates one weekly rule per ticked day, so "Mon–Fri
///   9–5" is one form post, not five.
/// * `oneoff`, a calendar date only. The weekday is computed from the
///   date on the server, so there is no separate day field to fall out
///   of sync with the picked date.
#[derive(Debug, Deserialize)]
pub struct SetAvailabilityForm {
    pub mode: String, // "recurring" | "oneoff"
    // Weekday checkboxes ("on" when ticked). serde_urlencoded cannot decode
    // repeated keys into a Vec, so each day is its own optional field.
    pub day_0: Option<String>,
    pub day_1: Option<String>,
    pub day_2: Option<String>,
    pub day_3: Option<String>,
    pub day_4: Option<String>,
    pub day_5: Option<String>,
    pub day_6: Option<String>,
    pub start_time: String,
    pub end_time: String,
    pub specific_date: Option<String>, // YYYY-MM-DD, one-off mode only
    pub is_blocked: Option<String>,    // "on" if blocked/leave
}

impl SetAvailabilityForm {
    /// HTML checkboxes send "on" when checked and are absent otherwise;
    /// that decoding detail belongs to the form, not the service layer.
    pub fn blocked(&self) -> bool {
        self.is_blocked.as_deref() == Some("on")
    }

    fn checked_days(&self) -> Vec<i32> {
        [
            &self.day_0, &self.day_1, &self.day_2, &self.day_3,
            &self.day_4, &self.day_5, &self.day_6,
        ]
        .iter()
        .enumerate()
        .filter(|(_, d)| d.as_deref() == Some("on"))
        .map(|(i, _)| i as i32)
        .collect()
    }

    /// Validate the form and normalise it into the entries to insert.
    /// Times must be well-formed with start strictly before end (windows
    /// are not forced onto the 30-minute booking grid, only appointments
    /// are). Recurring mode needs at least one day; one-off mode needs a
    /// valid, non-past date.
    pub fn entries(&self) -> Result<Vec<NewAvailabilityEntry>, crate::errors::AppError> {
        use crate::errors::AppError;
        crate::time::parse_time_range(&self.start_time, &self.end_time)?;

        match self.mode.as_str() {
            "recurring" => {
                let days = self.checked_days();
                if days.is_empty() {
                    return Err(AppError::BadRequest(
                        "Select at least one weekday for a recurring slot.".into(),
                    ));
                }
                Ok(days
                    .into_iter()
                    .map(|d| NewAvailabilityEntry { day_of_week: d, specific_date: None })
                    .collect())
            }
            "oneoff" => {
                let date = parse_future_date(self.specific_date.as_deref())?;
                Ok(vec![NewAvailabilityEntry {
                    day_of_week: weekday_of(date),
                    specific_date: Some(date),
                }])
            }
            _ => Err(AppError::BadRequest(
                "Choose either a recurring weekly slot or a specific date.".into(),
            )),
        }
    }
}

/// Form submitted to update a single existing availability slot. The slot
/// keeps its recurring/one-off kind: recurring rows edit the weekday,
/// one-off rows edit the date (weekday derived server-side, as on create).
#[derive(Debug, Deserialize)]
pub struct EditAvailabilityForm {
    pub day_of_week: Option<i32>,      // recurring rows only
    pub specific_date: Option<String>, // one-off rows only
    pub start_time: String,
    pub end_time: String,
    pub is_blocked: Option<String>,
}

impl EditAvailabilityForm {
    pub fn blocked(&self) -> bool {
        self.is_blocked.as_deref() == Some("on")
    }

    /// Validate against the row being edited and produce the new
    /// (day_of_week, specific_date) pair, preserving the row's kind.
    pub fn validated_target(
        &self,
        row_is_recurring: bool,
    ) -> Result<(i32, Option<chrono::NaiveDate>), crate::errors::AppError> {
        use crate::errors::AppError;
        crate::time::parse_time_range(&self.start_time, &self.end_time)?;

        if row_is_recurring {
            let day = self.day_of_week.ok_or_else(|| {
                AppError::BadRequest("Please choose a day of the week.".into())
            })?;
            if !(0..=6).contains(&day) {
                return Err(AppError::BadRequest(
                    "Day of week must be between 0 (Sunday) and 6 (Saturday)".into(),
                ));
            }
            Ok((day, None))
        } else {
            let date = parse_future_date(self.specific_date.as_deref())?;
            Ok((weekday_of(date), Some(date)))
        }
    }
}

/// Schema convention: 0 = Sunday .. 6 = Saturday.
pub fn weekday_of(date: chrono::NaiveDate) -> i32 {
    use chrono::Datelike;
    date.weekday().num_days_from_sunday() as i32
}

/// Parse a required YYYY-MM-DD field and reject dates already in the past.
/// Availability for a day that's over can never affect a booking.
fn parse_future_date(
    raw: Option<&str>,
) -> Result<chrono::NaiveDate, crate::errors::AppError> {
    use crate::errors::AppError;
    let raw = raw
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("Please pick a date for a one-off slot.".into()))?;
    let date = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|_| {
        AppError::BadRequest("Specific date must be a valid date in YYYY-MM-DD format".into())
    })?;
    if date < chrono::Utc::now().date_naive() {
        return Err(AppError::BadRequest(
            "The date for a one-off slot cannot be in the past.".into(),
        ));
    }
    Ok(date)
}
