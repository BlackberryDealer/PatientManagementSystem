use crate::appointments::models::{Appointment, BookAppointmentForm, BookRequestForm};
use crate::auth::{AuthUser, Role};
use crate::availability::services::ensure_doctor_available;
use crate::db;
use crate::errors::AppError;
use crate::time::{minutes_to_time, parse_slot};
use sqlx::SqlitePool;

use super::algorithms::check_conflict;
use super::helpers::{
    all_outranked, bump_all_conflicts, fetch_scheduled_conflicts, insert_appointment,
    insert_appointment_in_tx, NewAppointment,
};
use super::rooms::resolve_room;
use super::waitlist::try_rebook_bumped;

// ============================================================
// Role resolution: who is this appointment FOR, and WITH whom?
// ============================================================

/// Apply the role rules to a raw booking request, producing the patient's
/// user id plus a fully-resolved `BookAppointmentForm`:
///
/// * **Patient**, books for themselves with the doctor they picked. The
///   priority field is discarded: triage is a clinical decision, so a
///   patient booking is always Normal no matter what the request claims
///   (the UI never shows patients a priority field, but a hand-crafted
///   POST must not bypass that either).
/// * **Doctor**, books a chosen patient into *their own* schedule. Any
///   submitted doctor_id is ignored: a doctor never books another
///   doctor's clinic (moving a visit between doctors is the separate
///   reassignment flow).
/// * **Admin**, books a chosen patient with a chosen doctor (front-desk
///   booking on a patient's behalf).
pub async fn resolve_booking_target(
    pool: &SqlitePool,
    user: &AuthUser,
    form: &BookRequestForm,
) -> Result<(i64, BookAppointmentForm), AppError> {
    let need = |what: &str| AppError::BadRequest(format!("Please select a {what}."));

    let (patient_user_id, doctor_id, priority) = match user.role {
        Role::Patient => (
            user.user_id,
            form.doctor_id.ok_or_else(|| need("doctor"))?,
            None, // patients cannot self-triage, Normal priority
        ),
        Role::Doctor => {
            let patient_id = form.patient_id.ok_or_else(|| need("patient"))?;
            (
                db::get_patient_user_id(pool, patient_id).await?,
                db::get_doctor_id(pool, user.user_id).await?,
                form.priority,
            )
        }
        Role::Admin => {
            let patient_id = form.patient_id.ok_or_else(|| need("patient"))?;
            (
                db::get_patient_user_id(pool, patient_id).await?,
                form.doctor_id.ok_or_else(|| need("doctor"))?,
                form.priority,
            )
        }
    };

    Ok((
        patient_user_id,
        BookAppointmentForm {
            doctor_id,
            appointment_date: form.appointment_date.clone(),
            start_time: form.start_time.clone(),
            end_time: form.end_time.clone(),
            priority,
            notes: form.notes.clone(),
        },
    ))
}

// ============================================================
// Simple booking (no priority bumping)
// ============================================================

/// Book an appointment after checking conflicts.
/// Room is auto-assigned from the doctor's daily room allocation.
pub async fn book_appointment(
    pool: &SqlitePool,
    patient_user_id: i64,
    form: &BookAppointmentForm,
) -> Result<Appointment, AppError> {
    form.validate()?;
    // Canonical zero-padded "HH:MM": the UI's dropdowns always send padded
    // values, but a hand-crafted "9:00" parses fine while breaking the lexical
    // time comparisons in the conflict/availability checks, so everything
    // downstream uses the re-rendered canonical form, never the raw input.
    let (start_mins, end_mins) = parse_slot(&form.start_time, &form.end_time)?;
    let (start, end) = (minutes_to_time(start_mins), minutes_to_time(end_mins));

    ensure_doctor_available(pool, form.doctor_id, &form.appointment_date, &start, &end).await?;

    let patient_id = db::get_patient_id(pool, patient_user_id).await?;
    let priority = form.requested_priority() as i32;
    let room_id = resolve_room(pool, form.doctor_id, &form.appointment_date).await?;

    let has_conflict = check_conflict(
        pool, form.doctor_id, &form.appointment_date,
        &start, &end, room_id, None,
    ).await?;

    if has_conflict {
        return Err(AppError::BadRequest(
            "The requested time slot conflicts with an existing appointment.\
             \nPlease choose a different time, or use Find Available Slot for a suggestion."
                .into(),
        ));
    }

    insert_appointment(pool, &NewAppointment {
        patient_id,
        doctor_id: form.doctor_id,
        date: &form.appointment_date,
        start: &start,
        end: &end,
        priority,
        room_id,
        notes: &form.notes,
    }).await
}

// ============================================================
// Algorithm 3: Priority-Based Scheduling with BinaryHeap
// ============================================================

/// Priority-based booking: if the requested slot conflicts with a lower-priority
/// appointment, bump the lower-priority one to the waitlist and book the urgent one.
///
/// Only Emergency (1) and Urgent (2) can trigger priority override.
/// All operations run inside a database transaction for atomicity.
pub async fn book_with_priority(
    pool: &SqlitePool,
    patient_user_id: i64,
    form: &BookAppointmentForm,
) -> Result<(Appointment, Vec<Appointment>), AppError> {
    form.validate()?;
    // Canonical zero-padded "HH:MM", same normalisation as book_appointment.
    let (start_mins, end_mins) = parse_slot(&form.start_time, &form.end_time)?;
    let (start, end) = (minutes_to_time(start_mins), minutes_to_time(end_mins));
    ensure_doctor_available(pool, form.doctor_id, &form.appointment_date, &start, &end).await?;

    let new_priority = form.requested_priority();

    if !new_priority.can_override() {
        return Err(AppError::BadRequest(
            "Priority override is only available for Emergency or Urgent appointments.\
             \nUse standard booking for Normal or Follow-up visits."
                .into(),
        ));
    }

    let patient_id = db::get_patient_id(pool, patient_user_id).await?;
    let room_id = resolve_room(pool, form.doctor_id, &form.appointment_date).await?;

    let has_conflict = check_conflict(
        pool, form.doctor_id, &form.appointment_date,
        &start, &end, room_id, None,
    ).await?;

    if !has_conflict {
        let appt = insert_appointment(pool, &NewAppointment {
            patient_id,
            doctor_id: form.doctor_id,
            date: &form.appointment_date,
            start: &start,
            end: &end,
            priority: new_priority as i32,
            room_id,
            notes: &form.notes,
        }).await?;
        return Ok((appt, Vec::new()));
    }

    let conflicts =
        fetch_scheduled_conflicts(pool, form.doctor_id, room_id, &form.appointment_date, &start, &end)
            .await?;

    if conflicts.is_empty() {
        return Err(AppError::BadRequest(
            "This time slot is occupied by a completed appointment and cannot be overridden.\
             \nPlease choose a different time."
                .into(),
        ));
    }

    if !all_outranked(new_priority, &conflicts) {
        return Err(AppError::BadRequest(
            "This time slot is occupied by an appointment with equal or higher priority.\
             \nUse the suggestion feature to find an available slot, or join the waitlist."
                .into(),
        ));
    }

    let mut tx = pool.begin().await?;

    let bumped_waitlist_ids = bump_all_conflicts(&mut tx, &conflicts).await?;

    let appointment = insert_appointment_in_tx(
        &mut tx,
        &NewAppointment {
            patient_id,
            doctor_id: form.doctor_id,
            date: &form.appointment_date,
            start: &start,
            end: &end,
            priority: new_priority as i32,
            room_id,
            notes: &form.notes,
        },
        start_mins,
        end_mins,
    )
    .await?;

    tx.commit().await?;

    let mut rescheduled = Vec::new();
    for wl_id in bumped_waitlist_ids {
        if let Some(rebooked) = try_rebook_bumped(pool, wl_id).await? {
            rescheduled.push(rebooked);
        }
    }

    Ok((appointment, rescheduled))
}
