// ============================================================
// Domain Traits — OOP Abstractions for Patient Management System
// ============================================================
// Rust uses traits (instead of classical inheritance) to share
// behaviour across types. These traits apply the OOP principles
// covered in Tutorial 05: encapsulation, abstraction, and
// polymorphism.

// ============================================================
// TimeSlotted — Abstraction over any time-windowed entity
// ============================================================

/// Any entity that occupies a contiguous time window (an appointment,
/// a doctor availability slot, or a waitlist request).
///
/// The default `overlaps_with` and `duration_minutes` methods give every
/// implementing type the same scheduling logic for free — this is the
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
/// DoctorAvailability — any type that implements TimeSlotted.
pub fn any_conflict<T: TimeSlotted>(new_slot: &T, existing: &[impl TimeSlotted]) -> bool {
    existing
        .iter()
        .any(|e| new_slot.overlaps_with(e.start_time(), e.end_time()))
}

// ============================================================
// StatusManaged — Abstraction over lifecycle status
// ============================================================

/// Any entity with a lifecycle status (scheduled/cancelled, pending/paid…).
///
/// Polymorphism: Appointment interprets "scheduled" as active; Invoice
/// interprets "pending" as active — the same interface behaves differently
/// per concrete type.
pub trait StatusManaged {
    fn current_status(&self) -> &str;

    fn is_active(&self) -> bool;

    /// Returns a Bulma CSS badge class matching the current status.
    fn status_badge_class(&self) -> &str;

    /// The bare colour name behind the status badge (e.g. "info", "danger"),
    /// for consumers that need a colour without the Bulma `is-` prefix — such
    /// as the timeline marker. Keeping this here means the prefix-stripping
    /// detail lives with the badge logic, not inlined in the service layer.
    fn status_color(&self) -> &str {
        self.status_badge_class().trim_start_matches("is-")
    }
}

// ============================================================
// Reportable — Abstraction over summary generation
// ============================================================

/// Any entity that can produce a human-readable, single-line summary.
///
/// Polymorphism: Appointment, Invoice, and MedicalRecord all implement
/// this trait but produce different domain-relevant output.
pub trait Reportable {
    fn generate_summary(&self) -> String;
}

// ============================================================
// Prioritized — Abstraction over numeric priority
// ============================================================

/// Any entity with a 1–4 priority level (1 = Emergency, 4 = Follow-up).
///
/// Default methods provide shared label/badge logic; concrete types
/// supply only their own priority field via `priority_level`.
pub trait Prioritized {
    fn priority_level(&self) -> i32;

    fn priority_label(&self) -> &str {
        match self.priority_level() {
            1 => "Emergency",
            2 => "Urgent",
            3 => "Normal",
            4 => "Follow-up",
            _ => "Unknown",
        }
    }

    fn priority_badge_class(&self) -> &str {
        match self.priority_level() {
            1 => "is-danger",
            2 => "is-warning",
            3 => "is-info",
            4 => "is-success",
            _ => "is-light",
        }
    }

    /// Returns `true` if this entity has higher urgency (lower number) than `other`.
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
        // A ends exactly when B starts — boundary is exclusive
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
        assert_eq!(PriItem(99).priority_label(), "Unknown");
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
