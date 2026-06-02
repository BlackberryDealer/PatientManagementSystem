-- ============================================================
-- Initial Schema: Patient Management System
-- All tables with foreign keys, indexes, and constraints
-- ============================================================

-- Users: core authentication and role-based access
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('patient', 'doctor', 'admin')) DEFAULT 'patient',
    full_name TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Patients: profile data for patient-role users
CREATE TABLE IF NOT EXISTS patients (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL UNIQUE,
    date_of_birth DATE,
    phone TEXT,
    address TEXT,
    blood_group TEXT,
    emergency_contact TEXT,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Doctors: profile data for doctor-role users
CREATE TABLE IF NOT EXISTS doctors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL UNIQUE,
    specialization TEXT NOT NULL,
    license_number TEXT NOT NULL,
    phone TEXT,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

-- Rooms: consultation rooms and medical equipment resources
CREATE TABLE IF NOT EXISTS rooms (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    room_type TEXT NOT NULL DEFAULT 'consultation',
    floor TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    notes TEXT
);

-- Doctor Availability: recurring weekly slots and blocked/leave dates
CREATE TABLE IF NOT EXISTS doctor_availability (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    doctor_id INTEGER NOT NULL,
    day_of_week INTEGER NOT NULL CHECK (day_of_week BETWEEN 0 AND 6),
    start_time TEXT NOT NULL,          -- HH:MM format
    end_time TEXT NOT NULL,            -- HH:MM format
    is_recurring INTEGER NOT NULL DEFAULT 1,
    specific_date DATE,                -- non-null for blocked/override dates
    is_blocked INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY (doctor_id) REFERENCES doctors(id) ON DELETE CASCADE
);

-- Appointments: core scheduling entity with conflict-resolution support
CREATE TABLE IF NOT EXISTS appointments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    patient_id INTEGER NOT NULL,
    doctor_id INTEGER NOT NULL,
    appointment_date DATE NOT NULL,
    start_time TEXT NOT NULL,          -- HH:MM format
    end_time TEXT NOT NULL,            -- HH:MM format
    status TEXT NOT NULL CHECK (status IN ('scheduled', 'completed', 'cancelled')) DEFAULT 'scheduled',
    notes TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    room_id INTEGER,
    priority INTEGER NOT NULL DEFAULT 3 CHECK (priority BETWEEN 1 AND 4),
    FOREIGN KEY (patient_id) REFERENCES patients(id),
    FOREIGN KEY (doctor_id) REFERENCES doctors(id),
    FOREIGN KEY (room_id) REFERENCES rooms(id)
);

-- Appointment Slots: one row per 30-minute slot that an appointment occupies.
-- This is the occupancy ledger. A 30-min appointment writes 1 row; a 60-min
-- appointment writes 2 rows, etc. Appointments are always aligned to the grid.
--
-- The UNIQUE index on (doctor_id, appointment_date, slot_time) makes
-- double-booking IMPOSSIBLE at the database level: a second INSERT for the
-- same doctor+date+slot fails atomically, immune to the race conditions that
-- an application-level "check then insert" suffers from. Slot rows are deleted
-- when an appointment is cancelled, so a freed slot can be rebooked.
CREATE TABLE IF NOT EXISTS appointment_slots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    appointment_id INTEGER NOT NULL,
    doctor_id INTEGER NOT NULL,
    appointment_date DATE NOT NULL,
    slot_time TEXT NOT NULL,           -- HH:MM, aligned to a 30-minute grid
    room_id INTEGER,
    FOREIGN KEY (appointment_id) REFERENCES appointments(id) ON DELETE CASCADE,
    FOREIGN KEY (doctor_id) REFERENCES doctors(id),
    FOREIGN KEY (room_id) REFERENCES rooms(id)
);

-- A doctor can occupy each slot at most once (the core double-booking guard).
CREATE UNIQUE INDEX IF NOT EXISTS idx_slot_doctor_unique
    ON appointment_slots(doctor_id, appointment_date, slot_time);

-- A room (when assigned) can also be occupied at most once per slot.
-- Partial index: only enforced for rows that actually reserve a room.
CREATE UNIQUE INDEX IF NOT EXISTS idx_slot_room_unique
    ON appointment_slots(room_id, appointment_date, slot_time)
    WHERE room_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_slot_appointment ON appointment_slots(appointment_id);

-- Waitlist: priority queue for patients awaiting an available appointment slot
CREATE TABLE IF NOT EXISTS waitlist (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    patient_id INTEGER NOT NULL,
    doctor_id INTEGER NOT NULL,
    room_id INTEGER,
    appointment_date DATE NOT NULL,
    requested_start TEXT NOT NULL,     -- HH:MM format
    requested_end TEXT NOT NULL,       -- HH:MM format
    priority INTEGER NOT NULL DEFAULT 3 CHECK (priority BETWEEN 1 AND 4),
    notes TEXT,
    status TEXT NOT NULL CHECK (status IN ('waiting', 'offered', 'accepted', 'expired')) DEFAULT 'waiting',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (patient_id) REFERENCES patients(id),
    FOREIGN KEY (doctor_id) REFERENCES doctors(id),
    FOREIGN KEY (room_id) REFERENCES rooms(id)
);

-- Medical Records: diagnosis and treatment linked to appointments
CREATE TABLE IF NOT EXISTS medical_records (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    patient_id INTEGER NOT NULL,
    doctor_id INTEGER NOT NULL,
    appointment_id INTEGER,
    diagnosis TEXT,
    treatment TEXT,
    notes TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (patient_id) REFERENCES patients(id),
    FOREIGN KEY (doctor_id) REFERENCES doctors(id),
    FOREIGN KEY (appointment_id) REFERENCES appointments(id) ON DELETE SET NULL
);

-- Prescriptions: medication orders linked to appointments
CREATE TABLE IF NOT EXISTS prescriptions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    patient_id INTEGER NOT NULL,
    doctor_id INTEGER NOT NULL,
    appointment_id INTEGER,
    medication_name TEXT NOT NULL,
    dosage TEXT NOT NULL,
    frequency TEXT NOT NULL,
    duration TEXT,
    notes TEXT,
    prescribed_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (patient_id) REFERENCES patients(id),
    FOREIGN KEY (doctor_id) REFERENCES doctors(id),
    FOREIGN KEY (appointment_id) REFERENCES appointments(id) ON DELETE SET NULL
);

-- Invoices: billing header for patient charges
CREATE TABLE IF NOT EXISTS invoices (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    patient_id INTEGER NOT NULL,
    invoice_date DATE NOT NULL DEFAULT (date('now')),
    due_date DATE NOT NULL,
    total_amount REAL NOT NULL DEFAULT 0.0,
    status TEXT NOT NULL CHECK (status IN ('pending', 'paid', 'cancelled')) DEFAULT 'pending',
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (patient_id) REFERENCES patients(id)
);

-- Invoice Items: line items for each invoice
CREATE TABLE IF NOT EXISTS invoice_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    invoice_id INTEGER NOT NULL,
    description TEXT NOT NULL,
    quantity INTEGER NOT NULL DEFAULT 1,
    unit_price REAL NOT NULL,
    total_price REAL NOT NULL,
    FOREIGN KEY (invoice_id) REFERENCES invoices(id) ON DELETE CASCADE
);

-- Payments: payment records against invoices
CREATE TABLE IF NOT EXISTS payments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    invoice_id INTEGER NOT NULL,
    amount REAL NOT NULL,
    payment_date TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    payment_method TEXT NOT NULL,
    transaction_ref TEXT,
    FOREIGN KEY (invoice_id) REFERENCES invoices(id)
);

-- ============================================================
-- Indexes for query performance
-- ============================================================
CREATE INDEX IF NOT EXISTS idx_users_role ON users(role);
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_patients_user_id ON patients(user_id);
CREATE INDEX IF NOT EXISTS idx_doctors_user_id ON doctors(user_id);
CREATE INDEX IF NOT EXISTS idx_doctor_availability_doctor_id ON doctor_availability(doctor_id);
CREATE INDEX IF NOT EXISTS idx_doctor_availability_date ON doctor_availability(specific_date);
CREATE INDEX IF NOT EXISTS idx_appointments_patient_id ON appointments(patient_id);
CREATE INDEX IF NOT EXISTS idx_appointments_doctor_id ON appointments(doctor_id);
CREATE INDEX IF NOT EXISTS idx_appointments_date ON appointments(appointment_date);
CREATE INDEX IF NOT EXISTS idx_appointments_status ON appointments(status);
CREATE INDEX IF NOT EXISTS idx_appointments_doctor_date ON appointments(doctor_id, appointment_date);
CREATE INDEX IF NOT EXISTS idx_appointments_room ON appointments(room_id);
CREATE INDEX IF NOT EXISTS idx_waitlist_doctor_id ON waitlist(doctor_id);
CREATE INDEX IF NOT EXISTS idx_waitlist_patient_id ON waitlist(patient_id);
CREATE INDEX IF NOT EXISTS idx_waitlist_status ON waitlist(status);
CREATE INDEX IF NOT EXISTS idx_waitlist_priority ON waitlist(priority, created_at);
CREATE INDEX IF NOT EXISTS idx_medical_records_patient_id ON medical_records(patient_id);
CREATE INDEX IF NOT EXISTS idx_medical_records_appointment_id ON medical_records(appointment_id);
CREATE INDEX IF NOT EXISTS idx_prescriptions_patient_id ON prescriptions(patient_id);
CREATE INDEX IF NOT EXISTS idx_prescriptions_appointment_id ON prescriptions(appointment_id);
CREATE INDEX IF NOT EXISTS idx_invoices_patient_id ON invoices(patient_id);
CREATE INDEX IF NOT EXISTS idx_invoices_status ON invoices(status);
CREATE INDEX IF NOT EXISTS idx_invoice_items_invoice_id ON invoice_items(invoice_id);
CREATE INDEX IF NOT EXISTS idx_payments_invoice_id ON payments(invoice_id);
