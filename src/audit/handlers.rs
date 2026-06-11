use actix_web::{web, HttpResponse};
use tera::Context;

use crate::audit::models::AuditEntryView;
use crate::audit::services;
use crate::auth::{require_admin, AuthUser};
use crate::errors::AppError;

/// GET /audit — view the audit trail (admin only)
pub async fn list_audit_log(
    pool: web::Data<sqlx::SqlitePool>,
    tera: web::Data<tera::Tera>,
    user: AuthUser,
) -> Result<HttpResponse, AppError> {
    require_admin(&user)?;

    let entries: Vec<AuditEntryView> = services::recent(pool.get_ref(), 200)
        .await?
        .into_iter()
        .map(AuditEntryView::from)
        .collect();

    let mut ctx = Context::new();
    ctx.insert("user", &user);
    ctx.insert("entries", &entries);
    ctx.insert("title", "Audit Trail");
    let rendered = tera.render("audit/list.html.tera", &ctx)?;
    Ok(HttpResponse::Ok().body(rendered))
}
