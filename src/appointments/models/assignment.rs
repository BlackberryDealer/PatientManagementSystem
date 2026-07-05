// CostMatrix: the pure Hungarian (Kuhn-Munkres) assignment solver behind
// Algorithm 5, plus the plan types the batch-reassignment preview renders.
// No database code here, same idea as DaySchedule for Algorithm 2: the
// service layer builds the cost numbers and the solver just does the maths,
// which is why the tests below can check it against a brute-force optimum.

use crate::availability::models::DoctorAvailability;
use crate::availability::services::slot_allowed_by_rules;
use serde::Serialize;
use std::collections::HashMap;

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
pub struct SourceAppointment {
    pub id: i64,
    pub patient_name: String,
    pub start: String,
    pub end: String,
    pub start_min: i32,
    pub end_min: i32,
}

/// A colleague who might absorb some appointments, with the day's context the
/// cost function needs.
pub struct Candidate {
    pub doctor_id: i64,
    pub name: String,
    pub specialization: String,
    pub load: i64, // scheduled appointments already on this date
}

/// Does `[start, end)` (minutes) overlap any of a doctor's busy intervals?
fn overlaps_any(intervals: Option<&Vec<(i32, i32)>>, start: i32, end: i32) -> bool {
    intervals.is_some_and(|v| v.iter().any(|&(bs, be)| start < be && end > bs))
}

/// Build the Algorithm 5 cost matrix from already-fetched scheduling data.
///
/// Column layout: for each colleague `c`, `n` capacity copies at columns
/// `[c*n, c*n + n)`; then `n` "unassigned" fallback columns (`unassigned_base
/// = candidates.len() * appts.len()`, recomputed by the caller to translate a
/// chosen column back into a doctor or "unplaced").
///
/// Pure function, no database access, same split as `DaySchedule` for
/// Algorithm 2: the service layer fetches rows, this does the maths, which is
/// what keeps it unit-testable without a pool.
pub fn build_cost_matrix(
    appts: &[SourceAppointment],
    candidates: &[Candidate],
    source_spec: &str,
    busy: &HashMap<i64, Vec<(i32, i32)>>,
    rules_by_doctor: &HashMap<i64, Vec<DoctorAvailability>>,
) -> CostMatrix {
    let n = appts.len();
    let m = candidates.len();
    let cols = m * n + n;
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

    CostMatrix::new(data)
}

/// A rectangular cost matrix for the classic assignment problem.
///
/// Rows are "jobs" (in Algorithm 5, the appointments a leaving doctor must
/// shed) and columns are "targets" (candidate-doctor capacity slots plus
/// unassigned fallbacks). `assign_min_cost` returns the minimum-cost way to
/// give every row a distinct column.
pub struct CostMatrix {
    rows: usize,
    cols: usize,
    data: Vec<Vec<i64>>,
}

impl CostMatrix {
    /// Build from a row-major cost grid. Every row must have the same width.
    pub fn new(data: Vec<Vec<i64>>) -> Self {
        let rows = data.len();
        let cols = data.first().map(|r| r.len()).unwrap_or(0);
        debug_assert!(data.iter().all(|r| r.len() == cols), "ragged cost matrix");
        Self { rows, cols, data }
    }

    /// The cost of putting row `i` in column `j` (used to total up a plan).
    pub fn cost(&self, i: usize, j: usize) -> i64 { self.data[i][j] }

    /// Minimum-cost assignment giving every row a distinct column.
    ///
    /// Returns a vector `assign` of length `rows` where `assign[i]` is the
    /// column chosen for row `i`, minimising the total cost. Requires
    /// `rows <= cols` (the caller guarantees this by padding the columns with
    /// enough "unassigned" fallbacks).
    ///
    /// Uses the O(rows^2 * cols) potentials method (Kuhn-Munkres / Hungarian
    /// with the Jonker-Volgenant shortest-augmenting-path loop). `u`/`v` are
    /// the dual potentials, `p[j]` is the row currently matched to column
    /// `j`, and `way` records the alternating path used to augment.
    pub fn assign_min_cost(&self) -> Vec<usize> {
        let n = self.rows;
        let m = self.cols;
        assert!(n <= m, "assignment requires rows <= cols (got {n} rows, {m} cols)");
        if n == 0 {
            return Vec::new();
        }

        const INF: i64 = i64::MAX / 4;
        // 1-indexed working state; column 0 is the virtual starting node.
        let mut u = vec![0i64; n + 1];
        let mut v = vec![0i64; m + 1];
        let mut p = vec![0usize; m + 1]; // p[j] = row matched to column j (0 = free)
        let mut way = vec![0usize; m + 1];

        for i in 1..=n {
            p[0] = i;
            let mut j0 = 0usize;
            let mut minv = vec![INF; m + 1];
            let mut used = vec![false; m + 1];

            // Grow a shortest alternating path until it reaches a free column.
            loop {
                used[j0] = true;
                let i0 = p[j0];
                let mut delta = INF;
                let mut j1 = 0usize;
                for j in 1..=m {
                    if !used[j] {
                        let cur = self.data[i0 - 1][j - 1] - u[i0] - v[j];
                        if cur < minv[j] {
                            minv[j] = cur;
                            way[j] = j0;
                        }
                        if minv[j] < delta {
                            delta = minv[j];
                            j1 = j;
                        }
                    }
                }
                // Shift the dual potentials along the path by `delta`.
                for j in 0..=m {
                    if used[j] {
                        u[p[j]] += delta;
                        v[j] -= delta;
                    } else {
                        minv[j] -= delta;
                    }
                }
                j0 = j1;
                if p[j0] == 0 {
                    break;
                }
            }

            // Walk the path back, flipping matched columns onto their new rows.
            loop {
                let j1 = way[j0];
                p[j0] = p[j1];
                j0 = j1;
                if j0 == 0 {
                    break;
                }
            }
        }

        let mut assign = vec![0usize; n];
        for j in 1..=m {
            if p[j] != 0 {
                assign[p[j] - 1] = j - 1;
            }
        }
        assign
    }
}

/// One appointment's line in a batch-reassignment plan: where it is now and
/// which colleague the optimiser moved it to (or `None` when no feasible
/// colleague had capacity). Serialisable so the preview template renders it
/// directly.
#[derive(Debug, Serialize)]
pub struct ReassignRow {
    pub appointment_id: i64,
    pub patient_name: String,
    pub start_time: String,
    pub end_time: String,
    pub from_doctor_name: String,
    pub to_doctor_id: Option<i64>,
    pub to_doctor_name: Option<String>,
    pub same_specialization: bool,
}

/// The full result of running Algorithm 5 for one doctor on one date: the
/// per-appointment moves plus headline counts. A preview (before applying) and
/// the applied result share this shape.
#[derive(Debug, Serialize)]
pub struct ReassignPlan {
    pub source_doctor_id: i64,
    pub source_doctor_name: String,
    pub date: String,
    pub rows: Vec<ReassignRow>,
    pub assigned_count: usize,
    pub unassigned_count: usize,
    pub total_cost: i64,
}

// ============================================================
// CostMatrix: Hungarian assignment (Algorithm 5)
// ============================================================

#[cfg(test)]
mod tests {
    use super::{build_cost_matrix, Candidate, CostMatrix, SourceAppointment};
    use crate::availability::models::DoctorAvailability;
    use std::collections::HashMap;

    /// Sum the chosen cells of an assignment.
    fn total(data: &[Vec<i64>], assign: &[usize]) -> i64 {
        assign.iter().enumerate().map(|(i, &j)| data[i][j]).sum()
    }

    /// Reference optimum by exhaustive search over injective row→column maps.
    /// Only used in tests, on tiny matrices, to prove the Hungarian result is
    /// genuinely optimal (not merely feasible).
    fn brute_force_min(data: &[Vec<i64>]) -> i64 {
        fn rec(data: &[Vec<i64>], i: usize, used: &mut [bool]) -> i64 {
            if i == data.len() {
                return 0;
            }
            let mut best = i64::MAX;
            for j in 0..data[0].len() {
                if !used[j] {
                    used[j] = true;
                    let sub = rec(data, i + 1, used);
                    if sub != i64::MAX {
                        best = best.min(data[i][j] + sub);
                    }
                    used[j] = false;
                }
            }
            best
        }
        let mut used = vec![false; data[0].len()];
        rec(data, 0, &mut used)
    }

    #[test]
    fn assignment_picks_zero_diagonal() {
        // Cheapest choice is the diagonal; anything else costs 10.
        let data = vec![
            vec![0, 10, 10],
            vec![10, 0, 10],
            vec![10, 10, 0],
        ];
        let m = CostMatrix::new(data.clone());
        let a = m.assign_min_cost();
        assert_eq!(a, vec![0, 1, 2]);
        assert_eq!(total(&data, &a), 0);
    }

    #[test]
    fn assignment_beats_the_greedy_choice() {
        // Greedy by row takes row0→col0 (1), forcing row1 onto col1 (100) = 101.
        // The optimum sends row0→col1 (2) and row1→col0 (1) = 3. This is exactly
        // why Algorithm 5 uses global optimisation instead of the per-row greed
        // of Algorithm 4.
        let data = vec![vec![1, 2], vec![1, 100]];
        let m = CostMatrix::new(data.clone());
        let a = m.assign_min_cost();
        assert_eq!(total(&data, &a), 3);
        assert_eq!(a, vec![1, 0]);
    }

    #[test]
    fn assignment_swaps_when_cheaper() {
        let data = vec![vec![2, 1], vec![1, 2]];
        let m = CostMatrix::new(data.clone());
        let a = m.assign_min_cost();
        assert_eq!(a, vec![1, 0]);
        assert_eq!(total(&data, &a), 2);
    }

    #[test]
    fn assignment_handles_more_columns_than_rows() {
        // One job, three targets: it must take the cheapest column.
        let data = vec![vec![5, 9, 1]];
        let m = CostMatrix::new(data.clone());
        let a = m.assign_min_cost();
        assert_eq!(a, vec![2]);
        assert_eq!(total(&data, &a), 1);
    }

    #[test]
    fn assignment_matches_brute_force() {
        // Assorted square and rectangular matrices: the Hungarian total must
        // equal the exhaustive optimum every time.
        let cases: Vec<Vec<Vec<i64>>> = vec![
            vec![vec![9, 2, 7, 8], vec![6, 4, 3, 7], vec![5, 8, 1, 8], vec![7, 6, 9, 4]],
            vec![vec![4, 1, 3], vec![2, 0, 5], vec![3, 2, 2]],
            vec![vec![10, 19, 8, 15], vec![10, 18, 7, 17], vec![13, 16, 9, 14]],
            vec![vec![1, 1000, 1000], vec![1000, 1, 1000]],
        ];
        for data in cases {
            let m = CostMatrix::new(data.clone());
            let a = m.assign_min_cost();
            // Every row gets a distinct column.
            let mut cols = a.clone();
            cols.sort_unstable();
            cols.dedup();
            assert_eq!(cols.len(), a.len(), "columns must be distinct: {a:?}");
            assert_eq!(
                total(&data, &a),
                brute_force_min(&data),
                "Hungarian total must equal the exhaustive optimum for {data:?}"
            );
        }
    }

    // --- build_cost_matrix ---

    fn appt(id: i64, start: &str, end: &str, start_min: i32, end_min: i32) -> SourceAppointment {
        SourceAppointment {
            id,
            patient_name: format!("Patient {id}"),
            start: start.to_string(),
            end: end.to_string(),
            start_min,
            end_min,
        }
    }

    fn candidate(doctor_id: i64, specialization: &str, load: i64) -> Candidate {
        Candidate { doctor_id, name: format!("Dr {doctor_id}"), specialization: specialization.into(), load }
    }

    fn working_hours(doctor_id: i64) -> Vec<DoctorAvailability> {
        vec![DoctorAvailability::draft(doctor_id, 0, "08:00", "17:00", true, None, false)]
    }

    #[test]
    fn build_cost_matrix_prefers_same_specialization() {
        let appts = vec![appt(1, "09:00", "09:30", 540, 570)];
        let candidates = vec![candidate(2, "Cardiology", 0), candidate(3, "Dermatology", 0)];
        let busy = HashMap::new();
        let rules = HashMap::from([(2, working_hours(2)), (3, working_hours(3))]);

        let matrix = build_cost_matrix(&appts, &candidates, "Cardiology", &busy, &rules);
        let assignment = matrix.assign_min_cost();

        // Column 0 is colleague 2's (same specialisation) single capacity slot.
        assert_eq!(assignment[0], 0);
    }

    #[test]
    fn build_cost_matrix_marks_busy_colleague_infeasible() {
        let appts = vec![appt(1, "09:00", "09:30", 540, 570)];
        let candidates = vec![candidate(2, "Cardiology", 0)];
        let busy = HashMap::from([(2, vec![(540, 570)])]); // colleague 2 already booked over this slot
        let rules = HashMap::from([(2, working_hours(2))]);

        let matrix = build_cost_matrix(&appts, &candidates, "Cardiology", &busy, &rules);
        let assignment = matrix.assign_min_cost();

        // Only feasible choice is the "unassigned" fallback column (index 1).
        assert_eq!(assignment[0], 1);
    }

    #[test]
    fn build_cost_matrix_spreads_load_across_capacity_copies() {
        // Two appointments, one colleague: the second copy costs more (LOAD_WEIGHT
        // per extra pickup), so the optimiser should still place both rather than
        // stack them for free.
        let appts = vec![
            appt(1, "09:00", "09:30", 540, 570),
            appt(2, "10:00", "10:30", 600, 630),
        ];
        let candidates = vec![candidate(2, "Cardiology", 0)];
        let busy = HashMap::new();
        let rules = HashMap::from([(2, working_hours(2))]);

        let matrix = build_cost_matrix(&appts, &candidates, "Cardiology", &busy, &rules);
        let assignment = matrix.assign_min_cost();

        // Both capacity columns (0 and 1) for the sole colleague get used, one per row.
        let mut cols = assignment.clone();
        cols.sort_unstable();
        assert_eq!(cols, vec![0, 1]);
    }
}
