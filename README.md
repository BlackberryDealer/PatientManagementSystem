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
| **Scheduling algorithms** | Lazy doctor-room auto-allocation (`resolve_room()`) with 3-tier fallback and a race-safe claim-retry loop (plain `INSERT` guarded by dual UNIQUE indexes, retrying on a unique violation); availability enforcement engine — 3-rule priority system (blocked → windowed → open default) in the critical path of all 4 booking algorithms; greedy load-balanced doctor reassignment (`find_alternative_doctor()`) | Dylan | ✅ Implemented |
| **Patient history timelines** | Chronological record view with appointment/record/prescription/invoice merging | Raees | ✅ Implemented |
| **Medical report PDF generation** | Server-side PDF export of a medical report via the pure-Rust `printpdf` crate (paginated, word-wrapped, no external binaries) | Raees | ✅ Implemented |
| **Role-based staff access** | Type-level role enforcement via Rust trait system (`AuthUser`, `require_role`) | Afif | ✅ Implemented |
| Financial reporting | Analytics dashboard with revenue stats, busiest-doctors ranking, cancellation/collection rates | Hanzalah | ✅ Implemented |
| **Audit logging** | Immutable action trail (`src/audit/`) recording actor, action, entity, and entity ID across appointments, records, and PDF export; admin-only view | Hanzalah | ✅ Implemented |

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
│   ├── 005_doctor_room_assignments.sql
│   └── 006_room_assignment_unique.sql
├── templates/
│   ├── base.html.tera
│   ├── error.html.tera
│   └── shared/
│       ├── navbar.html.tera
│       └── footer.html.tera
├── tests/                   # Integration test suite (159 tests incl. unit tests)
│   ├── common/mod.rs        # Test infrastructure & macros
│   ├── test_auth.rs         # Authentication & authorization (20 tests)
│   ├── test_algorithms.rs   # Scheduling algorithms + reschedule + completion (36 tests)
│   ├── test_appointments.rs # Appointment booking + room override (23 tests)
│   ├── test_availability.rs # Doctor availability (3 tests)
│   ├── test_records.rs      # Medical records + PDF export (6 tests)
│   ├── test_billing.rs      # Invoices & payments (7 tests)
│   └── test_extended.rs     # Availability, reassignment, timeline, audit, dashboard (17 tests)
└── src/
    ├── main.rs              # Entry point, server config, route mounting
    ├── lib.rs               # Library crate for integration tests
    ├── db.rs                # Database pool & migration runner
    ├── errors.rs            # Unified AppError type
    ├── auth.rs              # Session-based auth extractors & role guards
    ├── traits.rs            # OOP traits (TimeSlotted, StatusManaged, etc.)
    ├── time.rs              # Clinic hours & time-slot helpers
    ├── bin/
    │   └── seed.rs          # Database seeder (`cargo run --bin seed`)
    ├── users/               # User registration, login, profiles
    ├── appointments/        # Scheduling engine (4 algorithms) + waitlist + calendar
    ├── availability/        # Doctor recurring & blocked availability
    ├── records/             # Medical records, prescriptions, timeline & PDF report export
    ├── billing/             # Invoices, line items, payments
    ├── audit/               # Immutable action trail (who did what, when)
    └── dashboard/           # Admin analytics & statistics
```

Each domain is a sibling `src/<module>.rs` file (route configuration via `pub fn configure()`) plus a matching `src/<module>/` folder containing:
- `models.rs` — struct definitions with serde + sqlx derives
- `services.rs` — business logic and database queries (the `appointments` module splits this into a `services/` folder: `algorithms`, `booking`, `queries`, `rooms`, `waitlist`, `helpers`)
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

Creates 6 users (1 admin, 2 doctors, 3 patients), 10 appointments, 10 availability slots, 6 doctor-room assignments, 4 medical records, 4 prescriptions, 3 invoices (1 settled, 2 pending), and 3 waitlist entries. All accounts use password `password123`.

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
| `availability` | Doctor Management | **Scheduling algorithms** — lazy room auto-allocation + 3-rule availability enforcement used by all 4 booking paths + greedy load-balanced doctor reassignment | Dylan |
| `records` | Medical Records, Prescription Tracking | **Patient history timelines** — chronological record view with multi-entity merging | Raees |
| `billing` + `audit` | Billing | Analytics dashboard with revenue stats, busiest-doctors ranking, cancellation/collection rates; **Audit logging** — immutable cross-cutting action trail | Hanzalah |

---

## 👤 Individual Contributions & Extended Features

### Afif — `users` + `auth`

**Group Development Component:** Built the users and auth modules, establishing the unified identity and authorization backbone that every other module depends on for secure session handling and profile management.

**Individual Extended Features & Technical Contributions:**
- **Type-Enforced Role-Based Access Control** — Implemented access rules into the type system using an Actix request extractor. This makes it impossible to compile a code route that accidentally omits security verification.
- **Fail-Safe Role Parsing** — Modeled roles as a typed enum with fail-safe parsing, ensuring a tampered session value falls back to the least-privileged role rather than accidentally leaking access privileges.
- **Advanced Ownership Guards** — Developed secure request guards that verify user permissions, blocking patients from viewing other accounts by manually typing different IDs in the browser address bar.
- **Anti-Enumeration Hardening** — Closed a timing side-channel in login by always running a full password hash comparison even when the username does not exist, preventing response variations from revealing valid database accounts.

### Lennon — `appointments`

**Group Development Component:** Developed the appointments module, building the core calendar engine and booking database routes for the clinic application.

**Individual Extended Features & Technical Contributions:**
- **Multi-Algorithmic Scheduling & Queue Logic** — Implemented three cooperating algorithms to make the scheduling engine safe and fair under load. Applied the interval-overlap test for standard conflict detection.
- **Pure Earliest-Slot Search** — Built a search feature that analyzes a doctor's booked day and returns the earliest open gap matching the required appointment length.
- **Priority Sorting Queue** — Built a custom triage queue using a binary heap, adjusting the key sorting so that urgent cases and older entries rise to the top of the queue correctly.
- **Automated Lifecycle Promotion** — Integrated an automatic waitlist promotion that automatically fills a freed slot with the most urgent waiting patient the moment a booking is cancelled.

### Dylan — `availability`

**Group Development Component:** Built the availability module and doctor management tables, establishing the core structural boundaries for the clinic schedule.

**Individual Extended Features & Technical Contributions:**
- **Three-Rule Availability Gate** — Built a validation gate applying three structured rules. Blocked entries (like leave or lunch breaks) reject bookings immediately. Valid slots must fit completely inside a doctor's custom hours, otherwise they fall back to default clinic hours. This checks recurring and one-off rules in a single step.
- **Dynamic Load-Balanced Reassignment** — Developed a greedy doctor reassignment algorithm that ranks every alternative doctor inside a single SQL query. The query optimizes for continuity of care by ranking matching specializations first, followed by the fewest appointments that day for structural load balancing, walking the ranked list to pick the first doctor available and free of conflicts.
- **Race-Safe Room Allocation** — Designed an allocation system where a doctor's first daily booking locks in an available room. The claim is a plain INSERT guarded by two database-level UNIQUE indexes — one room per doctor per day (migration 005) and one doctor per room per day (migration 006) — so a lost race surfaces as a unique violation and the allocator retries with the next free room instead of double-assigning. When more doctors than rooms are working, sharing degrades gracefully and the slot-level room index still blocks same-time clashes.

### Raees — `records`

**Group Development Component:** Built the medical records and prescription tracking modules, establishing the database models and clinical storage paths.

**Individual Extended Features & Technical Contributions:**
- **Unified Patient History Timeline** — Created a consolidated timeline that normalizes appointments, records, prescriptions, and bills into a single history sorted by date. Utilized a typed kind enum to supply each event's icon and color so presentation details are never hardcoded at the call site.
- **Pure-Rust PDF Engine** — Wrote a standalone PDF report generator using the printpdf crate and the built-in Helvetica font, allowing the printable report to run with a single command on any machine with nothing extra to install.
- **Manual Layout Math & Pagination** — Solved the limitation of the underlying crate (which only draws text at fixed coordinates) by building a custom layout engine. Tracked a manual cursor that walks down the page and opens a fresh page when it runs out of room so that long records paginate cleanly.
- **Glyph-Aware Greedy Word-Wrap** — Designed a custom greedy word-wrap algorithm that estimates glyph width, respects existing line breaks, and hard-breaks over-long words on character boundaries to prevent accented multi-byte names from splitting in the middle of a letter.

### Hanzalah — `billing` + `audit`

**Group Development Component:** Built the billing module, covering the core implementation of invoices, line items, and payments.

**Individual Extended Features & Technical Contributions:**
- **Clinic Analytics Dashboard** — Combined 12 real-time aggregation queries into a single data pass, providing summaries of patient counts, appointment states, and financial indicators.
- **Derived Ratio Modeling** — Designed the underlying model to derive cancellation rates, collection rates, and outstanding balances so administrators read actionable ratios rather than bare counts.
- **System Audit Logging** — Created an append-only log that notes the user, action type, and affected data row for all security events, designed to log data without interrupting active transactions.
- **Billing State Machine** — Enforced a strict state machine guarding how an invoice moves through pending, paid, and cancelled statuses. Implemented partial-payment support that tracks a running total against the balance and automatically triggers a settled state change once cleared.

---

## 🔐 Authentication & Authorization

- Passwords are hashed with **bcrypt** (cost factor 10)
- Sessions are stored in **signed cookies** (`actix-session`)
- Sessions are **rotated on login/registration** (`session.renew()`) as a session-fixation defence
- **Logout is POST-only** — state-changing actions are never reachable via a plain link or browser prefetch
- Timing-safe login: a bcrypt hash is burned even for unknown accounts, so response times cannot reveal whether a username exists
- Role-based guards: `require_role()`, `require_admin()`, `require_doctor()`
- `AuthUser` extractor — requires login; `OptionalAuthUser` — allows anonymous access
- All rejections (400/401/403/404) render a **styled error page** that preserves the specific reason and offers Go Back / Home — no raw text responses

---

## 📊 Database Schema

The schema is fully normalized with 15 tables across 6 migrations:

- `users` — authentication & role (patient/doctor/admin)
- `patients`, `doctors` — role-specific profile tables
- `doctor_availability` — recurring weekly slots + blocked dates
- `appointments` — scheduled meetings with priority (1-4) and auto-assigned rooms
- `appointment_slots` — occupancy ledger (one row per 30-min slot); the `UNIQUE(doctor_id, appointment_date, slot_time)` index makes double-booking impossible at the database layer
- `rooms` — consultation rooms, procedure rooms, and equipment resources
- `doctor_room_assignments` — daily doctor-to-room allocation, unique in both directions (one room per doctor per day AND one doctor per room per day)
- `waitlist` — priority queue for patients awaiting slots
- `medical_records` — diagnosis & treatment per appointment
- `prescriptions` — medication orders
- `invoices`, `invoice_items`, `payments` — billing module
- `audit_log` — immutable action trail (who did what, when)

See `migrations/001_initial_schema.sql` through `migrations/006_room_assignment_unique.sql` for the full DDL and data migrations.

---

## 📝 Development Notes

### Scheduling Algorithms (All Implemented)

- **Algorithm 1 — Time Interval Overlap Detection**: `check_conflict()` prevents double-booking using the overlap condition `start_time < ? AND end_time > ?`. Always checks both doctor and room conflicts (rooms are auto-assigned per doctor/day). Returns `bool` — `true` if a conflict exists.
- **Algorithm 2 — Earliest Available Slot**: `find_earliest_slot()` scans a doctor's schedule for a given date, walks through the gaps between existing appointments, and returns the first free slot ≥ the requested duration. Every candidate gap is also passed through the same 3-rule availability gate booking uses (`ensure_doctor_available`), so a suggested slot is always one the booking path would actually accept — leave and blocked breaks are skipped. Working hours: 08:00–17:00. Route: `GET/POST /appointments/suggest`.
- **Algorithm 3 — Priority-Based Scheduling**: `book_with_priority()` allows Emergency (1) and Urgent (2) appointments to bump lower-priority ones to the waitlist. Uses Rust's `BinaryHeap<PriorityItem>` for the priority queue (reversed `Ord` implementation). Bumping is transactional — all mutations run inside `pool.begin()...tx.commit()`. Route: `POST /appointments/book/priority`.
- **Algorithm 4 — Doctor Reassignment (greedy, load-balanced)**: `find_alternative_doctor()` ranks every other doctor by same-specialization-first (continuity of care) then fewest appointments that day (load balancing), and picks the first who is both available and conflict-free. `reassign_appointment()` moves the appointment and its occupancy slots atomically. Route: `POST /appointments/{id}/reassign`.

### Appointment Lifecycle

Appointments move through `scheduled → completed` (the visit took place; staff action via `POST /appointments/{id}/complete`) or `scheduled → cancelled` (patient-own or staff; frees the occupancy slots and triggers waitlist auto-promotion). Completed and cancelled appointments are immutable history — every transition is a guarded method on the `Appointment` struct itself (`complete()`, `cancel()`, `reschedule_to()`, `reassign_to()`), so no caller can drive an illegal state change. Completion deliberately **keeps** the occupancy slots: the time was genuinely used, and the conflict checker counts completed visits as occupying their window.

### Room & Resource Scheduling (Auto-Allocation)

- **`rooms` table**: 6 seeded rooms (3 consultation, 1 procedure, 1 equipment, 1 lab). Rooms are automatically assigned per doctor per day via the `doctor_room_assignments` table — patients no longer select a room manually. The first booking of the day claims a free room for that doctor, race-safely (dual UNIQUE indexes + claim-retry loop). Room conflicts are always checked alongside doctor conflicts. Staff can override a single appointment's room via `POST /appointments/{id}/assign-room` (e.g. moving a visit into the procedure room); a same-slot room clash is rejected by the slot ledger's UNIQUE index as a clean 400.
- **`waitlist` table**: Tracks bumped patients with priority, status (waiting→offered→accepted→expired), and promotion support. Routes: `GET /appointments/waitlist`, `POST /appointments/waitlist/join`, `POST /appointments/waitlist/{id}/promote`.

### OOP Traits (`src/traits.rs`)

Demonstrating Rust's trait-based polymorphism for the OOP marking criteria:

| Trait | Implemented By | What It Provides |
|---|---|---|
| `TimeSlotted` | `Appointment`, `WaitlistEntry`, `DoctorAvailability`, `TimeWindow` | Overlap detection, duration calculation — shared scheduling logic across all time-based entities. `TimeWindow` adapts a raw requested slot so `any_conflict` can compare it against a doctor's blocked entries |
| `StatusManaged` | `Appointment`, `Invoice`, `WaitlistEntry` | Status checking, Bulma CSS badge classes |
| `Prioritized` | `Appointment`, `WaitlistEntry`, `PriorityItem` | Priority labels (Emergency/Urgent/Normal/Follow-up), urgency comparison — `is_higher_priority_than` drives the waitlist `BinaryHeap` ordering |
| `Reportable` | `Appointment`, `Invoice`, `MedicalRecord`, `Prescription` | Human-readable summary generation for reports and auditing |

### Medical Report PDF Generation

- **`records/{id}/report.pdf`**: Streams a server-generated PDF of a medical report (`Content-Type: application/pdf`, `Content-Disposition: attachment`). Built with the **pure-Rust `printpdf`** crate using the standard built-in Helvetica fonts — **no external binaries** (wkhtmltopdf/Chrome) and **no font files to ship**, so `cargo run` works unchanged.
- The generator (`src/records/pdf.rs`) lays out a clinic letterhead, a two-column patient/doctor block, the clinical sections, and a prescriptions list. Body text is **word-wrapped** to the page width and **paginates** automatically for long records. Access reuses the record's ownership rule (patients can only export their own) and writes an `audit_log` entry.

### Other Features

- **Persistent Sessions**: Session encryption key is auto-generated once and saved to `.env` as `SESSION_SECRET`. Survives server restarts — no forced re-login. See `get_or_create_secret_key()` in `main.rs`.
- **Role-Based Access**: `AuthUser` extractor with `require_role()`, `require_admin()`, `require_doctor()` guards. Type-level role enforcement prevents accidental privilege escalation.
- **Frontend Polish**: Font Awesome 6 icons, Bulma components, mobile-responsive navbar with hamburger toggle, fade-in animations, hero-style empty states, breadcrumbs on detail pages.
- **Database**: SQLite for zero-config setup. 15 tables across 6 migrations. Switch to PostgreSQL via `DATABASE_URL` and `sqlx` features in `Cargo.toml`.
- **Styled Error Pages**: every error status (400/401/403/404/500) renders a consistent Bulma error screen. Domain rejections (e.g. a booking conflict) keep their specific message and offer a "Go Back" button; server errors hide internals behind a generic line. `AppError` renders its own page in `error_response()`; the `ErrorHandlers` middleware dresses up only *plain* error bodies (unmatched routes, malformed forms) and passes already-styled HTML through.
- **Server-Side Clinic Hours**: `parse_slot()` rejects any slot outside 08:00–17:00, so the booking form's slot grid is enforced on the server too — a hand-crafted POST cannot book a doctor at 02:00, even one with an otherwise open schedule.
- **Canonical Time Storage**: every write path re-renders times through `minutes_to_time()` before storing or comparing, so a hand-crafted `9:00` (unpadded) is persisted as `09:00` and can never break the lexical `HH:MM` comparisons the conflict and availability checks rely on.

### Design Decisions & Known Limitations

Deliberate scope decisions, with the reasoning we present in the demo:

- **Patient-reported priority**: patients may select Emergency/Urgent when booking. This models self-reported urgency at intake; staff re-triage via the waitlist and every priority booking is written to the audit log. A production system would gate the override behind a staff triage step.
- **Immutable clinical records**: medical records and prescriptions are create-and-read only — no update or delete. This is intentional: clinical history is an append-only record (corrections are new records), mirroring real healthcare data-integrity requirements.
- **CSRF**: no per-form CSRF tokens. State-changing routes are POST-only and the session cookie uses the `SameSite=Lax` default, which blocks cross-site form POSTs in modern browsers. Tokens would be the next hardening step.
- **`cookie_secure(false)`** is set for local HTTP development; flip to `true` behind HTTPS.
- **No login rate-limiting**: bcrypt (cost 10) plus the timing-equalized login path are the current brute-force mitigations.
- **Waitlist statuses** `offered`/`expired` are reserved in the schema for a future offer/expiry flow; the implemented lifecycle is `waiting → accepted`.
---

## 🧪 Testing

### Run the Test Suite

```bash
cargo test
```

### Test Coverage (159 tests, 8 suites)

| Test Suite | Tests | Covers |
|---|---|---|
| `test_auth.rs` | 20 | Registration (patient/doctor/admin), duplicate rejection, login success/failure/nonexistent, login-with-email, POST-only logout, role guards, admin-only routes, profile PII anti-enumeration, **styled 403 error page** |
| `test_algorithms.rs` | 36 | Conflict detection (empty/overlap/cancelled/room), earliest-slot (empty/after/full/gap/multi-gap, **skips blocked windows / respects declared working hours**), priority (bump/equal-rejected/normal-gate/ordering-proof), invalid time/duration rejection, ownership checks, waitlist (add/promote/cancel-triggers), **reschedule (move+frees-old-slot / conflict-rejected / self-overlap-allowed / ownership)**, **completion (flips status / keeps slots / double-complete + cancelled rejected)**, **unpadded-time normalisation**, **distinct rooms per doctor per day + live UNIQUE index** |
| `test_appointments.rs` | 23 | Booking form, HTTP booking (success/conflict/invalid-time), **clinic-hours rejection (before-open/past-close/night)**, **styled 400 error page keeps the domain message**, priority booking HTTP, cancel HTTP + list-verify, **complete HTTP (doctor) + patient forbidden**, **room override HTTP (appointment + slots move) + patient forbidden + same-slot clash → 400**, waitlist (doctor/patient), suggest form, promote forbidden (patient) |
| `test_availability.rs` | 3 | Doctor availability page, set-availability form, submit + verify persistence |
| `test_records.rs` | 6 | Records list, create form (doctor), patient blocked from create, HTTP create-submit + detail verification, **PDF export download (content-type + `%PDF` magic), PDF ownership enforcement** |
| `test_billing.rs` | 7 | Billing page (patient), create-invoice requires admin, admin creates invoice (single/multi-item), bad items rejected, payment recording, **settled invoice rejects further payment** |
| `test_extended.rs` | 17 | Availability enforcement (blocked/recurring/open-default/past-date), doctor reassignment (success/failure-no-alternative/skips-busy), patient timeline (multi-entity merge), prescriptions, medical reports, audit logging, dashboard stats |
| **Unit tests** (in `src/`) | **47** | Trait default-method tests (overlap/duration/priority/status), `DaySchedule` pure gap-finding, **`parse_slot` clinic-hours + grid rules**, **error-page HTML-escaping**, enum serialization round-trips, **PDF word-wrap + real `%PDF` byte rendering** |
| **Total** | **159** | **100% pass rate** |

### Architecture

Tests use **in-memory SQLite** (`sqlite::memory:`) with full migrations for zero-config, isolated test runs. The `with_test_app!` macro builds a complete Actix app with all routes, session middleware, and stub Tera templates. `register_and_login!` and `seed_and_login!` provide authenticated sessions without duplicating registration boilerplate. Both macros avoid `actix-http` version conflicts by letting Rust infer all `Service` trait types.

---

## 📄 License

This project is developed as part of the CSC1106 Web Programming coursework at the University of Glasgow.
