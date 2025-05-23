SELECT
{% for name in column_names -%}
  {%- if !loop.first %},{% endif %}MAIN."{{ name }}"
{%- endfor %}
FROM {% if let Some(db) = database_schema %}"{{ db }}".{% endif %}"{{ table_name }}" as MAIN
WHERE MAIN."{{ pk_column_name }}" = ?1
