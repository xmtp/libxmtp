#!/bin/bash
set -e

MAX_LIFETIME_SECONDS="${MAX_LIFETIME_SECONDS:-3600}"

echo "Starting Docker daemon..."
dockerd &

# Wait for Docker to be ready
while ! docker info > /dev/null 2>&1; do
    sleep 1
done
echo "Docker daemon ready"

# Pull and start services
echo "Starting services..."
docker-compose pull
docker-compose up -d

# Wait for node to be ready
echo "Waiting for XMTP node..."
for i in {1..60}; do
    if nc -z localhost 5556 2>/dev/null; then
        echo "XMTP node is ready!"
        break
    fi
    sleep 2
done

echo "Services started. Max lifetime: ${MAX_LIFETIME_SECONDS}s"

# Set up auto-shutdown timer
(
    sleep "$MAX_LIFETIME_SECONDS"
    echo "Max lifetime reached, shutting down..."
    docker-compose down
    kill $$
) &

# Stream logs until shutdown
docker-compose logs -f
