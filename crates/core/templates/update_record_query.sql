UPDATE {{ table_name }} SET
{%- for param in params -%}
  {%- if !loop.first %},{% endif %} "{{ param.column_name }}" = {{ param.placeholder }}
{%- endfor %}
WHERE "{{ pk_column_name }}" = :__pk_value
{%- match returning -%}
  {%- when Some with ("*") %} RETURNING *
  {%- when Some with (value) %} RETURNING "{{ value }}"
  {%- when None -%}
{%- endmatch -%}
