FROM docker:24-dind

RUN apk add --no-cache bash curl netcat-openbsd

WORKDIR /app
COPY ci/fly/docker-compose.yml /app/
COPY ci/fly/entrypoint.sh /app/
RUN chmod +x /app/entrypoint.sh

EXPOSE 5556 5558

ENTRYPOINT ["/app/entrypoint.sh"]
