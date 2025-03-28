INSERT {{ conflict_clause }} INTO "{{ table_name }}"
{%- if column_names.is_empty() %} DEFAULT VALUES
{%- else %} (
  {%- for name in column_names -%}
    {%- if !loop.first %},{% endif %}"{{ name }}"
  {%- endfor -%}
  ) VALUES (
  {%- for name in column_names -%}
    {%- if !loop.first %},{% endif %}:{{ name }}
  {%- endfor -%}
)
{%- endif -%}
{%- match returning -%}
  {%- when Some with ("*") %} RETURNING *
  {%- when Some with (value) %} RETURNING "{{ value }}"
  {%- when None -%}
{%- endmatch -%}
