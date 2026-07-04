//! `DaySchedule` — the pure earliest-gap finder behind Algorithm 2.
//!
//! Holds the scheduling maths for a single day so the service layer only fetches
//! rows and maps the result, and so the gap-finding logic is unit-testable
//! without a database. This mirrors how interval-overlap detection lives on the
//! `TimeSlotted` trait — the scheduling algorithms sit on domain objects, not
//! inline in services.

/// A doctor's booked intervals for a single day, as `(start, end)` pairs in
/// minutes since midnight.
pub struct DaySchedule {
    busy: Vec<(i32, i32)>, // sorted by start time
}

impl DaySchedule {
    /// Build from booked intervals; sorts defensively so callers need not
    /// rely on the SQL `ORDER BY`.
    pub fn new(mut busy: Vec<(i32, i32)>) -> Self {
        busy.sort_by_key(|&(start, _)| start);
        Self { busy }
    }

    /// Earliest start time (minutes since midnight) with room for a
    /// `duration`-minute appointment between `open` and `close`, walking the
    /// gaps between booked intervals. Returns `None` if the day is full.
    pub fn earliest_gap(&self, duration: i32, open: i32, close: i32) -> Option<i32> {
        let mut cursor = open;
        for &(start, end) in &self.busy {
            // A gap wide enough sits before this interval.
            if cursor + duration <= start {
                return Some(cursor);
            }
            // Otherwise advance past this interval.
            if end > cursor {
                cursor = end;
            }
        }
        // Gap at the end of the day, if one remains.
        if cursor + duration <= close {
            Some(cursor)
        } else {
            None
        }
    }

    /// Is the window `[start, start + duration)` free of every booked interval?
    ///
    /// The per-slot counterpart of `earliest_gap`: `earliest_gap` finds the
    /// first open gap of a given length, while this answers "is *this specific*
    /// slot open?" — exactly what the live booking dropdown needs when it tests
    /// each 30-minute start in turn. Uses the same half-open overlap condition
    /// as `check_conflict` (`start < busy_end AND end > busy_start`).
    pub fn is_free(&self, start: i32, duration: i32) -> bool {
        let end = start + duration;
        !self.busy.iter().any(|&(bs, be)| start < be && end > bs)
    }
}

// ============================================================
// Unit tests for the DaySchedule earliest-gap algorithm
// ============================================================

#[cfg(test)]
mod tests {
    use super::DaySchedule;

    // Working day: 08:00 (480) .. 17:00 (1020), matching the clinic hours.
    const OPEN: i32 = 8 * 60;
    const CLOSE: i32 = 17 * 60;

    #[test]
    fn empty_day_returns_opening_time() {
        let day = DaySchedule::new(vec![]);
        assert_eq!(day.earliest_gap(30, OPEN, CLOSE), Some(OPEN));
    }

    #[test]
    fn gap_before_first_appointment() {
        // First booking at 09:00 leaves the 08:00 slot free.
        let day = DaySchedule::new(vec![(9 * 60, 10 * 60)]);
        assert_eq!(day.earliest_gap(30, OPEN, CLOSE), Some(OPEN));
    }

    #[test]
    fn finds_gap_between_appointments() {
        // 08:00–09:00 and 09:30–10:00 booked; a 30-min slot fits at 09:00.
        let day = DaySchedule::new(vec![(8 * 60, 9 * 60), (9 * 60 + 30, 10 * 60)]);
        assert_eq!(day.earliest_gap(30, OPEN, CLOSE), Some(9 * 60));
    }

    #[test]
    fn unsorted_input_is_sorted_defensively() {
        // Same intervals supplied out of order must give the same answer.
        let day = DaySchedule::new(vec![(9 * 60 + 30, 10 * 60), (8 * 60, 9 * 60)]);
        assert_eq!(day.earliest_gap(30, OPEN, CLOSE), Some(9 * 60));
    }

    #[test]
    fn gap_at_end_of_day() {
        // Booked solid until 16:30; a 30-min slot fits at 16:30.
        let day = DaySchedule::new(vec![(OPEN, 16 * 60 + 30)]);
        assert_eq!(day.earliest_gap(30, OPEN, CLOSE), Some(16 * 60 + 30));
    }

    #[test]
    fn full_day_returns_none() {
        let day = DaySchedule::new(vec![(OPEN, CLOSE)]);
        assert_eq!(day.earliest_gap(30, OPEN, CLOSE), None);
    }

    #[test]
    fn remaining_gap_too_short_returns_none() {
        // Only 15 minutes left before close — a 30-min slot cannot fit.
        let day = DaySchedule::new(vec![(OPEN, CLOSE - 15)]);
        assert_eq!(day.earliest_gap(30, OPEN, CLOSE), None);
    }

    // --- is_free: per-slot occupancy (drives the live booking dropdown) ---

    #[test]
    fn is_free_on_empty_day() {
        let day = DaySchedule::new(vec![]);
        assert!(day.is_free(10 * 60, 30));
    }

    #[test]
    fn is_free_false_when_slot_overlaps_a_booking() {
        // 10:00–10:30 booked: the 10:00 slot is taken, its neighbours are not.
        let day = DaySchedule::new(vec![(10 * 60, 10 * 60 + 30)]);
        assert!(!day.is_free(10 * 60, 30));
        assert!(day.is_free(9 * 60 + 30, 30)); // 09:30 ends exactly at 10:00
        assert!(day.is_free(10 * 60 + 30, 30)); // 10:30 starts exactly at the end
    }

    #[test]
    fn is_free_covers_every_slot_of_a_multi_slot_booking() {
        // 10:00–11:00 booked: both the 10:00 and 10:30 slots are taken.
        let day = DaySchedule::new(vec![(10 * 60, 11 * 60)]);
        assert!(!day.is_free(10 * 60, 30));
        assert!(!day.is_free(10 * 60 + 30, 30));
        assert!(day.is_free(11 * 60, 30));
    }
}
