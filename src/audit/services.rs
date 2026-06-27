use crate::audit::models::AuditEntry;
use crate::auth::AuthUser;
use crate::errors::AppError;
use sqlx::SqlitePool;

/// Record an audit-trail entry for an action performed by a logged-in user.
///
/// Best-effort by design: auditing is a cross-cutting concern, so a failure
/// to write the log line must never fail the business action itself. Errors
/// are logged and swallowed.
pub async fn record(
    pool: &SqlitePool,
    user: &AuthUser,
    action: &str,
    entity: &str,
    entity_id: Option<i64>,
    details: &str,
) {
    record_raw(pool, Some(user.user_id), &user.username, user.role.as_str(), action, entity, entity_id, details)
        .await;
}

/// Record an audit entry with explicit actor fields — for flows where no
/// `AuthUser` extractor exists yet (e.g. registration and login, where the
/// session is created in the same request).
pub async fn record_raw(
    pool: &SqlitePool,
    user_id: Option<i64>,
    username: &str,
    role: &str,
    action: &str,
    entity: &str,
    entity_id: Option<i64>,
    details: &str,
) {
    let result = sqlx::query(
        "INSERT INTO audit_log (user_id, username, role, action, entity, entity_id, details)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(username)
    .bind(role)
    .bind(action)
    .bind(entity)
    .bind(entity_id)
    .bind(details)
    .execute(pool)
    .await;

    if let Err(e) = result {
        log::warn!("Failed to write audit log entry for action '{}': {}", action, e);
    }
}

/// Most recent audit entries, newest first.
pub async fn recent(pool: &SqlitePool, limit: i64) -> Result<Vec<AuditEntry>, AppError> {
    Ok(sqlx::query_as::<_, AuditEntry>(
        "SELECT * FROM audit_log ORDER BY created_at DESC, id DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}
