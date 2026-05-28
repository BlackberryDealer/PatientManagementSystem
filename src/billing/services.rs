use crate::billing::models::{CreateInvoiceForm, Invoice, InvoiceItem, InvoiceView, Payment, RecordPaymentForm};
use crate::errors::AppError;
use sqlx::SqlitePool;

/// Get the patient's internal ID from user ID.
async fn get_patient_id(pool: &SqlitePool, user_id: i64) -> Result<i64, AppError> {
    let row = sqlx::query_as::<_, (i64,)>("SELECT id FROM patients WHERE user_id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Patient profile not found".into()))?;
    Ok(row.0)
}

// ============================================================
// Invoice CRUD
// ============================================================

/// Create an invoice with a single default line item.
/// TODO: Support multiple items from form input (currently uses a stub item).
/// TODO: PDF generation using a crate like `printpdf` or `wkhtmltopdf`.
pub async fn create_invoice(
    pool: &SqlitePool,
    form: &CreateInvoiceForm,
) -> Result<Invoice, AppError> {
    // Create the invoice header
    let invoice = sqlx::query_as::<_, Invoice>(
        "INSERT INTO invoices (patient_id, due_date, total_amount, status)
         VALUES (?, ?, 0.0, 'pending')
         RETURNING id, patient_id, invoice_date, due_date, total_amount, status, created_at",
    )
    .bind(form.patient_id)
    .bind(&form.due_date)
    .fetch_one(pool)
    .await?;

    // Add a stub line item (placeholder)
    // In production, items would be parsed from the form's JSON array
    let item_total = 100.0; // stub amount
    sqlx::query(
        "INSERT INTO invoice_items (invoice_id, description, quantity, unit_price, total_price)
         VALUES (?, ?, 1, ?, ?)",
    )
    .bind(invoice.id)
    .bind("Consultation Fee (stub — implement itemized billing)")
    .bind(item_total)
    .bind(item_total)
    .execute(pool)
    .await?;

    // Update total
    sqlx::query("UPDATE invoices SET total_amount = ? WHERE id = ?")
        .bind(item_total)
        .bind(invoice.id)
        .execute(pool)
        .await?;

    // Return with updated total
    get_invoice_by_id(pool, invoice.id).await
}

/// Get all invoices (admin view) with patient names.
pub async fn get_all_invoices(pool: &SqlitePool) -> Result<Vec<InvoiceView>, AppError> {
    let rows = sqlx::query_as::<_, (i64, String, chrono::NaiveDate, chrono::NaiveDate, f64, String)>(
        "SELECT i.id, u.full_name AS patient_name, i.invoice_date, i.due_date, i.total_amount, i.status
         FROM invoices i
         JOIN patients p ON i.patient_id = p.id
         JOIN users u ON p.user_id = u.id
         ORDER BY i.invoice_date DESC",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, patient_name, invoice_date, due_date, total_amount, status)| {
            InvoiceView { id, patient_name, invoice_date, due_date, total_amount, status }
        })
        .collect())
}

/// Get invoices for a specific patient.
pub async fn get_invoices_for_patient(
    pool: &SqlitePool,
    patient_user_id: i64,
) -> Result<Vec<InvoiceView>, AppError> {
    let patient_id = get_patient_id(pool, patient_user_id).await?;

    let rows = sqlx::query_as::<_, (i64, String, chrono::NaiveDate, chrono::NaiveDate, f64, String)>(
        "SELECT i.id, u.full_name AS patient_name, i.invoice_date, i.due_date, i.total_amount, i.status
         FROM invoices i
         JOIN patients p ON i.patient_id = p.id
         JOIN users u ON p.user_id = u.id
         WHERE i.patient_id = ?
         ORDER BY i.invoice_date DESC",
    )
    .bind(patient_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, patient_name, invoice_date, due_date, total_amount, status)| {
            InvoiceView { id, patient_name, invoice_date, due_date, total_amount, status }
        })
        .collect())
}

/// Get a single invoice by ID.
pub async fn get_invoice_by_id(pool: &SqlitePool, invoice_id: i64) -> Result<Invoice, AppError> {
    sqlx::query_as::<_, Invoice>("SELECT * FROM invoices WHERE id = ?")
        .bind(invoice_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Invoice not found".into()))
}

/// Get line items for an invoice.
pub async fn get_invoice_items(
    pool: &SqlitePool,
    invoice_id: i64,
) -> Result<Vec<InvoiceItem>, AppError> {
    Ok(
        sqlx::query_as::<_, InvoiceItem>(
            "SELECT * FROM invoice_items WHERE invoice_id = ?",
        )
        .bind(invoice_id)
        .fetch_all(pool)
        .await?,
    )
}

/// Get payments for an invoice.
pub async fn get_invoice_payments(
    pool: &SqlitePool,
    invoice_id: i64,
) -> Result<Vec<Payment>, AppError> {
    Ok(
        sqlx::query_as::<_, Payment>("SELECT * FROM payments WHERE invoice_id = ?")
            .bind(invoice_id)
            .fetch_all(pool)
            .await?,
    )
}

// ============================================================
// Payment
// ============================================================

/// Record a payment against an invoice.
/// If the total paid >= invoice total, mark the invoice as paid.
/// TODO: Implement partial payment tracking and overpayment refund.
pub async fn record_payment(
    pool: &SqlitePool,
    invoice_id: i64,
    form: &RecordPaymentForm,
) -> Result<Payment, AppError> {
    let payment = sqlx::query_as::<_, Payment>(
        "INSERT INTO payments (invoice_id, amount, payment_method, transaction_ref)
         VALUES (?, ?, ?, ?)
         RETURNING id, invoice_id, amount, payment_date, payment_method, transaction_ref",
    )
    .bind(invoice_id)
    .bind(form.amount)
    .bind(&form.payment_method)
    .bind(&form.transaction_ref)
    .fetch_one(pool)
    .await?;

    // Check if invoice is fully paid
    let total_paid: (Option<f64>,) =
        sqlx::query_as("SELECT SUM(amount) FROM payments WHERE invoice_id = ?")
            .bind(invoice_id)
            .fetch_one(pool)
            .await?;

    let invoice = get_invoice_by_id(pool, invoice_id).await?;

    if let Some(paid) = total_paid.0 {
        if paid >= invoice.total_amount {
            sqlx::query("UPDATE invoices SET status = 'paid' WHERE id = ?")
                .bind(invoice_id)
                .execute(pool)
                .await?;
        }
    }

    Ok(payment)
}

// ============================================================
// PDF Generation Stub
// ============================================================
// TODO: Implement PDF invoice generation using a crate such as:
//   - `genpdf` — pure Rust, simple API
//   - `printpdf` — low-level PDF construction
//   - `wkhtmltopdf` — render HTML to PDF via external binary
//
// The entry point would be a handler like:
//   GET /billing/{id}/pdf
// which calls:
//   pub async fn generate_invoice_pdf(pool: &SqlitePool, invoice_id: i64) -> Result<Vec<u8>, AppError>
// and returns the PDF bytes with Content-Type: application/pdf.
