-- TimescaleDB hypertables will hold the raw check results (docs/adr/0002).
-- The timescale/timescaledb image preloads the library; this makes the extension
-- available inside the application database.
CREATE EXTENSION IF NOT EXISTS timescaledb;
