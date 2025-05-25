INSERT {{ conflict_clause }} INTO {{ table_name }}
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
{%- for col in returning -%}
  {%- if loop.first %} RETURNING {% endif -%}
  {%- if !loop.first %},{% endif %}"{{ col }}"
{%- endfor -%}
