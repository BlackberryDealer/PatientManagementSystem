use crate::traits::{Reportable, StatusManaged};
use serde::{Deserialize, Serialize};

// ============================================================
// Invoice
// ============================================================

/// Lifecycle status of an invoice. Stored as TEXT; both sqlx and serde render
/// the variants lowercase to match the DB CHECK values and the template
/// comparisons (`invoice.status == 'paid'`). Typed, so the badge match is
/// exhaustive and an invalid status is unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum InvoiceStatus {
    Pending,
    Paid,
    Cancelled,
}

impl InvoiceStatus {
    /// Canonical lowercase string matching DB CHECK values and template comparisons.
    pub fn as_str(&self) -> &'static str {
        match self {
            InvoiceStatus::Pending => "pending",
            InvoiceStatus::Paid => "paid",
            InvoiceStatus::Cancelled => "cancelled",
        }
    }
}

/// A billing invoice issued to a patient. The `status` field is private
/// with a guarded accessor; settlement is detected by `is_settled_by()`.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Invoice {
    pub id: i64,
    pub patient_id: i64,
    pub invoice_date: chrono::NaiveDate,
    pub due_date: chrono::NaiveDate,
    pub total_amount: f64,
    status: InvoiceStatus, // pending | paid | cancelled
    pub created_at: chrono::NaiveDateTime,
}

/// Form for creating a new invoice with itemized line items.
/// `items` is a newline-separated list of entries in the format:
///   "Description|quantity|unit_price"
/// Example: "Consultation Fee|1|80.00\nX-Ray|2|45.00"
#[derive(Debug, Deserialize)]
pub struct CreateInvoiceForm {
    pub patient_id: i64,
    pub due_date: String,
    pub items: String,
}

/// One parsed invoice line item from the pipe-delimited form input.
/// E.g. "Consultation|2|150.00" → description="Consultation", qty=2, unit=150.00.
pub struct LineItem {
    pub description: String,
    pub quantity: i32,
    pub unit_price: f64,
}

impl LineItem {
    /// Calculate the line total: quantity × unit_price.
    pub fn total_price(&self) -> f64 {
        (self.quantity as f64) * self.unit_price
    }

    /// Grand total of an invoice's line items. Keeps the money math on the
    /// domain type instead of inline in the service layer (sibling of
    /// `total_price`).
    pub fn grand_total(items: &[LineItem]) -> f64 {
        items.iter().map(LineItem::total_price).sum()
    }
}

/// Upper bound on a single line item's quantity. Guards against fat-fingered
/// or malicious input (e.g. a stray extra digit) producing an invoice total
/// that overflows sane display/PDF rendering; no real clinic line item comes
/// close to this.
const MAX_LINE_ITEM_QUANTITY: i32 = 10_000;

impl CreateInvoiceForm {
    /// Parse and validate the submitted line items.
    /// Malformed lines are skipped; at least one valid line is required,
    /// and the due date must be a real YYYY-MM-DD date.
    pub fn parse_line_items(&self) -> Result<Vec<LineItem>, crate::errors::AppError> {
        if chrono::NaiveDate::parse_from_str(&self.due_date, "%Y-%m-%d").is_err() {
            return Err(crate::errors::AppError::BadRequest(
                "Due date must be a valid date in YYYY-MM-DD format".into(),
            ));
        }

        let items: Vec<LineItem> = self
            .items
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(3, '|').collect();
                if parts.len() != 3 { return None; }
                let description = parts[0].trim().to_string();
                let quantity: i32 = parts[1].trim().parse().ok()?;
                let unit_price: f64 = parts[2].trim().parse().ok()?;
                if description.is_empty()
                    || quantity <= 0
                    || quantity > MAX_LINE_ITEM_QUANTITY
                    || unit_price < 0.0
                {
                    return None;
                }
                Some(LineItem { description, quantity, unit_price })
            })
            .collect();

        if items.is_empty() {
            return Err(crate::errors::AppError::BadRequest(
                "At least one valid line item is required. Format: Description|quantity|unit_price"
                    .into(),
            ));
        }
        Ok(items)
    }
}

// ============================================================
// InvoiceItem, line items
// ============================================================

/// A persisted line-item row belonging to an invoice.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct InvoiceItem {
    pub id: i64,
    pub invoice_id: i64,
    pub description: String,
    pub quantity: i32,
    pub unit_price: f64,
    pub total_price: f64,
}

// ============================================================
// Payment
// ============================================================

/// A payment recorded against an invoice. Multiple payments may be
/// made against a single invoice; settlement is detected when
/// SUM(payments) ≥ invoice.total_amount.
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Payment {
    pub id: i64,
    pub invoice_id: i64,
    pub amount: f64,
    pub payment_date: chrono::NaiveDateTime,
    pub payment_method: String,
    pub transaction_ref: Option<String>,
}

/// Form for recording a payment.
#[derive(Debug, Deserialize)]
pub struct RecordPaymentForm {
    pub amount: f64,
    pub payment_method: String,
    pub transaction_ref: Option<String>,
}

impl RecordPaymentForm {
    /// Reject zero, negative, or non-finite amounts before anything
    /// touches the database.
    pub fn validate(&self) -> Result<(), crate::errors::AppError> {
        if !self.amount.is_finite() || self.amount <= 0.0 {
            return Err(crate::errors::AppError::BadRequest(
                "Payment amount must be greater than zero".into(),
            ));
        }
        if self.payment_method.trim().is_empty() {
            return Err(crate::errors::AppError::BadRequest(
                "Payment method is required".into(),
            ));
        }
        Ok(())
    }
}

// ============================================================
// Trait implementations, OOP via Rust traits (Tutorial 05)
// ============================================================

impl StatusManaged for Invoice {
    fn current_status(&self) -> &str { self.status.as_str() }

    fn is_active(&self) -> bool { self.status == InvoiceStatus::Pending }

    fn status_badge_class(&self) -> &str {
        match self.status {
            InvoiceStatus::Pending   => "is-warning",
            InvoiceStatus::Paid      => "is-success",
            InvoiceStatus::Cancelled => "is-danger",
        }
    }
}

impl Reportable for Invoice {
    fn generate_summary(&self) -> String {
        format!(
            "Invoice #{} | Due: {} | Total: ${:.2} | Status: {}",
            self.id, self.due_date, self.total_amount, self.current_status()
        )
    }
}

impl Invoice {
    /// Tolerance for comparing accumulated payments against the invoice total.
    /// Money is stored as `f64`, so summing several payments can land a
    /// fraction of a cent short of the total through binary rounding alone;
    /// half a cent is comfortably smaller than any real underpayment.
    const SETTLEMENT_EPSILON: f64 = 0.005;

    /// Only a pending invoice can accept payments, paid and cancelled
    /// invoices are closed.
    pub fn can_accept_payment(&self) -> bool {
        self.is_active() // StatusManaged: pending == active
    }

    /// Does the given total of recorded payments settle this invoice?
    ///
    /// Compares with a half-a-cent tolerance rather than raw `>=`, so summing
    /// several payments (e.g. three payments of 33.33 against a 100.00 total)
    /// can't leave an invoice stuck "pending" over a floating-point rounding
    /// error a fraction of a cent wide.
    pub fn is_settled_by(&self, total_paid: f64) -> bool {
        total_paid + Self::SETTLEMENT_EPSILON >= self.total_amount
    }

    /// State transition to `paid`. Mutates internal state (&mut self);
    /// callers persist the new status afterwards.
    ///
    /// Domain rule: only a pending invoice may be marked paid, the guard
    /// lives on the object itself, consistent with `Appointment::cancel`
    /// and `WaitlistEntry::accept`, so no caller can drive a paid/cancelled
    /// invoice into an illegal state.
    pub fn mark_paid(&mut self) -> Result<(), crate::errors::AppError> {
        if !self.can_accept_payment() {
            return Err(crate::errors::AppError::BadRequest(
                "Only a pending invoice can be marked paid".into(),
            ));
        }
        self.status = InvoiceStatus::Paid;
        Ok(())
    }

    /// State transition to `cancelled`. Same guard as `mark_paid`: an
    /// invoice already paid or cancelled cannot be voided, only a pending
    /// one. Existing payments (if any were recorded before cancellation)
    /// are left as-is; the ledger stays append-only.
    pub fn mark_cancelled(&mut self) -> Result<(), crate::errors::AppError> {
        if !self.can_accept_payment() {
            return Err(crate::errors::AppError::BadRequest(
                "Only a pending invoice can be cancelled".into(),
            ));
        }
        self.status = InvoiceStatus::Cancelled;
        Ok(())
    }
}

/// Joined view: invoice with patient name for display. Column aliases in
/// `INVOICE_VIEW_SELECT` match these field names, so rows deserialize directly
/// via `FromRow` (no manual tuple mapping).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InvoiceView {
    pub id: i64,
    pub patient_name: String,
    pub invoice_date: chrono::NaiveDate,
    pub due_date: chrono::NaiveDate,
    pub total_amount: f64,
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::{CreateInvoiceForm, Invoice, InvoiceStatus};

    // The billing templates compare `invoice.status == 'paid'` / `'pending'`,
    // so the serde representation must stay lowercase and equal `as_str()`.
    #[test]
    fn invoice_status_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&InvoiceStatus::Pending).unwrap(), "\"pending\"");
        assert_eq!(serde_json::to_string(&InvoiceStatus::Paid).unwrap(), "\"paid\"");
        assert_eq!(serde_json::to_string(&InvoiceStatus::Cancelled).unwrap(), "\"cancelled\"");
        assert_eq!(InvoiceStatus::Paid.as_str(), "paid");
    }

    fn pending_invoice(total_amount: f64) -> Invoice {
        Invoice {
            id: 1,
            patient_id: 1,
            invoice_date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            due_date: chrono::NaiveDate::from_ymd_opt(2026, 1, 31).unwrap(),
            total_amount,
            status: InvoiceStatus::Pending,
            created_at: chrono::NaiveDateTime::default(),
        }
    }

    // --- Invoice::is_settled_by ---

    #[test]
    fn is_settled_by_exact_amount() {
        assert!(pending_invoice(100.0).is_settled_by(100.0));
    }

    #[test]
    fn is_settled_by_rejects_real_underpayment() {
        assert!(!pending_invoice(100.0).is_settled_by(90.0));
    }

    #[test]
    fn is_settled_by_tolerates_floating_point_rounding() {
        // Three payments that sum to 100.0 in decimal but land a hair short in
        // binary floating point; a raw `>=` would leave this invoice stuck
        // "pending" forever despite being fully paid.
        let total_paid = 33.33 + 33.33 + 33.34;
        assert!(pending_invoice(100.0).is_settled_by(total_paid));
    }

    #[test]
    fn is_settled_by_accepts_overpayment() {
        assert!(pending_invoice(100.0).is_settled_by(150.0));
    }

    // --- CreateInvoiceForm::parse_line_items quantity bounds ---

    fn form_with_item(item_line: &str) -> CreateInvoiceForm {
        CreateInvoiceForm {
            patient_id: 1,
            due_date: "2026-01-31".into(),
            items: item_line.to_string(),
        }
    }

    #[test]
    fn parse_line_items_rejects_zero_quantity() {
        assert!(form_with_item("Consultation|0|80.00").parse_line_items().is_err());
    }

    #[test]
    fn parse_line_items_rejects_absurd_quantity() {
        assert!(form_with_item("Consultation|1000000|80.00").parse_line_items().is_err());
    }

    #[test]
    fn parse_line_items_accepts_reasonable_quantity() {
        let items = form_with_item("Consultation|5|80.00").parse_line_items().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].quantity, 5);
    }
}
