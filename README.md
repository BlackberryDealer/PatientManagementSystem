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
| **Scheduling algorithms** | Lazy doctor-room auto-allocation (`resolve_room()`) with 3-tier fallback and a race-safe claim-retry loop (plain `INSERT` guarded by dual UNIQUE indexes, retrying on a unique violation); availability enforcement engine — 3-rule priority system (blocked → windowed → open default) in the critical path of every booking path; greedy single-appointment doctor reassignment (`find_alternative_doctor()`); optimal whole-day batch reassignment via the Hungarian algorithm (`plan_day_reassignment()`); live availability-driven booking slots (`free_slots()`) | Dylan | ✅ Implemented |
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
├── tests/                   # Integration test suite (195 tests incl. unit tests)
│   ├── common/mod.rs        # Test infrastructure & macros
│   ├── test_auth.rs         # Authentication & authorization (20 tests)
│   ├── test_algorithms.rs   # Scheduling algorithms (incl. batch reassign + live slots) + reschedule + completion (45 tests)
│   ├── test_appointments.rs # Appointment booking + room override + calendar view + waitlist lifecycle (35 tests)
│   ├── test_availability.rs # Doctor availability (3 tests)
│   ├── test_records.rs      # Medical records + PDF export (6 tests)
│   ├── test_billing.rs      # Invoices & payments (7 tests)
│   ├── test_extended.rs     # Availability, reassignment, timeline, audit, dashboard (17 tests)
│   └── test_templates.rs    # Tera template rendering (4 tests)
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
    ├── appointments/        # Scheduling engine (5 algorithms) + waitlist + calendar
    ├── availability/        # Doctor recurring & blocked availability
    ├── records/             # Medical records, prescriptions, timeline & PDF report export
    ├── billing/             # Invoices, line items, payments
    ├── audit/               # Immutable action trail (who did what, when)
    └── dashboard/           # Admin analytics & statistics
```

Each domain is a sibling `src/<module>.rs` file (route configuration via `pub fn configure()`) plus a matching `src/<module>/` folder containing:
- `models.rs` — struct definitions with serde + sqlx derives
- `services.rs` — business logic and database queries
- `handlers.rs` — HTTP request handlers
- `templates/` — Tera HTML templates for the module

The `appointments` module is the largest, so each of those files is expanded into its own folder for readability and maintainability. `models/` groups the domain types by concern (`appointment`, `waitlist`, `room`, `forms`, `scheduling`, `assignment`, `calendar`). `handlers/` groups the routes by workflow (`listing`, `booking`, `lifecycle`, `waitlist`, `reassign_day`). `services/` holds the business logic (`booking`, `queries`, `rooms`, `waitlist`, `helpers`), and `services/algorithms/` gives each scheduling algorithm its own file (`conflict`, `earliest_slot`, `priority_queue`, `reassign`, `batch_reassign`, `free_slots`), so a maintainer can open exactly the algorithm they need.

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
| `availability` | Doctor Management | **Scheduling algorithms** — lazy room auto-allocation + 3-rule availability enforcement in every booking path + greedy single-appointment reassignment + optimal whole-day batch reassignment (Hungarian) + live availability-driven booking slots | Dylan |
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
- **Optimal Batch Reassignment (Hungarian Algorithm)** — Extends the greedy reassignment into a provably optimal whole-day version for when a doctor goes on leave. Models the day as an assignment problem and solves it with the Hungarian (Kuhn-Munkres) method, minimizing total disruption instead of an early appointment grabbing the only same-specialty colleague. Costs reward matching specialization and load balance, pricing out unavailable colleagues. Staff preview and apply the plan in one transaction; the solver is unit-tested against a brute-force optimum.
- **Live Availability Booking** — Reuses the same availability gate to drive the booking form: picking a doctor and date fetches only that doctor's genuinely open 30-minute slots from a JSON endpoint, so a patient chooses from times that are actually free instead of guessing. A slot counts as open only when unoccupied and rule-allowed, reading occupancy from the same `appointments` table the conflict checker uses. It is purely a convenience layer — the slot ledger's UNIQUE index still guards the real race between two simultaneous bookings.
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

- **Algorithm 1 — Time Interval Overlap Detection**: `check_conflict()` prevents double-booking using the overlap condition `start_time < ? AND end_time > ?`, matched against *either* the doctor or the room (`doctor_id = ? OR room_id = ?`) — a doctor can't be in two places at once, and a room can't hold two visits at once. In routine bookings a doctor's daily room is fixed so the two clauses coincide; the OR only diverges when a caller checks a doctor against another doctor's room, which reassignment does. Returns `bool` — `true` if a conflict exists.
- **Algorithm 2 — Earliest Available Slot**: `find_earliest_slot()` scans a doctor's schedule for a given date, walks through the gaps between existing appointments, and returns the first free slot ≥ the requested duration. Every candidate gap is also passed through the same 3-rule availability gate booking uses (`ensure_doctor_available`), so a suggested slot is always one the booking path would actually accept — leave and blocked breaks are skipped. Working hours: 08:00–17:00. Route: `GET/POST /appointments/suggest`.
- **Algorithm 3 — Priority-Based Scheduling**: `book_with_priority()` allows Emergency (1) and Urgent (2) appointments to bump lower-priority ones to the waitlist. Uses Rust's `BinaryHeap<PriorityItem>` for the priority queue (reversed `Ord` implementation). Bumping is transactional — all mutations run inside `pool.begin()...tx.commit()`. There is a single booking endpoint (`POST /appointments/book`): when staff book at Emergency/Urgent, the handler routes through the priority path automatically, so no separate "override" action exists in the UI or the API. Patients are always booked at Normal priority (triage is a staff decision), so this path is unreachable for them.
- **Algorithm 4 — Doctor Reassignment (greedy, load-balanced)**: `find_alternative_doctor()` ranks every other doctor by same-specialization-first (continuity of care) then fewest appointments that day (load balancing), and picks the first who is both available and conflict-free (excluding the appointment being moved itself, since Algorithm 1's conflict check now also matches on room). `reassign_appointment()` moves the appointment and its occupancy slots atomically. Route: `POST /appointments/{id}/reassign`.
- **Algorithm 5 — Optimal Batch Reassignment (Hungarian / assignment problem)**: `plan_day_reassignment()` redistributes a leaving doctor's entire day at once. It builds a cost matrix (rows are the leaving doctor's appointments, columns are colleague capacity slots plus unassigned fallbacks) and solves it with the pure `CostMatrix::assign_min_cost()` implementation of the Kuhn-Munkres method, giving the globally minimum-disruption assignment rather than Algorithm 4's per-appointment greed. Costs reward the same specialization and a convex load term (balancing), and price out colleagues who are on leave or already booked. `apply_day_reassignment()` recomputes against the live schedule and commits every move in one transaction. Staff-only. Routes: `GET/POST /appointments/reassign-day`, `POST /appointments/reassign-day/apply`.
- **Live slot lookups for the booking form**: `GET /appointments/availability` returns only a doctor's genuinely free slots and is open to any signed-in role (it backs the patient-facing booking dropdown). `GET /appointments/all-slots` is the staff-only counterpart used by the priority-override UI — it returns every slot marked free-or-occupied so staff can see what they'd be bumping before submitting an Emergency/Urgent booking.

### Appointment Lifecycle

Appointments move through `scheduled → completed` (the visit took place; staff action via `POST /appointments/{id}/complete`) or `scheduled → cancelled` (patient-own or staff; frees the occupancy slots and triggers waitlist auto-promotion). Completed and cancelled appointments are immutable history — every transition is a guarded method on the `Appointment` struct itself (`complete()`, `cancel()`, `reschedule_to()`, `reassign_to()`), so no caller can drive an illegal state change. Completion deliberately **keeps** the occupancy slots: the time was genuinely used, and the conflict checker counts completed visits as occupying their window.

### Room & Resource Scheduling (Auto-Allocation)

- **`rooms` table**: 6 seeded rooms (3 consultation, 1 procedure, 1 equipment, 1 lab). Rooms are automatically assigned per doctor per day via the `doctor_room_assignments` table — patients no longer select a room manually. The first booking of the day claims a free room for that doctor, race-safely (dual UNIQUE indexes + claim-retry loop). A conflict is rejected if *either* the doctor or the room is already booked over the requested slot — a doctor moved into a different room for one visit still can't be double-booked, and a room can't hold two doctors at once. Staff can override a single appointment's room via `POST /appointments/{id}/assign-room` (e.g. moving a visit into the procedure room); a same-slot room clash is rejected by the slot ledger's UNIQUE index as a clean 400.
- **`waitlist` table**: Tracks bumped patients with priority and a full lifecycle status (`waiting → accepted` or `waiting → expired`). Patients join through a form on the waitlist page (entries are filed at Normal priority — bumped appointments keep their staff-set priority); promotion is staff-only. Routes: `GET /appointments/waitlist`, `POST /appointments/waitlist/join` (patient-only), `POST /appointments/waitlist/{id}/promote` (staff-only).
  - **Auto re-slot on bump**: a patient bumped by a priority override is not just parked on the waitlist pinned to the slot that was just taken (which could never re-open unless that override is later cancelled) — `try_rebook_bumped()` immediately searches the same doctor's day for the earliest free same-duration gap (reusing Algorithm 2's `find_earliest_slot`) and books it, marking the waitlist entry `accepted`. Only when the day is genuinely full does the entry fall back to `waiting`. This runs after the override's own transaction commits (the override itself must stay atomic; rebooking is best-effort, so a rebooking race can never roll back an emergency booking), and every auto-reschedule is audited (`appointment.auto_rescheduled`).
  - **Expiry**: a `waiting` entry whose requested date/time has passed is swept to `expired` by `expire_stale_waitlist()`, a single set-based `UPDATE`. There is no background scheduler in this app, so the sweep runs lazily wherever staleness would otherwise be visible or harmful — on `GET /appointments/waitlist` and before every promotion attempt — rather than on a timer. A stale `Promote` click gets a clear "already passed" notice instead of a confusing conflict error, and patients can still see their expired requests (doctors/admins only see the live `waiting` queue).

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

- **Monthly Calendar View**: `GET /appointments/calendar` renders a month grid (`CalendarMonth`, `src/appointments/models/calendar.rs`) with one cell per day showing that day's appointment count, scoped to the signed-in role. Each day links back to the list view filtered to that date (`GET /appointments?date=YYYY-MM-DD`).
- **Persistent Sessions**: Session encryption key is auto-generated once and saved to `.env` as `SESSION_SECRET`. Survives server restarts — no forced re-login. See `get_or_create_secret_key()` in `main.rs`.
- **Role-Based Access**: `AuthUser` extractor with `require_role()`, `require_admin()`, `require_doctor()` guards. Type-level role enforcement prevents accidental privilege escalation.
- **Frontend Polish**: Font Awesome 6 icons, Bulma components, mobile-responsive navbar with hamburger toggle, fade-in animations, hero-style empty states, breadcrumbs on detail pages.
- **Database**: SQLite for zero-config setup. 15 tables across 6 migrations. Switch to PostgreSQL via `DATABASE_URL` and `sqlx` features in `Cargo.toml`.
- **Styled Error Pages**: every error status (400/401/403/404/500) renders a consistent Bulma error screen. Domain rejections (e.g. a booking conflict) keep their specific message and offer a "Go Back" button; server errors hide internals behind a generic line. `AppError` renders its own page in `error_response()`; the `ErrorHandlers` middleware dresses up only *plain* error bodies (unmatched routes, malformed forms) and passes already-styled HTML through.
- **Server-Side Clinic Hours**: `parse_slot()` rejects any slot outside 08:00–17:00, so the booking form's slot grid is enforced on the server too — a hand-crafted POST cannot book a doctor at 02:00, even one with an otherwise open schedule.
- **Canonical Time Storage**: every write path re-renders times through `minutes_to_time()` before storing or comparing, so a hand-crafted `9:00` (unpadded) is persisted as `09:00` and can never break the lexical `HH:MM` comparisons the conflict and availability checks rely on.

### Design Decisions & Known Limitations

Deliberate scope decisions, with the reasoning we present in the demo:

- **Role-scoped booking**: the single booking form adapts to the signed-in role. A patient books for themselves with a chosen doctor and is always filed at Normal priority — a hand-crafted POST claiming Emergency is clamped server-side, and waitlist self-joins are clamped the same way. A doctor books a chosen patient into *their own* schedule (any submitted `doctor_id` is ignored — moving a visit between doctors is the separate reassignment flow). An admin books a chosen patient with any doctor, front-desk style. Staff pick the triage priority, and can re-triage any scheduled appointment afterwards from its detail page.
- **Immutable clinical records**: medical records and prescriptions are create-and-read only — no update or delete. This is intentional: clinical history is an append-only record (corrections are new records), mirroring real healthcare data-integrity requirements.
- **Doctor-only clinical writes**: creating a record or prescription requires `require_doctor_only()` (Doctor exclusively), not the broader `require_doctor()` (Doctor ∪ Admin) used elsewhere. An admin can administer the system but should not be able to stand in as the clinician of record for a diagnosis or prescription — the records list and create/prescribe buttons are hidden from admin accordingly, even though admin can still view all records.
- **CSRF**: no per-form CSRF tokens. State-changing routes are POST-only and the session cookie uses the `SameSite=Lax` default, which blocks cross-site form POSTs in modern browsers. Tokens would be the next hardening step.
- **`cookie_secure(false)`** is set for local HTTP development; flip to `true` behind HTTPS.
- **No login rate-limiting**: bcrypt (cost 10) plus the timing-equalized login path are the current brute-force mitigations.
- **Waitlist status `offered`** is reserved in the schema for a future explicit-offer flow (e.g. notify-and-hold before auto-booking); the implemented lifecycle is `waiting → accepted` / `waiting → expired`.
---

## 🧪 Testing

### Run the Test Suite

```bash
cargo test
```

### Test Coverage (195 tests, 9 suites)

| Test Suite | Tests | Covers |
|---|---|---|
| `test_auth.rs` | 20 | Registration (patient/doctor/admin), duplicate rejection, login success/failure/nonexistent, login-with-email, POST-only logout, role guards, admin-only routes, profile PII anti-enumeration, **styled 403 error page** |
| `test_algorithms.rs` | 45 | Conflict detection (empty/overlap/cancelled/room), earliest-slot (empty/after/full/gap/multi-gap, **skips blocked windows / respects declared working hours**), priority (bump/equal-rejected/normal-gate/ordering-proof), invalid time/duration rejection, ownership checks, waitlist (add/promote/cancel-triggers), **reschedule (move+frees-old-slot / conflict-rejected / self-overlap-allowed / ownership)**, **completion (flips status / keeps slots / double-complete + cancelled rejected)**, **unpadded-time normalisation**, **distinct rooms per doctor per day + live UNIQUE index**, **batch day reassignment (Hungarian plan + apply + no-appointments case)**, **live free-slot lookup (full grid / excludes booked / hides multi-slot coverage / respects blocked windows)** |
| `test_appointments.rs` | 35 | Booking form, HTTP booking (success/conflict/invalid-time), **clinic-hours rejection (before-open/past-close/night)**, **styled 400 error page keeps the domain message**, **role-aware booking (staff Emergency bump over Normal, patient priority clamped to Normal, patient cannot bump, doctor books own schedule only, missing patient → 400)**, cancel HTTP + list-verify, **complete HTTP (doctor) + patient forbidden**, **room override HTTP (appointment + slots move) + patient forbidden + same-slot clash → 400**, **doctor-busy-in-another-room still conflicts**, waitlist (doctor/patient views, **join patient-only + Normal-clamped**), **`?date=` list filter**, **calendar view (month grid, query params, requires login)**, suggest form, promote forbidden (patient), **staff re-triage HTTP (doctor sets, patient forbidden, out-of-range → 400)**, **waitlist lifecycle (expiry sweep hides stale entries from staff, auto-reschedule into the day's earliest free slot, stays waiting when the day is full, past-dated promote blocked with a clear notice, cancel-restores-waiter regression on a full day)** |
| `test_availability.rs` | 3 | Doctor availability page, set-availability form, submit + verify persistence |
| `test_records.rs` | 6 | Records list, create form (doctor), patient blocked from create, HTTP create-submit + detail verification, **PDF export download (content-type + `%PDF` magic), PDF ownership enforcement** |
| `test_billing.rs` | 7 | Billing page (patient), create-invoice requires admin, admin creates invoice (single/multi-item), bad items rejected, payment recording, **settled invoice rejects further payment** |
| `test_extended.rs` | 17 | Availability enforcement (blocked/recurring/open-default/past-date), doctor reassignment (success/failure-no-alternative/skips-busy), patient timeline (multi-entity merge), prescriptions, medical reports, audit logging, dashboard stats |
| `test_templates.rs` | 4 | Tera render tests for the new features: batch-reassignment page (empty form / populated plan / no-work states) and the live-availability booking form render with representative contexts, so a bad variable or filter fails the build |
| **Unit tests** (in `src/`) | **58** | Trait default-method tests (overlap/duration/priority/status), `DaySchedule` pure gap-finding **and per-slot `is_free` occupancy (drives the live booking dropdown)**, **`CostMatrix` Hungarian assignment (diagonal / beats-greedy / brute-force-optimum)**, **`parse_slot` clinic-hours + grid rules**, **error-page HTML-escaping**, enum serialization round-trips, **PDF word-wrap + real `%PDF` byte rendering**, **waitlist `expire()` domain transition (waiting→expired succeeds, accepted→expired rejected)** |
| **Total** | **195** | **100% pass rate** |

### Architecture

Tests use **in-memory SQLite** (`sqlite::memory:`) with full migrations for zero-config, isolated test runs. The `with_test_app!` macro builds a complete Actix app with all routes, session middleware, and stub Tera templates. `register_and_login!` and `seed_and_login!` provide authenticated sessions without duplicating registration boilerplate. Both macros avoid `actix-http` version conflicts by letting Rust infer all `Service` trait types.

---

## 📄 License

This project is developed as part of the CSC1106 Web Programming coursework at the University of Glasgow.
