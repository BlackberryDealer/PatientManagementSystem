//! `CalendarMonth` — the business object behind the monthly calendar view.
//!
//! Owns all calendar arithmetic (days-in-month, weekday offset, previous/next
//! month) so the route handler only extracts inputs and renders the result.

use serde::Serialize;

/// One cell in the calendar grid. Filler cells from the previous month
/// have `is_current_month = false` and `day = 0`.
#[derive(Debug, Serialize)]
pub struct CalendarDay {
    pub day: u32,
    pub date: String,
    pub is_today: bool,
    pub is_current_month: bool,
    pub count: usize,
}

/// A validated calendar month plus its week grid.
///
/// Construction validates the year/month, making an out-of-range month
/// unrepresentable downstream.
pub struct CalendarMonth {
    pub year: i32,
    pub month: u32,
    days: Vec<CalendarDay>,
}

impl CalendarMonth {
    const MONTH_NAMES: [&'static str; 12] = [
        "January", "February", "March", "April", "May", "June", "July",
        "August", "September", "October", "November", "December",
    ];

    /// Validation gate: rejects an out-of-range month/year with a 400
    /// instead of letting it panic deeper in date arithmetic.
    pub fn new(year: i32, month: u32) -> Result<Self, crate::errors::AppError> {
        if !(1..=12).contains(&month) || !(1970..=2100).contains(&year) {
            return Err(crate::errors::AppError::BadRequest(
                "Invalid calendar year or month".into(),
            ));
        }
        Ok(Self { year, month, days: Vec::new() })
    }

    fn first_day(&self) -> chrono::NaiveDate {
        // Safe: year/month were validated in `new`.
        chrono::NaiveDate::from_ymd_opt(self.year, self.month, 1).unwrap()
    }

    pub fn days_in_month(&self) -> u32 {
        use chrono::NaiveDate;
        let next = if self.month == 12 {
            NaiveDate::from_ymd_opt(self.year + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(self.year, self.month + 1, 1).unwrap()
        };
        next.signed_duration_since(self.first_day()).num_days() as u32
    }

    /// Inclusive `(from, to)` ISO date strings spanning the whole month,
    /// for querying appointment counts.
    pub fn date_range(&self) -> (String, String) {
        (
            format!("{}-{:02}-01", self.year, self.month),
            format!("{}-{:02}-{:02}", self.year, self.month, self.days_in_month()),
        )
    }

    /// Build the day grid: leading filler cells so the 1st lands on the
    /// correct weekday column (Monday-first), then one cell per day with
    /// its appointment count.
    pub fn build_grid(
        &mut self,
        today: chrono::NaiveDate,
        counts: &std::collections::HashMap<String, usize>,
    ) {
        use chrono::Datelike;
        self.days.clear();

        let start_dow = self.first_day().weekday().num_days_from_monday(); // 0=Mon
        for _ in 0..start_dow {
            self.days.push(CalendarDay {
                day: 0,
                date: String::new(),
                is_today: false,
                is_current_month: false,
                count: 0,
            });
        }

        let today_str = today.format("%Y-%m-%d").to_string();
        for d in 1..=self.days_in_month() {
            let date_str = format!("{}-{:02}-{:02}", self.year, self.month, d);
            self.days.push(CalendarDay {
                day: d,
                is_today: date_str == today_str,
                is_current_month: true,
                count: counts.get(&date_str).copied().unwrap_or(0),
                date: date_str,
            });
        }
    }

    /// The grid grouped into rows of seven for rendering.
    pub fn weeks(&self) -> Vec<&[CalendarDay]> {
        self.days.chunks(7).collect()
    }

    /// Previous month as (year, month).
    pub fn prev(&self) -> (i32, u32) {
        if self.month == 1 { (self.year - 1, 12) } else { (self.year, self.month - 1) }
    }

    /// Next month as (year, month).
    pub fn next(&self) -> (i32, u32) {
        if self.month == 12 { (self.year + 1, 1) } else { (self.year, self.month + 1) }
    }

    /// English month name ("January", "February", …).
    pub fn month_name(&self) -> &'static str {
        Self::MONTH_NAMES[(self.month - 1) as usize]
    }
}
