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

/// Form for creating a new invoice.
#[derive(Debug, Deserialize)]
pub struct CreateInvoiceForm {
    pub patient_id: i64,
    pub due_date: String,
    #[allow(dead_code)]
    pub items: String, // JSON array of {description, quantity, unit_price} (future: itemized billing UI)
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
