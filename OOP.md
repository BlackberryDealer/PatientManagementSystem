    # 🔷 OOP Principles Audit — Patient Management System

## Scoring Summary

| Principle | Grade | Evidence |
|-----------|-------|----------|
| **Encapsulation** | 8/10 | Strong on domain entities; data bags for passive rows |
| **Abstraction (Traits)** | 9/10 | 4 custom traits + 3 framework traits, well-designed |
| **Polymorphism** | 9/10 | Trait-based, `dyn` + generic, framework-level extractors |
| **Single Responsibility** | 9/10 | Clear models/services/handlers separation per module |
| **Open/Closed Principle** | 8/10 | Traits enable extension; some service functions tightly coupled |
| **Domain-Driven Design** | 9/10 | Domain objects own their rules; forms own their validation |
| **Overall** | **8.7/10** | Strong OOP in a non-OOP language |

---

## 1. Encapsulation (8/10)

### ✅ Strong Encapsulation (Private Fields + Guarded Mutators)

These domain objects **hide their internals** and expose only controlled access paths:

| Struct | Private Fields | Guarded By |
|--------|---------------|------------|
| `User` | `password_hash`, `role` | `verify_password()`, `role()` returns typed `Role` |
| `Appointment` | `doctor_id`, `status`, `priority` | `reassign_to()` enforces "only scheduled", `cancel()` enforces lifecycle, `priority()` returns typed `Priority` |
| `Invoice` | `status` | `mark_paid()` enforces "only pending → paid", `can_accept_payment()` |
| `WaitlistEntry` | `status`, `priority` | `accept()` enforces "only waiting → accepted", `priority()` returns typed `Priority` |
| `DaySchedule` | `busy: Vec<(i32,i32)>` | `new()` sorts defensively; `earliest_gap()` is the only query |

This is textbook encapsulation — no caller can corrupt an `Appointment`'s doctor by writing to a field directly; they must go through `reassign_to()` which checks `is_active()` first.

### ⚠️ Weak Encapsulation (Public Data Bags)

These structs map 1:1 to database rows and expose all fields publicly:

| Struct | Issue |
|--------|-------|
| `MedicalRecord` | All `pub` fields, no domain methods beyond `Reportable` |
| `Prescription` | All `pub` fields, no domain methods beyond `Reportable` |
| `Patient` | All `pub` fields, only static helpers (`is_valid_blood_group`) |
| `Doctor` | All `pub` fields, only constants (`DEFAULT_SPECIALIZATION`) |
| `DoctorAvailability` | All `pub` fields — including `i32` bools that leak SQLite's type system; `recurring()`/`blocked()` accessors exist but duplicate the raw fields |

**Verdict**: The distinction is intentional — entities with **lifecycle state transitions** (Appointment, Invoice, WaitlistEntry) and **security-sensitive data** (User) are properly encapsulated. Passive row types (Patient, Doctor, MedicalRecord) are data bags, which is idiomatic Rust. However, `DoctorAvailability` could benefit from making `is_recurring`/`is_blocked` private since `recurring()`/`blocked()` already exist.

---

## 2. Abstraction via Traits (9/10)

Four custom traits capture shared behaviour across unrelated types — the Rust equivalent of abstract base classes:

```
┌──────────────────────────────────────────────────────┐
│                    TimeSlotted                       │
│  start_time()  end_time()                            │
│  overlaps_with()  contains()  duration_minutes()     │
├──────────────────────────────────────────────────────┤
│  ▲ Appointment    ▲ WaitlistEntry    ▲ DoctorAvailability │
└──────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────┐
│                   StatusManaged                      │
│  current_status()  is_active()  status_badge_class() │
│  status_color() (default impl)                       │
├──────────────────────────────────────────────────────┤
│  ▲ Appointment    ▲ Invoice    ▲ WaitlistEntry        │
└──────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────┐
│                   Prioritized                        │
│  priority_level()  priority_label()                  │
│  priority_badge_class()  is_higher_priority_than()   │
├──────────────────────────────────────────────────────┤
│  ▲ Appointment    ▲ WaitlistEntry                    │
└──────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────┐
│                   Reportable                         │
│  generate_summary() → String                         │
├──────────────────────────────────────────────────────┤
│  ▲ Appointment  ▲ Invoice  ▲ MedicalRecord  ▲ Prescription │
└──────────────────────────────────────────────────────┘
```

**Key design decisions:**
- `TimeSlotted` provides **default method implementations** (`overlaps_with`, `contains`, `duration_minutes`) — implementors only supply `start_time()` and `end_time()`. This is Rust's equivalent of inheriting behaviour from a base class.
- `StatusManaged` has a **default `status_color()`** that strips the `is-` prefix from `status_badge_class()` — a derived method that costs nothing for implementors.
- `any_conflict<T: TimeSlotted>()` is a **generic free function** demonstrating compile-time polymorphism — works with any type implementing the trait.

**Minor gap**: `Prioritized` has default `priority_label()` and `priority_badge_class()` but `Appointment` **overrides** them to delegate to the `Priority` enum instead. This creates two parallel implementations of the same logic — the `Priority` enum has `label()`/`css_class()`, and `Prioritized` has its own match. The `Priority` enum methods are the better single source of truth; the trait defaults could call through to them instead.

---

## 3. Polymorphism (9/10)

### Trait-Based Polymorphism (Custom)

```rust
// Compile-time (monomorphization):
pub fn any_conflict<T: TimeSlotted>(new_slot: &T, existing: &[impl TimeSlotted]) -> bool

// Run-time (dynamic dispatch via &dyn):
fn is_higher_priority_than(&self, other: &dyn Prioritized) -> bool
```

Both forms are demonstrated. The `any_conflict` function is the stronger example — it accepts `Appointment`, `WaitlistEntry`, or `DoctorAvailability` without caring which concrete type it receives.

### Framework-Level Polymorphism

```rust
impl FromRequest for AuthUser       // Extractor: enforced authentication
impl FromRequest for OptionalAuthUser // Extractor: optional authentication
impl ResponseError for AppError      // Unified error → HTTP response mapping
impl From<sqlx::Error> for AppError  // ? operator propagation
impl From<tera::Error> for AppError
impl From<bcrypt::BcryptError> for AppError
```

The `FromRequest` implementations are particularly strong OOP — two different types implementing the same trait with **different behaviour** (one returns 401, the other returns `None`).

### `Ord` for Priority Queue

```rust
impl Ord for PriorityItem {
    fn cmp(&self, other: &Self) -> Ordering {
        other.priority.cmp(&self.priority)  // reversed: lower # = more urgent
            .then_with(|| other.created_at.cmp(&self.created_at))
    }
}
```

Implements Rust's standard ordering trait with a **custom domain-specific ordering** — lower priority number = higher urgency. This is what lets `BinaryHeap<PriorityItem>` work correctly.

---

## 4. Single Responsibility Principle (9/10)

Each module follows a consistent 4-file structure:

```
src/{module}/
├── mod.rs       — Route configuration only (pub fn configure)
├── models.rs    — Data structures, validation, domain methods
├── services.rs  — Business logic, database queries
└── handlers.rs  — HTTP request/response handling
```

**Good examples:**
- `BookAppointmentForm::validate()` — the form owns its own validation rules
- `CreateInvoiceForm::parse_line_items()` — the form owns its parsing logic
- `DaySchedule::earliest_gap()` — pure scheduling math, no DB dependency
- Handlers only extract inputs, call services, and render templates — no business logic in handlers

**One violation**: The `calendar_view` handler in `appointments/handlers.rs` contains a non-trivial `match user.role.as_str()` block that constructs role-specific SQL queries inline. This query-building logic should live in `appointments/services.rs` alongside `get_appointment_counts_by_date`. The handler should only pass the role and get back results.

---

## 5. Open/Closed Principle (8/10)

**Open for extension:**
- New appointment statuses? Add a variant to `AppointmentStatus` — the compiler flags every `match` that needs updating.
- New entity that occupies time? Implement `TimeSlotted` — `any_conflict` works immediately.
- New role? Add a variant to `Role` — all `require_role` guards update automatically.
- New module? Add a directory, impl `configure()`, and mount it in main.rs.

**Partially closed for modification:**
- Adding a new scheduling algorithm requires modifying `appointments/services.rs` (new function) and `appointments/handlers.rs` (new route handler) — but this is acceptable since it's new functionality, not modifying existing behaviour.
- The `check_conflict` function uses mutable string building for SQL (`sql.push_str(...)`) based on optional parameters. A query-builder pattern would be more extensible but the current approach works for the 4 optional clauses.

---

## 6. Domain-Driven Design (9/10)

### Domain Objects Own Their Rules

This is the strongest OOP aspect of the codebase. Business rules live ON the objects, not scattered through services:

```rust
// ✅ Rule on the domain object
appointment.cancel()?;          // Checks is_active() first
appointment.reassign_to(dr_id)?; // Checks is_active() first
invoice.mark_paid()?;           // Checks can_accept_payment() first
entry.accept()?;                // Checks status == Waiting first
user.verify_password(candidate)?; // Private field access only through this

// The caller just persists:
sqlx::query("UPDATE appointments SET status = ?, doctor_id = ? WHERE id = ?")
    .bind(appointment.current_status())
    .bind(appointment.doctor_id())
    .bind(appointment.id)
    .execute(&mut tx).await?;
```

### Form Validation Chain

Every form goes through the same pipeline:
```
Route (extract form) → Form::validate() → Service business logic → Database
```

This is consistent across all 7 modules. No validation logic is duplicated between forms and services.

### Typed Enums Replace Magic Strings

| Instead of | Uses | Benefit |
|-----------|------|---------|
| `role == "admin"` | `user.role() == Role::Admin` | Compiler checks exhaustiveness |
| `status = "scheduled"` | `AppointmentStatus::Scheduled` | Impossible to typo |
| `priority = 3` | `Priority::Normal` | Self-documenting |
| `if role == "pateint"` | Won't compile | Typos caught at build time |

---

## 📊 Detailed Findings

### 🔴 Issues (1)

| # | Issue | File | Severity |
|---|-------|------|----------|
| 1 | **Query-building in handler** — `calendar_view()` constructs role-specific SQL inline in the handler rather than delegating to the service layer. Violates SRP: the handler should not know SQL. | `src/appointments/handlers.rs:240-265` | Medium |

### 🟡 Recommendations (3)

| # | Recommendation | Rationale |
|---|---------------|-----------|
| 1 | Make `DoctorAvailability.is_recurring` and `is_blocked` private | `recurring()`/`blocked()` accessors already exist; raw `i32` fields leak SQLite's type system |
| 2 | Unify `Priority` enum methods with `Prioritized` trait defaults | Two parallel implementations of the same priority→label mapping exist — `Priority::label()` and `Prioritized::priority_label()`. The trait defaults should delegate to the enum. |
| 3 | Consider a `Lifecycled` trait combining `StatusManaged` with a guarded transition | `cancel()`, `mark_paid()`, `accept()` all follow the same pattern (check `is_active()`, mutate, return `Result`). A `try_transition(&mut self, from, to) -> Result` default method would eliminate this duplication. |

### 🟢 Strengths (7)

1. **Private fields on all stateful entities** — `User.password_hash`, `Appointment.doctor_id/status/priority`, `Invoice.status`, `WaitlistEntry.status/priority` are all properly encapsulated
2. **Guarded state transitions** — `cancel()`, `reassign_to()`, `mark_paid()`, `accept()` all check preconditions via `&mut self`
3. **Four well-designed custom traits** with meaningful default implementations
4. **Polymorphic `any_conflict` function** demonstrating compile-time trait-based dispatch
5. **Framework trait implementations** — `FromRequest` (2 types), `ResponseError`, `From` conversions (5 types)
6. **Consistent form validation pattern** — every form owns its own `validate()` method
7. **Type-safe enums throughout** — no stringly-typed comparisons anywhere in the codebase

---

## 🏆 Overall Assessment: 8.7/10

The codebase demonstrates **mature OOP design in Rust**. Encapsulation is applied where it matters most (lifecycle state, security credentials), traits provide meaningful polymorphism, and domain objects consistently own their business rules. The one SRP violation (SQL in a handler) and the minor duplication in priority mappings are the only blemishes on an otherwise well-crafted architecture.