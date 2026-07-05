// ============================================================
// Domain Traits, OOP Abstractions for Patient Management System
// ============================================================
// Rust uses traits (instead of classical inheritance) to share
// behaviour across types. These traits apply the OOP principles
// covered in Tutorial 05: encapsulation, abstraction, and
// polymorphism.

// ============================================================
// TimeSlotted, Abstraction over any time-windowed entity
// ============================================================

/// Any entity that occupies a contiguous time window (an appointment,
/// a doctor availability slot, or a waitlist request).
///
/// The default `overlaps_with` and `duration_minutes` methods give every
/// implementing type the same scheduling logic for free, this is the
/// Rust equivalent of inheriting shared behaviour through a base class.
pub trait TimeSlotted {
    fn start_time(&self) -> &str;
    fn end_time(&self) -> &str;

    /// Returns `true` when this slot overlaps with [other_start, other_end).
    ///
    /// Standard interval-overlap condition:
    ///   A overlaps B  iff  A.start < B.end  AND  A.end > B.start
    fn overlaps_with(&self, other_start: &str, other_end: &str) -> bool {
        self.start_time() < other_end && self.end_time() > other_start
    }

    /// Returns `true` when [other_start, other_end) sits entirely inside this
    /// slot, i.e. this window fully covers the requested one. Encapsulates the
    /// containment rule that constrains a booking to a doctor's declared working
    /// window, so the availability service no longer compares raw fields by hand.
    /// (Zero-padded "HH:MM" strings compare correctly.)
    fn contains(&self, other_start: &str, other_end: &str) -> bool {
        self.start_time() <= other_start && other_end <= self.end_time()
    }

    /// Returns the duration of this slot in minutes.
    fn duration_minutes(&self) -> i32 {
        fn to_mins(t: &str) -> i32 {
            let mut parts = t.splitn(2, ':');
            let h: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let m: i32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            h * 60 + m
        }
        to_mins(self.end_time()) - to_mins(self.start_time())
    }
}

/// Check whether `new_slot` conflicts with any entry in `existing`.
/// Demonstrates polymorphism: works for Appointment, WaitlistEntry, or
/// DoctorAvailability, any type that implements TimeSlotted. The
/// availability engine uses it to test a requested booking window against
/// a doctor's blocked entries (see `ensure_doctor_available`, Rule 1).
pub fn any_conflict<'a, T, I>(new_slot: &impl TimeSlotted, existing: I) -> bool
where
    T: TimeSlotted + 'a,
    I: IntoIterator<Item = &'a T>,
{
    existing
        .into_iter()
        .any(|e| new_slot.overlaps_with(e.start_time(), e.end_time()))
}

/// The simplest `TimeSlotted` implementor: a bare start/end pair.
///
/// Adapts a requested booking window, which arrives as two loose `&str`s
/// from a form, into the trait's world, so the same polymorphic checks
/// (`any_conflict`, `overlaps_with`, `contains`) that run on stored entities
/// also run on a request that has no entity yet.
pub struct TimeWindow<'a> {
    start: &'a str,
    end: &'a str,
}

impl<'a> TimeWindow<'a> {
    pub fn new(start: &'a str, end: &'a str) -> Self {
        Self { start, end }
    }
}

impl TimeSlotted for TimeWindow<'_> {
    fn start_time(&self) -> &str { self.start }
    fn end_time(&self) -> &str { self.end }
}

// ============================================================
// StatusManaged, Abstraction over lifecycle status
// ============================================================

/// Any entity with a lifecycle status (scheduled/cancelled, pending/paid…).
///
/// Polymorphism: Appointment interprets "scheduled" as active; Invoice
/// interprets "pending" as active, the same interface behaves differently
/// per concrete type.
pub trait StatusManaged {
    fn current_status(&self) -> &str;

    fn is_active(&self) -> bool;

    /// Returns a Bulma CSS badge class matching the current status.
    fn status_badge_class(&self) -> &str;

    /// The bare colour name behind the status badge (e.g. "info", "danger"),
    /// for consumers that need a colour without the Bulma `is-` prefix, such
    /// as the timeline marker. Keeping this here means the prefix-stripping
    /// detail lives with the badge logic, not inlined in the service layer.
    fn status_color(&self) -> &str {
        self.status_badge_class().trim_start_matches("is-")
    }
}

// ============================================================
// Reportable, Abstraction over summary generation
// ============================================================

/// Any entity that can produce a human-readable, single-line summary.
///
/// Polymorphism: Appointment, Invoice, and MedicalRecord all implement
/// this trait but produce different domain-relevant output.
pub trait Reportable {
    fn generate_summary(&self) -> String;
}

// ============================================================
// Priority Levels, single source of truth for triage mapping
// ============================================================

/// Priority levels matching hospital triage standards.
/// 1 = Emergency (life-threatening), 2 = Urgent, 3 = Normal, 4 = Follow-up.
///
/// This is the **single source of truth** for priority→label and
/// priority→CSS mapping. The `Prioritized` trait defaults delegate here
/// so the mapping is never duplicated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Priority {
    Emergency = 1,
    Urgent = 2,
    Normal = 3,
    FollowUp = 4,
}

impl Priority {
    /// Convert from the integer stored in the database.
    /// Values outside 1–4 fall back to Normal (fail-safe).
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => Priority::Emergency,
            2 => Priority::Urgent,
            4 => Priority::FollowUp,
            _ => Priority::Normal,
        }
    }

    /// Human-readable label for display in badges and dropdowns.
    pub fn label(&self) -> &'static str {
        match self {
            Priority::Emergency => "Emergency",
            Priority::Urgent => "Urgent",
            Priority::Normal => "Normal",
            Priority::FollowUp => "Follow-up",
        }
    }

    /// Bulma CSS class for the priority badge colour.
    /// Emergency=red, Urgent=orange, Normal=blue, Follow-up=green.
    pub fn css_class(&self) -> &'static str {
        match self {
            Priority::Emergency => "is-danger",
            Priority::Urgent => "is-warning",
            Priority::Normal => "is-info",
            Priority::FollowUp => "is-success",
        }
    }

    /// Triage rule: only Emergency and Urgent cases may bump an existing
    /// booking out of its slot. Normal and Follow-up visits must use
    /// standard booking or join the waitlist.
    pub fn can_override(&self) -> bool {
        matches!(self, Priority::Emergency | Priority::Urgent)
    }

    /// Triage precedence: may a case at *this* priority bump one currently
    /// holding the slot at `occupant`? Only when strictly more urgent (lower
    /// number).
    pub fn outranks(&self, occupant: Priority) -> bool {
        (*self as i32) < (occupant as i32)
    }
}

// ============================================================
// Prioritized, Abstraction over numeric priority
// ============================================================

/// Any entity with a 1–4 priority level (1 = Emergency, 4 = Follow-up).
///
/// All label/badge logic delegates to the `Priority` enum, the single
/// source of truth for the priority→display mapping. Concrete types
/// supply only their own priority field via `priority_level`.
pub trait Prioritized {
    fn priority_level(&self) -> i32;

    /// Human-readable label (e.g. "Emergency"). Delegates to `Priority::label`.
    fn priority_label(&self) -> &str {
        Priority::from_i32(self.priority_level()).label()
    }

    /// Bulma CSS badge class (e.g. "is-danger"). Delegates to `Priority::css_class`.
    fn priority_badge_class(&self) -> &str {
        Priority::from_i32(self.priority_level()).css_class()
    }

    /// Returns `true` if this entity has higher urgency (lower number) than
    /// `other`. Drives the `BinaryHeap` ordering of the waitlist's
    /// `PriorityItem`, so the "lower number wins" rule is encoded once.
    fn is_higher_priority_than(&self, other: &dyn Prioritized) -> bool {
        self.priority_level() < other.priority_level()
    }
}

// ============================================================
// Unit tests for all trait default methods
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct Slot { start: &'static str, end: &'static str }
    impl TimeSlotted for Slot {
        fn start_time(&self) -> &str { self.start }
        fn end_time(&self) -> &str { self.end }
    }

    struct PriItem(i32);
    impl Prioritized for PriItem {
        fn priority_level(&self) -> i32 { self.0 }
    }

    struct Statused(&'static str);
    impl StatusManaged for Statused {
        fn current_status(&self) -> &str { self.0 }
        fn is_active(&self) -> bool { self.0 == "active" }
        fn status_badge_class(&self) -> &str {
            if self.0 == "active" { "is-success" } else { "is-danger" }
        }
    }

    // --- TimeSlotted::overlaps_with ---

    #[test]
    fn overlap_partial() {
        // A (10:00–10:30) overlaps B (10:15–10:45)
        let a = Slot { start: "10:00", end: "10:30" };
        assert!(a.overlaps_with("10:15", "10:45"));
    }

    #[test]
    fn no_overlap_adjacent() {
        // A ends exactly when B starts, boundary is exclusive
        let a = Slot { start: "10:00", end: "10:30" };
        assert!(!a.overlaps_with("10:30", "11:00"));
    }

    #[test]
    fn no_overlap_entirely_before() {
        let a = Slot { start: "09:00", end: "09:30" };
        assert!(!a.overlaps_with("10:00", "10:30"));
    }

    #[test]
    fn no_overlap_entirely_after() {
        let a = Slot { start: "11:00", end: "11:30" };
        assert!(!a.overlaps_with("09:00", "10:00"));
    }

    #[test]
    fn overlap_contained_within() {
        // B fits entirely inside A
        let a = Slot { start: "09:00", end: "12:00" };
        assert!(a.overlaps_with("10:00", "11:00"));
    }

    #[test]
    fn overlap_containing() {
        // A fits entirely inside B
        let a = Slot { start: "10:00", end: "10:30" };
        assert!(a.overlaps_with("09:00", "12:00"));
    }

    // --- TimeSlotted::contains ---

    #[test]
    fn contains_inner_slot() {
        // Window 09:00–12:00 fully covers a 10:00–11:00 booking.
        let window = Slot { start: "09:00", end: "12:00" };
        assert!(window.contains("10:00", "11:00"));
    }

    #[test]
    fn contains_exact_bounds_is_inclusive() {
        // A booking flush against both edges of the window still fits.
        let window = Slot { start: "09:00", end: "12:00" };
        assert!(window.contains("09:00", "12:00"));
    }

    #[test]
    fn contains_rejects_slot_spilling_past_end() {
        let window = Slot { start: "09:00", end: "12:00" };
        assert!(!window.contains("11:30", "12:30"));
    }

    #[test]
    fn contains_rejects_slot_starting_before_window() {
        let window = Slot { start: "09:00", end: "12:00" };
        assert!(!window.contains("08:30", "10:00"));
    }

    #[test]
    fn duration_thirty_minutes() {
        assert_eq!(Slot { start: "10:00", end: "10:30" }.duration_minutes(), 30);
    }

    #[test]
    fn duration_one_hour() {
        assert_eq!(Slot { start: "09:00", end: "10:00" }.duration_minutes(), 60);
    }

    #[test]
    fn duration_full_working_day() {
        assert_eq!(Slot { start: "08:00", end: "17:00" }.duration_minutes(), 540);
    }

    // --- any_conflict ---

    #[test]
    fn any_conflict_detects_overlap() {
        let new = Slot { start: "10:15", end: "10:45" };
        let existing = [
            Slot { start: "09:00", end: "09:30" },
            Slot { start: "10:00", end: "10:30" },
        ];
        assert!(any_conflict(&new, &existing));
    }

    #[test]
    fn any_conflict_no_overlap() {
        let new = Slot { start: "11:00", end: "11:30" };
        let existing = [
            Slot { start: "09:00", end: "09:30" },
            Slot { start: "10:00", end: "10:30" },
        ];
        assert!(!any_conflict(&new, &existing));
    }

    #[test]
    fn any_conflict_empty_schedule() {
        let new = Slot { start: "10:00", end: "10:30" };
        let existing: [Slot; 0] = [];
        assert!(!any_conflict(&new, &existing));
    }

    // --- Prioritized ---

    #[test]
    fn priority_labels_all_levels() {
        assert_eq!(PriItem(1).priority_label(), "Emergency");
        assert_eq!(PriItem(2).priority_label(), "Urgent");
        assert_eq!(PriItem(3).priority_label(), "Normal");
        assert_eq!(PriItem(4).priority_label(), "Follow-up");
        // Out-of-range values fall back to Normal (same as Priority::from_i32)
        assert_eq!(PriItem(99).priority_label(), "Normal");
    }

    #[test]
    fn priority_badge_classes() {
        assert_eq!(PriItem(1).priority_badge_class(), "is-danger");
        assert_eq!(PriItem(2).priority_badge_class(), "is-warning");
        assert_eq!(PriItem(3).priority_badge_class(), "is-info");
        assert_eq!(PriItem(4).priority_badge_class(), "is-success");
    }

    #[test]
    fn higher_priority_wins_comparison() {
        assert!(PriItem(1).is_higher_priority_than(&PriItem(3)));
        assert!(!PriItem(3).is_higher_priority_than(&PriItem(1)));
    }

    #[test]
    fn equal_priority_is_not_higher() {
        assert!(!PriItem(2).is_higher_priority_than(&PriItem(2)));
    }

    // --- StatusManaged ---

    #[test]
    fn status_active_returns_true() {
        let s = Statused("active");
        assert!(s.is_active());
        assert_eq!(s.status_badge_class(), "is-success");
    }

    #[test]
    fn status_inactive_returns_false() {
        let s = Statused("cancelled");
        assert!(!s.is_active());
        assert_eq!(s.status_badge_class(), "is-danger");
    }
}
