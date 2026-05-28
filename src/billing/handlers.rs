use actix_web::{web, HttpResponse};
use tera::Context;

use crate::auth::{require_admin, AuthUser};
use crate::billing::models::{CreateInvoiceForm, RecordPaymentForm};
use crate::billing::services;
use crate::errors::AppError;

/// GET /billing — list invoices filtered by role
pub async fn list_invoices(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let invoices = match user.role.as_str() {
        "admin" => services::get_all_invoices(pool.get_ref()).await?,
        _ => services::get_invoices_for_patient(pool.get_ref(), user.user_id).await?,
    };

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("invoices", &invoices);
    ctx.insert("title", "Billing");
    let rendered = tera.render("billing/list.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// GET /billing/create — show create invoice form (admin only)
pub async fn create_invoice_form(
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_admin(&user)?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("title", "Create Invoice");
    let rendered = tera.render("billing/create.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /billing/create — create a new invoice (admin only)
pub async fn create_invoice(
    pool: web::Data<sqlx::SqlitePool>,
    user: AuthUser,
    form: web::Form<CreateInvoiceForm>,
) -> Result<HttpResponse, AppError> {
    require_admin(&user)?;

    let invoice = services::create_invoice(pool.get_ref(), &form).await?;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/billing/{}", invoice.id)))
        .finish())
}

/// GET /billing/{id} — view invoice detail with items and payments
pub async fn invoice_detail(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    path: web::Path<i64>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    let invoice_id = path.into_inner();
    let invoice = services::get_invoice_by_id(pool.get_ref(), invoice_id).await?;
    let items = services::get_invoice_items(pool.get_ref(), invoice_id).await?;
    let payments = services::get_invoice_payments(pool.get_ref(), invoice_id).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("invoice", &invoice);
    ctx.insert("items", &items);
    ctx.insert("payments", &payments);
    ctx.insert("title", &format!("Invoice #{}", invoice.id));
    let rendered = tera.render("billing/detail.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}

/// POST /billing/{id}/pay — record a payment
pub async fn record_payment(
    pool: web::Data<sqlx::SqlitePool>,
    path: web::Path<i64>,
    user: AuthUser,
    form: web::Form<RecordPaymentForm>,
) -> Result<HttpResponse, AppError> {
    // Admin can record payments; patients could also in production
    require_admin(&user)?;

    let invoice_id = path.into_inner();
    services::record_payment(pool.get_ref(), invoice_id, &form).await?;

    Ok(HttpResponse::SeeOther()
        .append_header(("Location", format!("/billing/{}", invoice_id)))
        .finish())
}
