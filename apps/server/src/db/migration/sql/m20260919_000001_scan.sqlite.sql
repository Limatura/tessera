-- What a malware scanner reported about each file and note.
--
-- # `scan_state`
--
-- NULL when the content was never meant to be scanned: it arrived while no
-- scanner was configured. Otherwise one of:
--
-- - `pending`: waiting for a verdict. The share is not served.
-- - `deferred`: the scanner was unreachable. Served anyway, and rescanned once
--   the scanner is back.
-- - `clean`: scanned, nothing found.
-- - `infected`: scanned, and the share was disabled.
-- - `skipped`: nothing a scanner could inspect; `scan_detail` says why.
--
-- # `scan_detail`
--
-- The threat name for `infected`; `encrypted` or `too_large` for `skipped`.
--
-- Indexed on `file_shares`, since the scanner's queue is "every row still
-- `pending` or `deferred`" and that is the large table.
ALTER TABLE file_shares ADD COLUMN scan_state TEXT;
ALTER TABLE file_shares ADD COLUMN scan_detail TEXT;
ALTER TABLE text_shares ADD COLUMN scan_state TEXT;
ALTER TABLE text_shares ADD COLUMN scan_detail TEXT;
CREATE INDEX file_shares_scan_state ON file_shares (scan_state);
