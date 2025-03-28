SELECT
  ({{ create_access_rule }})
FROM
  (SELECT :__user_id AS id) AS _USER_,
  (SELECT
    {% for name in column_names %}
      {% if !loop.first %},{% endif %} :{{ name }} AS "{{ name }}"
    {% endfor %}
  ) AS _REQ_
