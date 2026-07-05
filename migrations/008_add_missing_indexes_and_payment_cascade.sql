-- Migration 008: fill two indexing gaps and give payments a real cascade.
--
-- 1. medical_records and prescriptions were indexed on patient_id and
--    appointment_id but not doctor_id, even though "a doctor's records" /
--    "a doctor's prescriptions" are common lookups (e.g. the patient
--    timeline's doctor-name joins). Cheap to add, no schema change needed.
--
-- 2. payments.invoice_id had no ON DELETE clause. Invoices are never
--    actually deleted today (cancel_invoice only flips status), but leaving
--    the FK bare means a future hard-delete path would be silently blocked
--    by orphaned payment rows instead of cleanly cascading. SQLite has no
--    ALTER TABLE ... ALTER CONSTRAINT, so the table has to be rebuilt with
--    the new FK clause (same approach as migration 007).

CREATE INDEX IF NOT EXISTS idx_medical_records_doctor_id ON medical_records(doctor_id);
CREATE INDEX IF NOT EXISTS idx_prescriptions_doctor_id ON prescriptions(doctor_id);

CREATE TABLE payments_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    invoice_id INTEGER NOT NULL,
    amount REAL NOT NULL,
    payment_date TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    payment_method TEXT NOT NULL,
    transaction_ref TEXT,
    FOREIGN KEY (invoice_id) REFERENCES invoices(id) ON DELETE CASCADE
);

INSERT INTO payments_new (id, invoice_id, amount, payment_date, payment_method, transaction_ref)
    SELECT id, invoice_id, amount, payment_date, payment_method, transaction_ref FROM payments;

DROP TABLE payments;
ALTER TABLE payments_new RENAME TO payments;

CREATE INDEX IF NOT EXISTS idx_payments_invoice_id ON payments(invoice_id);
