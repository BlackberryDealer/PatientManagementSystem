use actix_web::{web, HttpResponse};
use tera::Context;

use crate::auth::{require_admin, AuthUser, Role};
use crate::billing::models::{CreateInvoiceForm, RecordPaymentForm};
use crate::billing::services;
use crate::errors::AppError;

/// GET /billing: list invoices filtered by role
pub async fn list_invoices(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let invoices = match user.role {
        Role::Admin => services::get_all_invoices(pool.get_ref()).await?,
        Role::Patient => services::get_invoices_for_patient(pool.get_ref(), user.user_id).await?,
        // Doctors are not part of the billing workflow and have no billing nav
        // link. If one reaches this URL directly, quietly send them back to
        // their home page rather than showing an error.
        Role::Doctor => {
            return Ok(HttpResponse::SeeOther()
                .append_header(("Location", "/appointments"))
                .finish())
        }
    };

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("invoices", &invoices);
    ctx.insert("title", "Billing");
    let rendered = tera.render("billing/list.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// GET /billing/create: show create invoice form (admin only)
pub async fn create_invoice_form(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_admin(&user)?;

    let patients = services::get_all_patients(pool.get_ref()).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("patients", &patients);
    ctx.insert("title", "Create Invoice");
    let rendered = tera.render("billing/create.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /billing/create: create a new invoice (admin only)
pub async fn create_invoice(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<CreateInvoiceForm>,
) -> Result<HttpResponse, AppError> {
    require_admin(&user)?;

    let invoice = services::create_invoice(pool.get_ref(), &form).await?;
    crate::audit::services::record(
        pool.get_ref(), &user, "invoice.created", "invoice", Some(invoice.id),
        &format!("${:.2} due {}", invoice.total_amount, invoice.due_date),
    ).await;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/billing/{}", invoice.id)))
        .finish())
}

/// GET /billing/{id}: view invoice detail with items and payments
pub async fn invoice_detail(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let invoice_id = path.into_inner();
    // Ownership rule (patients see only their own) lives in the service layer
    let invoice =
        services::get_invoice_checked(pool.get_ref(), invoice_id, user.user_id, user.role)
            .await?;

    let items = services::get_invoice_items(pool.get_ref(), invoice_id).await?;
    let payments = services::get_invoice_payments(pool.get_ref(), invoice_id).await?;

    // Calculate payment progress for the summary bar
    let total_paid: f64 = payments.iter().map(|p| p.amount).sum();
    let remaining = (invoice.total_amount - total_paid).max(0.0);
    let pct_paid = if invoice.total_amount > 0.0 {
        ((total_paid / invoice.total_amount) * 100.0).min(100.0)
    } else {
        0.0
    };

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("invoice", &invoice);
    ctx.insert("items", &items);
    ctx.insert("payments", &payments);
    ctx.insert("total_paid", &total_paid);
    ctx.insert("remaining", &remaining);
    ctx.insert("pct_paid", &pct_paid);
    ctx.insert("title", &format!("Invoice #{}", invoice.id));
    let rendered = tera.render("billing/detail.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /billing/{id}/pay: record a payment against an invoice.
/// Admins can record payments for any invoice.
/// Patients may pay their own invoices only.
pub async fn record_payment(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    user: AuthUser,
    form: web::Form<RecordPaymentForm>,
) -> Result<HttpResponse, AppError> {
    let invoice_id = path.into_inner();

    if user.role == Role::Patient {
        // Ownership rule enforced by the service layer
        services::get_invoice_checked(pool.get_ref(), invoice_id, user.user_id, user.role)
            .await?;
    } else {
        require_admin(&user)?;
    }

    let payment = services::record_payment(pool.get_ref(), invoice_id, &form).await?;
    crate::audit::services::record(
        pool.get_ref(), &user, "payment.recorded", "invoice", Some(invoice_id),
        &format!("${:.2} via {}", payment.amount, payment.payment_method),
    ).await;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/billing/{}", invoice_id)))
        .finish())
}

/// POST /billing/{id}/cancel: void a pending invoice (admin only)
pub async fn cancel_invoice(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_admin(&user)?;
    let invoice_id = path.into_inner();

    services::cancel_invoice(pool.get_ref(), invoice_id).await?;
    crate::audit::services::record(
        pool.get_ref(), &user, "invoice.cancelled", "invoice", Some(invoice_id), "",
    ).await;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/billing/{}", invoice_id)))
        .finish())
}
