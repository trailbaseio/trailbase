CREATE TABLE IF NOT EXISTS file_upload_table (
  id             BLOB PRIMARY KEY NOT NULL CHECK(is_uuid_v7(id)) DEFAULT(uuid_v7()),
  single_file    TEXT CHECK(jsonschema('std.FileUpload', single_file)),
  multiple_files TEXT CHECK(jsonschema('std.FileUploads', multiple_files)),
  name           TEXT
) STRICT;
