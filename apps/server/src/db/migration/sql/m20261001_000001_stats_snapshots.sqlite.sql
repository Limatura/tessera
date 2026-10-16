-- One row per UTC day of what the instance held, for the dashboard's trend.
--
-- # Why it is recorded, not computed
--
-- The past cannot be reconstructed: the sweep deletes a dead share along with
-- its size and download tally, so yesterday's totals are gone by the time
-- anyone asks. The hourly sweep writes today's row and overwrites it on every
-- later pass that day, so a day's row reflects its last pass.
--
-- The four figures mean what `GET /admin/stats` means by them, so a point on
-- the chart and the number beside it agree.
--
-- `day` is the date as `YYYY-MM-DD` text on every dialect: it sorts like the
-- calendar, and clients only ever want it as that string.
CREATE TABLE IF NOT EXISTS stats_snapshots (
    day          TEXT    NOT NULL PRIMARY KEY,
    stored_bytes INTEGER NOT NULL,
    shares       INTEGER NOT NULL,
    live_shares  INTEGER NOT NULL,
    bytes_served INTEGER NOT NULL,
    recorded_at  TEXT    NOT NULL
);
