curl \
  --header "Content-Type: application/json" \
  --header "Authorization: Bearer ${AUTH_TOKEN}" \
  --request POST \
  --data '{"text_not_null": "test"}' \
  http://localhost:4000/api/records/v1/simple_strict_table
