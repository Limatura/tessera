-- One row per UTC day of what the instance held, for the dashboard's trend.
--
-- # Why it is recorded, not computed
--
-- The past cannot be worked out afterwards: the sweep deletes a dead share
-- together with its size and its download tally, so yesterday's totals are gone
-- by the time anybody asks. The hourly sweep writes today's row and overwrites
-- it on every later pass that day, so a day's row is the last pass of that day.
--
-- The four figures mean what `GET /admin/stats` means by them, so a point on the
-- graph and the number beside it agree.
--
-- `day` is the date as text, `YYYY-MM-DD`, on every dialect: it sorts the same
-- way the calendar does, and a client only ever wants it as that string.
CREATE TABLE IF NOT EXISTS stats_snapshots (
    day          TEXT    NOT NULL PRIMARY KEY,
    stored_bytes INTEGER NOT NULL,
    shares       INTEGER NOT NULL,
    live_shares  INTEGER NOT NULL,
    bytes_served INTEGER NOT NULL,
    recorded_at  TEXT    NOT NULL
);
