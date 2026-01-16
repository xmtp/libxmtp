#!/bin/bash
set -e

MAX_LIFETIME_SECONDS="${MAX_LIFETIME_SECONDS:-3600}"

# Start Docker daemon using the dind entrypoint in the background
# The dind image's entrypoint handles cgroups, storage drivers, etc.
echo "Starting Docker daemon..."
/usr/local/bin/dockerd-entrypoint.sh dockerd > /dev/null 2>&1 &
DOCKERD_PID=$!

# Wait for Docker to be ready (check socket)
SECONDS=0
until docker info > /dev/null 2>&1; do
    if [ $SECONDS -gt 60 ]; then
        echo "ERROR: Docker daemon failed to start within 60 seconds"
        exit 1
    fi
    sleep 1
done
echo "Docker daemon ready (${SECONDS}s)"

# Pull images (quiet mode)
echo "Pulling images..."
docker-compose pull -q
echo "Images pulled"

# Start services
echo "Starting services..."
docker-compose up -d --quiet-pull 2>&1 | grep -v "Pulling" | grep -v "Pull complete" || true

# Wait for node to be ready (5 minutes to allow for image pulls)
echo "Waiting for XMTP node..."
SECONDS=0
while ! nc -z localhost 5556 2>/dev/null; do
    if [ $SECONDS -gt 300 ]; then
        echo "ERROR: XMTP node failed to start within 300 seconds"
        docker-compose ps
        docker-compose logs --tail=30
        exit 1
    fi
    sleep 2
done
echo "XMTP node ready (${SECONDS}s)"

echo "All services up. TTL: ${MAX_LIFETIME_SECONDS}s"

# Set up auto-shutdown timer
(
    sleep "$MAX_LIFETIME_SECONDS"
    echo "Max lifetime reached, shutting down..."
    docker-compose down
    kill $DOCKERD_PID 2>/dev/null || true
    kill $$ 2>/dev/null || true
) &

# Stream logs until shutdown
docker-compose logs -f
