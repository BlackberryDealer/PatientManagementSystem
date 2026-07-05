mod algorithms;
mod booking;
mod helpers;
mod queries;
mod rooms;
mod waitlist;

pub use algorithms::{
    all_slots, apply_day_reassignment, check_conflict, find_earliest_slot, free_slots,
    plan_day_reassignment, reassign_appointment,
};
pub use booking::{book_appointment, book_with_priority, resolve_booking_target};
pub use queries::{
    assign_room,
    cancel_appointment, cancel_appointment_checked,
    complete_appointment,
    get_all_appointment_counts,
    get_all_appointments,
    get_appointment_by_id, get_appointment_by_id_checked,
    get_appointment_counts_for_doctor, get_appointment_counts_for_patient,
    get_appointment_options_for_patient,
    get_appointments_for_doctor, get_appointments_for_patient, get_appointments_for_patient_id,
    reschedule_appointment, reschedule_appointment_checked,
    set_priority,
};
pub use rooms::{get_all_doctors, get_all_rooms};
pub use waitlist::{
    add_to_waitlist, auto_promote_waitlist, expire_stale_waitlist, get_all_waitlist,
    get_waitlist_for_doctor, get_waitlist_for_patient,
    promote_from_waitlist, try_rebook_bumped, PromotionOutcome,
};

// Grid slot options for the booking UI, pure functions, no DB access.
use crate::time::{minutes_to_time, CLINIC_CLOSE_MINUTES, CLINIC_OPEN_MINUTES, SLOT_MINUTES};

pub fn start_time_slots() -> Vec<String> {
    (CLINIC_OPEN_MINUTES..CLINIC_CLOSE_MINUTES)
        .step_by(SLOT_MINUTES as usize)
        .map(minutes_to_time)
        .collect()
}

pub fn end_time_slots() -> Vec<String> {
    ((CLINIC_OPEN_MINUTES + SLOT_MINUTES)..=CLINIC_CLOSE_MINUTES)
        .step_by(SLOT_MINUTES as usize)
        .map(minutes_to_time)
        .collect()
}
