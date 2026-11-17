-- When a cancelled scan paused this directory; NULL while it is not paused
-- (first-scan spec, §7 "Pause"). `schedule::scheduled` passes a paused
-- directory over, so the next tick does not undo the cancel, and the pause is
-- a column apart from `scan_interval_secs` so the schedule setting is kept.
-- `scan::pause` and `scan::unpause` are its writers, and a pressed scan
-- clears it. Every existing directory starts unpaused: no cancel paused it.
ALTER TABLE root ADD COLUMN paused_at TEXT;
