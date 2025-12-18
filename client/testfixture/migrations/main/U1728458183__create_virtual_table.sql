-- Create a virtual R-star table backed by physical, shadow tables.
--
-- NOTE: The column types are here only for readability. rtree doesn't care.
CREATE VIRTUAL TABLE virtual_spatial_index USING rtree(
  id INTEGER,

  -- Minimum and maximum X coordinate (rtree uses f32)
  minX,
  maxX,

  -- Minimum and maximum Y coordinate (rtree uses f32)
  minY,
  maxY,

  -- From the docs:
  --
  -- "For auxiliary columns, only the name of the column matters. The type
  -- affinity is ignored. Constraints such as NOT NULL, UNIQUE, REFERENCES, or
  -- CHECK are also ignored. However, future versions of SQLite might start
  -- paying attention to the type affinity and constraints, so users of
  -- auxiliary columns are advised to leave both blank, to avoid future
  -- compatibility problems."
  +uuid BLOB
);

-- 14 zipcodes near Charlotte, NC. Inspired by https://sqlite.org/rtree.html.
INSERT INTO virtual_spatial_index VALUES
  (28215, -80.781227, -80.604706, 35.208813, 35.297367, uuid_v7()),
  (28216, -80.957283, -80.840599, 35.235920, 35.367825, uuid_v7()),
  (28217, -80.960869, -80.869431, 35.133682, 35.208233, uuid_v7()),
  (28226, -80.878983, -80.778275, 35.060287, 35.154446, uuid_v7()),
  (28227, -80.745544, -80.555382, 35.130215, 35.236916, uuid_v7()),
  (28244, -80.844208, -80.841988, 35.223728, 35.225471, uuid_v7()),
  (28262, -80.809074, -80.682938, 35.276207, 35.377747, uuid_v7()),
  (28269, -80.851471, -80.735718, 35.272560, 35.407925, uuid_v7()),
  (28270, -80.794983, -80.728966, 35.059872, 35.161823, uuid_v7()),
  (28273, -80.994766, -80.875259, 35.074734, 35.172836, uuid_v7()),
  (28277, -80.876793, -80.767586, 35.001709, 35.101063, uuid_v7()),
  (28278, -81.058029, -80.956375, 35.044701, 35.223812, uuid_v7()),
  (28280, -80.844208, -80.841972, 35.225468, 35.227203, uuid_v7()),
  (28282, -80.846382, -80.844193, 35.223972, 35.225655, uuid_v7());

-- NOTE: define rejects mutating statements.
-- CREATE VIRTUAL TABLE virtual_spatial_index_writer USING define(
--   (INSERT INTO virtual_spatial_index VALUES ($1, $2, $3, $4, $5, uuid_v7()) RETURNING *));

-- Create a virtual table based on a stored procedure.
CREATE VIRTUAL TABLE simple_vtable_from_stored_procedure
  USING define((SELECT UNIXEPOCH() AS epoch, $1 AS random_number));
