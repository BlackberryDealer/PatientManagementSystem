// HTTP handlers for the appointments module, split by workflow so each file
// stays focused: listing.rs has the read-only views (list, calendar, detail),
// booking.rs the booking form/POST and the live slot APIs, lifecycle.rs the
// per-appointment actions (complete, cancel, reschedule, reassign, room,
// priority), waitlist.rs the waitlist view/join/promote, and reassign_day.rs
// the batch "cover a doctor's leave" preview and apply (Algorithm 5). Every
// handler is re-exported here so the route table in appointments::configure
// can refer to them all as handlers::<name>.

mod booking;
mod lifecycle;
mod listing;
mod reassign_day;
mod waitlist;

pub use booking::{
    all_slots_api, available_slots_api, book_appointment, book_form, suggest_slot,
    suggest_slot_form,
};
pub use lifecycle::{
    assign_room, cancel_appointment, complete_appointment, reassign_appointment,
    reschedule_appointment, reschedule_form, update_priority,
};
pub use listing::{appointment_detail, calendar_view, list_appointments};
pub use reassign_day::{reassign_day_apply, reassign_day_form, reassign_day_preview};
pub use waitlist::{join_waitlist, list_waitlist, promote_waitlist};

/// Audit-log every appointment that was auto-rescheduled after a priority
/// override bumped its original slot. Shared by the booking and waitlist
/// promotion handlers, which both produce this same kind of side-effect list.
async fn audit_rescheduled_bumps(
    pool: &sqlx::SqlitePool,
    user: &crate::auth::AuthUser,
    rescheduled: &[crate::appointments::models::Appointment],
) {
    for r in rescheduled {
        crate::audit::services::record(
            pool, user, "appointment.auto_rescheduled", "appointment", Some(r.id),
            &format!("Auto-rescheduled to {} {}–{} after a priority override bumped the original slot",
                r.appointment_date, r.start_time, r.end_time),
        ).await;
    }
}
