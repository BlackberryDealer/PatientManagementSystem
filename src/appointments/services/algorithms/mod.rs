//! Scheduling algorithms for the appointments module.
//!
//! Each algorithm lives in its own file so a future maintainer can open exactly
//! the one they need. All of them are the *orchestration* layer: they fetch rows
//! and translate results, while the pure, database-free maths lives on the
//! domain objects in `appointments::models` (`DaySchedule` for Algorithm 2,
//! `CostMatrix` for Algorithm 5) so it stays unit-testable in isolation.
//!
//! | File                 | Algorithm                                            |
//! |----------------------|------------------------------------------------------|
//! | `conflict.rs`        | 1 — time-interval overlap detection                  |
//! | `earliest_slot.rs`   | 2 — earliest available slot (gap walking)            |
//! | `priority_queue.rs`  | 3 support — `BinaryHeap` waitlist priority queue     |
//! | `reassign.rs`        | 4 — greedy, load-balanced single-appointment move    |
//! | `batch_reassign.rs`  | 5 — optimal whole-day batch reassignment (Hungarian) |
//! | `free_slots.rs`      | live free-slot lookup that drives the booking form   |

mod batch_reassign;
mod conflict;
mod earliest_slot;
mod free_slots;
mod priority_queue;
mod reassign;

pub use batch_reassign::{apply_day_reassignment, plan_day_reassignment};
pub use conflict::check_conflict;
pub use earliest_slot::find_earliest_slot;
pub use free_slots::free_slots;
pub use reassign::reassign_appointment;

// Used by the waitlist service (a sibling module) to build the auto-promotion
// queue; not part of the crate's public API.
pub(crate) use priority_queue::build_priority_queue;
