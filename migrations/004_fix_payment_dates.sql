-- Fix payment_date values that were seeded as date-only strings (YYYY-MM-DD).
-- The Payment struct maps this column to chrono::NaiveDateTime, which expects
-- "YYYY-MM-DD HH:MM:SS". Date-only strings cause a deserialization error and
-- a 500 page when viewing any invoice that has a recorded payment.
-- CURRENT_TIMESTAMP records the actual moment the migration runs rather than
-- fabricating a fake midnight time.
UPDATE payments
SET payment_date = CURRENT_TIMESTAMP
WHERE LENGTH(payment_date) = 10
  AND payment_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]';
