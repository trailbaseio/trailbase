set -x

curl \
  --header "Content-Type: application/json" \
  --request POST \
  --data '{"email": "admin@localhost", "password": "secret"}' \
  http://localhost:4000/api/auth/v1/login
