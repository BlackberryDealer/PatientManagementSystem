# 🏥 Manual Vetting Guide

## Step 1 — Start the Server

```powershell
cd "c:\Users\tdmca\OneDrive\Desktop\webProg\Project\PatientManagementSystem"
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
| `dr.smith` | Doctor | General Practice, Mon/Tue/Thu/Fri 9-5, Wed off, July 20 blocked |
| `dr.jones` | Doctor | Cardiology, varied schedule |
| `john.doe` | Patient | O+, born 1990-03-15, 3 appointments, 2 records, 2 prescriptions, 1 paid invoice |
| `jane.doe` | Patient | A+, born 1985-07-22, 2 appointments, 1 record, 1 prescription, 1 pending invoice |
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
- [ ] Table shows John Doe's appointments (3 total: 1 completed, 1 scheduled, 1 cancelled)
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
- [ ] Date picker (YYYY-MM-DD format)
- [ ] Start/End time dropdowns on 30-min grid
- [ ] Room is auto-assigned (no manual room selection)
- [ ] Priority radio buttons: Emergency, Urgent, Normal, Follow-up
- [ ] Notes textarea
- [ ] **Try booking a past date** (e.g. 2020-01-01) → should show error
- [ ] **Try booking end before start** (e.g. 10:30–10:00) → should show error
- [ ] **Try non-grid time** (e.g. 10:15–10:45) → should show error
- [ ] Book a valid slot: pick a future date, 09:00–09:30, Normal priority → should redirect to appointment list

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

### 10. My Profile (`/users/1`)
- [ ] Shows username, email, full name, role badge
- [ ] Patient section: DOB, phone, address, blood group (O+), emergency contact
- [ ] **Edit Profile** button → click it

### 11. Edit Profile (`/users/1/edit`)
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
- [ ] **Priority booking visible**: book an Emergency appointment that conflicts → should work

### 3. Availability (`/availability`)
- [ ] Shows Dr. Smith's weekly schedule slots
- [ ] Mon 09:00-17:00 (recurring)
- [ ] Tue 09:00-17:00 (recurring)
- [ ] Wed — NOT listed (day off)
- [ ] Thu 09:00-17:00 (recurring)
- [ ] Fri 09:00-17:00 (recurring)
- [ ] July 20 shown as blocked (holiday)
- [ ] Click **Set Availability** button

### 4. Set Availability (`/availability/set`)
- [ ] Day of week dropdown (0=Sun to 6=Sat)
- [ ] Start/End time fields
- [ ] Recurring checkbox
- [ ] Specific date (optional, for one-offs/blocks)
- [ ] Blocked checkbox (for leave/holiday)
- [ ] Submit a new slot → redirects back to list
- [ ] Verify new slot appears

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
| `/users/logout` | GET | ✅ | ✅ | ✅ |
| users | GET | ❌ | ❌ | ✅ |
| `/users/new` | GET/POST | ❌ | ❌ | ✅ |
| `/users/{id}` | GET | Self only | ✅ | ✅ |
| `/users/{id}/edit` | GET/POST | Self only | ❌ | ✅ |
| `/appointments` | GET | ✅ (own) | ✅ (own) | ✅ (all) |
| `/appointments/book` | GET/POST | ✅ | — | — |
| `/appointments/book/priority` | POST | ✅ | — | — |
| `/appointments/suggest` | GET/POST | ✅ | ✅ | ✅ |
| `/appointments/calendar` | GET | ✅ | ✅ | ✅ |
| `/appointments/{id}` | GET | Own only | ✅ | ✅ |
| `/appointments/{id}/cancel` | POST | Own only | ✅ | ✅ |
| `/appointments/{id}/reassign` | POST | ❌ | ✅ | ✅ |
| `/appointments/waitlist` | GET | ✅ (own) | ✅ (own) | ✅ (all) |
| `/appointments/waitlist/join` | POST | ✅ | — | — |
| `/appointments/waitlist/{id}/promote` | POST | ❌ | ✅ | ✅ |
| `/availability` | GET | ❌ | Own only | ✅ |
| `/availability/set` | GET/POST | ❌ | ✅ | ✅ |
| `/records` | GET | ✅ (own) | ✅ (all) | ✅ (all) |
| `/records/create` | GET/POST | ❌ | ✅ | ✅ |
| `/records/prescriptions/create` | GET/POST | ❌ | ✅ | ✅ |
| `/records/timeline` | GET | ✅ (own) | ✅ | ✅ |
| `/records/{id}` | GET | Own only | ✅ | ✅ |
| `/records/{id}/report` | GET | Own only | ✅ | ✅ |
| `/billing` | GET | ✅ (own) | ❌ | ✅ (all) |
| `/billing/create` | GET/POST | ❌ | ❌ | ✅ |
| `/billing/{id}` | GET | Own only | ❌ | ✅ |
| `/billing/{id}/pay` | POST | ❌ | ❌ | ✅ |
| `/dashboard` | GET | ❌ | ❌ | ✅ |
| `/audit` | GET | ❌ | ❌ | ✅ |

That covers every route, button, access control, and feature in the system.