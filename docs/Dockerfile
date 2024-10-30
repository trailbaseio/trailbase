FROM nginx:mainline-alpine AS runner

COPY ./nginx.conf /etc/nginx/conf.d/default.conf
COPY ./dist /usr/share/nginx/html

EXPOSE 80

HEALTHCHECK CMD curl --fail http://localhost:80 || exit 1
