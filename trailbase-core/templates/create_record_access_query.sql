WITH _REQ_FIELDS_(_) AS (SELECT value FROM (json_each(:__fields)))
SELECT
  CAST(({{ create_access_rule }}) AS INTEGER)
FROM
  (SELECT :__user_id AS id) AS _USER_
  {% if !column_names.is_empty() -%}
  , (SELECT
    {%- for name in column_names -%}
      {% if !loop.first %},{% endif %} :{{ name }} AS "{{ name }}"
    {%- endfor -%}
  ) AS _REQ_
  {%- endif %}
