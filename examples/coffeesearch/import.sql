-- Create table if it doesn't exist.
CREATE TABLE IF NOT EXISTS coffee (
  Species TEXT,
  Owner TEXT,

  Aroma REAL,
  Flavor REAL,
  Acidity REAL,
  Sweetness REAL,

  embedding BLOB
) STRICT;

-- Go on to import data.
DROP TABLE IF EXISTS temporary;

.mode csv
.import arabica_data_cleaned.csv temporary

INSERT INTO coffee (Species, Owner, Aroma, Flavor, Acidity, Sweetness)
SELECT
  Species,
  Owner,

  CAST(Aroma AS REAL) AS Aroma,
  CAST(Flavor AS REAL) AS Flavor,
  CAST(Acidity AS REAL) AS Acidity,
  CAST(Sweetness AS REAL) AS Sweetness
FROM temporary;

-- Clean up.
DROP TABLE temporary;
