UPDATE {{ table_name }} SET
{%- for name in column_names -%}
  {%- if !loop.first %},{% endif %} "{{ name }}" = {{ crate::records::util::named_placeholder(name) }}
{%- endfor %}
WHERE "{{ pk_column_name }}" = :__pk_value
{%- match returning -%}
  {%- when Some with ("*") %} RETURNING *
  {%- when Some with (value) %} RETURNING "{{ value }}"
  {%- when None -%}
{%- endmatch -%}
