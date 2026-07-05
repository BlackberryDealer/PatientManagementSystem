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
    record_raw(
        pool,
        NewAuditEntry {
            user_id: Some(user.user_id),
            username: &user.username,
            role: user.role.as_str(),
            action,
            entity,
            entity_id,
            details,
        },
    )
    .await;
}

/// One `audit_log` row's actor + action fields, named rather than positional
/// so a call site reads as `action: "user.login"` instead of an easily
/// misordered string of bare `&str`s.
pub struct NewAuditEntry<'a> {
    pub user_id: Option<i64>,
    pub username: &'a str,
    pub role: &'a str,
    pub action: &'a str,
    pub entity: &'a str,
    pub entity_id: Option<i64>,
    pub details: &'a str,
}

/// Record an audit entry with explicit actor fields, for flows where no
/// `AuthUser` extractor exists yet (e.g. registration and login, where the
/// session is created in the same request).
pub async fn record_raw(pool: &SqlitePool, entry: NewAuditEntry<'_>) {
    let result = sqlx::query(
        "INSERT INTO audit_log (user_id, username, role, action, entity, entity_id, details)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(entry.user_id)
    .bind(entry.username)
    .bind(entry.role)
    .bind(entry.action)
    .bind(entry.entity)
    .bind(entry.entity_id)
    .bind(entry.details)
    .execute(pool)
    .await;

    if let Err(e) = result {
        log::warn!("Failed to write audit log entry for action '{}': {}", entry.action, e);
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
