use crate::auth::Role;
use crate::billing::models::{CreateInvoiceForm, Invoice, InvoiceItem, InvoiceView, LineItem, Payment, RecordPaymentForm};
use crate::db;
use crate::errors::AppError;
use crate::traits::StatusManaged;
use sqlx::SqlitePool;

/// Shared SELECT for the joined invoice view. Each query appends its own
/// `WHERE` / `ORDER BY`. Columns are aliased to match `InvoiceView` field names
/// so rows deserialize directly via `FromRow`. Only static SQL is interpolated;
/// all user values are bound as parameters.
const INVOICE_VIEW_SELECT: &str = "\
    SELECT i.id, u.full_name AS patient_name, i.invoice_date, i.due_date,
           i.total_amount, i.status
    FROM invoices i
    JOIN patients p ON i.patient_id = p.id
    JOIN users u ON p.user_id = u.id";

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
    // Validation: the form owns its own parsing rules
    let line_items = form.parse_line_items()?;

    // Money math lives on the domain type, not inline here.
    let grand_total = LineItem::grand_total(&line_items);

    // Persist header + items atomically, an invoice can never exist
    // without its line items.
    let mut tx = pool.begin().await?;

    let invoice = sqlx::query_as::<_, Invoice>(
        "INSERT INTO invoices (patient_id, due_date, total_amount, status)
         VALUES (?, ?, ?, 'pending')
         RETURNING id, patient_id, invoice_date, due_date, total_amount, status, created_at",
    )
    .bind(form.patient_id)
    .bind(&form.due_date)
    .bind(grand_total)
    .fetch_one(&mut *tx)
    .await?;

    for item in &line_items {
        sqlx::query(
            "INSERT INTO invoice_items (invoice_id, description, quantity, unit_price, total_price)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(invoice.id)
        .bind(&item.description)
        .bind(item.quantity)
        .bind(item.unit_price)
        .bind(item.total_price())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(invoice)
}

/// Get all invoices (admin view) with patient names.
pub async fn get_all_invoices(pool: &SqlitePool) -> Result<Vec<InvoiceView>, AppError> {
    Ok(sqlx::query_as::<_, InvoiceView>(&format!(
        "{INVOICE_VIEW_SELECT} ORDER BY i.invoice_date DESC"
    ))
    .fetch_all(pool)
    .await?)
}

/// Get all patients as (id, name) for dropdowns. Delegates to the shared
/// helper in `db` (also used by records and the staff booking form).
pub async fn get_all_patients(pool: &SqlitePool) -> Result<Vec<(i64, String)>, AppError> {
    crate::db::get_all_patients(pool).await
}

/// Get invoices for a specific patient.
pub async fn get_invoices_for_patient(
    pool: &SqlitePool,
    patient_user_id: i64,
) -> Result<Vec<InvoiceView>, AppError> {
    let patient_id = db::get_patient_id(pool, patient_user_id).await?;

    Ok(sqlx::query_as::<_, InvoiceView>(&format!(
        "{INVOICE_VIEW_SELECT} WHERE i.patient_id = ? ORDER BY i.invoice_date DESC"
    ))
    .bind(patient_id)
    .fetch_all(pool)
    .await?)
}

/// Every invoice for a patient, keyed by the `patients` table row id rather
/// than `users.id` (unlike `get_invoices_for_patient`), and returning the
/// bare `Invoice` row rather than the name-joined `InvoiceView`. Used by the
/// cross-entity patient history timeline
/// (`records::services::build_patient_timeline`), which already has
/// `patient_id` on hand and only needs each invoice's own `Reportable`
/// summary, not a patient-name join it would immediately discard.
pub async fn get_invoices_for_patient_id(
    pool: &SqlitePool,
    patient_id: i64,
) -> Result<Vec<Invoice>, AppError> {
    Ok(sqlx::query_as::<_, Invoice>(
        "SELECT id, patient_id, invoice_date, due_date, total_amount, status, created_at
         FROM invoices WHERE patient_id = ?",
    )
    .bind(patient_id)
    .fetch_all(pool)
    .await?)
}

/// Get a single invoice by ID.
pub async fn get_invoice_by_id(pool: &SqlitePool, invoice_id: i64) -> Result<Invoice, AppError> {
    sqlx::query_as::<_, Invoice>("SELECT * FROM invoices WHERE id = ?")
        .bind(invoice_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Invoice not found".into()))
}

/// Get an invoice, enforcing ownership for patients.
/// Patients may only access their own invoices; staff roles see any.
/// Mirrors `appointments::services::cancel_appointment_checked`.
pub async fn get_invoice_checked(
    pool: &SqlitePool,
    invoice_id: i64,
    user_id: i64,
    role: Role,
) -> Result<Invoice, AppError> {
    let invoice = get_invoice_by_id(pool, invoice_id).await?;
    if role == Role::Patient {
        let patient_id = db::get_patient_id(pool, user_id).await?;
        if invoice.patient_id != patient_id {
            return Err(AppError::Forbidden(
                "You do not have permission to access this invoice.".into(),
            ));
        }
    }
    Ok(invoice)
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
/// Flow: validate the form, check the invoice can accept payments,
/// then insert the payment and (if settled) flip the status, all in
/// one transaction so the books always balance.
///
/// Partial payments are supported: each call appends a payment row, and the
/// invoice only flips to `paid` once the accumulated total settles it (see
/// `Invoice::is_settled_by`). Overpayment is accepted but not auto-refunded,
/// refund handling is intentionally out of scope for this system.
pub async fn record_payment(
    pool: &SqlitePool,
    invoice_id: i64,
    form: &RecordPaymentForm,
) -> Result<Payment, AppError> {
    // 1. Validation, nothing touches the database before this passes
    form.validate()?;

    // 2 + 3. Business rule and persistence share one transaction: the invoice
    //    is loaded and its can-accept-payment rule checked *inside* the same
    //    transaction that inserts the payment, so a concurrent payment cannot
    //    slip between the status check and the write (no check-then-act race).
    //    An early return before commit rolls the transaction back.
    let mut tx = pool.begin().await?;

    let mut invoice = sqlx::query_as::<_, Invoice>("SELECT * FROM invoices WHERE id = ?")
        .bind(invoice_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("Invoice not found".into()))?;
    if !invoice.can_accept_payment() {
        return Err(AppError::BadRequest(
            "This invoice is not open for payment (already paid or cancelled).".into(),
        ));
    }

    let payment = sqlx::query_as::<_, Payment>(
        "INSERT INTO payments (invoice_id, amount, payment_method, transaction_ref)
         VALUES (?, ?, ?, ?)
         RETURNING id, invoice_id, amount, payment_date, payment_method, transaction_ref",
    )
    .bind(invoice.id)
    .bind(form.amount)
    .bind(&form.payment_method)
    .bind(&form.transaction_ref)
    .fetch_one(&mut *tx)
    .await?;

    let total_paid: Option<f64> =
        sqlx::query_scalar("SELECT SUM(amount) FROM payments WHERE invoice_id = ?")
            .bind(invoice.id)
            .fetch_one(&mut *tx)
            .await?;

    if invoice.is_settled_by(total_paid.unwrap_or(0.0)) {
        invoice.mark_paid()?; // encapsulated, self-guarding state transition (&mut self)
        sqlx::query("UPDATE invoices SET status = ? WHERE id = ?")
            .bind(invoice.current_status())
            .bind(invoice.id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(payment)
}

/// Cancel (void) an invoice. Admin-only, enforced by the caller via
/// `require_admin`, since voiding a bill is an administrative action, not
/// something a patient does to their own account. Same transaction shape as
/// `record_payment`: load, run the guarded state transition, persist.
pub async fn cancel_invoice(pool: &SqlitePool, invoice_id: i64) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;

    let mut invoice = sqlx::query_as::<_, Invoice>("SELECT * FROM invoices WHERE id = ?")
        .bind(invoice_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound("Invoice not found".into()))?;

    invoice.mark_cancelled()?;
    sqlx::query("UPDATE invoices SET status = ? WHERE id = ?")
        .bind(invoice.current_status())
        .bind(invoice.id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(())
}

