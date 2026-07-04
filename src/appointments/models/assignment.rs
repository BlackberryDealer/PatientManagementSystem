//! `CostMatrix` — the pure Hungarian (Kuhn–Munkres) assignment solver behind
//! Algorithm 5, plus the plan types the batch-reassignment preview renders.
//!
//! Kept free of any database, exactly like `DaySchedule` holds the pure
//! gap-finding for Algorithm 2: the service layer builds the costs from the
//! schedule and interprets the result, while the optimisation itself is
//! unit-testable in isolation (below, against an exhaustive brute-force optimum).

use serde::Serialize;

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
    /// Implementation: the O(rows² · cols) potentials method (Kuhn–Munkres /
    /// Hungarian with the Jonker–Volgenant shortest-augmenting-path loop). The
    /// `u`/`v` arrays are dual potentials, `p[j]` is the row currently matched
    /// to column `j`, and `way` records the alternating path used to augment.
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
// CostMatrix — Hungarian assignment (Algorithm 5)
// ============================================================

#[cfg(test)]
mod tests {
    use super::CostMatrix;

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
}
