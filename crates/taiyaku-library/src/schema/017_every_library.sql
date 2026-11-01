-- An account sees every library (1) or only those its `user_library` rows name
-- (0); 0 with no rows sees nothing. Without the column, an account with no rows
-- would have to mean "every library", so emptying a grant would widen it.
ALTER TABLE user ADD COLUMN every_library INTEGER NOT NULL DEFAULT 1
    CHECK (every_library IN (0, 1));
UPDATE user SET every_library = 0
    WHERE id IN (SELECT user_id FROM user_library);
-- Removing an account's only library does not widen it, so nothing is left for
-- these to stop.
DROP TRIGGER only_grant_refuses_delete;
DROP TRIGGER only_grant_refuses_move;
