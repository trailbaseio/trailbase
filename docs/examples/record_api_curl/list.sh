curl --globoff \
  --header "Content-Type: application/json" \
  --header "Authorization: Bearer ${AUTH_TOKEN}" \
  --request GET \
  'http://localhost:4000/api/records/v1/movies?limit=3&order=rank&filter[watch_time][$gte]=90&filter[watch_time][$lt]=120&filter[release_date][$gte]=2020-01-01&filter[release_date][$lte]=2023-12-31'
