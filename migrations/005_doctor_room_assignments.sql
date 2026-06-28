-- ============================================================
-- Migration 005: Doctor-Room auto-assignment support
-- ============================================================
-- Each doctor is assigned exactly one room per day. Patients no
-- longer choose a room when booking — the system resolves it
-- automatically from the doctor-room assignment for that date.
--
-- "At the start of each day, a doctor is allocated a room."
-- If no explicit assignment exists yet, the first booking of the
-- day auto-claims any free room for that doctor.
-- ============================================================

CREATE TABLE IF NOT EXISTS doctor_room_assignments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    doctor_id INTEGER NOT NULL,
    room_id INTEGER NOT NULL,
    assignment_date DATE NOT NULL,
    FOREIGN KEY (doctor_id) REFERENCES doctors(id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(id),
    UNIQUE(doctor_id, assignment_date)
);

-- Index for fast lookup by doctor+date (the hot path on every booking)
CREATE INDEX IF NOT EXISTS idx_dra_doctor_date
    ON doctor_room_assignments(doctor_id, assignment_date);
