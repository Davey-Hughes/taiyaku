-- The reader signature (`taiyaku_read::reader_signature`) a file's page count
-- was taken under; a scan counts a file again, without hashing it, where this
-- differs from the signature of its format. Per entry, not per store or per
-- directory: scans run one directory at a time and a nested directory takes
-- rows from the one around it, so one stored value would be set by the first
-- directory scanned and skip the rest. Existing rows stay NULL: they were
-- counted before signatures existed, so the first scan counts each zip row
-- once more and hashes none of them again.
ALTER TABLE scan_entry ADD COLUMN counted_by TEXT;
