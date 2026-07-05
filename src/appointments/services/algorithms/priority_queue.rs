// Algorithm 3 support: the waitlist priority queue (BinaryHeap). The
// priority-based booking path itself (bumping a lower-priority appointment)
// lives in services::booking; this file just owns the queue that decides
// which waiting patient gets promoted when a slot frees up, see
// services::waitlist.

use crate::errors::AppError;
use crate::traits::Prioritized;
use sqlx::SqlitePool;
use std::collections::BinaryHeap;

/// A waitlist item ordered by priority for the BinaryHeap.
/// Lower priority number = higher urgency (flipped for max-heap behaviour).
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PriorityItem {
    pub(crate) waitlist_id: i64,
    pub(crate) patient_id: i64,
    pub(crate) priority: i32,
    pub(crate) requested_start: String,
    pub(crate) requested_end: String,
    pub(crate) created_at: chrono::NaiveDateTime,
}

/// PriorityItem joins the same `Prioritized` family as Appointment and
/// WaitlistEntry, so the heap ordering below reuses the shared urgency
/// comparison instead of re-encoding "lower number wins" a second time.
impl Prioritized for PriorityItem {
    fn priority_level(&self) -> i32 { self.priority }
}

// BinaryHeap is a max-heap; `pop` yields the GREATEST element. We want the
// most urgent (lowest priority number), then the oldest, to pop first, so the
// winner must compare as the greatest. BOTH keys are reversed to achieve that.
impl Ord for PriorityItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        if self.is_higher_priority_than(other) {
            Ordering::Greater
        } else if other.is_higher_priority_than(self) {
            Ordering::Less
        } else {
            // Equal urgency: FIFO, the older entry must pop first.
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
pub(crate) async fn build_priority_queue(
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
