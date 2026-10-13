INSERT INTO {{ table_name }}
{%- if params.is_empty() %} DEFAULT VALUES
{%- else %} (
  {%- for param in params -%}
    {%- if !loop.first %}, {% endif %}"{{ param.column_name }}"
  {%- endfor -%}
  ) VALUES (
  {%- for param in params -%}
    {%- if !loop.first %}, {% endif %}{{ param.placeholder }}
  {%- endfor -%}
)

{%- if !ignore_conflict && !column_metadata.is_empty() %}
  ON CONFLICT ("{{ pk_column_name }}") DO UPDATE SET
  {%- for meta in column_metadata -%}
    {%- if !loop.first %},{% endif %} "{{ meta.column.name }}" = EXCLUDED."{{ meta.column.name }}"
  {%- endfor -%}
{%- elif ignore_conflict %}
  ON CONFLICT ("{{ pk_column_name }}") DO NOTHING
{%- endif -%}
{%- endif -%}

{%- for col in returning -%}
  {%- if loop.first %} RETURNING {% endif -%}
  {%- if !loop.first %},{% endif %}"{{ col }}"
{%- endfor -%}
