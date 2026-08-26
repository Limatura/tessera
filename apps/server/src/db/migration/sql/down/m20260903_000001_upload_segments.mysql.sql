-- As the SQLite one, but written the way MySQL takes it: a correlated subquery
-- in `UPDATE` cannot read the table it is updating, so the restore is a join.
ALTER TABLE uploads ADD COLUMN object_key TEXT;
ALTER TABLE uploads ADD COLUMN backend VARCHAR(191);
ALTER TABLE uploads ADD COLUMN multipart_id TEXT;

UPDATE uploads u
  JOIN upload_segments s ON s.upload_id = u.id AND s.seq = 0
   SET u.object_key = s.object_key,
       u.backend = s.backend,
       u.multipart_id = s.multipart_id;

DROP TABLE IF EXISTS upload_segments;
