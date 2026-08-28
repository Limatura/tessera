-- Reversible while every session still has one segment: `seq` is zero on every
-- row, so dropping it loses nothing. That stops being true the moment a session
-- is planned onto two backends — by then two rows share a `(upload_id,
-- part_number)` and this would refuse, which is the honest failure.
ALTER TABLE upload_parts RENAME TO upload_parts_new;

CREATE TABLE upload_parts (
    upload_id   TEXT NOT NULL REFERENCES uploads (id) ON DELETE CASCADE,
    part_number INTEGER NOT NULL,
    content_id  TEXT NOT NULL,
    PRIMARY KEY (upload_id, part_number)
);

INSERT INTO upload_parts (upload_id, part_number, content_id)
SELECT upload_id, part_number, content_id FROM upload_parts_new;

DROP TABLE upload_parts_new;
