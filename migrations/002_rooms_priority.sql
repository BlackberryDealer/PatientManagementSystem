-- ============================================================
-- Migration 002: Rooms, Priority Scheduling & Waitlist
-- ============================================================

-- Rooms: consultation rooms and equipment resources
CREATE TABLE IF NOT EXISTS rooms (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    room_type TEXT NOT NULL CHECK (room_type IN ('consultation', 'procedure', 'equipment', 'lab')),
    floor TEXT,
    is_active BOOLEAN NOT NULL DEFAULT 1,
    notes TEXT
);

-- Seed default rooms
INSERT OR IGNORE INTO rooms (id, name, room_type, floor) VALUES (1, 'Consultation Room A', 'consultation', 'Floor 1');
INSERT OR IGNORE INTO rooms (id, name, room_type, floor) VALUES (2, 'Consultation Room B', 'consultation', 'Floor 1');
INSERT OR IGNORE INTO rooms (id, name, room_type, floor) VALUES (3, 'Consultation Room C', 'consultation', 'Floor 2');
INSERT OR IGNORE INTO rooms (id, name, room_type, floor) VALUES (4, 'Procedure Room', 'procedure', 'Floor 2');
INSERT OR IGNORE INTO rooms (id, name, room_type, floor) VALUES (5, 'X-Ray Suite', 'equipment', 'Floor 3');
INSERT OR IGNORE INTO rooms (id, name, room_type, floor) VALUES (6, 'Lab Room', 'lab', 'Floor 3');

-- Add room and priority columns to appointments
ALTER TABLE appointments ADD COLUMN room_id INTEGER REFERENCES rooms(id);
ALTER TABLE appointments ADD COLUMN priority INTEGER NOT NULL DEFAULT 3;
-- priority: 1=Emergency, 2=Urgent, 3=Normal, 4=Follow-up

-- Waitlist: priority queue for patients waiting for slots
CREATE TABLE IF NOT EXISTS waitlist (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    patient_id INTEGER NOT NULL,
    doctor_id INTEGER NOT NULL,
    room_id INTEGER,
    appointment_date DATE NOT NULL,
    requested_start TEXT NOT NULL,  -- HH:MM
    requested_end TEXT NOT NULL,    -- HH:MM
    priority INTEGER NOT NULL CHECK (priority BETWEEN 1 AND 4),
    notes TEXT,
    status TEXT NOT NULL CHECK (status IN ('waiting', 'offered', 'accepted', 'expired')) DEFAULT 'waiting',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (patient_id) REFERENCES patients(id),
    FOREIGN KEY (doctor_id) REFERENCES doctors(id),
    FOREIGN KEY (room_id) REFERENCES rooms(id)
);

CREATE INDEX IF NOT EXISTS idx_waitlist_doctor_date ON waitlist(doctor_id, appointment_date);
CREATE INDEX IF NOT EXISTS idx_waitlist_priority ON waitlist(priority);
CREATE INDEX IF NOT EXISTS idx_appointments_room_id ON appointments(room_id);
CREATE INDEX IF NOT EXISTS idx_rooms_type ON rooms(room_type);
