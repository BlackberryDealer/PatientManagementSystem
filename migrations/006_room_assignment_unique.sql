-- ============================================================
-- Migration 006: one room per doctor per day, enforced both ways
-- ============================================================
-- Migration 005 made (doctor_id, assignment_date) unique: a doctor
-- holds at most one room per day. This adds the mirror guarantee —
-- (room_id, assignment_date) unique — so a room is held by at most
-- one doctor per day. With both indexes, two concurrent "first
-- bookings" can never claim the same room: the second INSERT fails
-- atomically and the allocator retries with the next free room.
--
-- Existing databases may already contain double-assigned rooms from
-- the pre-index race window; keep the earliest claim per (room, date)
-- so the index can be created. Appointments keep their own room_id,
-- so historical bookings are unaffected.

DELETE FROM doctor_room_assignments
WHERE id NOT IN (
    SELECT MIN(id) FROM doctor_room_assignments
    GROUP BY room_id, assignment_date
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_dra_room_unique
    ON doctor_room_assignments(room_id, assignment_date);
