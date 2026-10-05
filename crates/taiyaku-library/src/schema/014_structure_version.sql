-- Part A of docs/superpowers/specs/2026-10-04-day-to-day-deployment-design.md
-- (§2.2): a counter that moves whenever anything a served `Work` is built
-- from is written, so `taiyaku serve` can tell its in-memory works are stale
-- without every job that changes them having to say so.
--
-- **The six tables are exactly the ones `project::sidecar` reads**: `work`,
-- `edition`, `member`, `page_flag`, `alignment` and `run`. A table added to
-- that projection later needs its three triggers too, in a migration of its
-- own. `structure::tests` lists the six by name and would not know about a
-- seventh. A later migration that rebuilds one of these tables (create a
-- replacement, drop the original, rename, as migration 004 did for `user`)
-- drops its triggers with it, so that migration must recreate them;
-- `structure::tests` runs the whole migration list and would catch it.
--
-- **Triggers, not a call in each writer**, because a trigger fires for every
-- connection and every process: a `taiyaku library scan` run from a shell
-- beside the server is seen exactly as a scan the server ran itself.
--
-- **`progress` and `note` have none, deliberately.** They are written on a
-- page turn and a keystroke, and nothing a `Work` holds comes from them.
--
-- One row, pinned by `CHECK (id = 1)`, so "the counter" names one value.
CREATE TABLE structure_version (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  n  INTEGER NOT NULL
);
INSERT INTO structure_version (id, n) VALUES (1, 0);

CREATE TRIGGER structure_work_ins AFTER INSERT ON work BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_work_upd AFTER UPDATE ON work BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_work_del AFTER DELETE ON work BEGIN
  UPDATE structure_version SET n = n + 1;
END;

CREATE TRIGGER structure_edition_ins AFTER INSERT ON edition BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_edition_upd AFTER UPDATE ON edition BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_edition_del AFTER DELETE ON edition BEGIN
  UPDATE structure_version SET n = n + 1;
END;

CREATE TRIGGER structure_member_ins AFTER INSERT ON member BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_member_upd AFTER UPDATE ON member BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_member_del AFTER DELETE ON member BEGIN
  UPDATE structure_version SET n = n + 1;
END;

CREATE TRIGGER structure_page_flag_ins AFTER INSERT ON page_flag BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_page_flag_upd AFTER UPDATE ON page_flag BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_page_flag_del AFTER DELETE ON page_flag BEGIN
  UPDATE structure_version SET n = n + 1;
END;

CREATE TRIGGER structure_alignment_ins AFTER INSERT ON alignment BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_alignment_upd AFTER UPDATE ON alignment BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_alignment_del AFTER DELETE ON alignment BEGIN
  UPDATE structure_version SET n = n + 1;
END;

CREATE TRIGGER structure_run_ins AFTER INSERT ON run BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_run_upd AFTER UPDATE ON run BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_run_del AFTER DELETE ON run BEGIN
  UPDATE structure_version SET n = n + 1;
END;
