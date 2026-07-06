//! Render tests for the templates touched by the new features.
//!
//! The server parses every template at startup, so a *syntax* error is caught
//! there — but Tera only resolves variables and filters at render time. These
//! tests render the batch-reassignment page (empty, populated, and no-work
//! states) and the enhanced booking form with representative contexts, so a
//! bad variable or filter fails the build rather than a live request.

use patient_management_system::appointments::models::{ReassignPlan, ReassignRow};
use serde_json::json;
use std::fs;
use tera::{Context, Tera};

/// Build a Tera instance the same way `main.rs` does: root templates via glob,
/// then each module's templates under `src/<module>/templates` as
/// `"<module>/<file>"`.
fn build_tera() -> Tera {
    let mut tera = Tera::new("templates/**/*.tera").expect("root templates load");
    let modules = ["users", "appointments", "availability", "records", "billing", "audit", "dashboard"];
    for module in &modules {
        let dir = format!("src/{}/templates", module);
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "tera") {
                    let name = format!("{}/{}", module, path.file_name().unwrap().to_str().unwrap());
                    let content = fs::read_to_string(&path).unwrap();
                    tera.add_raw_template(&name, &content).expect("module template adds");
                }
            }
        }
    }
    tera.autoescape_on(vec![".tera"]);
    tera
}

fn staff_user() -> serde_json::Value {
    json!({ "user_id": 1, "username": "admin", "role": "admin" })
}

fn doctor_list() -> Vec<(i64, String)> {
    vec![(1, "Dr. Sarah Smith".to_string()), (2, "Dr. Michael Jones".to_string())]
}

#[test]
fn reassign_day_renders_empty_form() {
    let tera = build_tera();
    let mut ctx = Context::new();
    ctx.insert("user", &staff_user());
    ctx.insert("doctors", &doctor_list());
    ctx.insert("plan", &Option::<()>::None);
    ctx.insert("title", "Cover Doctor Leave");
    let html = tera.render("appointments/reassign_day.html.tera", &ctx).expect("renders");
    assert!(html.contains("Cover a Doctor's Leave"));
    // With no plan, the proposal section must be absent.
    assert!(!html.contains("Proposed Plan"));
}

#[test]
fn reassign_day_renders_populated_plan() {
    let tera = build_tera();
    let plan = ReassignPlan {
        source_doctor_id: 1,
        source_doctor_name: "Dr. Sarah Smith".into(),
        date: "2027-09-01".into(),
        rows: vec![
            ReassignRow {
                appointment_id: 10,
                patient_name: "John Doe".into(),
                start_time: "10:00".into(),
                end_time: "10:30".into(),
                from_doctor_name: "Dr. Sarah Smith".into(),
                to_doctor_id: Some(2),
                to_doctor_name: Some("Dr. Michael Jones".into()),
                same_specialization: true,
            },
            ReassignRow {
                appointment_id: 11,
                patient_name: "Jane Doe".into(),
                start_time: "11:00".into(),
                end_time: "11:30".into(),
                from_doctor_name: "Dr. Sarah Smith".into(),
                to_doctor_id: None,
                to_doctor_name: None,
                same_specialization: false,
            },
        ],
        assigned_count: 1,
        unassigned_count: 1,
        total_cost: 42,
    };
    let mut ctx = Context::new();
    ctx.insert("user", &staff_user());
    ctx.insert("doctors", &doctor_list());
    ctx.insert("plan", &plan);
    ctx.insert("form_doctor_id", &1i64);
    ctx.insert("form_date", "2027-09-01");
    ctx.insert("title", "Cover Doctor Leave");
    let html = tera.render("appointments/reassign_day.html.tera", &ctx).expect("renders");
    assert!(html.contains("Proposed Plan"));
    assert!(html.contains("Dr. Michael Jones"));   // the assigned target
    assert!(html.contains("John Doe"));
    assert!(html.contains("Could not place"));     // the unassigned row
    assert!(html.contains("Apply Plan (1)"));       // apply button reflects the count
}

#[test]
fn reassign_day_renders_no_work_plan() {
    let tera = build_tera();
    let plan = ReassignPlan {
        source_doctor_id: 1,
        source_doctor_name: "Dr. Sarah Smith".into(),
        date: "2027-09-01".into(),
        rows: vec![],
        assigned_count: 0,
        unassigned_count: 0,
        total_cost: 0,
    };
    let mut ctx = Context::new();
    ctx.insert("user", &staff_user());
    ctx.insert("doctors", &doctor_list());
    ctx.insert("plan", &plan);
    ctx.insert("form_doctor_id", &1i64);
    ctx.insert("form_date", "2027-09-01");
    ctx.insert("title", "Cover Doctor Leave");
    let html = tera.render("appointments/reassign_day.html.tera", &ctx).expect("renders");
    assert!(html.contains("no scheduled appointments"));
}

#[test]
fn availability_set_form_renders_two_mode_scaffolding() {
    let tera = build_tera();
    let mut ctx = Context::new();
    ctx.insert("user", &json!({ "user_id": 2, "username": "dr.smith", "role": "doctor" }));
    ctx.insert("title", "Set Availability");
    let html = tera.render("availability/set.html.tera", &ctx).expect("renders");
    // Both modes and the multi-day checkboxes the JS toggles between.
    assert!(html.contains("name=\"mode\" value=\"recurring\""));
    assert!(html.contains("name=\"mode\" value=\"oneoff\""));
    for day in ["day_0", "day_1", "day_2", "day_3", "day_4", "day_5", "day_6"] {
        assert!(html.contains(&format!("name=\"{day}\"")), "missing checkbox {day}");
    }
    assert!(html.contains("name=\"specific_date\""));
    assert!(html.contains("name=\"is_blocked\""));
}

#[test]
fn availability_edit_form_renders_both_slot_kinds() {
    let tera = build_tera();
    // Recurring slot: day dropdown, no date input.
    let mut ctx = Context::new();
    ctx.insert("user", &json!({ "user_id": 2, "username": "dr.smith", "role": "doctor" }));
    ctx.insert("slot", &json!({
        "id": 7, "doctor_id": 1, "day_of_week": 1,
        "start_time": "09:00", "end_time": "17:00",
        "is_recurring": 1, "specific_date": null, "is_blocked": 0,
    }));
    ctx.insert("title", "Edit Availability");
    let html = tera.render("availability/edit.html.tera", &ctx).expect("renders recurring");
    assert!(html.contains("action=\"/availability/7/edit\""));
    assert!(html.contains("name=\"day_of_week\""));
    assert!(!html.contains("name=\"specific_date\""));

    // One-off slot: date input, no day dropdown.
    let mut ctx = Context::new();
    ctx.insert("user", &json!({ "user_id": 2, "username": "dr.smith", "role": "doctor" }));
    ctx.insert("slot", &json!({
        "id": 8, "doctor_id": 1, "day_of_week": 1,
        "start_time": "00:00", "end_time": "23:59",
        "is_recurring": 0, "specific_date": "2026-08-17", "is_blocked": 1,
    }));
    ctx.insert("title", "Edit Availability");
    let html = tera.render("availability/edit.html.tera", &ctx).expect("renders one-off");
    assert!(html.contains("name=\"specific_date\""));
    assert!(html.contains("value=\"2026-08-17\""));
    assert!(!html.contains("name=\"day_of_week\""));
}

#[test]
fn availability_list_renders_actions_per_row() {
    let tera = build_tera();
    let mut ctx = Context::new();
    ctx.insert("user", &json!({ "user_id": 2, "username": "dr.smith", "role": "doctor" }));
    ctx.insert("slots", &json!([{
        "id": 3, "doctor_id": 1, "day_of_week": 1,
        "start_time": "09:00", "end_time": "17:00",
        "is_recurring": 1, "specific_date": null, "is_blocked": 0,
        "doctor_name": "Dr. Sarah Smith",
    }]));
    ctx.insert("title", "Doctor Availability");
    let html = tera.render("availability/list.html.tera", &ctx).expect("renders");
    assert!(html.contains("Dr. Sarah Smith"));
    assert!(html.contains("/availability/3/edit"));
    assert!(html.contains("/availability/3/delete"));
}

#[test]
fn booking_form_renders_with_live_slots_scaffolding() {
    let tera = build_tera();
    let mut ctx = Context::new();
    ctx.insert("user", &json!({ "user_id": 4, "username": "john.doe", "role": "patient" }));
    ctx.insert("doctors", &doctor_list());
    ctx.insert("patients", &Vec::<(i64, String)>::new());
    ctx.insert("self_doctor_id", &Option::<i64>::None);
    ctx.insert("start_slots", &vec!["08:00", "08:30", "09:00"]);
    ctx.insert("end_slots", &vec!["08:30", "09:00", "09:30"]);
    ctx.insert("today", "2026-01-01");
    ctx.insert("title", "Book Appointment");
    let html = tera.render("appointments/book.html.tera", &ctx).expect("renders");
    // The dynamic-slot hooks the JS relies on must be present.
    assert!(html.contains("data-self-doctor-id"));
    assert!(html.contains("id=\"slot-status\""));
    assert!(html.contains("Live Availability"));
    assert!(html.contains("/appointments/availability"));
}
