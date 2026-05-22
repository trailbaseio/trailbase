WITH _REQ_FIELDS_(_) AS (SELECT value FROM json_array_elements_text(:__fields))
SELECT
  CAST(({{ create_access_rule }}) AS INTEGER)
FROM
  (SELECT CAST(:__user_id AS uuid) AS id) AS _USER_
  {%- if !column_metadata.is_empty() -%},
  (SELECT
    {%- for metadata in column_metadata -%}
      {% if !loop.first %},{% endif %} CAST({{ crate::records::util::named_placeholder(metadata.column.name) }} AS {{ metadata.column.type_name }}) AS "{{ metadata.column.name }}"
    {%- endfor -%}
  ) AS _REQ_
  {%- endif -%}
