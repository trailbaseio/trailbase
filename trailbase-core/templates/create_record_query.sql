INSERT <%= conflict_clause %> INTO "<%= table_name %>"
<% if !column_names.is_empty() { %>
(
<% for (i, name) in column_names.iter().enumerate() { %>
  <% if i > 0 { %>,<% } %> <%= name%>
<% } %>
) VALUES (
<% for (i, name) in column_names.iter().enumerate() { %>
  <% if i > 0 { %>,<% } %> :<%= name %>
<% } %>
)
<% } else { %> DEFAULT VALUES <% } %>
<% if let Some(r) = returning { %>
<% if r == "*" { %>
RETURNING *
<% } else { %>
RETURNING "<%= r %>"
<% } %>
<% } %>
