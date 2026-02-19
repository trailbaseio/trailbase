CREATE TABLE geometry (
  id            INTEGER PRIMARY KEY,
  description   TEXT,
  geom          BLOB CHECK(ST_IsValid(geom))
) STRICT;

CREATE INDEX _geometry_geom ON geometry(geom);

INSERT INTO geometry (description, geom) VALUES
  ('Colloseo',     ST_GeomFromText('POINT(12.4924 41.8902)', 4326)),
  ('A Line',       ST_GeomFromText('LINESTRING(10 20, 20 30)')),
  ('br-quadrant',  ST_MakeEnvelope(0, -0, 180, -90)),
  ('null',         NULL)
;
