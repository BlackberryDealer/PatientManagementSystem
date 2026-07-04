//! HTTP handlers for the appointments module, grouped by workflow so each file
//! stays focused. All handlers are re-exported here, so the route table in
//! `appointments::configure` refers to them as `handlers::<name>` unchanged.
//!
//! | File            | Handlers                                                      |
//! |-----------------|---------------------------------------------------------------|
//! | `listing.rs`    | list, calendar, single-appointment detail (read views)        |
//! | `booking.rs`    | booking form + POST, live slot API, slot suggestion           |
//! | `lifecycle.rs`  | complete / cancel / reschedule / reassign / room / priority   |
//! | `waitlist.rs`   | waitlist view, join, promote                                   |
//! | `reassign_day.rs`| batch "cover a doctor's leave" preview + apply (Algorithm 5)  |

mod booking;
mod lifecycle;
mod listing;
mod reassign_day;
mod waitlist;

pub use booking::{
    available_slots_api, book_appointment, book_form, suggest_slot, suggest_slot_form,
};
pub use lifecycle::{
    assign_room, cancel_appointment, complete_appointment, reassign_appointment,
    reschedule_appointment, reschedule_form, update_priority,
};
pub use listing::{appointment_detail, calendar_view, list_appointments};
pub use reassign_day::{reassign_day_apply, reassign_day_form, reassign_day_preview};
pub use waitlist::{join_waitlist, list_waitlist, promote_waitlist};
