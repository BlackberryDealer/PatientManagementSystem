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
