INSERT INTO {{ table_name }}
{%- if req_column_names.is_empty() %} DEFAULT VALUES
{%- else %} (
  {%- for name in req_column_names -%}
    {%- if !loop.first %},{% endif %}"{{ name }}"
  {%- endfor -%}
  ) VALUES (
  {%- for name in req_column_names -%}
    {%- if !loop.first %},{% endif %}{{ crate::records::util::named_placeholder(name) }}
  {%- endfor -%}
)
{%- endif -%}

{%- if !column_metadata.is_empty() %}
  ON CONFLICT ({{ pk_column_name }}) DO UPDATE SET
  {%- for meta in column_metadata -%}
    {%- if !loop.first %},{% endif %}"{{ meta.column.name }}" = EXCLUDED."{{ meta.column.name }}"
  {%- endfor -%}
{%- endif -%}

{%- for col in returning -%}
  {%- if loop.first %} RETURNING {% endif -%}
  {%- if !loop.first %},{% endif %}"{{ col }}"
{%- endfor -%}
