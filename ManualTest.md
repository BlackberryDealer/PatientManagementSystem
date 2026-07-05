# 🏥 Manual Vetting Guide

## Step 1 — Start the Server

```powershell
cd PatientManagementSystem
cargo run --bin seed    # populate database (skip if already done)
cargo run               # start server at http://localhost:8080
```

Open **http://localhost:8080** in your browser.

---

## 🔑 Login Credentials

All passwords: **`password123`** (login works with username OR email)

| Account | Role | Key Facts |
|---------|------|-----------|
| `admin` | Admin | Full system access |
| `dr.smith` | Doctor | General Practice, Mon/Tue/Thu/Fri 9-5, Wed off, Aug 17 2026 blocked |
| `dr.jones` | Doctor | Cardiology, varied schedule |
| `john.doe` | Patient | O+, born 1990-03-15, 4 appointments, 2 records, 2 prescriptions, 1 paid invoice |
| `jane.doe` | Patient | A+, born 1985-07-22, 3 appointments, 1 record, 1 prescription, 1 pending invoice |
| `bob.wilson` | Patient | B+, born 1978-11-08, 3 appointments, 1 record, 1 prescription, 1 pending invoice, waitlisted |

---

## 🟢 PATIENT WALKTHROUGH — Login as `john.doe`

### 1. Login Page
- **Go to** `http://localhost:8080/users/login`
- [ ] Page loads with login form
- [ ] Type `john.doe` + `password123` → click **Login** → redirects to Appointments
- [ ] **Log out**, then login again using email `john.doe@email.com` + `password123` → should also work
- [ ] Try `JOHN.DOE@EMAIL.COM` (uppercase) → case-insensitive login should work

### 2. Navbar (visible on every page after login)
- [ ] Brand "PatientMS" top-left
- [ ] **Appointments** link visible
- [ ] **Calendar** link visible
- [ ] **Medical Records** link visible
- [ ] **My Invoices** link visible
- [ ] Username badge with "patient" tag top-right
- [ ] **Profile** button visible
- [ ] **Logout** button visible
- [ ] **NOT visible**: Availability, Users, Dashboard, Audit (patient-restricted)

### 3. Appointments List (`/appointments`)
- [ ] Table shows John Doe's appointments (4 total: 1 completed, 3 scheduled)
- [ ] Columns: Date, Doctor, Time, Room, Priority badge, Status badge, Actions
- [ ] Doctor names show as real names (not IDs)
- [ ] Priority badges coloured: Emergency=red, Urgent=orange, Normal=blue, Follow-up=green
- [ ] Status badges: scheduled=blue, completed=green, cancelled=red
- [ ] **View** button on each row → click one
- [ ] **Cancel** button only on "scheduled" appointments

### 4. Appointment Detail (`/appointments/1`)
- [ ] Shows patient name, doctor name, date, time, priority badge, status
- [ ] Room shows "Auto-Assigned" (room is resolved automatically)
- [ ] Notes visible
- [ ] Cancel button visible if status is "scheduled"
- [ ] **Create Medical Record** button visible to doctor/admin for scheduled appointments
- [ ] **Try to view someone else's appointment**: go to `/appointments/4` → should be FORBIDDEN (403)

### 5. Book an Appointment (`/appointments/book`)
- [ ] Doctor dropdown shows Dr. Smith and Dr. Jones
- [ ] **No patient dropdown** (patients book for themselves) and **no priority selector** (patient bookings are always Normal — triage is a staff decision)
- [ ] Date picker (YYYY-MM-DD format)
- [ ] Start/End time dropdowns on 30-min grid
- [ ] Room is auto-assigned (no manual room selection)
- [ ] Notes textarea
- [ ] Single **Book Appointment** button (no separate override button)
- [ ] **Try booking a past date** (e.g. 2020-01-01) → should show error
- [ ] **Try booking end before start** (e.g. 10:30–10:00) → should show error
- [ ] **Try non-grid time** (e.g. 10:15–10:45) → should show error
- [ ] Book a valid slot: pick a future date, 09:00–09:30 → should redirect to appointment detail showing a **Normal** priority badge

### 6. Calendar View (`/appointments/calendar`)
- [ ] Month grid loads showing current month
- [ ] Days with appointments show a count badge
- [ ] Today's date highlighted
- [ ] **← → arrows** navigate to previous/next month
- [ ] Click a date → goes to filtered appointment list
- [ ] **Book** button in header works
- [ ] **List View** button at bottom redirects to `/appointments`

### 7. Medical Records (`/records`)
- [ ] Table shows John Doe's records (2: Hypertension + Seasonal Allergies)
- [ ] Click **View** on a record
- [ ] Detail page shows breadcrumb, diagnosis card (red), treatment card (green), notes card (yellow)
- [ ] "Not specified" fallback text if diagnosis/treatment is empty
- [ ] **Back to Records** button
- [ ] **Printable Report** button → click → report page loads
- [ ] **PDF Download**: go to `/records/1/report.pdf` → browser downloads a PDF file (check it opens and shows patient/diagnosis/treatment)

### 8. Patient Timeline (`/records/timeline`)
- [ ] Shows chronological feed of appointments, records, prescriptions, invoices
- [ ] Each event has an icon and type label
- [ ] Events ordered by date (newest first)

### 9. My Invoices (`/billing`)
- [ ] Shows John Doe's invoice (1 paid: £150.00)
- [ ] Status badge shows "paid" in green
- [ ] Click **View** → shows line items (Consultation £100, Blood Test £50)
- [ ] Payment section shows payment record (Credit Card, TXN-001)
- [ ] **Try to access another patient's invoice**: go to `/billing/2` → should be FORBIDDEN

### 10. My Profile (`/users/4`)
- [ ] Shows username, email, full name, role badge
- [ ] Patient section: DOB, phone, address, blood group (O+), emergency contact
- [ ] **Edit Profile** button → click it

### 11. Edit Profile (`/users/4/edit`)
- [ ] Form pre-filled with current data
- [ ] Change full name to "John Doe Updated"
- [ ] Change phone to "555-9999"
- [ ] Click **Save Changes** → redirects to profile
- [ ] Verify changes persisted on profile page

### 12. Logout
- [ ] Click **Logout** in navbar → redirects to login page
- [ ] Try accessing `/appointments` → redirects to login (401)

---

## 🔵 DOCTOR WALKTHROUGH — Login as `dr.smith`

### 1. Login & Navbar
- [ ] Login with `dr.smith` / `password123`
- [ ] Navbar shows: Appointments, Calendar, **Availability**, Medical Records
- [ ] Role badge shows "doctor" in blue
- [ ] **NOT visible**: Users, Dashboard, Audit

### 2. Appointments List
- [ ] Shows Dr. Smith's appointments across all patients
- [ ] Columns include Patient Name (not just Doctor)
- [ ] Mixed statuses: scheduled, completed, cancelled
- [ ] Room names displayed

### 2b. Book for a Patient (`/appointments/book`)
- [ ] **Patient dropdown** shows all patients; **no doctor dropdown** — the booking lands in Dr. Smith's own schedule
- [ ] Priority dropdown visible (Emergency, Urgent, Normal, Follow-up)
- [ ] Book an **Emergency** appointment for a patient into a slot held by a Normal booking → succeeds; the displaced booking is cancelled. If the rest of that day is open, the displaced patient is auto-rescheduled into the earliest free slot (see section 9 below); only when the day is genuinely full does the entry land on the waitlist as `waiting`
- [ ] On any scheduled appointment's detail page: **Change Priority** dropdown re-triages it (e.g. Normal → Urgent)

### 3. Availability (`/availability`)
- [ ] Shows Dr. Smith's weekly schedule slots
- [ ] Mon 09:00-17:00 (recurring)
- [ ] Tue 09:00-17:00 (recurring)
- [ ] Wed — NOT listed (day off; patients get no bookable Wednesday slots — days without published hours are closed)
- [ ] Thu 09:00-17:00 (recurring)
- [ ] Fri 09:00-17:00 (recurring)
- [ ] Aug 17 shown as blocked (holiday); its Day tag reads Mon, matching the date
- [ ] Every row has **Edit** (pen) and **Delete** (trash) actions
- [ ] Click **Set Availability** button

### 4. Set Availability (`/availability/set`)
- [ ] Slot Type radio: **Recurring weekly** (default) vs **Specific date**
- [ ] Recurring mode: weekday checkboxes with **Mon–Fri** / **Clear** quick buttons — tick several days, save once, and one weekly rule per day appears in the list
- [ ] Recurring mode with no day ticked → styled 400 "Select at least one weekday"
- [ ] Specific date mode: date picker only, no day dropdown; the hint below shows the weekday of the picked date, and the saved row's Day tag always matches it
- [ ] Past date in specific-date mode → styled 400
- [ ] A window overlapping an existing one for the same day → styled 400 duplicate message
- [ ] **Block this time** checkbox (leave/holiday, or a recurring lunch break) works in both modes
- [ ] Submit → redirects back to list; verify the new slot(s) appear

### 4b. Edit / Delete Availability
- [ ] **Edit** a recurring slot: prefilled day dropdown + times + blocked checkbox; save → list reflects the change
- [ ] **Edit** a one-off slot: date picker shown instead of a day dropdown
- [ ] **Delete** asks for confirmation, then removes the row
- [ ] Guard: with an upcoming booked appointment inside a window, deleting/narrowing that window (or blocking that date) → styled 400 naming the first affected appointment; widen/covering edits still succeed
- [ ] A doctor editing/deleting another doctor's slot → 403; admin can manage any slot

### 5. Medical Records
- [ ] Can view records for any patient (doctor access)
- [ ] **Create Record** button → form loads with patient dropdown (not manual ID entry)
- [ ] Select patient from dropdown, enter diagnosis + treatment → submit → redirects
- [ ] **Write Prescription** → form loads → patient dropdown + medication/dosage/frequency → submit
- [ ] Can also create record from appointment detail page via **Create Medical Record** button

### 6. Suggest a Slot (`/appointments/suggest`)
- [ ] Select a doctor, enter a date, enter duration (e.g. 60 mins)
- [ ] Click **Find Slot** → shows earliest available time or "no availability"
- [ ] Test with a fully booked scenario

### 7. Waitlist (`/appointments/waitlist`)
- [ ] Shows entries waiting for slots with real patient and doctor names
- [ ] 3 entries seeded (1 Emergency, 1 Urgent, 1 Normal)
- [ ] Patient name column shows actual name (not "Patient #123")
- [ ] Doctor name column shows which doctor the patient is waiting for
- [ ] Doctor can see **Promote** button → click on one
- [ ] If slot is free, patient gets booked; if not, returns to waitlist

### 8. Cancel & Auto-Promote
- [ ] Cancel a scheduled appointment → waitlist auto-promotion triggers
- [ ] Highest priority waitlist entry should get the freed slot

### 9. Waitlist Auto Re-slot & Expiry

**Bump-and-rebook (open day):**
- [ ] As a patient (or via staff booking), book a Normal appointment at e.g. `09:00–09:30` on a date where the rest of the doctor's day is otherwise open
- [ ] As `dr.smith`, book an **Emergency** appointment for a different patient into that same `09:00–09:30` slot
- [ ] The displaced patient's original appointment shows as `cancelled`
- [ ] The displaced patient has a **new** `scheduled` appointment at the doctor's earliest free slot that day (may be earlier than the original time if nothing was booked before it — the search starts from clinic opening, not from the original slot)
- [ ] On `/appointments/waitlist`, the displaced patient's entry does **not** appear as `waiting` — it was resolved automatically, not queued

**Bump-and-rebook (full day, fallback):**
- [ ] Fill every 30-minute slot for a doctor on one date with Normal bookings
- [ ] Book an Emergency override into one of those slots
- [ ] The displaced patient's entry now appears on `/appointments/waitlist` with status `waiting` (no gap existed, so the fallback holds)
- [ ] Cancel the overriding Emergency appointment → the existing auto-promote-on-cancel flow restores the displaced patient into their original slot, and the waitlist entry flips to `accepted`

**Expiry:**
- [ ] Join the waitlist for a date, then let that date pass (or seed a past-dated row directly in the DB for testing)
- [ ] Reload `/appointments/waitlist` as the patient → the entry now shows status `expired` (light gray badge) with a note: "This request expired before a slot opened. You can book a new appointment from the booking page."
- [ ] Reload `/appointments/waitlist` as `dr.smith` or `admin` → the expired entry is **not** shown (their view stays a live action queue, not history)
- [ ] Attempt `POST /appointments/waitlist/{id}/promote` on the expired entry as `dr.smith` → redirects back to the waitlist with the notice "Could not promote: the requested time has already passed." instead of a generic error

---

## 🔴 ADMIN WALKTHROUGH — Login as `admin`

### 1. Navbar (Full Access)
- [ ] All links visible: Appointments, Calendar, Availability, Medical Records, Billing, **Users**, **Dashboard**, **Audit**

### 2. Appointments (All-Patients View)
- [ ] Sees ALL appointments across all doctors and patients
- [ ] Can view any appointment detail
- [ ] **Reassign** button on scheduled appointments → reassign to another doctor

### 3. Manage Users (users)
- [ ] Table of all 6 users with username, email, role badge, full name, created date
- [ ] Click any user → view their profile
- [ ] Can edit any user's profile (self-or-admin guard)

### 4. Add Staff (`/users/new`)
- [ ] Form: username, email, password, full name, role dropdown (doctor/admin)
- [ ] Doctor: specialization + license fields appear
- [ ] Create a new doctor → redirects to user list → verify new user appears
- [ ] The new doctor can log in immediately

### 5. Dashboard (`/dashboard`)
- [ ] Stats cards: total patients (3), total doctors (2)
- [ ] Today's appointments count, this week's count
- [ ] Status breakdown (scheduled/completed/cancelled counts)
- [ ] Waitlist count
- [ ] Billing summary (total invoiced, outstanding, collection rate)
- [ ] Busiest doctors ranking
- [ ] Cancellation rate percentage

### 6. Audit Log (`/audit`)
- [ ] Table of recent actions with timestamp, username, role, action, entity, details
- [ ] Registration actions visible
- [ ] Appointment booking actions visible
- [ ] Only admin can access (test with doctor/patient → 403)

### 7. Billing (Admin)
- [ ] `/billing` → sees ALL invoices (not just own)
- [ ] `/billing/create` → create new invoice for any patient (patient dropdown, not manual ID)
- [ ] Enter items in pipe-delimited format: `Consultation|1|80.00`
- [ ] Submit → verify invoice appears with correct total
- [ ] Record payment on a pending invoice → verify status flips to "paid" when settled

### 8. Cancel Any Appointment
- [ ] Cancel a patient's appointment → verify slots are freed
- [ ] Waitlist auto-promotion should trigger if matching waitlist entry exists

---

## 🟠 CROSS-CUTTING CHECKS (Any User)

| Test | URL | Expected |
|------|-----|----------|
| Unauthenticated redirect | `/appointments` | Redirects to `/users/login` |
| 404 page | `/nonexistent` | Friendly error page with "Page Not Found" |
| Register new patient | `/users/register` | Public form, role forced to patient |
| Register as doctor (self) | `/users/register` with `role=doctor` | Rejected — "patients only" |
| Login with wrong password | Login form | "Invalid username/email or password" |
| Login with empty fields | Login form | Same error (no field-specific leak) |
| IDOR — view other's appointment | Patient A tries `/appointments/X` | 403 Forbidden |
| IDOR — view other's invoice | Patient A tries `/billing/X` | 403 Forbidden |
| IDOR — edit other's profile | Patient A tries `/users/Y/edit` | 403 Forbidden |
| Mobile responsive | Resize browser < 768px | Hamburger menu appears, navbar collapses |
| Session persistence | Close browser, reopen | Still logged in (signed cookie) |

---

## 📋 Quick Reference: All Routes

| Route | Method | Patient | Doctor | Admin |
|-------|--------|---------|--------|-------|
| `/` | GET | → Appointments | → Appointments | → Appointments |
| `/users/login` | GET/POST | ✅ | ✅ | ✅ |
| `/users/register` | GET/POST | ✅ (patient only) | — | — |
| `/users/logout` | POST | ✅ | ✅ | ✅ |
| users | GET | ❌ | ❌ | ✅ |
| `/users/new` | GET/POST | ❌ | ❌ | ✅ |
| `/users/{id}` | GET | Self only | ✅ | ✅ |
| `/users/{id}/edit` | GET/POST | Self only | ❌ | ✅ |
| `/appointments` | GET | ✅ (own) | ✅ (own) | ✅ (all) |
| `/appointments/book` | GET/POST | ✅ (self, Normal priority) | ✅ (for a patient, own schedule) | ✅ (for a patient, any doctor) |
| `/appointments/suggest` | GET/POST | ✅ | ✅ | ✅ |
| `/appointments/calendar` | GET | ✅ | ✅ | ✅ |
| `/appointments/availability` (JSON) | GET | ✅ | ✅ | ✅ |
| `/appointments/all-slots` (JSON) | GET | ❌ | ✅ | ✅ |
| `/appointments/reassign-day` | GET/POST | ❌ | ✅ | ✅ |
| `/appointments/reassign-day/apply` | POST | ❌ | ✅ | ✅ |
| `/appointments/{id}` | GET | Own only | ✅ | ✅ |
| `/appointments/{id}/reschedule` | GET/POST | Own only | ✅ | ✅ |
| `/appointments/{id}/cancel` | POST | Own only | ✅ | ✅ |
| `/appointments/{id}/complete` | POST | ❌ | ✅ | ✅ |
| `/appointments/{id}/assign-room` | POST | ❌ | ✅ | ✅ |
| `/appointments/{id}/priority` | POST | ❌ | ✅ | ✅ |
| `/appointments/{id}/reassign` | POST | ❌ | ✅ | ✅ |
| `/appointments/waitlist` | GET | ✅ (own) | ✅ (own) | ✅ (all) |
| `/appointments/waitlist/join` | POST | ✅ (filed as Normal) | ❌ | ❌ |
| `/appointments/waitlist/{id}/promote` | POST | ❌ | ✅ | ✅ |
| `/availability` | GET | ❌ | Own only | ✅ |
| `/availability/set` | GET/POST | ❌ | ✅ | ✅ |
| `/availability/{id}/edit` | GET/POST | ❌ | Own only | ✅ |
| `/availability/{id}/delete` | POST | ❌ | Own only | ✅ |
| `/records` | GET | ✅ (own) | ✅ (all) | ✅ (all) |
| `/records/create` | GET/POST | ❌ | ✅ (doctor only, not admin) | ❌ |
| `/records/patient-appointments` (JSON) | GET | ❌ | ✅ (doctor only, not admin) | ❌ |
| `/records/prescriptions/create` | GET/POST | ❌ | ✅ (doctor only, not admin) | ❌ |
| `/records/timeline` | GET | ✅ (own) | ✅ | ✅ |
| `/records/{id}` | GET | Own only | ✅ | ✅ |
| `/records/{id}/report` | GET | Own only | ✅ | ✅ |
| `/records/{id}/report.pdf` | GET | Own only | ✅ | ✅ |
| `/billing` | GET | ✅ (own) | ❌ | ✅ (all) |
| `/billing/create` | GET/POST | ❌ | ❌ | ✅ |
| `/billing/{id}` | GET | Own only | ❌ | ✅ |
| `/billing/{id}/pay` | POST | ❌ | ❌ | ✅ |
| `/billing/{id}/cancel` | POST | ❌ | ❌ | ✅ |
| `/users/{id}/change-password` | GET/POST | Self only | Self only | Self or any user |
| `/users/{id}/delete` | POST | ❌ | ❌ | ✅ (not self) |
| `/dashboard` | GET | ❌ | ❌ | ✅ |
| `/audit` | GET | ❌ | ❌ | ✅ |

That covers every route, button, access control, and feature in the system.