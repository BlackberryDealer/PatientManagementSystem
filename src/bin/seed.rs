//! Seed script: populates the database with realistic test data.
//! Run with:  cargo run --bin seed

use patient_management_system::db;
use patient_management_system::time::{minutes_to_time, time_to_minutes, SLOT_MINUTES};
use sqlx::SqlitePool;

#[actix_rt::main]
async fn main() {
    dotenv::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:patient_management.db?mode=rwc".to_string());

    println!("🌱 Seeding database...");
    let pool = db::create_pool(&database_url).await;
    db::run_migrations(&pool).await;

    // Clean existing data so repeated runs never duplicate rows.
    // Order matters: delete children before parents to satisfy FK constraints.
    print!("  Cleaning existing data... ");
    sqlx::query("DELETE FROM payments").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM invoice_items").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM invoices").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM appointment_slots").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM prescriptions").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM medical_records").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM waitlist").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM appointments").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM doctor_room_assignments").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM doctor_availability").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM audit_log").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM patients").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM doctors").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM users").execute(&pool).await.unwrap();
    // Reset auto-increment counters so IDs are predictable on every run
    sqlx::query("DELETE FROM sqlite_sequence").execute(&pool).await.unwrap();
    println!("done");

    seed_users(&pool).await;
    seed_availability(&pool).await;
    seed_room_assignments(&pool).await;
    seed_appointments(&pool).await;
    seed_medical_records(&pool).await;
    seed_prescriptions(&pool).await;
    seed_billing(&pool).await;
    seed_waitlist(&pool).await;

    println!("✅ Database seeded successfully!");
    println!("   Login with any of these accounts (password: password123):");
    println!("   admin           — admin@clinic.com        (Admin)");
    println!("   dr.smith        — dr.smith@clinic.com      (Doctor)");
    println!("   dr.jones        — dr.jones@clinic.com      (Doctor)");
    println!("   john.doe        — john.doe@email.com       (Patient)");
    println!("   jane.doe        — jane.doe@email.com       (Patient)");
    println!("   bob.wilson      — bob.wilson@email.com     (Patient)");
}

async fn seed_users(pool: &SqlitePool) {
    print!("  Creating users... ");

    let users = [
        ("admin", "admin@clinic.com", "admin", "System Administrator"),
        ("dr.smith", "dr.smith@clinic.com", "doctor", "Dr. Sarah Smith"),
        ("dr.jones", "dr.jones@clinic.com", "doctor", "Dr. Michael Jones"),
        ("john.doe", "john.doe@email.com", "patient", "John Doe"),
        ("jane.doe", "jane.doe@email.com", "patient", "Jane Doe"),
        ("bob.wilson", "bob.wilson@email.com", "patient", "Bob Wilson"),
    ];

    let hash = bcrypt::hash("password123", 4).unwrap(); // fast hash for seed data

    for (user, email, role, name) in &users {
        sqlx::query(
            "INSERT OR IGNORE INTO users (username, email, password_hash, role, full_name)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(user).bind(email).bind(&hash).bind(role).bind(name)
        .execute(pool).await.unwrap();
    }

    // Patient profiles
    sqlx::query("INSERT OR IGNORE INTO patients (user_id, date_of_birth, phone, address, blood_group, emergency_contact) VALUES (4, '1990-03-15', '555-0101', '123 Main St, Glasgow', 'O+', '555-9999')").execute(pool).await.unwrap();
    sqlx::query("INSERT OR IGNORE INTO patients (user_id, date_of_birth, phone, address, blood_group, emergency_contact) VALUES (5, '1985-07-22', '555-0102', '456 Oak Ave, Glasgow', 'A+', '555-9998')").execute(pool).await.unwrap();
    sqlx::query("INSERT OR IGNORE INTO patients (user_id, date_of_birth, phone, address, blood_group, emergency_contact) VALUES (6, '1978-11-08', '555-0103', '789 Pine Rd, Glasgow', 'B+', '555-9997')").execute(pool).await.unwrap();

    // Doctor profiles
    sqlx::query("INSERT OR IGNORE INTO doctors (user_id, specialization, license_number, phone) VALUES (2, 'General Practice', 'LIC-001', '555-0201')").execute(pool).await.unwrap();
    sqlx::query("INSERT OR IGNORE INTO doctors (user_id, specialization, license_number, phone) VALUES (3, 'Cardiology', 'LIC-002', '555-0202')").execute(pool).await.unwrap();

    println!("done (6 users)");
}

async fn seed_availability(pool: &SqlitePool) {
    print!("  Creating availability... ");

    // Dr. Smith: Mon-Fri 9-5, Wed off
    for day in &[1, 2, 4, 5] { // Mon, Tue, Thu, Fri
        sqlx::query(
            "INSERT OR IGNORE INTO doctor_availability (doctor_id, day_of_week, start_time, end_time, is_recurring, is_blocked)
             VALUES (1, ?, '09:00', '17:00', 1, 0)",
        ).bind(day).execute(pool).await.unwrap();
    }

    // Dr. Jones: Mon-Wed 8-4, Thu-Fri 10-6
    for (day, start, end) in &[(1, "08:00", "16:00"), (2, "08:00", "16:00"), (3, "08:00", "16:00"), (4, "10:00", "18:00"), (5, "10:00", "18:00")] {
        sqlx::query(
            "INSERT OR IGNORE INTO doctor_availability (doctor_id, day_of_week, start_time, end_time, is_recurring, is_blocked)
             VALUES (2, ?, ?, ?, 1, 0)",
        ).bind(day).bind(start).bind(end).execute(pool).await.unwrap();
    }

    // Blocked dates (holidays)
    sqlx::query(
        "INSERT OR IGNORE INTO doctor_availability (doctor_id, day_of_week, start_time, end_time, is_recurring, specific_date, is_blocked)
         VALUES (1, 0, '00:00', '23:59', 0, '2026-08-17', 1)",
    ).execute(pool).await.unwrap();

    println!("done (10 slots)");
}

async fn seed_room_assignments(pool: &SqlitePool) {
    print!("  Creating room assignments... ");

    // Doctor 1 (Dr. Smith) → Consultation Room A on most days
    // Doctor 2 (Dr. Jones) → Consultation Room B on most days
    let assignments = [
        // doctor_id, room_id, date
        (1, 1, "2026-08-03"),
        (2, 2, "2026-08-03"),
        (1, 1, "2026-08-04"),
        (2, 2, "2026-08-04"),
        (1, 1, "2026-08-05"),
        (2, 2, "2026-08-05"),
    ];

    for (doc, room, date) in &assignments {
        sqlx::query(
            "INSERT OR IGNORE INTO doctor_room_assignments (doctor_id, room_id, assignment_date)
             VALUES (?, ?, ?)",
        )
        .bind(doc).bind(room).bind(date)
        .execute(pool).await.unwrap();
    }

    println!("done (6 assignments)");
}

async fn seed_appointments(pool: &SqlitePool) {
    print!("  Creating appointments... ");

    let apps = [
        // patient_id, doctor_id, date, start, end, status, priority, room_id, notes
        (1, 1, "2026-08-03", "09:00", "09:30", "completed",   3, Some(1), "Annual check-up — all clear"),
        (1, 1, "2026-08-03", "09:30", "10:00", "scheduled",   3, Some(1), "Follow-up blood test"),
        (2, 1, "2026-08-03", "10:00", "10:30", "scheduled",   3, Some(2), "Headache and dizziness"),
        (3, 2, "2026-08-03", "08:30", "09:00", "completed",   4, Some(3), "Post-surgery check"),
        (1, 1, "2026-08-04", "11:00", "11:30", "scheduled",   2, Some(1), "Chest pain — URGENT"),
        (2, 2, "2026-08-04", "09:00", "09:30", "scheduled",   3, Some(4), "Routine cardiology consult"),
        (3, 1, "2026-08-04", "14:00", "14:30", "cancelled",   3, None,      "Patient cancelled — rescheduled"),
        (1, 1, "2026-08-05", "09:00", "09:30", "scheduled",   3, Some(1), "Prescription renewal"),
        (2, 2, "2026-08-05", "10:00", "10:30", "scheduled",   4, Some(4), "ECG follow-up"),
        (3, 1, "2026-08-05", "15:00", "15:30", "scheduled",   3, Some(2), "Vaccination"),
    ];

    for (pat, doc, date, start, end, status, pri, room, notes) in &apps {
        sqlx::query(
            "INSERT OR IGNORE INTO appointments (patient_id, doctor_id, appointment_date, start_time, end_time, status, priority, room_id, notes, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now', ?))",
        )
        .bind(pat).bind(doc).bind(date).bind(start).bind(end).bind(status)
        .bind(pri).bind(room).bind(notes)
        .bind(format!("-{} days", apps.len() as i32)) // stagger creation dates
        .execute(pool).await.unwrap();
    }

    // Populate the occupancy ledger for the seeded appointments so the database
    // is internally consistent — the same 30-minute slot rows the booking path
    // would have written. Cancelled appointments free their time, so they are
    // skipped. Without this, seeded appointments would exist with no ledger
    // rows, and the live free-slot lookup / DB double-booking guard would not
    // see them.
    let booked = sqlx::query_as::<_, (i64, i64, String, String, String, Option<i64>)>(
        "SELECT id, doctor_id, appointment_date, start_time, end_time, room_id
         FROM appointments WHERE status != 'cancelled'",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    for (id, doctor_id, date, start, end, room_id) in booked {
        let (Some(start_min), Some(end_min)) = (time_to_minutes(&start), time_to_minutes(&end))
        else {
            continue;
        };
        let mut m = start_min;
        while m < end_min {
            sqlx::query(
                "INSERT OR IGNORE INTO appointment_slots
                 (appointment_id, doctor_id, appointment_date, slot_time, room_id)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(doctor_id)
            .bind(&date)
            .bind(minutes_to_time(m))
            .bind(room_id)
            .execute(pool)
            .await
            .unwrap();
            m += SLOT_MINUTES;
        }
    }

    println!("done (10 appointments)");
}

async fn seed_medical_records(pool: &SqlitePool) {
    print!("  Creating medical records... ");

    let records = [
        (1, 1, Some(1), "Hypertension (stage 1)", "Lifestyle changes, monitor BP monthly", "Patient reports occasional headaches"),
        (3, 2, Some(4), "Post-operative recovery — normal", "Continue prescribed medication, light activity only", "Incision healing well"),
        (1, 1, None,      "Seasonal allergies", "Antihistamines as needed", "Pollen allergy — worse in spring"),
        (2, 1, Some(3), "Tension headache", "OTC pain relief, stress management", "Work-related stress identified as trigger"),
    ];

    for (pat, doc, appt, diag, treat, notes) in &records {
        sqlx::query(
            "INSERT OR IGNORE INTO medical_records (patient_id, doctor_id, appointment_id, diagnosis, treatment, notes)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(pat).bind(doc).bind(appt).bind(diag).bind(treat).bind(notes)
        .execute(pool).await.unwrap();
    }

    println!("done (4 records)");
}

async fn seed_prescriptions(pool: &SqlitePool) {
    print!("  Creating prescriptions... ");

    let scripts = [
        (1, 1, Some(1), "Lisinopril", "10mg", "Once daily", "30 days", "Take with food"),
        (2, 1, Some(3), "Ibuprofen", "400mg", "As needed, max 3x daily", "7 days", "Do not exceed recommended dose"),
        (3, 2, Some(4), "Amoxicillin", "500mg", "Three times daily", "10 days", "Complete full course"),
        (1, 1, None,      "Cetirizine", "10mg", "Once daily", "90 days", "Non-drowsy formula"),
    ];

    for (pat, doc, appt, med, dose, freq, dur, notes) in &scripts {
        sqlx::query(
            "INSERT OR IGNORE INTO prescriptions (patient_id, doctor_id, appointment_id, medication_name, dosage, frequency, duration, notes)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(pat).bind(doc).bind(appt).bind(med).bind(dose).bind(freq).bind(dur).bind(notes)
        .execute(pool).await.unwrap();
    }

    println!("done (4 prescriptions)");
}

async fn seed_billing(pool: &SqlitePool) {
    print!("  Creating invoices & payments... ");

    // Invoice 1: paid (John Doe — settled)
    sqlx::query("INSERT OR IGNORE INTO invoices (id, patient_id, invoice_date, due_date, total_amount, status) VALUES (1, 1, '2026-07-01', '2026-07-15', 150.00, 'paid')").execute(pool).await.unwrap();
    sqlx::query("INSERT OR IGNORE INTO invoice_items (invoice_id, description, quantity, unit_price, total_price) VALUES (1, 'Consultation', 1, 100.00, 100.00)").execute(pool).await.unwrap();
    sqlx::query("INSERT OR IGNORE INTO invoice_items (invoice_id, description, quantity, unit_price, total_price) VALUES (1, 'Blood Test', 1, 50.00, 50.00)").execute(pool).await.unwrap();
    sqlx::query("INSERT OR IGNORE INTO payments (invoice_id, amount, payment_method, transaction_ref) VALUES (1, 150.00, 'Credit Card', 'TXN-001')").execute(pool).await.unwrap();

    // Invoice 2: pending (Jane Doe — awaiting payment)
    sqlx::query("INSERT OR IGNORE INTO invoices (id, patient_id, invoice_date, due_date, total_amount, status) VALUES (2, 2, '2026-07-10', '2026-08-10', 200.00, 'pending')").execute(pool).await.unwrap();
    sqlx::query("INSERT OR IGNORE INTO invoice_items (invoice_id, description, quantity, unit_price, total_price) VALUES (2, 'Cardiology Consult', 1, 150.00, 150.00)").execute(pool).await.unwrap();
    sqlx::query("INSERT OR IGNORE INTO invoice_items (invoice_id, description, quantity, unit_price, total_price) VALUES (2, 'ECG', 1, 50.00, 50.00)").execute(pool).await.unwrap();

    // Invoice 3: pending (Bob Wilson — awaiting payment)
    sqlx::query("INSERT OR IGNORE INTO invoices (id, patient_id, invoice_date, due_date, total_amount, status) VALUES (3, 3, '2026-07-15', '2026-08-15', 75.00, 'pending')").execute(pool).await.unwrap();
    sqlx::query("INSERT OR IGNORE INTO invoice_items (invoice_id, description, quantity, unit_price, total_price) VALUES (3, 'Follow-up Visit', 1, 75.00, 75.00)").execute(pool).await.unwrap();

    println!("done (3 invoices)");
}

async fn seed_waitlist(pool: &SqlitePool) {
    print!("  Creating waitlist entries... ");

    let entries = [
        (3, 1, "2026-08-05", "09:00", "09:30", 1, "Emergency — chest pain follow-up"),
        (2, 2, "2026-08-05", "14:00", "14:30", 2, "Urgent ECG review"),
        (1, 1, "2026-08-06", "10:00", "10:30", 3, "Routine blood work"),
    ];

    for (pat, doc, date, start, end, pri, notes) in &entries {
        sqlx::query(
            "INSERT OR IGNORE INTO waitlist (patient_id, doctor_id, appointment_date, requested_start, requested_end, priority, notes, status)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'waiting')",
        )
        .bind(pat).bind(doc).bind(date).bind(start).bind(end).bind(pri).bind(notes)
        .execute(pool).await.unwrap();
    }

    println!("done (3 entries)");
}
