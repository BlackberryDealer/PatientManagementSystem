//! Domain models for the appointments module, grouped by concern so each file
//! stays small and focused:
//!
//! | File            | Contents                                                     |
//! |-----------------|--------------------------------------------------------------|
//! | `appointment.rs`| `Appointment`, its status enum, the joined `AppointmentView` |
//! | `waitlist.rs`   | `WaitlistEntry`, its status enum, and the join form          |
//! | `room.rs`       | `Room` and the daily `DoctorRoomAssignment`                  |
//! | `forms.rs`      | HTTP request/response DTOs for the handlers                  |
//! | `scheduling.rs` | `DaySchedule` — pure gap-finding maths (Algorithm 2)         |
//! | `assignment.rs` | `CostMatrix` — pure Hungarian solver + plan types (Algo 5)   |
//! | `calendar.rs`   | `CalendarMonth` — the monthly calendar business object       |
//!
//! Lifecycle status is modelled with typed enums rather than raw strings, so
//! illegal states are unrepresentable and every `match` on status stays
//! exhaustive. The pure scheduling maths (`DaySchedule`, `CostMatrix`) lives
//! here as database-free domain objects so it can be unit-tested in isolation,
//! mirroring how interval-overlap logic sits on the `TimeSlotted` trait.

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
