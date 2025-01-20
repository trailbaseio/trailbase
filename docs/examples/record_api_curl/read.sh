curl \
  --header "Content-Type: application/json" \
  --header "Authorization: Bearer ${AUTH_TOKEN}" \
  http://localhost:4000/api/records/v1/simple_strict_table/${RECORD_ID}
