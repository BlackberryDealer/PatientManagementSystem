-- ============================================================
-- Migration 002: Rooms seeding (structure now in 001)
-- ============================================================
-- Note: rooms table, room_id, priority, and waitlist were
-- moved to migration 001. This migration only seeds default rooms.

-- Seed default rooms
INSERT OR IGNORE INTO rooms (id, name, room_type, floor) VALUES (1, 'Consultation Room A', 'consultation', 'Floor 1');
INSERT OR IGNORE INTO rooms (id, name, room_type, floor) VALUES (2, 'Consultation Room B', 'consultation', 'Floor 1');
INSERT OR IGNORE INTO rooms (id, name, room_type, floor) VALUES (3, 'Consultation Room C', 'consultation', 'Floor 2');
INSERT OR IGNORE INTO rooms (id, name, room_type, floor) VALUES (4, 'Procedure Room', 'procedure', 'Floor 2');
INSERT OR IGNORE INTO rooms (id, name, room_type, floor) VALUES (5, 'X-Ray Suite', 'equipment', 'Floor 3');
INSERT OR IGNORE INTO rooms (id, name, room_type, floor) VALUES (6, 'Lab Room', 'lab', 'Floor 3');

