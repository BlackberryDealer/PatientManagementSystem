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
| 4 | **Billing** | `billing` module — invoices, line items, payments |
| 5 | **Doctor Management** | `users` + `availability` modules — doctor profiles, recurring schedules, leave management |
| 6 | **Prescription Tracking** | `records` module — medication orders linked to appointments |

### Core Focus
**Appointment Scheduling & Conflict Resolution** — The system prevents double-booking by checking time-slot overlaps before confirming any appointment. This implements **scheduling algorithms** and **time slot validation** as specified in the project brief.

### Advanced Features (Individual)

Each team member implements one or more advanced features aligned with the official spec:

| Spec Feature | Implementation | Owner | Status |
|---|---|---|---|
| **Queue management system** / **Priority queues** | Waitlist auto-scheduler with priority triage via `BinaryHeap` | Lennon | ✅ Implemented |
| **Time slot validation** | Conflict detection & earliest-slot suggestion algorithm | Lennon | ✅ Implemented |
| **Scheduling algorithms** | Multi-resource scheduling (rooms/equipment) | Dylan | ✅ Implemented |
| **Patient history timelines** | Chronological record view with appointment/record/prescription/invoice merging | Raees | ✅ Implemented |
| **Medical report PDF generation** | Server-side PDF export of a medical report via the pure-Rust `printpdf` crate (paginated, word-wrapped, no external binaries) | Raees | ✅ Implemented |
| **Role-based staff access** | Type-level role enforcement via Rust trait system (`AuthUser`, `require_role`) | Afif | ✅ Implemented |
| Financial reporting | Analytics dashboard with revenue stats, busiest-doctors ranking, cancellation/collection rates | Hanzalah | ✅ Implemented |

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
| PDF Export     | printpdf (pure Rust, built-in fonts)    |

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
│   ├── 002_rooms_priority.sql
│   ├── 003_audit_log.sql
│   ├── 004_fix_payment_dates.sql
│   └── 005_doctor_room_assignments.sql
├── templates/
│   ├── base.html.tera
│   └── shared/
│       ├── navbar.html.tera
│       └── footer.html.tera
├── tests/                   # Integration test suite (133 tests)
│   ├── common/mod.rs        # Test infrastructure & macros
│   ├── test_auth.rs         # Authentication & authorization (19 tests)
│   ├── test_algorithms.rs   # Scheduling algorithms + reschedule (30 tests)
│   ├── test_appointments.rs # Appointment booking (16 tests)
│   ├── test_availability.rs # Doctor availability (3 tests)
│   ├── test_records.rs      # Medical records + PDF export (6 tests)
│   ├── test_billing.rs      # Invoices & payments (6 tests)
│   └── test_extended.rs     # Availability, reassignment, timeline, audit, dashboard (17 tests)
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
    ├── appointments/        # Scheduling engine (3 algorithms) + waitlist + calendar
    ├── availability/        # Doctor recurring & blocked availability
    ├── records/             # Medical records, prescriptions, timeline & PDF report export
    ├── billing/             # Invoices, line items, payments
    ├── audit/               # Immutable action trail (who did what, when)
    └── dashboard/           # Admin analytics & statistics
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
3. Login with any seeded account — all use password `password123`

### Default Test Accounts

| Username | Email | Password | Role |
|---|---|---|---|
| `admin` | `admin@clinic.com` | `password123` | **Admin** |
| `dr.smith` | `dr.smith@clinic.com` | `password123` | Doctor |
| `dr.jones` | `dr.jones@clinic.com` | `password123` | Doctor |
| `john.doe` | `john.doe@email.com` | `password123` | Patient |
| `jane.doe` | `jane.doe@email.com` | `password123` | Patient |
| `bob.wilson` | `bob.wilson@email.com` | `password123` | Patient |

> The admin account has access to the dashboard, billing management, staff user creation, and all admin-only routes.

---

## 👥 Module Ownership & Spec Mapping (Team of 5)

| Code Module | Official PMS Module(s) | Advanced Feature (Individual) | Owner |
|---|---|---|---|
| `users` + `auth` | Patient Registration, Doctor Management | **Role-based staff access** — type-level role enforcement | Afif |
| `appointments` | Appointment Scheduling (core) | **Queue management system** & **Priority queues** — waitlist with `BinaryHeap` priority triage; **Time slot validation** — overlap detection & earliest-slot algorithm | Lennon |
| `availability` | Doctor Management | **Scheduling algorithms** — doctor-room auto-allocation (daily room assignment per doctor) | Dylan |
| `records` | Medical Records, Prescription Tracking | **Patient history timelines** — chronological record view with multi-entity merging | Raees |
| `billing` | Billing | Analytics dashboard with revenue stats, busiest-doctors ranking, cancellation/collection rates | Hanzalah |

---

## 🔐 Authentication & Authorization

- Passwords are hashed with **bcrypt** (cost factor 10)
- Sessions are stored in **signed cookies** (`actix-session`)
- Role-based guards: `require_role()`, `require_admin()`, `require_doctor()`
- `AuthUser` extractor — requires login; `OptionalAuthUser` — allows anonymous access

---

## 📊 Database Schema

The schema is fully normalized with 13 tables across 4 migrations:

- `users` — authentication & role (patient/doctor/admin)
- `patients`, `doctors` — role-specific profile tables
- `doctor_availability` — recurring weekly slots + blocked dates
- `appointments` — scheduled meetings with priority (1-4) and auto-assigned rooms
- `rooms` — consultation rooms, procedure rooms, and equipment resources
- `doctor_room_assignments` — daily doctor-to-room allocation (one room per doctor per day)
- `waitlist` — priority queue for patients awaiting slots
- `medical_records` — diagnosis & treatment per appointment
- `prescriptions` — medication orders
- `invoices`, `invoice_items`, `payments` — billing module
- `audit_log` — immutable action trail (who did what, when)

See `migrations/001_initial_schema.sql` through `migrations/005_doctor_room_assignments.sql` for the full DDL and data migrations.

---

## 📝 Development Notes

### Scheduling Algorithms (All Implemented)

- **Algorithm 1 — Time Interval Overlap Detection**: `check_conflict()` prevents double-booking using the overlap condition `start_time < ? AND end_time > ?`. Always checks both doctor and room conflicts (rooms are auto-assigned per doctor/day). Returns `bool` — `true` if a conflict exists.
- **Algorithm 2 — Earliest Available Slot**: `find_earliest_slot()` scans a doctor's schedule for a given date, walks through the gaps between existing appointments, and returns the first free slot ≥ the requested duration. Working hours: 08:00–17:00. Route: `GET/POST /appointments/suggest`.
- **Algorithm 3 — Priority-Based Scheduling**: `book_with_priority()` allows Emergency (1) and Urgent (2) appointments to bump lower-priority ones to the waitlist. Uses Rust's `BinaryHeap<PriorityItem>` for the priority queue (reversed `Ord` implementation). Bumping is transactional — all mutations run inside `pool.begin()...tx.commit()`. Route: `POST /appointments/book/priority`.
- **Algorithm 4 — Doctor Reassignment (greedy, load-balanced)**: `find_alternative_doctor()` ranks every other doctor by same-specialization-first (continuity of care) then fewest appointments that day (load balancing), and picks the first who is both available and conflict-free. `reassign_appointment()` moves the appointment and its occupancy slots atomically. Route: `POST /appointments/{id}/reassign`.

### Room & Resource Scheduling (Auto-Allocation)

- **`rooms` table**: 6 seeded rooms (3 consultation, 1 procedure, 1 equipment, 1 lab). Rooms are automatically assigned per doctor per day via the `doctor_room_assignments` table — patients no longer select a room manually. The first booking of the day claims a free room for that doctor. Room conflicts are always checked alongside doctor conflicts.
- **`waitlist` table**: Tracks bumped patients with priority, status (waiting→offered→accepted→expired), and promotion support. Routes: `GET /appointments/waitlist`, `POST /appointments/waitlist/join`, `POST /appointments/waitlist/{id}/promote`.

### OOP Traits (`src/traits.rs`)

Demonstrating Rust's trait-based polymorphism for the OOP marking criteria:

| Trait | Implemented By | What It Provides |
|---|---|---|
| `TimeSlotted` | `Appointment`, `WaitlistEntry`, `DoctorAvailability` | Overlap detection, duration calculation — shared scheduling logic across all time-based entities |
| `StatusManaged` | `Appointment`, `Invoice` | Status checking, Bulma CSS badge classes |
| `Prioritized` | `Appointment`, `WaitlistEntry` | Priority labels (Emergency/Urgent/Normal/Follow-up), comparison between entities |
| `Reportable` | `Appointment`, `Invoice`, `MedicalRecord` | Human-readable summary generation for reports and auditing |

### Medical Report PDF Generation

- **`records/{id}/report.pdf`**: Streams a server-generated PDF of a medical report (`Content-Type: application/pdf`, `Content-Disposition: attachment`). Built with the **pure-Rust `printpdf`** crate using the standard built-in Helvetica fonts — **no external binaries** (wkhtmltopdf/Chrome) and **no font files to ship**, so `cargo run` works unchanged.
- The generator (`src/records/pdf.rs`) lays out a clinic letterhead, a two-column patient/doctor block, the clinical sections, and a prescriptions list. Body text is **word-wrapped** to the page width and **paginates** automatically for long records. Access reuses the record's ownership rule (patients can only export their own) and writes an `audit_log` entry.

### Other Features

- **Persistent Sessions**: Session encryption key is auto-generated once and saved to `.env` as `SESSION_SECRET`. Survives server restarts — no forced re-login. See `get_or_create_secret_key()` in `main.rs`.
- **Role-Based Access**: `AuthUser` extractor with `require_role()`, `require_admin()`, `require_doctor()` guards. Type-level role enforcement prevents accidental privilege escalation.
- **Frontend Polish**: Font Awesome 6 icons, Bulma components, mobile-responsive navbar with hamburger toggle, fade-in animations, hero-style empty states, breadcrumbs on detail pages.
- **Database**: SQLite for zero-config setup. 13 tables across 4 migrations. Switch to PostgreSQL via `DATABASE_URL` and `sqlx` features in `Cargo.toml`.

---

## � Marking Criteria (Spec v1.2.2)

### Group Implementation (60%)

| Criterion | Weight | Our Approach |
|---|---|---|
| System Architecture & OOP Design | 15% | Modular crate with 5 domains; 4 custom traits (`TimeSlotted`, `StatusManaged`, `Prioritized`, `Reportable`) demonstrating polymorphism; `FromRequest`/`ResponseError`/`FromRow` trait impls; structs with `impl` blocks; separation of concerns (models/services/handlers) |
| Backend Functionality & Business Logic | 15% | Complete CRUD across all modules; 4 scheduling algorithms (overlap detection, earliest-slot, priority-based with BinaryHeap, greedy doctor reassignment); transactional priority override; appointment rescheduling (transactional slot-rebuild); automatic doctor-room allocation; waitlist with promotion; session-based auth; role guards |
| Database Design & Integration | 10% | 13 normalized tables across 4 migrations; foreign keys with CASCADE/SET NULL; composite + partial-unique indexes on the slot-occupancy ledger; SQLx migrations |
| Frontend Design & SSR | 10% | Tera template inheritance (30 templates); Font Awesome 6 icons; Bulma responsive CSS with mobile navbar; itemized invoice builder; hero-style empty states; breadcrumbs; fade-in animations |
| Documentation, Presentation & Demo | 10% | Professional README; WALKTHROUGH.md guide; inline code documentation; clear setup instructions |

### Individual Extended Features (40%)

| Criterion | Weight | Evidence |
|---|---|---|
| Extended Feature Development | 15% | Each member's advanced feature (see table above) — independently implemented with measurable complexity: priority-queue waitlist, doctor-room auto-allocation, patient timelines, **server-side PDF report generation**, analytics dashboard, audit logging |
| Technical Complexity & Problem Solving | 15% | 4 scheduling algorithms with an O(log n) BinaryHeap priority queue; transactional database mutations; race-proof slot-occupancy ledger (UNIQUE-index double-booking guard); trait-based FromRequest extractors; pure-Rust PDF generation with manual page layout, word-wrap, and pagination; async/await throughout |
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

### Test Coverage (133 tests, 8 suites)

| Test Suite | Tests | Covers |
|---|---|---|
| `test_auth.rs` | 19 | Registration (patient/doctor/admin), duplicate rejection, login success/failure/nonexistent, login-with-email, logout, role guards, admin-only routes, profile PII anti-enumeration |
| `test_algorithms.rs` | 30 | Conflict detection (empty/overlap/cancelled/room), earliest-slot (empty/after/full/gap/multi-gap), priority (bump/equal-rejected/normal-gate/ordering-proof), invalid time/duration rejection, ownership checks, waitlist (add/promote/cancel-triggers), **reschedule (move+frees-old-slot / conflict-rejected / self-overlap-allowed / ownership)** |
| `test_appointments.rs` | 16 | Booking form, HTTP booking (success/conflict/invalid-time), priority booking HTTP, cancel HTTP + list-verify, waitlist (doctor/patient), suggest form, promote forbidden (patient) |
| `test_availability.rs` | 3 | Doctor availability page, set-availability form, submit + verify persistence |
| `test_records.rs` | 6 | Records list, create form (doctor), patient blocked from create, HTTP create-submit + detail verification, **PDF export download (content-type + `%PDF` magic), PDF ownership enforcement** |
| `test_billing.rs` | 6 | Billing page (patient), create-invoice requires admin, admin creates invoice (single/multi-item), bad items rejected, payment recording |
| `test_extended.rs` | 17 | Availability enforcement (blocked/recurring/open-default/past-date), doctor reassignment (success/failure-no-alternative/skips-busy), patient timeline (multi-entity merge), prescriptions, medical reports, audit logging, dashboard stats |
| **Unit tests** (in `src/`) | **36** | Trait default-method tests (overlap/duration/priority/status), `DaySchedule` pure gap-finding, enum serialization round-trips, **PDF word-wrap + real `%PDF` byte rendering** |
| **Total** | **133** | **100% pass rate** |

### Architecture

Tests use **in-memory SQLite** (`sqlite::memory:`) with full migrations for zero-config, isolated test runs. The `with_test_app!` macro builds a complete Actix app with all routes, session middleware, and stub Tera templates. `register_and_login!` and `seed_and_login!` provide authenticated sessions without duplicating registration boilerplate. Both macros avoid `actix-http` version conflicts by letting Rust infer all `Service` trait types.

---

## 📄 License

This project is developed as part of the CSC1106 Web Programming coursework at the University of Glasgow.
