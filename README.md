# Patient Management System

**University of Glasgow — CSC1106 Web Programming**

A full-stack enterprise web application built with **Rust**, **Actix Web 4**, **Tera** (SSR), and **SQLx** (SQLite).

---

## 🏥 Project Overview

The Patient Management System (PMS) is a modular web application designed to simulate real-world healthcare administration workflows. It supports patient registration, appointment scheduling with conflict resolution, medical records, prescription tracking, doctor availability management, and billing with invoice generation.

### Core Focus
**Appointment Scheduling & Conflict Resolution** — The system prevents double-booking by checking time-slot overlaps before confirming any appointment.

---

## 🛠 Tech Stack

| Layer          | Technology                              |
|----------------|-----------------------------------------|
| Language       | Rust (2021 edition)                     |
| Web Framework  | Actix Web 4                             |
| Templating     | Tera (Server-Side Rendering)            |
| Database       | SQLite (via SQLx, migratable to PostgreSQL) |
| Auth           | Actix Session (signed cookies) + bcrypt |
| Styling        | Bulma CSS (CDN)                         |
| Date/Time      | Chrono                                  |

---

## 📁 Project Structure

```
PatientManagementSystem/
├── Cargo.toml
├── .env
├── .gitignore
├── README.md
├── migrations/
│   └── 001_initial_schema.sql
├── templates/
│   ├── base.html.tera
│   ├── layout.html.tera
│   └── shared/
│       ├── navbar.html.tera
│       └── footer.html.tera
└── src/
    ├── main.rs              # Entry point, server config, route mounting
    ├── db.rs                # Database pool & migration runner
    ├── errors.rs            # Unified AppError type
    ├── auth.rs              # Session-based auth extractors & role guards
    ├── users/               # User registration, login, profiles
    ├── appointments/        # Scheduling engine with conflict detection
    ├── availability/        # Doctor recurring & blocked availability
    ├── records/             # Medical records & prescriptions
    └── billing/             # Invoices, line items, payments (PDF stub)
```

Each module contains:
- `mod.rs` — route configuration (`pub fn configure()`)
- `models.rs` — struct definitions with serde + sqlx derives
- `services.rs` — business logic and database queries
- `handlers.rs` — HTTP request handlers
- `templates/` — Tera HTML templates for the module

---

## 🚀 Quick Start

### Prerequisites
- [Rust](https://www.rust-lang.org/tools/install) (stable, 1.70+)
- SQLite (bundled via `sqlx`, no separate install needed)

### Setup

```bash
# 1. Clone the repository
git clone <repo-url>
cd PatientManagementSystem

# 2. Configure environment (default values work out of the box)
# The .env file is already provided; edit if needed

# 3. Build and run
cargo run
```

The server starts at **http://localhost:8080**. The SQLite database (`patient_management.db`) is created and migrated automatically on first run.

### First Steps
1. Visit **http://localhost:8080/users/register** to create an account
2. Choose a role: **Patient**, **Doctor**, or **Admin**
3. Log in and explore the dashboard

---

## 👥 Module Ownership (Team of 5)

| Module           | Team Member | Responsibilities                                                   |
|------------------|-------------|--------------------------------------------------------------------|
| `users`          | Afif    | Registration, login/logout, session management, role-based access   |
| `appointments`   | lennon    | **Core**: Booking, conflict detection, cancellation, schedule views |
| `availability`   | Dylan    | Doctor recurring slots, blocked dates, leave management             |
| `records`        | Raees    | Medical records CRUD, prescriptions, diagnosis/treatment tracking   |
| `billing`        | Hanzalah    | Invoices, line items, payments, PDF generation (future)             |

---

## 🔐 Authentication & Authorization

- Passwords are hashed with **bcrypt** (cost factor 10)
- Sessions are stored in **signed cookies** (`actix-session`)
- Role-based guards: `require_role()`, `require_admin()`, `require_doctor()`
- `AuthUser` extractor — requires login; `OptionalAuthUser` — allows anonymous access

---

## 📊 Database Schema

The schema is fully normalized with 10 tables:

- `users` — authentication & role (patient/doctor/admin)
- `patients`, `doctors` — role-specific profile tables
- `doctor_availability` — recurring weekly slots + blocked dates
- `appointments` — scheduled meetings with status tracking
- `medical_records` — diagnosis & treatment per appointment
- `prescriptions` — medication orders
- `invoices`, `invoice_items`, `payments` — billing module

See `migrations/001_initial_schema.sql` for the full DDL.

---

## 📝 Development Notes

- **Conflict Detection**: The `check_conflict()` function in `appointments/services.rs` uses an overlap query (`start_time < ? AND end_time > ?`) to prevent double-booking.
- **PDF Generation**: Billing has a stub for PDF invoice generation. See comments in `billing/services.rs` for recommended crates.
- **Template Loading**: Module templates are loaded from `src/{module}/templates/` at startup via `load_module_templates()` in `main.rs`.
- **Database**: Defaults to SQLite for zero-config setup. Switch to PostgreSQL by changing `DATABASE_URL` and the `sqlx` features in `Cargo.toml`.

---

## 📄 License

This project is developed as part of the CSC1106 Web Programming coursework at the University of Glasgow.
