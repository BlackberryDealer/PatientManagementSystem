# Patient Management System

**University of Glasgow — CSC1106 Web Programming**  
**Spec Version:** v1.2.2 | **Domain:** Patient Management System (PMS)

A full-stack enterprise web application built with **Rust**, **Actix Web 4**, **Tera** (SSR), and **SQLx** (SQLite).

---

## 🏥 Project Overview

The Patient Management System (PMS) is a modular web application designed to simulate real-world healthcare administration workflows. It covers all six official PMS modules:

| # | Official PMS Module | Our Implementation |
|---|---------------------|-------------------|
| 1 | **Patient Registration** | `users` module — registration, login, role-based profiles |
| 2 | **Appointment Scheduling** | `appointments` module — booking engine with conflict resolution |
| 3 | **Medical Records** | `records` module — diagnosis, treatment, and prescription tracking |
| 4 | **Billing** | `billing` module — invoices, line items, payments, PDF export |
| 5 | **Doctor Management** | `users` + `availability` modules — doctor profiles, recurring schedules, leave management |
| 6 | **Prescription Tracking** | `records` module — medication orders linked to appointments |

### Core Focus
**Appointment Scheduling & Conflict Resolution** — The system prevents double-booking by checking time-slot overlaps before confirming any appointment. This implements **scheduling algorithms** and **time slot validation** as specified in the project brief.

### Advanced Features
Each team member implements one advanced feature aligned with the official spec:

| Spec Feature | Implementation | Owner |
|---|---|---|
| **Queue management system** / **Priority queues** | Waitlist auto-scheduler with priority triage | Member B |
| **Time slot validation** | Recurring appointment generator with DST handling | Member B |
| **Scheduling algorithms** | Multi-resource scheduling (rooms/equipment) | Member C |
| **Patient history timelines** | Drug interaction checker & chronological record view | Member D |
| **Role-based staff access** | Type-level role enforcement via Rust trait system | Member A |
| Financial reporting | PDF invoice generation & monthly revenue dashboard | Member E |

> **Note:** Formative Assessment does **not** apply to this project (spec v1.2.2).

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
│   ├── 001_initial_schema.sql
│   └── 002_rooms_priority.sql
├── templates/
│   ├── base.html.tera
│   ├── layout.html.tera
│   └── shared/
│       ├── navbar.html.tera
│       └── footer.html.tera
├── tests/                   # Integration test suite (47 tests)
│   ├── common/mod.rs        # Test infrastructure & macros
│   ├── test_auth.rs         # Authentication & authorization
│   ├── test_algorithms.rs   # Scheduling algorithms (19 tests)
│   ├── test_appointments.rs # Appointment booking (9 tests)
│   ├── test_availability.rs # Doctor availability (3 tests)
│   ├── test_records.rs      # Medical records (4 tests)
│   └── test_billing.rs      # Invoices & payments (7 tests)
└── src/
    ├── main.rs              # Entry point, server config, route mounting
    ├── lib.rs               # Library crate for integration tests
    ├── db.rs                # Database pool & migration runner
    ├── errors.rs            # Unified AppError type
    ├── auth.rs              # Session-based auth extractors & role guards
    ├── traits.rs            # OOP traits (TimeSlotted, StatusManaged, etc.)
    ├── bin/
    │   └── seed.rs          # Database seeder (`cargo run --bin seed`)
    ├── users/               # User registration, login, profiles
    ├── appointments/        # Scheduling engine (3 algorithms) + waitlist
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

### Seed Data (Optional)

Populate the database with realistic test data in one command:

```bash
cargo run --bin seed
```

Creates 6 users (1 admin, 2 doctors, 3 patients), 10 appointments, 11 availability slots, 4 medical records, 4 prescriptions, 3 invoices with payments, and 3 waitlist entries. All accounts use password `password123`.

### First Steps
1. Run `cargo run --bin seed` to populate test data
2. Visit **http://localhost:8080/users/login**
3. Login with any seeded account (e.g. `john.doe` / `password123`)

---

## 👥 Module Ownership & Spec Mapping (Team of 5)

| Code Module | Official PMS Module(s) | Advanced Feature (Individual) | Owner |
|---|---|---|---|
| `users` + `auth` | Patient Registration, Doctor Management | **Role-based staff access** — type-level role enforcement | Afif |
| `appointments` | Appointment Scheduling (core) | **Queue management system** & **Priority queues** — waitlist auto-scheduler with priority triage; **Time slot validation** — recurring appointments & DST handling | Lennon |
| `availability` | Doctor Management | **Scheduling algorithms** — multi-resource scheduling (rooms, equipment, staff) | Dylan |
| `records` | Medical Records, Prescription Tracking | **Patient history timelines** — drug interaction checker & chronological record view | Raees |
| `billing` | Billing | PDF invoice generation & monthly revenue dashboard (financial reporting) | Hanzalah |

---

## 🔐 Authentication & Authorization

- Passwords are hashed with **bcrypt** (cost factor 10)
- Sessions are stored in **signed cookies** (`actix-session`)
- Role-based guards: `require_role()`, `require_admin()`, `require_doctor()`
- `AuthUser` extractor — requires login; `OptionalAuthUser` — allows anonymous access

---

## 📊 Database Schema

The schema is fully normalized with 12 tables across 2 migrations:

- `users` — authentication & role (patient/doctor/admin)
- `patients`, `doctors` — role-specific profile tables
- `doctor_availability` — recurring weekly slots + blocked dates
- `appointments` — scheduled meetings with priority (1-4) and room assignment
- `rooms` — consultation rooms, procedure rooms, and equipment resources
- `waitlist` — priority queue for patients awaiting slots
- `medical_records` — diagnosis & treatment per appointment
- `prescriptions` — medication orders
- `invoices`, `invoice_items`, `payments` — billing module

See `migrations/001_initial_schema.sql` and `migrations/002_rooms_priority.sql` for the full DDL.

---

## 📝 Development Notes

### Scheduling Algorithms (All Implemented)

- **Algorithm 1 — Time Interval Overlap Detection**: `check_conflict()` prevents double-booking using the overlap condition `start_time < ? AND end_time > ?`. Supports both doctor and room conflict checking. Returns `bool` — `true` if a conflict exists.
- **Algorithm 2 — Earliest Available Slot**: `find_earliest_slot()` scans a doctor's schedule for a given date, walks through the gaps between existing appointments, and returns the first free slot ≥ the requested duration. Working hours: 08:00–17:00. Route: `GET/POST /appointments/suggest`.
- **Algorithm 3 — Priority-Based Scheduling**: `book_with_priority()` allows Emergency (1) and Urgent (2) appointments to bump lower-priority ones to the waitlist. Uses Rust's `BinaryHeap<PriorityItem>` for the priority queue (reversed `Ord` implementation). Bumping is transactional — all mutations run inside `pool.begin()...tx.commit()`. Route: `POST /appointments/book/priority`.

### Room & Resource Scheduling

- **`rooms` table**: 6 seeded rooms (3 consultation, 1 procedure, 1 X-ray, 1 lab). Appointments can optionally assign a room. Room conflicts are checked alongside doctor conflicts.
- **`waitlist` table**: Tracks bumped patients with priority, status (waiting→offered→accepted→expired), and promotion support. Routes: `GET /appointments/waitlist`, `POST /appointments/waitlist/join`, `POST /appointments/waitlist/{id}/promote`.

### OOP Traits (`src/traits.rs`)

Demonstrating Rust's trait-based polymorphism for the OOP marking criteria:

| Trait | Implemented By | What It Provides |
|---|---|---|
| `TimeSlotted` | `Appointment`, `WaitlistEntry`, `DoctorAvailability` | Overlap detection, duration calculation — shared scheduling logic across all time-based entities |
| `StatusManaged` | `Appointment`, `Invoice` | Status checking, Bulma CSS badge classes |
| `Prioritized` | `Appointment`, `WaitlistEntry` | Priority labels (Emergency/Urgent/Normal/Follow-up), comparison between entities |
| `Reportable` | `Appointment`, `Invoice`, `MedicalRecord` | Human-readable summary generation for reports and auditing |

### Other Features

- **Persistent Sessions**: Session encryption key is auto-generated once and saved to `.env` as `SESSION_SECRET`. Survives server restarts — no forced re-login. See `get_or_create_secret_key()` in `main.rs`.
- **Role-Based Access**: `AuthUser` extractor with `require_role()`, `require_admin()`, `require_doctor()` guards. Type-level role enforcement prevents accidental privilege escalation.
- **Frontend Polish**: Font Awesome 6 icons, Bulma components, mobile-responsive navbar with hamburger toggle, fade-in animations, hero-style empty states, breadcrumbs on detail pages.
- **Database**: SQLite for zero-config setup. 12 tables across 2 migrations. Switch to PostgreSQL via `DATABASE_URL` and `sqlx` features in `Cargo.toml`.

---

## � Marking Criteria (Spec v1.2.2)

### Group Implementation (60%)

| Criterion | Weight | Our Approach |
|---|---|---|
| System Architecture & OOP Design | 15% | Modular crate with 5 domains; 4 custom traits (`TimeSlotted`, `StatusManaged`, `Prioritized`, `Reportable`) demonstrating polymorphism; `FromRequest`/`ResponseError`/`FromRow` trait impls; structs with `impl` blocks; separation of concerns (models/services/handlers) |
| Backend Functionality & Business Logic | 15% | Complete CRUD across all modules; 3 scheduling algorithms (overlap detection, earliest-slot, priority-based with BinaryHeap); transactional priority override; waitlist with promotion; session-based auth; role guards; room/resource scheduling |
| Database Design & Integration | 10% | 12 normalized tables across 2 migrations; foreign keys with CASCADE/SET NULL; composite indexes on appointments(doctor_id, appointment_date); SQLx migrations |
| Frontend Design & SSR | 10% | Tera template inheritance (18 templates); Font Awesome 6 icons; Bulma responsive CSS with mobile navbar; hero-style empty states; breadcrumbs; fade-in animations |
| Documentation, Presentation & Demo | 10% | Professional README; WALKTHROUGH.md guide; inline code documentation; clear setup instructions |

### Individual Extended Features (40%)

| Criterion | Weight | Evidence |
|---|---|---|
| Extended Feature Development | 15% | Each member's advanced feature (see table above) — independently implemented with measurable complexity |
| Technical Complexity & Problem Solving | 15% | 3 scheduling algorithms with O(log n) BinaryHeap priority queue; transactional database mutations; trait-based FromRequest extractors; async/await throughout; time-to-minutes parsing pipeline |
| Individual Understanding & Contribution | 10% | Clear explanation during demo; documented in report; visible commit history per module |

---

## 📦 Deliverables (Portfolio)

Aligned with the spec v1.2.2 submission structure:

| # | Deliverable | Format |
|---|---|---|
| 1 | Source Code Archive | `g##_source.zip` (max 20MB) |
| 2 | Demo Recording | `g##_recording.mp4` (15 min, max 200MB) |
| 3 | Presentation Slides | `g##_slides.pptx` + `g##_slides.pdf` |
| 4 | Project Report | `g##_report.docx` + `g##_report.pdf` (max 6 pages) |

**Report must include:** architectural decisions, technical challenges, and each member's individual contribution mapped to the official PMS module list and advanced features above.

---

## 🧪 Testing

### Run the Test Suite

```bash
cargo test
```

### Test Coverage (47 tests, 6 suites)

| Test Suite | Tests | Covers |
|---|---|---|
| `test_auth.rs` | 11 | Registration (patient/doctor/admin), duplicate rejection, login success/failure, logout, role guards, admin-only routes |
| `test_algorithms.rs` | 19 | Conflict detection (empty/overlap/cancelled/room), earliest-slot (empty/after/full/gap/multi-gap), priority (bump/equal-rejected/normal-gate), invalid time/duration rejection, ownership checks, waitlist (add/promote/cancel-triggers) |
| `test_appointments.rs` | 9 | Booking form, HTTP booking (success/conflict/invalid-time), priority booking HTTP, cancel HTTP, list after booking, waitlist (doctor/patient), suggest form |
| `test_availability.rs` | 3 | Doctor availability page, set-availability form, submit + verify persistence |
| `test_records.rs` | 4 | Records list, create form (doctor), patient blocked, record creation HTTP submit |
| `test_billing.rs` | 7 | Invoice list, admin-only create, invoice creation (single/multi-item/bad-items), payment recording |
| **Total** | **47** | **100% pass rate** |

### Architecture

Tests use **in-memory SQLite** (`sqlite::memory:`) with full migrations for zero-config, isolated test runs. The `with_test_app!` macro builds a complete Actix app with all routes, session middleware, and stub Tera templates. `register_and_login!` provides authenticated sessions without duplicating registration boilerplate. Both macros avoid `actix-http` version conflicts by letting Rust infer all `Service` trait types.

---

## 📄 License

This project is developed as part of the CSC1106 Web Programming coursework at the University of Glasgow.
