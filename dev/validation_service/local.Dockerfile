FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y sqlite3
COPY .cache/mls-validation-service /usr/local/bin/mls-validation-service
ENV RUST_LOG=info
CMD ["mls-validation-service"]