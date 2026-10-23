SELECT
  CAST(({{ read_access_rule }}) AS INTEGER)
FROM
  (SELECT :__user_id AS id, :__tenant_id AS tenant) AS _USER_
  {% if !column_names.is_empty() -%}
  , (SELECT
    {%- for name in column_names -%}
      {% if !loop.first %},{% endif %} {{ crate::records::util::named_placeholder(name) }} AS "{{ name }}"
    {%- endfor -%}
  ) AS _ROW_
  {%- endif %}
