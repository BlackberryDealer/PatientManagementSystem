# 🏥 Patient Management System — The Complete Walkthrough

> **A beginner-to-pro guide covering every concept, every file, and every decision in this project.**  
> Written for the CSC1106 Web Programming course at the University of Glasgow.

---

## Table of Contents

1. [What Are We Building? (The ELI5)](#1-what-are-we-building-the-eli5)
2. [The Technology Stack — What Each Tool Does and Why We Chose It](#2-the-technology-stack--what-each-tool-does-and-why-we-chose-it)
3. [How a Web Application Actually Works](#3-how-a-web-application-actually-works)
4. [The Project Structure — A Guided Tour](#4-the-project-structure--a-guided-tour)
5. [The `main()` Function — Where Everything Starts](#5-the-main-function--where-everything-starts)
6. [Database & Migrations — How We Store Data](#6-database--migrations--how-we-store-data)
7. [Error Handling — Why We Made Our Own `AppError`](#7-error-handling--why-we-made-our-own-apperror)
8. [Authentication & Sessions — Who Are You?](#8-authentication--sessions--who-are-you)
9. [Module Deep Dives](#9-module-deep-dives)
   - [9.1 Users Module](#91-users-module)
   - [9.2 Appointments Module — Three Scheduling Algorithms](#92-appointments-module-core-feature)
     - [Algorithm 1: Time Interval Overlap Detection](#algorithm-1-time-interval-overlap-detection)
     - [Algorithm 2: Earliest Available Slot](#algorithm-2-earliest-available-slot)
     - [Algorithm 3: Priority-Based Scheduling with BinaryHeap](#algorithm-3-priority-based-scheduling-with-binaryheap)
   - [9.2b Rooms & Resource Scheduling](#92b-rooms--resource-scheduling)
   - [9.2c Waitlist & Priority Queue](#92c-waitlist--priority-queue)
   - [9.3 Availability Module](#93-availability-module)
   - [9.4 Medical Records Module](#94-medical-records-module)
   - [9.5 Billing Module](#95-billing-module)
10. [Templates (HTML) — How the Browser Sees Your App](#10-templates-html--how-the-browser-sees-your-app)
11. [Life of a Request — Two Full Walkthroughs](#11-life-of-a-request--two-full-walkthroughs)
12. [Key Rust Concepts Used in This Project](#12-key-rust-concepts-used-in-this-project)
13. [How to Add a New Feature (Step-by-Step)](#13-how-to-add-a-new-feature-step-by-step)
14. [Common Errors and How to Fix Them](#14-common-errors-and-how-to-fix-them)
15. [Glossary of Jargon](#15-glossary-of-jargon)
16. [Testing the Application](#16-testing-the-application)

---

## 1. What Are We Building? (The ELI5)

Imagine a hospital needs software to manage patients, doctors, appointments, medical records, and bills. Instead of paper folders and phone calls, everything lives in a web application that anyone with a web browser can access.

**You open your browser, go to a URL like `http://localhost:8080`, and you see web pages where you can:**

- 🧑‍⚕️ **Register** as a patient, doctor, or admin
- 📅 **Book appointments** with doctors — the system automatically prevents double-booking
- 🕐 **Set availability** — doctors tell the system when they work
- 📋 **Create medical records** — doctors write diagnoses and treatments
- 💰 **Generate invoices** and record payments

All of this runs on a single computer (your laptop) as a **web server** — a program that listens for requests from your browser, does work, and sends back HTML pages.

> **"Web server"** = A program that waits for incoming connections (from browsers) and responds with data (usually HTML pages). Think of it like a waiter at a restaurant: the browser places an order (a URL), the kitchen (your Rust code) prepares the dish (the HTML page), and the waiter delivers it back.

---

## 2. The Technology Stack — What Each Tool Does and Why We Chose It

A "tech stack" is simply the list of technologies a project uses, stacked on top of each other like layers of a cake. Here's ours, from bottom to top:

```
┌──────────────────────────────────────────────────┐
│  Browser (Chrome/Firefox/Edge)                   │  ← User sees this
├──────────────────────────────────────────────────┤
│  Bulma CSS (styling framework)                   │  ← Makes pages look nice
├──────────────────────────────────────────────────┤
│  Tera Templates (HTML with superpowers)          │  ← Generates HTML dynamically
├──────────────────────────────────────────────────┤
│  Actix Web 4 (HTTP framework)                    │  ← Handles requests/responses
├──────────────────────────────────────────────────┤
│  Our Rust Code (business logic)                  │  ← The actual "brain"
├──────────────────────────────────────────────────┤
│  SQLx (database access library)                  │  ← Talks to the database
├──────────────────────────────────────────────────┤
│  SQLite (database engine)                        │  ← Stores all the data
└──────────────────────────────────────────────────┘
```

### 2.1 Rust (The Programming Language)

**What is it?** Rust is a systems programming language created by Mozilla (the Firefox people). It's known for being extremely fast, memory-safe without needing a garbage collector, and having a strict compiler that catches bugs before your code even runs.

**Why did we choose it for a web app?** The course requires it. But beyond that:

| Rust Feature | What It Means for This Project |
|---|---|
| **Speed** | Rust is as fast as C/C++. Our server can handle thousands of requests per second. |
| **Memory Safety** | Rust prevents entire categories of bugs (null pointers, buffer overflows, use-after-free). The compiler won't let you write code that could crash in these ways. |
| **`async`/`await`** | Rust supports asynchronous programming (doing many things at once without getting confused). Our web server can handle 100 users simultaneously. |
| **Strong Type System** | If your code compiles, an enormous number of potential bugs are impossible. You can't accidentally treat a number as a string, or forget to handle an error. |
| **Cargo** | Rust's package manager and build system. `cargo run` downloads dependencies, compiles everything, and runs your app. One command. |

> **"Compile"** = The process of translating human-readable code into machine-readable instructions. Rust's compiler (`rustc`) is notorious for being strict — if your code has a potential problem, it will refuse to compile and tell you exactly what's wrong, rather than letting the bug slip into production.

### 2.2 Actix Web 4 (The Web Framework)

**What is it?** Actix Web is the most popular web framework for Rust. A "web framework" is a toolbox that handles all the boring plumbing of a web server so you can focus on your application logic.

**What does it do for us?**

| Problem | Actix Web's Solution |
|---|---|
| "How do I listen for HTTP requests?" | `HttpServer::new()` — one line to start a server |
| "How do I route `/users/login` to a function?" | `.route("/users/login", web::get().to(login))` |
| "How do I share the database connection?" | `web::Data<SqlitePool>` — thread-safe shared state |
| "How do I read form data from a POST request?" | `web::Form<LoginForm>` — automatic deserialization |
| "How do I send back HTML?" | Return `HttpResponse::Ok().body(html_string)` |

> **"HTTP"** = HyperText Transfer Protocol. It's the language browsers and servers speak to each other. When you type a URL in your browser, it sends an HTTP request (like "GET /users/login"). The server processes it and sends back an HTTP response (like "200 OK" with an HTML page).

### 2.3 Tera (The Templating Engine)

**What is it?** Tera is a "templating engine" — it takes HTML files with special placeholder syntax and fills in real data at runtime.

**Why not just write HTML strings in Rust?** Because that would be terrible:

```rust
// ❌ Horrible — mixing HTML strings in Rust code
let html = format!("<h1>Welcome, {}</h1><p>Your role: {}</p>", user.name, user.role);
```

```html
<!-- ✅ Clean — Tera template (users/profile.html.tera) -->
<h1>Welcome, {{ user.username }}</h1>
<p>Your role: {{ user.role }}</p>
```

**Key Tera features we use:**

| Feature | Syntax | Example |
|---|---|---|
| **Variable** | `{{ name }}` | `{{ user.username }}` |
| **If/Else** | `{% if condition %}` | `{% if user.role == "admin" %}` |
| **For Loop** | `{% for item in list %}` | `{% for a in appointments %}` |
| **Template Inheritance** | `{% extends "base.html.tera" %}` | All pages share the same navbar/footer |
| **Includes** | `{% include "shared/navbar.html.tera" %}` | Reusable components |
| **Filters** | `{{ value \| filter }}` | `{{ name \| upper }}`, `{{ price \| format("%.2f") }}` |

> **"SSR" (Server-Side Rendering)** = The server generates the full HTML page before sending it to the browser. This is what Tera does. The alternative is CSR (Client-Side Rendering) where JavaScript in the browser builds the page. SSR is simpler, works without JavaScript, and is better for SEO.

### 2.4 SQLx + SQLite (The Database Layer)

**What is SQLite?** SQLite is a database that stores everything in a single file (`patient_management.db`). Unlike MySQL or PostgreSQL which require separate server processes, SQLite is embedded directly in your application. It's perfect for development and small-to-medium applications.

**What is SQLx?** SQLx is a Rust library that lets you write SQL queries directly in your Rust code, with compile-time checking. It's not an ORM — you write real SQL, but the library handles all the type conversion and connection pooling for you.

> **"ORM" (Object-Relational Mapper)** = A tool that converts database rows into programming language objects automatically. SQLx is NOT a full ORM — it's a "query builder" that lets you write real SQL but handles the Rust ↔ SQL type conversions. This gives you more control than an ORM while still being safe.

**Why SQLx instead of raw SQLite?**

| Without SQLx | With SQLx |
|---|---|
| Manual string building: `format!("SELECT * FROM users WHERE id = {}", id)` | Parameterized queries: `.bind(id)` — safe from SQL injection |
| Manual type conversion: parse strings into Rust types | Automatic: `query_as::<_, User>(sql)` maps columns to struct fields |
| No connection pooling: one connection, one query at a time | Connection pool: 5 connections, many queries simultaneously |
| No migration system: manually run SQL files | `sqlx::migrate!()`: runs all pending migrations automatically |

> **"SQL Injection"** = A security attack where someone puts SQL code into input fields. Example: entering `' OR '1'='1` as a username tricks the database into returning all users. SQLx prevents this by using parameterized queries (`?` placeholders), which separate the SQL structure from the data values.

> **"Connection Pool"** = A set of pre-opened database connections that are reused. Opening a new database connection is slow (like dialing a phone number). A pool keeps several connections "on hold" (like having multiple phone lines), so you can grab one instantly when needed and return it when done.

### 2.5 bcrypt (Password Hashing)

**What is it?** bcrypt is an algorithm that turns a password (like `"hunter2"`) into a scrambled, irreversible string (like `"$2b$10$N9qo8uLOickgx2ZMRZoMyeIjZAgcfl7p92ldGxad68LJZdL17lhWy"`).

**Why can't we just store passwords as plain text?** If someone steals your database, they'd see every user's password. With bcrypt:
1. The original password can NEVER be recovered from the hash (one-way function)
2. Even if two users have the same password, their hashes look completely different (salting)
3. bcrypt is intentionally slow (by design), making brute-force attacks impractical

> **"Hash"** = A one-way mathematical function. You can turn `"password123"` into `"$2b$10$abc...xyz"`, but you can't go backwards. When a user logs in, you hash their input and compare it with the stored hash — if they match, the password was correct.

> **"Salt"** = Random data added to each password before hashing. This ensures that even identical passwords produce different hashes, preventing attackers from using pre-computed "rainbow tables."

### 2.6 Bulma CSS (Styling)

**What is it?** Bulma is a CSS framework — a pre-built collection of styles that makes web pages look professional without writing custom CSS. We load it from a CDN (Content Delivery Network) link in our base template.

**Why not write our own CSS?** Because this is a backend-focused course. Bulma gives us:
- Responsive navigation bars
- Clean tables and forms
- Cards, notifications, tags
- Grid layout system (columns)
- All without writing a single line of CSS

**What we use Bulma for:**

| Component | Where | What It Looks Like |
|---|---|---|
| `navbar` | `shared/navbar.html.tera` | Top navigation bar |
| `box` | Login/register forms | Card-like container with shadow |
| `table` | Every list page | Striped, hoverable data tables |
| `tag` | Status badges | `scheduled`, `paid`, `cancelled` |
| `notification` | Empty state messages | "No appointments found" |
| `card` | Detail pages | Structured info display |
| `columns` | Layout | Side-by-side content |

---

## 3. How a Web Application Actually Works

Before diving into our code, let's understand the fundamental flow:

```
┌──────────────┐         ① HTTP Request           ┌──────────────────┐
│              │  ──────────────────────────────>  │                  │
│   Browser    │     GET /appointments             │   Our Server     │
│  (Chrome)    │                                   │  (Actix Web)    │
│              │  <──────────────────────────────  │  (Rust code)    │
└──────────────┘    ② HTTP Response (HTML)         └──────┬───────────┘
                                                          │
                                                    ③ Queries
                                                          │
                                                    ┌─────▼───────────┐
                                                    │   SQLite DB     │
                                                    │  (.db file)     │
                                                    └─────────────────┘
```

**Step by step when you visit `http://localhost:8080/appointments`:**

1. **Your browser** sends an HTTP GET request to the server
2. **Actix Web** receives it, looks at the URL path `/appointments`
3. **Routing**: Actix matches `/appointments` → `appointments::handlers::list_appointments`
4. **Middleware**: The session middleware reads your session cookie to identify you
5. **Extractor**: `AuthUser` checks if you're logged in (401 if not)
6. **Handler**: `list_appointments()` runs, querying the database for your appointments
7. **Tera**: The data is passed to a Tera template, which renders HTML
8. **Response**: The HTML is sent back to your browser as an HTTP response
9. **Your browser** displays the HTML page

> **"Middleware"** = Code that runs on every request before your handler, like a security checkpoint at an airport. Our session middleware reads the session cookie and makes user info available. Other middleware could handle logging, compression, or CORS headers.

> **"Extractor"** = An Actix Web concept: types that implement `FromRequest` can be used as handler parameters. Actix automatically calls the extractor to get the value from the incoming request. `AuthUser` is our custom extractor.

---

## 4. The Project Structure — A Guided Tour

Let's walk through every folder and understand its purpose:

```
PatientManagementSystem/
│
├── Cargo.toml                  ← 📋 Project manifest: name, version, dependencies
├── Cargo.lock                  ← 🔒 Exact versions of all dependencies (auto-generated)
├── .env                        ← 🔧 Environment variables (DATABASE_URL, RUST_LOG)
├── .gitignore                  ← 🙈 Files Git should ignore (target/, *.db)
├── README.md                   ← 📖 Project overview and setup instructions
├── WALKTHROUGH.md              ← 📚 This file! The comprehensive guide.
│
├── migrations/                 ← 🗄️ Database schema versions
│   ├── 001_initial_schema.sql  ←    Creates all 13 tables and indexes
│   ├── 002_rooms_priority.sql  ←    Seeds default consultation rooms
│   ├── 003_audit_log.sql       ←    Immutable action trail table
│   ├── 004_fix_payment_dates.sql
│   └── 005_doctor_room_assignments.sql ← Doctor-room auto-allocation table
│
├── tests/                      ← 🧪 Integration test suite (133 tests)
│   ├── common/
│   │   └── mod.rs              ←    Test macros, in-memory DB, auth helpers
│   ├── test_auth.rs            ←    Registration, login, role guards (19)
│   ├── test_algorithms.rs      ←    Scheduling algorithm tests (26)
│   ├── test_appointments.rs    ←    HTTP booking & waitlist tests (16)
│   ├── test_availability.rs    ←    Availability CRUD tests (3)
│   ├── test_records.rs         ←    Medical records + PDF export tests (6)
│   ├── test_billing.rs         ←    Invoice & payment tests (6)
│   └── test_extended.rs        ←    Reassignment, timeline, audit, dashboard (17)
│
├── templates/                  ← 🎨 Root HTML templates (loaded first by Tera)
│   ├── base.html.tera          ←    The HTML skeleton every page uses
│   └── shared/                 ←    Reusable template pieces
│       ├── navbar.html.tera    ←    Top navigation bar
│       └── footer.html.tera    ←    Page footer
│
└── src/                        ← 🦀 All Rust source code
    ├── main.rs                 ←    🚀 Entry point: starts the server
    ├── lib.rs                  ←    📚 Library crate for integration tests
    ├── db.rs                   ←    🗃️ Database pool creation & migration runner
    ├── errors.rs               ←    ❌ Custom error type for the whole app
    ├── auth.rs                 ←    🔐 Session user extractors & role guards
    ├── traits.rs               ←    🧬 OOP traits: TimeSlotted, StatusManaged, etc.
    ├── bin/
    │   └── seed.rs             ←    🌱 Database seeder (cargo run --bin seed)
    │
    ├── users/                  ← 👤 User management module
    │   ├── mod.rs              ←    Route configuration
    │   ├── models.rs           ←    Data structures (User, RegisterForm, Patient, Doctor)
    │   ├── services.rs         ←    Business logic (register, authenticate, queries)
    │   ├── handlers.rs         ←    HTTP request handlers (endpoints)
    │   └── templates/          ←    User-related HTML templates
    │       ├── register.html.tera
    │       ├── login.html.tera
    │       ├── list.html.tera
    │       └── profile.html.tera
    │
    ├── appointments/           ← 📅 Appointment scheduling module (CORE)
    │   ├── mod.rs
    │   ├── models.rs           ←    Appointment, Room, DoctorRoomAssignment, Priority, WaitlistEntry, forms
    │   ├── services.rs         ←    4 scheduling algorithms, waitlist, priority queue
    │   ├── handlers.rs         ←    List, book, priority-book, suggest, waitlist, cancel
    │   └── templates/
    │       ├── list.html.tera
    │       ├── book.html.tera
    │       ├── suggest.html.tera
    │       ├── waitlist.html.tera
    │       └── detail.html.tera
    │
    ├── availability/           ← 🕐 Doctor schedule module
    │   ├── mod.rs
    │   ├── models.rs           ←    DoctorAvailability, SetAvailabilityForm
    │   ├── services.rs         ←    CRUD for availability slots
    │   ├── handlers.rs         ←    List and set availability endpoints
    │   └── templates/
    │       ├── list.html.tera
    │       └── set.html.tera
    │
    ├── records/                ← 📋 Medical records, prescriptions, timeline & PDF
    │   ├── mod.rs
    │   ├── models.rs           ←    MedicalRecord, Prescription, RecordReportData, TimelineEvent
    │   ├── services.rs         ←    Create/list records, prescriptions, timeline, report data
    │   ├── pdf.rs              ←    Pure-Rust PDF generation for medical reports
    │   ├── handlers.rs         ←    CRUD endpoints + PDF export
    │   └── templates/          ←    list, create, detail, timeline, report, prescription_form
    │
    ├── billing/                ← 💰 Billing & invoices module
    │   ├── mod.rs
    │   ├── models.rs           ←    Invoice, InvoiceItem, Payment, forms
    │   ├── services.rs         ←    Invoice CRUD, payment processing (partial payments)
    │   ├── handlers.rs         ←    List, create, view, pay endpoints
    │   └── templates/          ←    list, create (itemized builder), detail
    │
    ├── audit/                  ← 📝 Immutable action trail (who did what, when)
    │   ├── mod.rs · models.rs · services.rs · handlers.rs
    │   └── templates/list.html.tera
    │
    └── dashboard/              ← 📊 Admin analytics & statistics
        ├── mod.rs · models.rs · services.rs · handlers.rs
        └── templates/index.html.tera
```

### The Module Pattern

Every module follows the exact same four-file pattern. Let's understand why:

| File | Purpose | Real-World Analogy |
|---|---|---|
| **`mod.rs`** | Declares sub-modules and routes | The receptionist — directs incoming requests to the right handler |
| **`models.rs`** | Defines data structures | The paperwork forms — what data looks like |
| **`services.rs`** | Contains business logic | The expert — knows HOW to do things (register, book, check conflicts) |
| **`handlers.rs`** | HTTP endpoint functions | The customer service rep — receives requests, calls services, returns responses |

> **Why separate models, services, and handlers?** This is called **Separation of Concerns**. If you put everything in one file:
> - Changing the database would require rewriting the entire app
> - Testing business logic would require simulating HTTP requests
> - Multiple team members would constantly conflict on the same file
>
> With separation, each person can work on their module independently. The handler doesn't know about SQL. The service doesn't know about HTTP. The model doesn't know about either.

---

## 5. The `main()` Function — Where Everything Starts

Let's trace through `src/main.rs` line by line:

### 5.1 Module Declarations

```rust
// Cross-cutting infrastructure
pub mod auth;
pub mod db;
pub mod errors;
pub mod time;     // clinic-hours / slot-grid time helpers
pub mod traits;   // OOP traits: TimeSlotted, StatusManaged, Prioritized, Reportable

// Business domains (one folder each)
pub mod users;
pub mod appointments;
pub mod availability;
pub mod records;
pub mod billing;
pub mod audit;      // immutable action trail
pub mod dashboard;  // admin analytics
```

These declarations live in `src/lib.rs` (the library crate), so both `main.rs` and the integration tests can share the same modules. In Rust, `mod` tells the compiler "these modules exist as separate files." Without these lines, Rust wouldn't know about `src/users/mod.rs`, `src/db.rs`, etc. This is how Rust organizes code across multiple files — you declare the module in the parent, and Rust finds the corresponding file.

> **The difference between `mod` and `use`:**
> - `mod foo;` — "There is a module called `foo`. Find it in `foo.rs` or `foo/mod.rs`."
> - `use foo::Bar;` — "Bring `Bar` from module `foo` into the current scope so I can write `Bar` instead of `foo::Bar`."

### 5.2 The `#[actix_web::main]` Attribute

```rust
#[actix_web::main]
async fn main() -> std::io::Result<()> {
```

This is a **procedural macro** that transforms our `main()` function behind the scenes. It sets up the **async runtime** (tokio) that Actix Web needs. Without this attribute, `async fn main()` wouldn't work in Rust because the standard library doesn't include an async runtime.

> **"Async" (Asynchronous)** = Code that can do multiple things at once without getting stuck. In a web server, while one request is waiting for the database, the server can handle other requests. Think of a chef who puts a pizza in the oven and starts making a salad instead of standing and waiting for the pizza.

### 5.3 Environment Setup

```rust
dotenv::dotenv().ok();
env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));
```

| Line | What It Does |
|---|---|
| `dotenv::dotenv().ok()` | Reads the `.env` file and makes variables available via `std::env::var("DATABASE_URL")` |
| `env_logger::init_from_env(...)` | Sets up logging. `RUST_LOG=info` means show info, warning, and error messages |

> **`.env` files** separate configuration from code. Instead of hardcoding `DATABASE_URL` in your Rust source, you put it in `.env`. This means different environments (development, testing, production) can use different `.env` files without changing any code. The `.gitignore` ensures `.env` is never committed to version control, protecting secrets.

### 5.4 Database Initialization

```rust
let database_url = std::env::var("DATABASE_URL")
    .unwrap_or_else(|_| "sqlite:patient_management.db?mode=rwc".to_string());

let pool = db::create_pool(&database_url).await;
db::run_migrations(&pool).await;
```

1. Read `DATABASE_URL` from `.env` (or use the default SQLite path)
2. Create a connection pool (5 connections)
3. Run all SQL migration files in the `migrations/` folder

> **"Migration"** = A version-controlled SQL script that changes the database schema. Each migration adds or modifies tables. They're applied in order and tracked, so the database is always in the correct state. Think of them as "commits" for your database structure.

### 5.5 Template Engine Setup

```rust
let mut tera = match tera::Tera::new("templates/**/*.tera") {
    Ok(t) => t,
    Err(e) => {
        log::error!("Failed to load root templates: {}", e);
        std::process::exit(1);
    }
};

load_module_templates(&mut tera).expect("Failed to load module templates");
tera.autoescape_on(vec![".tera"]);
```

| Line | What It Does |
|---|---|
| `Tera::new("templates/**/*.tera")` | Loads all `.tera` files from `templates/` using a glob pattern |
| `load_module_templates(&mut tera)` | Our custom function — walks through `src/{module}/templates/` and loads each `.tera` file |
| `tera.autoescape_on(vec![".tera"])` | **Enables** HTML auto-escaping for all `.tera` templates, so any user-supplied value interpolated with `{{ }}` is escaped — a key XSS defence. Use the `\| safe` filter only for trusted, pre-escaped HTML. |

The `load_module_templates()` function is interesting. It:
1. Iterates over the 7 module names: `["users", "appointments", "availability", "records", "billing", "audit", "dashboard"]`
2. Reads each `src/{module}/templates/` directory
3. For every `.tera` file, reads its content and registers it with the name `{module}/{filename}`

This means `src/appointments/templates/list.html.tera` becomes available as `appointments/list.html.tera` in Tera.

### 5.6 Session Middleware (with Persistent Key)

```rust
// Session encryption key — persisted in .env across server restarts
let secret_key = get_or_create_secret_key();

HttpServer::new(move || {
    App::new()
        .app_data(web::Data::new(pool.clone()))
        .app_data(web::Data::new(tera.clone()))
        .wrap(
            SessionMiddleware::builder(CookieSessionStore::default(), secret_key.clone())
                .cookie_secure(false)
                .build(),
        )
        // ... routes
})
```

| Component | Purpose |
|---|---|
| `get_or_create_secret_key()` | **Persistent key** — reads `SESSION_SECRET` from `.env`, or generates 64 random bytes (128 hex chars) and saves them to `.env` on first run |
| `CookieSessionStore` | Stores session data inside encrypted cookies (no server-side storage needed) |
| `cookie_secure(false)` | Allows cookies over HTTP (in production, this should be `true` for HTTPS) |

**Why persist the key?** Without persistence, `Key::generate()` creates a new random key on every server restart. Old browser cookies can't be decrypted, producing warnings like:

```
WARN  actix_session::middleware] The session cookie failed to pass cryptographic checks
```

With `get_or_create_secret_key()`, the same key is reused across restarts. Sessions survive — no forced re-login, no warning spam. The key is auto-generated once and stored as `SESSION_SECRET=<128 hex chars>` in `.env`.

> **"Cookie"** = A small piece of data stored in your browser. When you log in, the server tells your browser "remember this session ID." On every subsequent request, your browser automatically sends the cookie back. This is how the server knows you're the same person across different page loads.

> **"Session"** = Server-side data associated with a specific user. Our project stores the session data directly in a signed, encrypted cookie — no server-side storage needed. The `Key` ensures the cookie can't be forged.

### 5.7 Route Mounting

```rust
.route("/", web::get().to(index))
.configure(users::configure)
.configure(appointments::configure)
.configure(availability::configure)
.configure(records::configure)
.configure(billing::configure)
```

Each module's `configure()` function registers its routes under a scope. For example, `users::configure()` does:

```rust
cfg.service(
    web::scope("/users")
        .route("/register", web::get().to(handlers::register_form))
        .route("/register", web::post().to(handlers::register))
        .route("/login", web::get().to(handlers::login_form))
        .route("/login", web::post().to(handlers::login))
        // ...
);
```

Notice that `/register` appears twice — once for GET (show the form) and once for POST (process the form). This is standard RESTful design.

> **"GET vs POST"**: GET requests are for reading data (showing a page). POST requests are for sending data (submitting a form). GET parameters appear in the URL. POST parameters are in the request body (hidden from the URL).

---

## 6. Database & Migrations — How We Store Data

### 6.1 The Entity-Relationship Model

Before writing SQL, we designed the data model. Here's how the tables relate:

```
┌──────────┐         ┌──────────────┐         ┌───────────┐
│  users   │────────>│   patients   │<────────│ invoices  │
│          │    1:1   │              │   1:N    │           │
│ id (PK)  │         │ user_id (FK) │         │ patient_id│──────┐
│ username │         │ date_of_birth│         │ status    │      │
│ role     │         │ phone        │         │ total     │      │
└──────────┘         └──────────────┘         └───────────┘      │
     │                                                           │
     │ 1:1                                             1:N ┌─────▼────────┐
     ▼                                                     │ invoice_items │
┌──────────┐                                              │ invoice_id(FK)│
│ doctors  │                                              │ description   │
│          │                                              └───────────────┘
│ user_id  │──┐                                                  │
│ spec     │  │ 1:N                                   1:N ┌──────▼──────┐
└──────────┘  │                                            │  payments   │
              │                                            │ invoice_id  │
              ├──────────────┬──────────────┐              │ amount      │
              │              │              │              └─────────────┘
         ┌────▼─────┐  ┌─────▼──────┐  ┌───▼───────────┐
         │ doctor_  │  │appointments│  │medical_records │
         │availability│ │            │  │                │
         │          │  │ doctor_id  │  │ patient_id(FK) │
         │ doctor_id│  │ patient_id │  │ doctor_id(FK)  │
         │ day/time │  │ date/time  │  │ appt_id (FK)   │
         └──────────┘  │ status     │  │ diagnosis      │
                       └────────────┘  └────────────────┘
                              │
                              │ 1:N
                        ┌─────▼───────┐
                        │prescriptions│
                        │ patient_id  │
                        │ doctor_id   │
                        │ medication  │
                        └─────────────┘
```

> **"PK" (Primary Key)** = A column that uniquely identifies each row in a table. Every table must have one. In our schema, all PKs are `id INTEGER PRIMARY KEY AUTOINCREMENT`, which means SQLite automatically assigns incrementing numbers.
>
> **"FK" (Foreign Key)** = A column that references the primary key of another table. This creates a relationship. For example, `appointments.doctor_id` is an FK to `doctors.id`. This ensures you can't create an appointment for a non-existent doctor.
>
> **"1:1" (One-to-One)** = Each user has exactly one patient or doctor profile.
>
> **"1:N" (One-to-Many)** = One patient can have many appointments, many invoices, many medical records.

### 6.2 The Schema — Table by Table

#### `users` — Everyone starts here
```sql
CREATE TABLE users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,    -- NEVER stores plain-text passwords
    role TEXT NOT NULL CHECK (role IN ('patient', 'doctor', 'admin')),
    full_name TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

The `CHECK` constraint ensures `role` can only be one of three values. The `UNIQUE` constraint on `username` and `email` prevents duplicates. The `password_hash` stores the bcrypt-hashed password, never the original.

#### `patients` and `doctors` — Role-specific profiles
Patients have medical info (DOB, blood group, emergency contact). Doctors have professional info (specialization, license number). Both link back to `users` via `user_id` FK with CASCADE delete — if a user is deleted, their profile is automatically deleted too.

> **"CASCADE DELETE"** = When you delete a parent row, all child rows are automatically deleted. If we delete a user, their patient/doctor profile is also deleted, preventing "orphan" records.

#### `doctor_availability` — When doctors work
```sql
CREATE TABLE doctor_availability (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    doctor_id INTEGER NOT NULL,
    day_of_week INTEGER NOT NULL CHECK (day_of_week BETWEEN 0 AND 6),
    start_time TEXT NOT NULL,        -- "09:00"
    end_time TEXT NOT NULL,          -- "17:00"
    is_recurring BOOLEAN NOT NULL DEFAULT 1,
    specific_date DATE,              -- for one-off blocked days
    is_blocked BOOLEAN NOT NULL DEFAULT 0,
    ...
);
```

Two modes:
- **Recurring** (`is_recurring = 1`): "Every Monday 9 AM to 5 PM"
- **Specific** (`specific_date` set): "Blocked on Dec 25" or "Available on a holiday"

#### `appointments` — The core table
```sql
CREATE TABLE appointments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    patient_id INTEGER NOT NULL,
    doctor_id INTEGER NOT NULL,
    appointment_date DATE NOT NULL,
    start_time TEXT NOT NULL,        -- "14:00"
    end_time TEXT NOT NULL,          -- "14:30"
    status TEXT NOT NULL CHECK (status IN ('scheduled', 'completed', 'cancelled')),
    notes TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ...
);
```

Status workflow: `scheduled` → `completed` (doctor marks done) or `cancelled` (patient/doctor cancels). Cancelled appointments are kept for audit but excluded from conflict checks.

### 6.3 Indexes — Making Queries Fast

Without indexes, finding appointments for a specific doctor requires scanning EVERY row in the appointments table (a "full table scan"). With an index on `(doctor_id, appointment_date)`, the database can jump directly to the relevant rows — like using a book's index instead of reading every page.

```sql
CREATE INDEX idx_appointments_doctor_date ON appointments(doctor_id, appointment_date);
```

This specific index makes our conflict detection query extremely fast — it only checks appointments for the relevant doctor on the relevant date.

---

## 7. Error Handling — Why We Made Our Own `AppError`

### 7.1 The Problem

Different parts of our code produce different error types:
- Database queries → `sqlx::Error`
- Template rendering → `tera::Error`
- Password hashing → `bcrypt::BcryptError`
- Session operations → `SessionInsertError`, `SessionGetError`
- Actix Web → `actix_web::Error`

Without a unified error type, every handler would need to handle all these different errors:

```rust
// ❌ Horrible without unified errors
fn handler() -> Result<HttpResponse, Box<dyn Error>> { ... }
```

### 7.2 The Solution: `AppError` Enum

```rust
pub enum AppError {
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    Internal(String),
    DatabaseError(sqlx::Error),
    TemplateError(tera::Error),
    BcryptError(bcrypt::BcryptError),
}
```

This is a **sum type** (enum) — a value can be exactly ONE of these variants. Each variant wraps a different piece of data.

### 7.3 The `From` Trait — Automatic Conversion

The magic that makes `?` work with our error type:

```rust
impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::DatabaseError(e)
    }
}
```

This means: "If you see a `sqlx::Error`, you know how to turn it into an `AppError`." Then `?` works automatically:

```rust
let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
    .bind(user_id)
    .fetch_one(pool)
    .await?;  // ← If this returns sqlx::Error, Rust auto-converts to AppError::DatabaseError
```

### 7.4 `ResponseError` — Sending Errors to the Browser

```rust
impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        match self {
            AppError::NotFound(msg) => HttpResponse::NotFound().body(msg.clone()),
            AppError::Unauthorized(msg) => HttpResponse::Unauthorized().body(msg.clone()),
            // ...
        }
    }
}
```

This trait tells Actix Web: "When an `AppError` is returned from a handler, here's how to convert it to an HTTP response." This means our handlers can return `Result<HttpResponse, AppError>` and Actix Web handles the error rendering.

---

## 8. Authentication & Sessions — Who Are You?

### 8.1 How Login Works (End to End)

```
Step 1: User visits /users/login
        → login_form() handler renders the login page

Step 2: User enters username + password, clicks "Login"
        → Browser sends POST /users/login with form data

Step 3: login() handler runs:
        a. authenticate_user() queries DB for the username
        b. bcrypt::verify() checks if password matches the hash
        c. If match: session.insert("user_id", user.id)
                      session.insert("username", user.username)
                      session.insert("role", user.role)
        d. Redirect to /appointments

Step 4: Browser receives redirect, goes to /appointments
        → Session cookie is automatically sent with the request
        → AuthUser extractor reads user_id from session
        → Handler knows who you are
```

### 8.2 The `AuthUser` Extractor

```rust
impl FromRequest for AuthUser {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let session = req.get_session();           // Get the session from the cookie
        match session.get::<i64>("user_id") {      // Try to read "user_id"
            Ok(Some(user_id)) => {                 // Success! User is logged in
                // Read username and role from session
                ready(Ok(AuthUser { user_id, username, role }))
            }
            Ok(None) => {                          // No user_id in session
                ready(Err(ErrorUnauthorized("Please log in")))
            }
            Err(_) => {                            // Session error
                ready(Err(ErrorInternalServerError("Session read error")))
            }
        }
    }
}
```

> **`FromRequest`** is the trait that makes extractors work. When you write a handler like:
> ```rust
> async fn dashboard(user: AuthUser) -> HttpResponse { ... }
> ```
> Actix Web sees `user: AuthUser`, calls `AuthUser::from_request()`, and either passes the `AuthUser` to your handler or returns an error to the browser.

### 8.3 `OptionalAuthUser` — For Public Pages

Some pages (like the login form) need to know IF a user is logged in, but shouldn't require it. `OptionalAuthUser` returns `OptionalAuthUser(None)` instead of an error when no session exists. This is used for the navbar and landing page.

### 8.4 Role-Based Guards

```rust
pub fn require_admin(user: &AuthUser) -> Result<(), AppError> {
    require_role(user, &["admin"])
}

pub fn require_doctor(user: &AuthUser) -> Result<(), AppError> {
    require_role(user, &["doctor", "admin"])
}
```

Usage in handlers:
```rust
pub async fn list_users(user: AuthUser) -> Result<HttpResponse, AppError> {
    require_admin(&user)?;  // ← If not admin, returns AppError::Forbidden immediately
    // ... only admins reach this code
}
```

> **The `?` operator in Rust** = "If this is an error, return it immediately. Otherwise, unwrap the success value and continue." It's like saying "handle the error case for me, I just want the happy path."

---

## 9. Module Deep Dives

### 9.1 Users Module

**Routes:** `/users/register`, `/users/login`, `/users/logout`, `/users` (list), `/users/{id}` (profile)

**Key Implementation: Registration**

```rust
pub async fn register_user(pool: &SqlitePool, form: &RegisterForm) -> Result<User, AppError> {
    // Step 1: Validate the role is one of the three valid options
    if !["patient", "doctor", "admin"].contains(&form.role.as_str()) {
        return Err(AppError::BadRequest("Invalid role specified".into()));
    }

    // Step 2: Hash the password (bcrypt cost factor 10)
    let password_hash = bcrypt::hash(&form.password, 10)?;

    // Step 3: Insert into users table, get back the full row
    let user = sqlx::query_as::<_, User>(
        "INSERT INTO users (username, email, password_hash, role, full_name)
         VALUES (?, ?, ?, ?, ?)
         RETURNING id, username, email, password_hash, role, full_name, created_at",
    )
    .bind(&form.username)
    .bind(&form.email)
    .bind(&password_hash)
    .bind(&form.role)
    .bind(&form.full_name)
    .fetch_one(pool)
    .await?;

    // Step 4: Create role-specific profile
    match form.role.as_str() {
        "patient" => {
            sqlx::query("INSERT INTO patients (user_id) VALUES (?)")
                .bind(user.id).execute(pool).await?;
        }
        "doctor" => {
            sqlx::query("INSERT INTO doctors (user_id, specialization, license_number) VALUES (?, ?, ?)")
                .bind(user.id).bind("General Practice").bind("PENDING")
                .execute(pool).await?;
        }
        _ => {} // admin: no extra profile table
    }

    Ok(user)
}
```

> **`RETURNING` clause**: In SQL, INSERT normally doesn't return data. `RETURNING *` tells the database "after inserting, give me back all the columns of the new row." This avoids a separate SELECT query.

**Key Implementation: Authentication**

```rust
pub async fn authenticate_user(pool: &SqlitePool, form: &LoginForm) -> Result<User, AppError> {
    // Step 1: Find the user by username
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE username = ?")
        .bind(&form.username)
        .fetch_optional(pool)
        .await?;

    // Step 2: Verify password (or return error)
    match user {
        Some(u) if bcrypt::verify(&form.password, &u.password_hash)? => Ok(u),
        _ => Err(AppError::Unauthorized("Invalid username or password".into())),
    }
}
```

The `Some(u) if condition` is a **match guard** — it only matches `Some(u)` if the bcrypt verification succeeds. This is more elegant than nested if-statements.

> **Security note**: We return the same error for "user not found" and "wrong password." If we said "user not found" vs "wrong password," attackers could enumerate valid usernames. The generic "Invalid username or password" message prevents this.

### 9.2 Appointments Module (Core Feature)

**Routes:** `/appointments` (list), `/appointments/book` (form + submit), `/appointments/book/priority` (priority override), `/appointments/suggest` (find slot), `/appointments/waitlist` (view queue), `/appointments/waitlist/join`, `/appointments/waitlist/{id}/promote`, `/appointments/{id}` (detail), `/appointments/{id}/cancel` (cancel)

This is the most important module — it contains **four scheduling algorithms** implementing the project's core focus: the three covered in depth below, plus a greedy, load-balanced **doctor reassignment** algorithm (`find_alternative_doctor` / `reassign_appointment`) that moves a scheduled appointment to the best available alternative doctor when the original goes on leave.

#### Algorithm 1: Time Interval Overlap Detection

```rust
pub async fn check_conflict(
    pool: &SqlitePool,
    doctor_id: i64,
    appointment_date: &str,
    start_time: &str,
    end_time: &str,
    room_id: i64,                        // Always checked — rooms are auto-assigned
    exclude_appointment_id: Option<i64>,
) -> Result<bool, AppError> {
    // Builds SQL: always checks doctor + date + room,
    // optionally excludes a specific appointment
}
```

**How it works:** The overlap condition is `A_start < B_end AND A_end > B_start`. The SQL binds are intentionally swapped: `end_time` (new) goes to the first `?`, `start_time` (new) goes to the second `?`. This single condition catches ALL three overlap cases (new-starts-during, new-ends-during, new-envelops-existing).

**Room support:** Every appointment has an auto-assigned room, so `room_id` is always checked (`AND room_id = ?`). This prevents double-booking of both doctors and consultation rooms/equipment.

#### Algorithm 2: Earliest Available Slot

```rust
pub async fn find_earliest_slot(
    pool: &SqlitePool,
    form: &SuggestSlotForm,
) -> Result<Option<String>, AppError> {
    // 1. Fetch all scheduled appointments for the doctor on that date
    // 2. Sort by start_time (handled by ORDER BY in SQL)
    // 3. Walk through the gaps, cursor starts at 08:00
    // 4. For each appointment: if cursor + duration ≤ appointment_start, return cursor
    // 5. Move cursor past the appointment
    // 6. Check end-of-day gap (up to 17:00)
    // 7. Return None if fully booked
}
```

**Why this algorithm?** Rather than rejecting a conflicting request with "try again," it proactively helps the user find an open slot. The working hours are hardcoded as 08:00–17:00 (480–1020 minutes since midnight). The `time_to_minutes()` and `minutes_to_time()` helpers convert between "HH:MM" strings and integer minutes for clean comparison.

> **O(n) complexity**: The algorithm does a single pass over the day's appointments. No nested loops, no backtracking. For a typical doctor's schedule (10-30 appointments/day), this is instantaneous.

#### Algorithm 3: Priority-Based Scheduling with BinaryHeap

```rust
pub async fn book_with_priority(
    pool: &SqlitePool,
    patient_user_id: i64,
    form: &BookAppointmentForm,
) -> Result<Appointment, AppError> {
    // 1. Validate: only Emergency (1) or Urgent (2) can trigger override
    // 2. Check for conflicts
    // 3. If no conflict → simple booking
    // 4. If conflict → find ALL overlapping appointments
    // 5. Verify new priority is higher than ALL conflicting ones
    // 6. BEGIN TRANSACTION
    //    a. Move bumped appointments to waitlist (INSERT INTO waitlist SELECT...)
    //    b. Cancel bumped appointments (UPDATE status = 'cancelled')
    //    c. Book the emergency appointment (INSERT INTO appointments)
    // 7. COMMIT
}
```

**Priority levels** (lower number = higher urgency):

| Level | Name | Can Bump | Visual |
|---|---|---|---|
| 1 | Emergency | Urgent, Normal, Follow-up | 🔴 Red badge |
| 2 | Urgent | Normal, Follow-up | 🟡 Yellow badge |
| 3 | Normal | Follow-up only via standard booking | 🔵 Blue badge |
| 4 | Follow-up | Nothing | 🟢 Green badge |

**The `BinaryHeap<PriorityItem>`** is Rust's standard priority queue (a max-heap). To make the most urgent patient come out first, we **reverse** the `Ord` implementation: `other.priority.cmp(&self.priority)`. This flips the heap so the lowest priority number (most urgent) is at the top. Tie-breaking uses `created_at` — oldest waitlist entry wins.

**Transaction safety:** All mutations (cancelling bumped appointments, inserting waitlist rows, booking the new appointment) run inside `pool.begin()...tx.commit()`. If ANY step fails, the entire operation rolls back — no half-cancelled appointments left dangling.

> **"BinaryHeap"** = A tree-based data structure where the "largest" (or in our case, "most urgent") element is always at the top. Insertion and extraction are O(log n). We use it to efficiently retrieve the highest-priority patient from the waitlist.

### 9.2b Rooms & Resource Scheduling (Auto-Allocation)

The `rooms` table (migration 002) provides six consultation and specialist rooms seeded automatically. A `doctor_room_assignments` table (migration 005) allocates one room per doctor per day:

| Room | Type | Floor |
|---|---|---|
| Consultation Room A, B, C | consultation | Floor 1, 1, 2 |
| Procedure Room | procedure | Floor 2 |
| X-Ray Suite | equipment | Floor 3 |
| Lab Room | lab | Floor 3 |

**How auto-allocation works:**
1. At booking time, `resolve_room()` checks if the doctor already has a room assigned for that date.
2. If yes → reuses it. If no → claims the first available active room and persists the assignment.
3. Every appointment always has a room — room conflicts are always checked via `AND room_id = ?` in `check_conflict`.
4. Patients never see a room dropdown; the booking form shows "Auto-Assigned Room" instead.

This design ensures each doctor works from a consistent room each day while eliminating manual room selection from the patient booking flow.

### 9.2c Waitlist & Priority Queue

The `waitlist` table tracks patients who were bumped (or manually added) while waiting for a slot:

- **Status workflow:** `waiting` → `offered` (slot found) → `accepted` (booked) or `expired`
- **Promotion:** Doctors/admins can promote waitlist entries via `/appointments/waitlist/{id}/promote`. If the slot is now free, the patient is automatically booked and the waitlist entry marked accepted.
- **Ordering:** The waitlist view is ORDER BY `priority ASC, created_at ASC` — most urgent patients first, then oldest entries first.
- **Patient & doctor names:** Waitlist queries JOIN on `users` and `doctors` tables to show real names (e.g. "John Doe" / "Dr. Sarah Smith") instead of raw IDs.



### 9.3 Availability Module

**Routes:** `/availability` (list), `/availability/set` (form + submit)

Manages when doctors are available. The availability module is the data source, and the appointments module is the consumer: every booking path (`book_appointment`, `book_with_priority`, and waitlist promotion) calls `ensure_doctor_available()` before confirming, so a booking outside a doctor's declared working window — or on a blocked/leave date — is rejected. The containment rule lives on the `TimeSlotted::contains` trait method, so the availability service no longer compares raw time fields by hand.

### 9.4 Medical Records Module

**Routes:** `/records` (list), `/records/create` (form + submit), `/records/{id}` (detail), `/records/{id}/report` (printable report), `/records/{id}/report.pdf` (**PDF download**), `/records/timeline` (patient history), `/records/prescriptions/create` (write prescription)

Allows doctors to create medical records with diagnosis, treatment, and notes. Records can optionally link to an appointment. The patient is selected from a searchable dropdown (not manual ID entry). The **Create Medical Record** button on the appointment detail page pre-fills patient and appointment IDs. Patients can view their own records (with prescriptions listed separately). This demonstrates a common pattern: **role-filtered queries** — the same `/records` URL shows different data based on who's logged in.

**Patient history timeline** (`/records/timeline`) merges appointments, records, prescriptions, and invoices into one chronological view — each source contributes its own line via the `Reportable` trait (polymorphism in action).

**Medical report PDF generation** (`/records/{id}/report.pdf`) is an advanced feature: the same data that renders the HTML report is fed to `src/records/pdf.rs`, which builds a paginated, word-wrapped A4 PDF with the pure-Rust `printpdf` crate (built-in Helvetica fonts — no external binaries or font files). The handler streams it back as `application/pdf` with a `Content-Disposition: attachment` header, reusing the record's ownership rule so patients can only export their own.

### 9.5 Billing Module

**Routes:** `/billing` (list), `/billing/create` (form + submit), `/billing/{id}` (detail), `/billing/{id}/pay` (record payment)

Handles the full billing lifecycle:
1. Admin selects a patient from a searchable dropdown (not manual ID entry) and creates an invoice with itemized line items → status: `pending`
2. Admin records payment → system checks if fully paid
3. If total paid >= invoice total → status: `paid`

The payment check logic:
```rust
let total_paid: (Option<f64>,) =
    sqlx::query_as("SELECT SUM(amount) FROM payments WHERE invoice_id = ?")
        .bind(invoice_id)
        .fetch_one(pool).await?;

if let Some(paid) = total_paid.0 {
    if paid >= invoice.total_amount {
        sqlx::query("UPDATE invoices SET status = 'paid' WHERE id = ?")
            .bind(invoice_id).execute(pool).await?;
    }
}
```

---

## 10. Templates (HTML) — How the Browser Sees Your App

### 10.1 Template Inheritance

Our templates use a parent-child hierarchy:

```
base.html.tera                    ← HTML skeleton (<!DOCTYPE>, <head>, <body>)
├── includes navbar.html.tera     ← Navigation bar (shared across ALL pages)
├── includes footer.html.tera     ← Footer (shared across ALL pages)
├── block: flash_messages         ← Optional notification area
├── block: content                ← 👈 Every page fills this block
├── block: head_extra             ← Extra CSS/JS in <head>
└── block: scripts                ← Extra JavaScript at end of <body>


├── block: sidebar                ← Navigation menu
└── block: main_content           ← 👈 Filled by page templates

users/login.html.tera extends base   ← Uses full width (no sidebar)
appointments/list.html.tera extends base  ← Uses full width
records/detail.html.tera extends base    ← Uses full width
```

### 10.2 How Template Rendering Works

When a handler calls:
```rust
let mut ctx = Context::new();
ctx.insert("user", &user);
ctx.insert("appointments", &appointments);
ctx.insert("title", "Appointments");
let rendered = tera.render("appointments/list.html.tera", &ctx)?;
```

1. Tera loads `appointments/list.html.tera`
2. Sees `{% extends "base.html.tera" %}`, so it loads `base.html.tera` first
3. `base.html.tera` has `{% include "shared/navbar.html.tera" %}` — loads that too
4. `base.html.tera` has `{% block content %}{% endblock %}` — fills this with the content from `list.html.tera`
5. All `{{ variable }}` placeholders are replaced with actual values from `ctx`
6. The final HTML string is returned

> **"Context"** = A dictionary (key-value map) of data passed to the template. Think of it as the template's "inputs." The template can only access what's in the context.

---

## 11. Life of a Request — Two Full Walkthroughs

### 11.1 Registration (Anonymous User)

```
1. Browser: GET http://localhost:8080/users/register
   │
2. Actix Web: Matches route → users::handlers::register_form
   │
3. OptionalAuthUser extractor: Checks session → finds none → OptionalAuthUser(None)
   │
4. register_form() handler:
   a. Creates Tera Context
   b. ctx.insert("user", None)  → navbar shows "Login" and "Register" buttons
   c. Renders "users/register.html.tera"
   │
5. Browser receives HTML, displays registration form
   │
6. User fills form, clicks "Register"
   │
7. Browser: POST http://localhost:8080/users/register
   Body: username=johndoe&email=john@example.com&password=secret&full_name=John+Doe&role=patient
   │
8. Actix Web: Matches route → users::handlers::register
   │
9. web::Form<RegisterForm> extractor: Parses form data into RegisterForm struct
   │
10. register() handler:
    a. Calls services::register_user()
       i. Validates role
       ii. bcrypt::hash("secret", 10) → "$2b$10$..."
       iii. INSERT INTO users ... RETURNING *
       iv. INSERT INTO patients (user_id) VALUES (...)
    b. session.insert("user_id", user.id)
    c. session.insert("username", "johndoe")
    d. session.insert("role", "patient")
    e. Returns redirect to /appointments
    │
11. Browser: GET http://localhost:8080/appointments
    (Cookie header includes session data automatically)
    │
12. AuthUser extractor: Reads session → AuthUser { user_id: 1, username: "johndoe", role: "patient" }
    │
13. list_appointments() handler:
    a. role is "patient" → calls get_appointments_for_patient(1)
    b. Renders "appointments/list.html.tera" with (probably empty) appointments list
    │
14. Browser displays appointment list (empty — no appointments yet!)
```

### 11.2 Booking an Appointment (Patient)

```
1. User clicks "Book Appointment" → GET /appointments/book
   │
2. book_form() handler:
    a. Calls get_all_doctors() → [(1, "Dr. Smith"), (2, "Dr. Jones")]
    b. Renders form with doctor dropdown
   │
3. User selects Dr. Smith, picks date 2026-06-01, time 10:00-10:30
   Clicks "Book Appointment"
   │
4. POST /appointments/book
   Body: doctor_id=1&appointment_date=2026-06-01&start_time=10:00&end_time=10:30
   │
5. book_appointment() handler:
    Calls services::book_appointment()
    │
6. book_appointment():
    a. Validates: 10:00 < 10:30 ✓
    b. Looks up patient: SELECT id FROM patients WHERE user_id=1 → patient_id=1
    c. Conflict check: SELECT COUNT(*) FROM appointments
       WHERE doctor_id=1 AND appointment_date='2026-06-01'
       AND status != 'cancelled'
       AND start_time < '10:30' AND end_time > '10:00'
       → No conflicts (count = 0)
    d. INSERT INTO appointments (patient_id=1, doctor_id=1, ...) RETURNING *
    e. Returns the new Appointment
   │
7. Redirect to /appointments/{new_id}
   │
8. appointment_detail() renders the confirmation page
```

---

## 12. Key Rust Concepts Used in This Project

### 12.1 `async`/`await` — Doing Things Without Waiting

```rust
// Without async: the server blocks until the DB query finishes
fn get_user(id: i64) -> User {
    db.query("SELECT * FROM users WHERE id = ?", id) // ← Server frozen!
}

// With async: the server handles other requests while waiting
async fn get_user(id: i64) -> User {
    db.query("SELECT * FROM users WHERE id = ?", id).await // ← Other requests proceed!
}
```

Every database query, template render, and HTTP operation in our project is async. The `#[actix_web::main]` macro sets up the runtime that executes all these async tasks.

### 12.2 `impl` Blocks — Rust's OOP

Rust doesn't have classes, but it has `struct` (data) + `impl` (behavior):

```rust
// This is our "class"
pub struct AuthUser {
    pub user_id: i64,
    pub username: String,
    pub role: String,
}

// These are our "methods"
impl FromRequest for AuthUser {
    fn from_request(req: &HttpRequest, ...) -> ... { /* ... */ }
}
```

> **Rust's OOP**: The course requires OOP principles. In Rust:
> - **Encapsulation**: `pub` (public) vs private (default) controls visibility
> - **Polymorphism**: Achieved through **traits** (like interfaces in Java/C#)
> - **Inheritance**: Rust doesn't have class inheritance, but uses trait inheritance and composition

### 12.3 Traits — Rust's Interfaces

A **trait** defines behavior that types can implement:

```rust
// The trait (interface)
pub trait ResponseError {
    fn error_response(&self) -> HttpResponse;
}

// Our implementation
impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        match self { /* map variants to HTTP status codes */ }
    }
}
```

Key traits in this project:
| Trait | Purpose | Where Used |
|---|---|---|
| `FromRequest` | Extract data from HTTP requests | `AuthUser`, `OptionalAuthUser`, `web::Form<T>`, `web::Path<T>` |
| `ResponseError` | Convert errors to HTTP responses | `AppError` |
| `From<T>` | Automatic type conversion | `From<sqlx::Error> for AppError` (enables `?` operator) |
| `Serialize` / `Deserialize` | JSON/form conversion | All our model structs (via `#[derive(Serialize, Deserialize)]`) |
| `FromRow` | Map database rows to Rust structs | All our model structs (via `#[derive(sqlx::FromRow)]`) |

### 12.4 Derive Macros — Automatic Code Generation

```rust
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
    // ...
}
```

These `#[derive(...)]` annotations tell the compiler: "Generate the boilerplate code for me." Without them, you'd have to manually write:
- How to print a User for debugging (`Debug`)
- How to convert a User to JSON (`Serialize`)
- How to parse JSON into a User (`Deserialize`)
- How to map database columns to User fields (`FromRow`)

This is Rust's answer to "convention over configuration" — you declare your intent, and the compiler does the rest.

### 12.5 `Result<T, E>` — Error Handling Without Exceptions

Rust doesn't have try/catch exceptions. Instead, fallible functions return `Result`:

```rust
fn divide(a: f64, b: f64) -> Result<f64, String> {
    if b == 0.0 {
        Err("Cannot divide by zero".to_string())
    } else {
        Ok(a / b)
    }
}

// Using it:
let result = divide(10.0, 2.0)?; // ← If error, return it. Otherwise, unwrap.
```

Every function in our services returns `Result<T, AppError>`. The `?` operator propagates errors up the call stack until they reach a handler, where `ResponseError` converts them to HTTP responses.

### 12.6 `web::Data<T>` — Shared State

```rust
// In main.rs: make the pool available to all handlers
.app_data(web::Data::new(pool.clone()))

// In any handler: access the pool
pub async fn handler(pool: web::Data<SqlitePool>) -> HttpResponse {
    let users = sqlx::query_as::<_, User>("SELECT * FROM users")
        .fetch_all(pool.get_ref())
        .await;
    // ...
}
```

`web::Data<T>` uses `Arc` (Atomic Reference Counting) internally — it's a thread-safe smart pointer. Multiple worker threads can read the same pool simultaneously.

> **"Arc" (Atomic Reference Counted)** = A smart pointer that allows multiple owners of the same data. When the last owner is dropped, the data is freed. "Atomic" means it's safe to share across threads.

---

## 13. How to Add a New Feature (Step-by-Step)

Let's say you want to add a "Doctor Rating" feature where patients can rate doctors after appointments.

### Step 1: Database Migration

Create `migrations/002_doctor_ratings.sql`:
```sql
CREATE TABLE IF NOT EXISTS doctor_ratings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    doctor_id INTEGER NOT NULL,
    patient_id INTEGER NOT NULL,
    appointment_id INTEGER NOT NULL,
    rating INTEGER NOT NULL CHECK (rating BETWEEN 1 AND 5),
    comment TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (doctor_id) REFERENCES doctors(id),
    FOREIGN KEY (patient_id) REFERENCES patients(id),
    FOREIGN KEY (appointment_id) REFERENCES appointments(id)
);
```

### Step 2: Models

In `src/appointments/models.rs` (or create a new `src/ratings/models.rs`):
```rust
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct DoctorRating {
    pub id: i64,
    pub doctor_id: i64,
    pub patient_id: i64,
    pub appointment_id: i64,
    pub rating: i32,
    pub comment: Option<String>,
    pub created_at: chrono::NaiveDateTime,
}

#[derive(Debug, Deserialize)]
pub struct RateDoctorForm {
    pub appointment_id: i64,
    pub rating: i32,
    pub comment: Option<String>,
}
```

### Step 3: Services

In `src/appointments/services.rs`:
```rust
pub async fn rate_doctor(
    pool: &SqlitePool,
    patient_user_id: i64,
    form: &RateDoctorForm,
) -> Result<DoctorRating, AppError> {
    let patient_id = get_patient_id(pool, patient_user_id).await?;

    // Verify the appointment belongs to this patient and is completed
    let appt = sqlx::query_as::<_, Appointment>(
        "SELECT * FROM appointments WHERE id = ? AND patient_id = ? AND status = 'completed'"
    )
    .bind(form.appointment_id)
    .bind(patient_id)
    .fetch_optional(pool).await?
    .ok_or_else(|| AppError::BadRequest("Invalid or incomplete appointment".into()))?;

    Ok(sqlx::query_as::<_, DoctorRating>(
        "INSERT INTO doctor_ratings (doctor_id, patient_id, appointment_id, rating, comment)
         VALUES (?, ?, ?, ?, ?) RETURNING ..."
    )
    .bind(appt.doctor_id).bind(patient_id).bind(form.appointment_id)
    .bind(form.rating).bind(&form.comment)
    .fetch_one(pool).await?)
}
```

### Step 4: Handlers & Routes

Add to `src/appointments/handlers.rs`:
```rust
pub async fn rate_doctor_form(/* ... */) -> Result<HttpResponse, AppError> { /* ... */ }
pub async fn submit_rating(/* ... */) -> Result<HttpResponse, AppError> { /* ... */ }
```

Add to `src/appointments/mod.rs`:
```rust
.route("/rate", web::get().to(handlers::rate_doctor_form))
.route("/rate", web::post().to(handlers::submit_rating))
```

### Step 5: Templates

Create `src/appointments/templates/rate.html.tera` with a star-rating form.

### The Pattern

Every feature follows this exact pattern: **Migration → Model → Service → Handler → Template**. Once you learn this flow, you can add anything.

---

## 14. Common Errors and How to Fix Them

### 14.1 `error[E0277]: the trait bound ... is not satisfied`

**Meaning:** You're trying to use `?` on a type that can't be converted to your error type.

**Fix:** Add the appropriate `From` impl:
```rust
impl From<SomeNewErrorType> for AppError {
    fn from(e: SomeNewErrorType) -> Self {
        AppError::Internal(format!("{:?}", e))
    }
}
```

### 14.2 Template not found: `appointments/list.html.tera`

**Meaning:** Tera can't find the template at the name you're requesting.

**Check:**
1. Does the `.tera` file exist in the correct directory?
2. Was `load_module_templates()` called in `main.rs`?
3. Is the module name in the `modules` array in `load_module_templates()`?

### 14.3 `no such column: ...`

**Meaning:** Your Rust struct's field name doesn't match the database column.

**Fix:** SQLx maps snake_case Rust fields to snake_case SQL columns automatically. Make sure they match exactly.

### 14.4 `the trait bound ...: sqlx::FromRow is not satisfied`

**Meaning:** You're trying to use `query_as::<_, YourStruct>()` but `YourStruct` doesn't derive `FromRow`.

**Fix:** Add `#[derive(sqlx::FromRow)]` to the struct.

### 14.5 `connection pool timed out`

**Meaning:** All 5 connections are in use and a new query is waiting.

**Fix:** This shouldn't happen under normal development load. If it does, check for queries that aren't being `.await`ed (leaking connections) or increase `max_connections`.

### 14.6 `The session cookie failed to pass cryptographic checks`

**Meaning:** The server's session encryption key changed (typically because the server restarted and generated a new `Key`), but the browser still has an old cookie signed with the previous key.

**Fix:** Our project now persists the key automatically. If you still see this:
1. Delete `SESSION_SECRET` from `.env` and restart — a fresh key will be generated
2. Clear your browser cookies for `localhost:8080`
3. The warning is harmless — it just means the old cookie was ignored and a new one will be created on the next page load

### 14.7 `no such column: room_id` / `no such table: rooms`

**Meaning:** Migration 002 hasn't run yet, or the database was created before migration 002 was added. The `appointments` table is missing the `room_id` and `priority` columns.

**Fix:** Delete `patient_management.db` and restart. All migrations run fresh on the next `cargo run`.

---

## 16. Testing the Application

### 16.1 Why Tests Matter

Without tests, every code change is a gamble — you might break the login page while fixing the billing module and not discover it until a user complains. Tests give you confidence that your code works and continues to work as you make changes.

Our project uses **integration tests** — tests that spin up a complete Actix Web app with an in-memory SQLite database and make real HTTP requests against it. This is more realistic than unit tests and catches bugs that only appear when components interact.

> **"Integration Test"** = A test that exercises multiple parts of the system together (database + HTTP + templates + auth), rather than testing a single function in isolation. In Rust, integration tests live in the `tests/` directory and are compiled as separate binaries.

### 16.2 How to Run

```bash
cargo test
```

The test output shows each test file and individual test result. Pass `-- --nocapture` to see log output:

```bash
cargo test -- --nocapture
```

### 16.3 Test Architecture

```
tests/
├── common/
│   └── mod.rs              # Shared test infrastructure
├── test_auth.rs            # 19 tests — registration, login, role guards
├── test_algorithms.rs      # 26 tests — all 4 scheduling algorithms
├── test_appointments.rs    # 16 tests — appointment pages & booking
├── test_availability.rs    # 3 tests — availability CRUD
├── test_records.rs         # 6 tests — medical records + PDF export
├── test_billing.rs         # 6 tests — invoices & payments
└── test_extended.rs        # 17 tests — reassignment, timeline, audit, dashboard
```

Plus **36 unit tests** inside `src/` (trait default methods, the `DaySchedule` gap-finder,
enum serialisation, and the PDF word-wrap + rendering).

**Total: 133 tests across 8 integration suites + inline unit tests — 100% pass rate.**

### 16.4 The `with_test_app!` Macro

Instead of writing complex type annotations to reference Actix Web's internal `Service` trait, we use a macro:

```rust
with_test_app!(pool, app, {
    let req = test::TestRequest::get().uri("/appointments").to_request();
    let resp = test::call_service(&app, req).await;
    assert!(resp.status().is_success());
});
```

The macro:
1. Creates stub Tera templates so pages render without the real filesystem
2. Generates a session encryption key
3. Configures all 7 module routes
4. Wraps everything in `test::init_service()` which returns an opaque type
5. The `app` variable is available in the code block

> **Why a macro?** Explicit `Service<Request<BoxBody>, Response = ServiceResponse<BoxBody>, Error = Error>` type annotations caused version conflicts with `actix-http`. The macro avoids ALL explicit type annotations and lets Rust infer the types automatically.

### 16.5 The `register_and_login!` Macro

```rust
let cookie = register_and_login!(app, "username", "patient");
```

Registers a user via POST `/users/register`, asserts a redirect (auto-login), extracts the session cookie, and returns it. Use with `auth_get()` / `auth_post()` helpers for authenticated requests.

### 16.6 Test Coverage by Suite

| Suite | Tests | What's Covered |
|---|---|---|
| `test_auth.rs` | 11 | Registration (3 roles), duplicate rejection, login success/failure, logout, role guards, admin access |
| `test_algorithms.rs` | 19 | Conflict detection (empty/overlap/cancelled/room), earliest-slot (empty/after-existing/full/gap/multi-gap), priority (bump/equal-rejected/normal-gate), invalid time/duration rejection, ownership checks, waitlist (add/promote/cancel-triggers) |
| `test_appointments.rs` | 9 | Booking form, HTTP booking (success/conflict/invalid-time), priority booking HTTP, cancel HTTP, list after booking, waitlist (doctor/patient), suggest form |
| `test_availability.rs` | 3 | Doctor availability list, set form, submit + verify persistence |
| `test_records.rs` | 4 | Records list, create form (doctor), patient blocked, record creation HTTP submit |
| `test_billing.rs` | 7 | Invoice list, admin-only create, invoice creation (single/multi-item/bad-items), payment recording |

### 16.7 In-Memory Database

Every test uses `sqlite::memory:` — a temporary SQLite database in RAM:

```rust
pub async fn test_db_pool() -> SqlitePool {
    let pool = db::create_pool("sqlite::memory:").await;
    db::run_migrations(&pool).await;  // runs both migrations
    pool
}
```

Benefits: zero filesystem pollution, each test has a fresh DB, tests are fully isolated, full migration support matches production exactly.

### 16.8 Stub Templates

The test app registers minimal HTML stubs instead of loading real `.tera` files from disk:

```rust
("appointments/list.html.tera", "<html><body>Apps: {{ appointments | length }}</body></html>"),
```

This means templates render successfully, dynamic Tera content is still injected, tests verify HTTP behavior (status codes, redirects, role enforcement), and tests remain fast with no filesystem I/O.

### 16.9 Adding a New Test

```rust
#[actix_web::test]
async fn test_my_new_feature() {
    let pool = test_db_pool().await;
    with_test_app!(pool, app, {
        let cookie = register_and_login!(app, "testuser", "patient");
        let req = auth_get("/some/page", &cookie).to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
    });
}
```

### 16.10 Common Test Failures

| Symptom | Likely Cause |
|---|---|
| `assertion failed: resp.status().is_redirection()` | Missing prerequisite data (e.g., creating an invoice before a patient exists) |
| `assertion failed: resp.status().is_success()` | Route requires a role guard or returns an error |
| Template render error | Missing stub template in `test_tera()` |
| `connection pool timed out` | A test leaked a database connection |

---

## 15. Glossary of Jargon

| Term | Definition |
|---|---|
| **API (Application Programming Interface)** | A set of rules for how software components communicate. Our HTTP endpoints are our API. |
| **Async/Await** | A way to write code that doesn't block while waiting for slow operations (database, network). |
| **Backend** | The server-side code that handles business logic, database access, and generates pages. |
| **BinaryHeap** | Rust's priority queue data structure. A tree where the "largest" element is always at the top. We use it to serve the most urgent patient first by reversing the comparison. O(log n) insert/extract. |
| **Cargo** | Rust's package manager and build tool. Like npm for JavaScript or pip for Python. |
| **CDN (Content Delivery Network)** | A network of servers that deliver static content (like CSS files) from locations close to users. |
| **Cookie** | A small piece of data stored in the browser, sent with every request to the same server. |
| **CRUD** | Create, Read, Update, Delete — the four basic database operations. |
| **Database Index** | A data structure that speeds up database lookups, like a book's index. |
| **Database Transaction** | A group of database operations that either ALL succeed or ALL fail (atomicity). Our priority booking uses transactions to prevent half-cancelled appointments. |
| **Dependency** | An external library your project uses. Listed in `Cargo.toml`. |
| **Endpoint** | A specific URL path and HTTP method combination (e.g., `POST /users/login`). |
| **Enum** | A type that can be one of several variants. Rust's enums can hold data in each variant. |
| **Extractor** | An Actix Web type that implements `FromRequest` to pull data from incoming requests. |
| **Frontend** | The part users see and interact with — HTML, CSS, JavaScript in the browser. |
| **Full-Stack** | An application that includes both frontend and backend components. |
| **Git** | Version control system that tracks changes to files. `.gitignore` excludes files from tracking. |
| **Glob Pattern** | A pattern like `*.tera` or `templates/**/*` that matches multiple file paths. |
| **Handler** | A function that processes an HTTP request and returns a response. |
| **HTTP (HyperText Transfer Protocol)** | The protocol browsers and servers use to communicate. |
| **JSON (JavaScript Object Notation)** | A lightweight data format: `{"name": "John", "age": 30}`. |
| **Macro** | Rust code that generates other Rust code. `#[derive(...)]` and `println!()` are macros. |
| **Middleware** | Code that runs on every request before/after the handler. Like a pipeline filter. |
| **Migration** | A version-controlled SQL script that modifies the database schema. |
| **ORM (Object-Relational Mapper)** | A library that maps database rows to programming language objects. SQLx is a lighter alternative. |
| **Pool (Connection Pool)** | A set of pre-opened database connections that are reused for efficiency. |
| **Redirect** | An HTTP response (302/303) that tells the browser to navigate to a different URL. |
| **Route** | A mapping from URL path + HTTP method to a handler function. |
| **Schema** | The structure of a database — tables, columns, types, and relationships. |
| **Session** | Server-side data associated with a specific user, typically identified by a cookie. |
| **SQL (Structured Query Language)** | The language used to query and modify databases. |
| **SQL Injection** | A security vulnerability where malicious SQL is injected through user input. |
| **SSR (Server-Side Rendering)** | Generating HTML on the server before sending it to the browser. |
| **Struct** | A Rust data type that groups related fields together. Like a `class` in other languages. |
| **Trait** | Rust's version of an interface — defines behavior that types can implement. |
| **URL (Uniform Resource Locator)** | A web address like `http://localhost:8080/users/login`. |

---

## 🎓 Final Words

This project demonstrates the full lifecycle of a web application:

1. **Design** — Entity-relationship modeling, module separation
2. **Implementation** — Rust with Actix Web, async database access, templates
3. **Architecture** — Separation of concerns (models/services/handlers), error handling
4. **Security** — Password hashing, session management, SQL injection prevention
5. **Features** — Conflict detection algorithm, role-based access, billing workflow

Every file and every line has a purpose. Once you understand the module pattern (models → services → handlers → templates), you can build any feature by following the same structure.

Happy coding! 🦀
