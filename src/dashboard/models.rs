use serde::Serialize;

/// A doctor's scheduled-appointment load (for the busiest-doctors ranking).
#[derive(Debug, Serialize)]
pub struct DoctorLoad {
    pub doctor_name: String,
    pub appointment_count: i64,
}

/// Aggregated operational statistics for the admin analytics dashboard.
///
/// The struct stores raw counts gathered by the service layer; derived
/// metrics (rates, outstanding balances) are computed by methods so the
/// business formulas live with the data they describe.
#[derive(Debug, Serialize)]
pub struct DashboardStats {
    pub total_patients: i64,
    pub total_doctors: i64,
    pub appointments_today: i64,
    pub appointments_this_week: i64,
    pub scheduled_count: i64,
    pub completed_count: i64,
    pub cancelled_count: i64,
    pub waitlist_waiting: i64,
    pub billed_total: f64,
    pub collected_total: f64,
    pub pending_invoices: i64,
    pub busiest_doctors: Vec<DoctorLoad>,
}

impl DashboardStats {
    /// Share of all appointments that ended up cancelled, as a percentage.
    pub fn cancellation_rate(&self) -> f64 {
        let total = self.scheduled_count + self.completed_count + self.cancelled_count;
        if total == 0 {
            0.0
        } else {
            (self.cancelled_count as f64 / total as f64) * 100.0
        }
    }

    /// Money billed but not yet collected (never negative, overpayments
    /// don't create phantom debt).
    pub fn outstanding_amount(&self) -> f64 {
        (self.billed_total - self.collected_total).max(0.0)
    }

    /// Share of billed money that has been collected, as a percentage.
    pub fn collection_rate(&self) -> f64 {
        if self.billed_total <= 0.0 {
            100.0
        } else {
            (self.collected_total / self.billed_total * 100.0).min(100.0)
        }
    }
}
