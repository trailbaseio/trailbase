-- First create the strictly typed "coffee" table.
CREATE TABLE coffee (
  Species TEXT,
  Owner TEXT,

  Aroma REAL,
  Flavor REAL,
  Acidity REAL,
  Sweetness REAL,

  embedding BLOB
) STRICT;

-- Then import the data into a "temporary" table.
.mode csv
.import arabica_data_cleaned.csv temporary

-- Then import the un-typed temporary data into the typed "coffee" table.
INSERT INTO coffee (Species, Owner, Aroma, Flavor, Acidity, Sweetness)
  SELECT
    Species,
    Owner,

    CAST(Aroma AS REAL) AS Aroma,
    CAST(Flavor AS REAL) AS Flavor,
    CAST(Acidity AS REAL) AS Acidity,
    CAST(Sweetness AS REAL) AS Sweetness
  FROM temporary;

-- And clean up.
DROP TABLE temporary;
