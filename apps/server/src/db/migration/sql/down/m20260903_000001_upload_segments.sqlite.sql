-- Reversible exactly, because every session still has exactly one segment: the
-- three columns are restored from `seq = 0` and nothing is lost. That stops
-- being true the moment a session is planned onto two backends, which is why
-- this migration is the one that has to land before splitting does.
--
-- `object_key` comes back nullable where it was `NOT NULL`. Adding a `NOT NULL`
-- column to a table with rows needs a default that would be a lie, and a
-- rollback that leaves the column nullable is a smaller wrong than one that
-- invents keys.
ALTER TABLE uploads ADD COLUMN object_key TEXT;
ALTER TABLE uploads ADD COLUMN backend TEXT;
ALTER TABLE uploads ADD COLUMN multipart_id TEXT;

UPDATE uploads
   SET object_key = (SELECT s.object_key FROM upload_segments s
                      WHERE s.upload_id = uploads.id AND s.seq = 0),
       backend = (SELECT s.backend FROM upload_segments s
                   WHERE s.upload_id = uploads.id AND s.seq = 0),
       multipart_id = (SELECT s.multipart_id FROM upload_segments s
                        WHERE s.upload_id = uploads.id AND s.seq = 0);

DROP TABLE IF EXISTS upload_segments;
