use crate::billing::models::{CreateInvoiceForm, Invoice, InvoiceItem, InvoiceView, Payment, RecordPaymentForm};
use crate::db;
use crate::errors::AppError;
use sqlx::SqlitePool;

// ============================================================
// Invoice CRUD
// ============================================================

/// Create an invoice with itemized line items parsed from the form.
/// The `items` field must contain one item per line in the format:
///   "Description|quantity|unit_price"
/// At least one valid line is required; malformed lines are skipped.
pub async fn create_invoice(
    pool: &SqlitePool,
    form: &CreateInvoiceForm,
) -> Result<Invoice, AppError> {
    // Parse itemized line items from the submitted text
    let line_items: Vec<(String, i32, f64)> = form
        .items
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(3, '|').collect();
            if parts.len() != 3 { return None; }
            let description = parts[0].trim().to_string();
            let quantity: i32 = parts[1].trim().parse().ok()?;
            let unit_price: f64 = parts[2].trim().parse().ok()?;
            if description.is_empty() || quantity <= 0 || unit_price < 0.0 { return None; }
            Some((description, quantity, unit_price))
        })
        .collect();

    if line_items.is_empty() {
        return Err(AppError::BadRequest(
            "At least one valid line item is required. Format: Description|quantity|unit_price"
                .into(),
        ));
    }

    let grand_total: f64 = line_items
        .iter()
        .map(|(_, qty, price)| (*qty as f64) * price)
        .sum();

    // Create the invoice header
    let invoice = sqlx::query_as::<_, Invoice>(
        "INSERT INTO invoices (patient_id, due_date, total_amount, status)
         VALUES (?, ?, ?, 'pending')
         RETURNING id, patient_id, invoice_date, due_date, total_amount, status, created_at",
    )
    .bind(form.patient_id)
    .bind(&form.due_date)
    .bind(grand_total)
    .fetch_one(pool)
    .await?;

    // Insert each line item
    for (description, quantity, unit_price) in &line_items {
        let total_price = (*quantity as f64) * unit_price;
        sqlx::query(
            "INSERT INTO invoice_items (invoice_id, description, quantity, unit_price, total_price)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(invoice.id)
        .bind(description)
        .bind(quantity)
        .bind(unit_price)
        .bind(total_price)
        .execute(pool)
        .await?;
    }

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
    let patient_id = db::get_patient_id(pool, patient_user_id).await?;

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

