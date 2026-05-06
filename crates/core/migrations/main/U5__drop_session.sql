-- Drop legacy `main._session` table. Sessions are now store separately in `data/session.db`.
DROP TABLE _session;
