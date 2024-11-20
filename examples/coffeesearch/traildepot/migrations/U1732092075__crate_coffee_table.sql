CREATE TABLE IF NOT EXISTS coffee (
  Species TEXT NOT NULL,
  Owner TEXT NOT NULL,

  Aroma REAL NOT NULL,
  Flavor REAL NOT NULL,
  Acidity REAL NOT NULL,
  Sweetness REAL NOT NULL,

  embedding BLOB
) STRICT;

UPDATE coffee SET embedding = VECTOR(FORMAT("[%f, %f, %f, %f]", Aroma, Flavor, Acidity, Sweetness));
