{%- if count -%}
WITH
  total_count AS (
    SELECT COUNT(*) AS _value_
    FROM
      (SELECT :__user_id AS id) AS _USER_,
      {{ table_name }} as _ROW_
    WHERE
      ({{ read_access_clause }}) AND ({{ filter_clause }})
  )
{% endif -%}

SELECT
{% for name in column_names -%}
  {%- if !loop.first %},{% endif %}_ROW_."{{ name }}"
{%- endfor %}
{%- for expanded in expanded_tables -%}
  , F{{ loop.index0 }}.*
{%- endfor %}
{%- if count -%}, total_count._value_ AS _total_count_{%- endif %}
{%- if is_table -%}, _ROW_._rowid_ AS _rowid_{%- endif %}
FROM
  (SELECT :__user_id AS id) AS _USER_,
{%- if count %}
  total_count,
{%- endif %}
  {{ table_name }} AS _ROW_
{%- for expanded in expanded_tables %}
    LEFT JOIN "{{ expanded.foreign_table_name }}" AS F{{ loop.index0 }} ON _ROW_."{{ expanded.local_column_name }}" = F{{ loop.index0 }}."{{ expanded.foreign_column_name }}"
{%- endfor %}
WHERE
  ({{ read_access_clause }}) AND ({{ filter_clause }})
{%- if let Some(cursor_clause) = cursor_clause -%}
  AND ({{ cursor_clause }})
{%- endif %}
ORDER BY
  {{ order_clause }}
LIMIT :__limit
{%- if offset %}
OFFSET :__offset
{%- endif -%}
