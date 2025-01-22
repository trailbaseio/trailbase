curl --globoff \
  --header "Content-Type: application/json" \
  --header "Authorization: Bearer ${AUTH_TOKEN}" \
  --request GET \
  'http://localhost:4000/api/records/v1/movies?limit=3&order=rank&watch_time[lt]=120&description[like]=%love%'
