use crate::traits::{Reportable, StatusManaged};
use serde::{Deserialize, Serialize};

// ============================================================
// Invoice
// ============================================================

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Invoice {
    pub id: i64,
    pub patient_id: i64,
    pub invoice_date: chrono::NaiveDate,
    pub due_date: chrono::NaiveDate,
    pub total_amount: f64,
    pub status: String, // pending | paid | cancelled
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

// ============================================================
// InvoiceItem — line items
// ============================================================

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

// ============================================================
// Trait implementations — OOP via Rust traits (Tutorial 05)
// ============================================================

impl StatusManaged for Invoice {
    fn current_status(&self) -> &str { &self.status }

    fn is_active(&self) -> bool { self.status == "pending" }

    fn status_badge_class(&self) -> &str {
        match self.status.as_str() {
            "pending"   => "is-warning",
            "paid"      => "is-success",
            "cancelled" => "is-danger",
            _           => "is-light",
        }
    }
}

impl Reportable for Invoice {
    fn generate_summary(&self) -> String {
        format!(
            "Invoice #{} | Due: {} | Total: £{:.2} | Status: {}",
            self.id, self.due_date, self.total_amount, self.status
        )
    }
}

/// Joined view: invoice with patient name for display.
#[derive(Debug, Serialize)]
pub struct InvoiceView {
    pub id: i64,
    pub patient_name: String,
    pub invoice_date: chrono::NaiveDate,
    pub due_date: chrono::NaiveDate,
    pub total_amount: f64,
    pub status: String,
}
