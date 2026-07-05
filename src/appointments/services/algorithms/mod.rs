// Scheduling algorithms for the appointments module, one file per algorithm
// so a maintainer can open exactly the one they need: conflict.rs is
// Algorithm 1 (overlap detection), earliest_slot.rs Algorithm 2 (gap
// walking), priority_queue.rs the BinaryHeap behind Algorithm 3,
// reassign.rs Algorithm 4 (greedy single-appointment move), batch_reassign.rs
// Algorithm 5 (Hungarian whole-day reassignment), and free_slots.rs the live
// free-slot lookup that drives the booking form. All of them are the
// orchestration layer, fetching rows and translating results, while the
// pure database-free maths lives on the domain objects in
// appointments::models (DaySchedule, CostMatrix) so it stays unit-testable.

mod batch_reassign;
mod conflict;
mod earliest_slot;
mod free_slots;
mod priority_queue;
mod reassign;

pub use batch_reassign::{apply_day_reassignment, plan_day_reassignment};
pub use conflict::check_conflict;
pub use earliest_slot::find_earliest_slot;
pub use free_slots::{all_slots, free_slots};
pub use reassign::reassign_appointment;

// Used by the waitlist service (a sibling module) to build the auto-promotion
// queue; not part of the crate's public API.
pub(crate) use priority_queue::build_priority_queue;
