-- ============================================================
-- Migration 003: Audit Log
-- Immutable trail of security- and business-relevant actions,
-- mirroring the audit-trail mechanisms found in enterprise
-- healthcare systems (who did what, to which entity, and when).
-- ============================================================

CREATE TABLE IF NOT EXISTS audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER,                   -- NULL for anonymous/system actions
    username TEXT NOT NULL DEFAULT '',
    role TEXT NOT NULL DEFAULT '',
    action TEXT NOT NULL,              -- e.g. 'appointment.booked'
    entity TEXT NOT NULL,              -- e.g. 'appointment', 'invoice'
    entity_id INTEGER,                 -- primary key of the affected row
    details TEXT,                      -- human-readable context
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (user_id) REFERENCES users(id)
);

CREATE INDEX IF NOT EXISTS idx_audit_log_created_at ON audit_log(created_at);
CREATE INDEX IF NOT EXISTS idx_audit_log_user_id ON audit_log(user_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_action ON audit_log(action);
