-- Migration 007: let audit_log survive user deletion.
-- audit_log already stores its own username/role snapshot on each row so the
-- trail stays meaningful after an account is gone, but the FK didn't match
-- that intent: with no ON DELETE clause, SQLite refused to delete any user
-- who had ever logged in or acted, because their audit rows still pointed at
-- them. This rebuilds the table with ON DELETE SET NULL instead, so an admin
-- can delete a user while the historical entries stick around (attributed
-- only via the stored username/role from here on).
-- SQLite has no ALTER TABLE ... ALTER CONSTRAINT, so the table has to be
-- rebuilt with the new FK clause. Nothing else references audit_log (it only
-- holds an outgoing FK to users), so no other table is affected.

CREATE TABLE audit_log_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER,                   -- NULL for anonymous/system actions or a deleted account
    username TEXT NOT NULL DEFAULT '',
    role TEXT NOT NULL DEFAULT '',
    action TEXT NOT NULL,
    entity TEXT NOT NULL,
    entity_id INTEGER,
    details TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL
);

INSERT INTO audit_log_new (id, user_id, username, role, action, entity, entity_id, details, created_at)
    SELECT id, user_id, username, role, action, entity, entity_id, details, created_at FROM audit_log;

DROP TABLE audit_log;
ALTER TABLE audit_log_new RENAME TO audit_log;

CREATE INDEX IF NOT EXISTS idx_audit_log_created_at ON audit_log(created_at);
CREATE INDEX IF NOT EXISTS idx_audit_log_user_id ON audit_log(user_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_action ON audit_log(action);
