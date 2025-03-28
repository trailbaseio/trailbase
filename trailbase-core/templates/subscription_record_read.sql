SELECT
  (<%= access_rule %>)
FROM
  (SELECT :__user_id AS id) AS _USER_,
  (SELECT
    <% for (i, name) in column_names.iter().enumerate() { %>
      <% if i > 0 { %>,<% } %> :<%= name %> AS <%= name%>
    <% } %>
  ) AS _ROW_
