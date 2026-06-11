use serde::{Deserialize, Serialize};

/// One immutable entry in the audit trail.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: i64,
    pub user_id: Option<i64>,
    pub username: String,
    pub role: String,
    pub action: String,     // e.g. "appointment.booked"
    pub entity: String,     // e.g. "appointment"
    pub entity_id: Option<i64>,
    pub details: Option<String>,
    pub created_at: chrono::NaiveDateTime,
}

impl AuditEntry {
    /// Bulma badge colour for the action category (presentation hint,
    /// derived from data — keeps the template free of business rules).
    pub fn action_badge_class(&self) -> &'static str {
        if self.action.starts_with("user.") {
            "is-link"
        } else if self.action.starts_with("appointment.") || self.action.starts_with("waitlist.") {
            "is-info"
        } else if self.action.starts_with("invoice.") || self.action.starts_with("payment.") {
            "is-warning"
        } else if self.action.starts_with("record.") || self.action.starts_with("prescription.") {
            "is-success"
        } else {
            "is-light"
        }
    }
}

/// View model: an audit entry plus its precomputed badge class, so the
/// template never needs to interpret action strings.
#[derive(Debug, Serialize)]
pub struct AuditEntryView {
    #[serde(flatten)]
    pub entry: AuditEntry,
    pub badge_class: &'static str,
}

impl From<AuditEntry> for AuditEntryView {
    fn from(entry: AuditEntry) -> Self {
        let badge_class = entry.action_badge_class();
        AuditEntryView { entry, badge_class }
    }
}
