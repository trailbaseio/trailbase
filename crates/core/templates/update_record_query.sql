UPDATE {{ table_name }} SET
{%- for name in column_names -%}
  {%- if !loop.first %},{% endif %} "{{ name }}" = :{{ name }}
{%- endfor %}
WHERE "{{ pk_column_name }}" = :{{ pk_column_name }}
{%- match returning -%}
  {%- when Some with ("*") %} RETURNING *
  {%- when Some with (value) %} RETURNING "{{ value }}"
  {%- when None -%}
{%- endmatch -%}
