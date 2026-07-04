//! Consultation rooms and the daily doctor-to-room allocation.

use serde::{Deserialize, Serialize};

/// A consultation room or equipment resource (e.g. X-ray suite, lab).
/// Six rooms are seeded in migration 002: three consultation rooms,
/// a procedure room, an X-ray suite, and a lab.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Room {
    pub id: i64,
    pub name: String,
    pub room_type: String,
    pub floor: Option<String>,
    pub is_active: i32,
    pub notes: Option<String>,
}

/// Each doctor is assigned exactly one room per day. Patients no longer
/// choose a room when booking — the system resolves it automatically from
/// this assignment. If no explicit assignment exists, the first booking of
/// the day auto-claims any free room for that doctor.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct DoctorRoomAssignment {
    pub id: i64,
    pub doctor_id: i64,
    pub room_id: i64,
    pub assignment_date: chrono::NaiveDate,
}
