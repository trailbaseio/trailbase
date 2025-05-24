SELECT
{% for name in column_names -%}
  {%- if !loop.first %},{% endif %}MAIN."{{ name }}"
{%- endfor %}
{% for expanded in expanded_tables -%}
  , F{{ loop.index0 }}.*
{%- endfor %}
FROM {{ table_name.escaped_string() }} AS MAIN
{% for expanded in expanded_tables %}
  LEFT JOIN "{{ expanded.foreign_table_name }}" AS F{{ loop.index0 }} ON MAIN."{{ expanded.local_column_name }}" = F{{ loop.index0 }}."{{ expanded.foreign_column_name }}"
{% endfor %}
WHERE MAIN."{{ pk_column_name }}" = ?1
