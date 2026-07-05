// Algorithm 5: Optimal Batch Doctor Reassignment (Hungarian / Kuhn-Munkres
// assignment problem).
//
// When a doctor goes on leave for a whole day, every one of their scheduled
// appointments has to go to a colleague. Algorithm 4 does this greedily, one
// appointment at a time, so an early appointment can grab the only
// same-specialisation colleague and leave the rest worse off. This solves the
// whole day as one assignment problem instead, which is provably optimal: it
// minimises the total reassignment cost across every appointment at once.
//
// Rows are the leaving doctor's appointments, columns are capacity slots on
// candidate colleagues plus "unassigned" fallbacks. The cost of giving
// appointment i to colleague c is INFEASIBLE if c is on leave, outside hours,
// or already busy, otherwise spec_penalty + load_cost. spec_penalty favours
// matching specialisation (continuity of care), and load_cost grows with how
// many appointments c already has plus how many they've picked up so far
// today (each extra one costs a bit more), which is what spreads the load
// instead of dumping everyone on the first free colleague.
//
// Capacity duplication works here because a doctor's own appointments never
// overlap each other, so a colleague can absorb several of them and each
// pairing's feasibility only depends on that colleague's existing schedule,
// there's no coupling between two reassigned appointments. That's what lets
// a plain assignment solver model a many-to-one redistribution.
//
// The solver itself is CostMatrix in appointments::models, kept free of any
// database code and unit-tested against a brute-force optimum.

use crate::appointments::models::{CostMatrix, ReassignPlan, ReassignRow};
use crate::availability::models::DoctorAvailability;
use crate::availability::services::{fetch_rules_for_day, slot_allowed_by_rules};
use crate::errors::AppError;
use crate::time::{parse_slot, time_to_minutes};
use sqlx::SqlitePool;
use std::collections::HashMap;

use super::super::helpers::{insert_slots, load_appointment};
use super::super::rooms::resolve_room;

/// Cost of a pairing the schedule forbids (colleague unavailable or already
/// booked over the slot). Far above any feasible cost, so the optimiser only
/// ever chooses it when literally nothing else is free, and even then prefers
/// an "unassigned" column, which is cheaper.
const INFEASIBLE_COST: i64 = 1_000_000;
/// Cost of leaving an appointment unplaced. Below `INFEASIBLE_COST` (so it is
/// always preferred over an impossible pairing) but far above any real
/// assignment (so a feasible colleague always wins when one exists).
const UNASSIGNED_COST: i64 = 10_000;
/// Penalty added when a colleague has a different specialisation to the leaving
/// doctor. Outweighs a moderate load difference, so continuity of care is
/// preferred until the load gap becomes large.
const SPECIALIZATION_PENALTY: i64 = 200;
/// Marginal cost per appointment already on a colleague's plate (and per extra
/// one taken today). The convex term that spreads the load.
const LOAD_WEIGHT: i64 = 10;

/// One of the leaving doctor's appointments, with its slot pre-parsed to
/// minutes so the feasibility scan does not re-parse per candidate.
struct SourceAppointment {
    id: i64,
    patient_name: String,
    start: String,
    end: String,
    start_min: i32,
    end_min: i32,
}

/// A colleague who might absorb some appointments, with the day's context the
/// cost function needs.
struct Candidate {
    doctor_id: i64,
    name: String,
    specialization: String,
    load: i64, // scheduled appointments already on this date
}

/// Does `[start, end)` (minutes) overlap any of a doctor's busy intervals?
fn overlaps_any(intervals: Option<&Vec<(i32, i32)>>, start: i32, end: i32) -> bool {
    intervals.is_some_and(|v| v.iter().any(|&(bs, be)| start < be && end > bs))
}

/// Build a fresh reassignment plan for `source_doctor_id` on `date` without
/// changing anything. This is the preview staff review before applying it.
///
/// Gathers the leaving doctor's appointments and every colleague's
/// availability and current load, builds the cost matrix described above,
/// solves it, then translates the column each appointment won back into
/// "moved to Dr X" or "could not place".
pub async fn plan_day_reassignment(
    pool: &SqlitePool,
    source_doctor_id: i64,
    date: &str,
) -> Result<ReassignPlan, AppError> {
    // Validation gate (Route -> Validation -> Logic): a real, non-past date.
    crate::time::parse_booking_date(date)?;

    let (source_name, source_spec) = sqlx::query_as::<_, (String, String)>(
        "SELECT u.full_name, d.specialization
         FROM doctors d JOIN users u ON d.user_id = u.id WHERE d.id = ?",
    )
    .bind(source_doctor_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Doctor not found".into()))?;

    // The leaving doctor's scheduled appointments for the day (the rows).
    let appts_raw = sqlx::query_as::<_, (i64, String, String, String)>(
        "SELECT a.id, u.full_name, a.start_time, a.end_time
         FROM appointments a
         JOIN patients p ON a.patient_id = p.id
         JOIN users u ON p.user_id = u.id
         WHERE a.doctor_id = ? AND a.appointment_date = ? AND a.status = 'scheduled'
         ORDER BY a.start_time",
    )
    .bind(source_doctor_id)
    .bind(date)
    .fetch_all(pool)
    .await?;

    let appts: Vec<SourceAppointment> = appts_raw
        .into_iter()
        .filter_map(|(id, patient_name, start, end)| {
            let start_min = time_to_minutes(&start)?;
            let end_min = time_to_minutes(&end)?;
            Some(SourceAppointment { id, patient_name, start, end, start_min, end_min })
        })
        .collect();

    let n = appts.len();
    if n == 0 {
        // Nothing scheduled, an empty but valid plan.
        return Ok(ReassignPlan {
            source_doctor_id,
            source_doctor_name: source_name,
            date: date.to_string(),
            rows: Vec::new(),
            assigned_count: 0,
            unassigned_count: 0,
            total_cost: 0,
        });
    }

    // Every other doctor, with today's scheduled load.
    let candidates: Vec<Candidate> = sqlx::query_as::<_, (i64, String, String, i64)>(
        "SELECT d.id, u.full_name, d.specialization,
                (SELECT COUNT(*) FROM appointments a
                  WHERE a.doctor_id = d.id AND a.appointment_date = ?
                    AND a.status = 'scheduled') AS load
         FROM doctors d JOIN users u ON d.user_id = u.id
         WHERE d.id != ?
         ORDER BY d.id",
    )
    .bind(date)
    .bind(source_doctor_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(doctor_id, name, specialization, load)| Candidate { doctor_id, name, specialization, load })
    .collect();
    let m = candidates.len();

    // Each colleague's busy intervals for the day, in one query, so the
    // feasibility scan is in-memory rather than n×m round-trips.
    let mut busy: HashMap<i64, Vec<(i32, i32)>> = HashMap::new();
    let busy_rows = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT doctor_id, start_time, end_time FROM appointments
         WHERE appointment_date = ? AND status != 'cancelled' AND doctor_id != ?",
    )
    .bind(date)
    .bind(source_doctor_id)
    .fetch_all(pool)
    .await?;
    for (did, s, e) in busy_rows {
        if let (Some(sm), Some(em)) = (time_to_minutes(&s), time_to_minutes(&e)) {
            busy.entry(did).or_default().push((sm, em));
        }
    }

    // Each colleague's availability rules for the day (leave / working windows).
    let mut rules_by_doctor: HashMap<i64, Vec<DoctorAvailability>> = HashMap::new();
    for cand in &candidates {
        let rules = fetch_rules_for_day(pool, cand.doctor_id, date).await?;
        rules_by_doctor.insert(cand.doctor_id, rules);
    }

    // Column layout: for each colleague c, `n` capacity copies at columns
    // [c*n, c*n + n); then `n` "unassigned" fallback columns. Width guarantees
    // rows (n) <= cols (n*(m+1)), which the solver requires.
    let unassigned_base = m * n;
    let cols = unassigned_base + n;
    let mut data = vec![vec![UNASSIGNED_COST; cols]; n];

    for (i, appt) in appts.iter().enumerate() {
        for (c, cand) in candidates.iter().enumerate() {
            let spec_penalty = if cand.specialization == source_spec { 0 } else { SPECIALIZATION_PENALTY };
            let feasible = slot_allowed_by_rules(&rules_by_doctor[&cand.doctor_id], &appt.start, &appt.end)
                && !overlaps_any(busy.get(&cand.doctor_id), appt.start_min, appt.end_min);
            for k in 0..n {
                data[i][c * n + k] = if feasible {
                    spec_penalty + LOAD_WEIGHT * (cand.load + k as i64)
                } else {
                    INFEASIBLE_COST
                };
            }
        }
        // Unassigned columns already hold UNASSIGNED_COST from initialisation.
    }

    let matrix = CostMatrix::new(data);
    let assignment = matrix.assign_min_cost();

    let mut rows = Vec::with_capacity(n);
    let mut assigned_count = 0;
    let mut unassigned_count = 0;
    let mut total_cost = 0;
    for (i, appt) in appts.iter().enumerate() {
        let col = assignment[i];
        let (to_doctor_id, to_doctor_name, same_specialization) = if col < unassigned_base {
            let cand = &candidates[col / n];
            assigned_count += 1;
            total_cost += matrix.cost(i, col);
            (Some(cand.doctor_id), Some(cand.name.clone()), cand.specialization == source_spec)
        } else {
            unassigned_count += 1;
            (None, None, false)
        };
        rows.push(ReassignRow {
            appointment_id: appt.id,
            patient_name: appt.patient_name.clone(),
            start_time: appt.start.clone(),
            end_time: appt.end.clone(),
            from_doctor_name: source_name.clone(),
            to_doctor_id,
            to_doctor_name,
            same_specialization,
        });
    }

    Ok(ReassignPlan {
        source_doctor_id,
        source_doctor_name: source_name,
        date: date.to_string(),
        rows,
        assigned_count,
        unassigned_count,
        total_cost,
    })
}

/// Recompute the optimal plan against the current schedule and apply it.
///
/// Rooms are resolved up front (same as Algorithm 4). Every move then runs
/// inside a single transaction (the `doctor_id`/`room_id` update plus the
/// rebuilt occupancy ledger), so the whole redistribution commits together or
/// not at all. The `appointment_slots` UNIQUE index remains the concurrency backstop:
/// if a colleague's slot was taken between preview and apply, that insert fails
/// and the batch rolls back cleanly. Returns `(source doctor name, moved,
/// unplaced)`.
pub async fn apply_day_reassignment(
    pool: &SqlitePool,
    source_doctor_id: i64,
    date: &str,
) -> Result<(String, usize, usize), AppError> {
    let plan = plan_day_reassignment(pool, source_doctor_id, date).await?;

    // Resolve each target's room and re-load the appointment (applying the
    // domain guard `reassign_to`) before opening the transaction.
    let mut moves: Vec<(i64, i64, i64, i32, i32)> = Vec::new(); // (appt_id, new_doctor, room, start_min, end_min)
    for row in &plan.rows {
        let Some(new_doctor_id) = row.to_doctor_id else { continue };
        let mut appt = load_appointment(pool, row.appointment_id).await?;

        let (start_min, end_min) = parse_slot(&appt.start_time, &appt.end_time)?;
        let room_id = resolve_room(pool, new_doctor_id, date).await?;
        appt.reassign_to(new_doctor_id)?; // domain rule: scheduled-only
        moves.push((appt.id, new_doctor_id, room_id, start_min, end_min));
    }

    let moved = moves.len();
    let unassigned = plan.unassigned_count;

    let mut tx = pool.begin().await?;
    for (appt_id, new_doctor_id, room_id, start_min, end_min) in &moves {
        sqlx::query("UPDATE appointments SET doctor_id = ?, room_id = ? WHERE id = ?")
            .bind(new_doctor_id)
            .bind(room_id)
            .bind(appt_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM appointment_slots WHERE appointment_id = ?")
            .bind(appt_id)
            .execute(&mut *tx)
            .await?;
        insert_slots(&mut tx, *appt_id, *new_doctor_id, date, *start_min, *end_min, *room_id).await?;
    }
    tx.commit().await?;

    Ok((plan.source_doctor_name, moved, unassigned))
}
