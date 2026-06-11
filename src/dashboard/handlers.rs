use actix_web::{web, HttpResponse};
use tera::Context;

use crate::auth::{require_admin, AuthUser};
use crate::dashboard::services;
use crate::errors::AppError;

/// GET /dashboard — operational analytics overview (admin only)
pub async fn show_dashboard(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_admin(&user)?;

    let stats = services::gather_stats(pool.get_ref()).await?;

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("stats", &stats);
    // Derived business metrics come from the struct's methods, not the template
    ctx.insert("cancellation_rate", &format!("{:.1}", stats.cancellation_rate()));
    ctx.insert("outstanding_amount", &format!("{:.2}", stats.outstanding_amount()));
    ctx.insert("collection_rate", &format!("{:.1}", stats.collection_rate()));
    ctx.insert("title", "Dashboard");
    let rendered = tera.render("dashboard/index.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}
