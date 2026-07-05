// Domain models for the appointments module, split one file per concern:
// appointment/waitlist/room hold the entities and their status enums, forms.rs
// has the handler DTOs, and scheduling/assignment hold the pure gap-finding and
// Hungarian solver maths (Algorithms 2 and 5) with no database involved, so
// they can be unit tested on their own.
//
// Status fields use typed enums instead of raw strings, so an invalid status
// can't exist and every match on status stays exhaustive.

mod appointment;
mod assignment;
mod calendar;
mod forms;
mod room;
mod scheduling;
mod waitlist;

pub use appointment::{Appointment, AppointmentStatus, AppointmentView};
pub use assignment::{CostMatrix, ReassignPlan, ReassignRow};
pub use calendar::{CalendarDay, CalendarMonth};
pub use forms::{
    AllSlotsResponse, AssignRoomForm, BookAppointmentForm, BookRequestForm, FreeSlotsResponse,
    ReassignDayForm, RescheduleForm, SetPriorityForm, SlotAvailability, SuggestSlotForm,
};
pub use room::{DoctorRoomAssignment, Room};
pub use scheduling::DaySchedule;
pub use waitlist::{WaitlistEntry, WaitlistForm, WaitlistStatus};
