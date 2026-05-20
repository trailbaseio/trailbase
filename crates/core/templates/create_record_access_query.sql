WITH _REQ_FIELDS_(_) AS (SELECT value FROM json_each(:__fields))
SELECT
  CAST(({{ create_access_rule }}) AS INTEGER)
FROM
  (SELECT :__user_id AS id) AS _USER_
  {% if !column_metadata.is_empty() -%}
  , (SELECT
    {%- for metadata in column_metadata -%}
      {% if !loop.first %},{% endif %}
      {{ crate::records::util::named_placeholder(metadata.column.name) }} AS "{{ metadata.column.name }}"
    {%- endfor -%}
  ) AS _REQ_
  {%- endif %}
