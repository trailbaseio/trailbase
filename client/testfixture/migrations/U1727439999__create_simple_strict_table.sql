-- Create a canonical table satisfying API requirements.
CREATE TABLE simple_strict_table (
    id BLOB PRIMARY KEY CHECK (is_uuid_v7(id)) DEFAULT (uuid_v7()) NOT NULL,

    text_null TEXT,
    text_default TEXT DEFAULT '',
    text_not_null TEXT NOT NULL DEFAULT '',

    int_null INTEGER,
    int_default INTEGER DEFAULT 5,
    int_not_null INTEGER NOT NULL DEFAULT 7,

    real_null REAL,
    real_default REAL DEFAULT 5.1,
    real_not_null REAL NOT NULL DEFAULT 7.1,

    blob_null BLOB,
    blob_default BLOB DEFAULT X'AABBCCDD',
    blob_not_null BLOB NOT NULL DEFAULT X'AABBCCDD'
) STRICT;


-- Create a variety of views.
CREATE VIEW simple_complete_view AS SELECT * FROM simple_strict_table;
CREATE VIEW simple_subset_view AS SELECT id, text_null AS t_null, text_default AS t_default, text_not_null AS t_not_null FROM simple_strict_table;
CREATE VIEW simple_subset_wo_id_view AS SELECT text_null, text_default, text_not_null FROM simple_strict_table;
CREATE VIEW simple_filter_view AS SELECT * FROM simple_strict_table WHERE (int_not_null % 2) = 0;


INSERT INTO simple_strict_table
  (text_default, text_not_null, int_default, int_not_null, real_default, real_not_null, blob_default, blob_not_null)
VALUES
  ('1', '1', 1, 1, 1.1, 1.2, X'01', X'01'),
  ('2', '2', 2, 2, 2.1, 2.2, X'02', X'02'),
  ('3', '3', 3, 3, 3.1, 3.2, X'03', X'03'),
  ('4', '4', 4, 4, 4.1, 4.2, X'04', X'04'),
  ('5', '5', 5, 5, 5.1, 5.2, X'05', X'05'),
  ('6', '6', 6, 6, 6.1, 6.2, X'06', X'06'),
  ('7', '7', 7, 7, 7.1, 7.2, X'07', X'07'),
  ('8', '8', 8, 8, 8.1, 8.2, X'08', X'08'),
  ('9', '9', 9, 9, 9.1, 9.2, X'09', X'09'),
  ('10', '10', 10, 10, 10.1, 10.2, X'0A', X'0A'),
  ('11', '11', 11, 11, 11.1, 11.2, X'0B', X'0B'),
  ('12', '12', 12, 12, 12.1, 12.2, X'0C', X'0C'),
  ('13', '13', 13, 13, 13.1, 13.2, X'0D', X'0D'),
  ('14', '14', 14, 14, 14.1, 14.2, X'0E', X'0E'),
  ('15', '15', 15, 15, 15.1, 15.2, X'0F', X'0F'),
  ('16', '16', 16, 16, 16.1, 16.2, X'10', X'10'),
  ('17', '17', 17, 17, 17.1, 17.2, X'11', X'11'),
  ('18', '18', 18, 18, 18.1, 18.2, X'12', X'12'),
  ('19', '19', 19, 19, 19.1, 19.2, X'13', X'13'),
  ('20', '20', 20, 20, 20.1, 20.2, X'14', X'14'),
  ('21', '21', 21, 21, 21.1, 21.2, X'15', X'15');

CREATE TABLE simple_strict_table_int (
    id INTEGER PRIMARY KEY,

    text_null TEXT,
    blob_null BLOB,
    int_null INTEGER,
    real_null REAL,
    any_col ANY
) STRICT;

INSERT INTO simple_strict_table_int (id, text_null, blob_null, int_null, real_null, any_col)
VALUES
  (NULL, '1', X'01', 1, 1.1, 'one'),
  (NULL, '2', X'02', 2, 2.2, 2),
  (NULL, '3', X'03', 3, 3.3, 3.3);
