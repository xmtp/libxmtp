#!/bin/bash
set -e

MAX_LIFETIME_SECONDS="${MAX_LIFETIME_SECONDS:-3600}"

# Start Docker daemon using the dind entrypoint in the background
# The dind image's entrypoint handles cgroups, storage drivers, etc.
echo "Starting Docker daemon via dind entrypoint..."
/usr/local/bin/dockerd-entrypoint.sh dockerd &
DOCKERD_PID=$!

# Wait for Docker to be ready (check socket)
echo "Waiting for Docker daemon to be ready..."
SECONDS=0
until docker info > /dev/null 2>&1; do
    if [ $SECONDS -gt 60 ]; then
        echo "Docker daemon failed to start within 60 seconds"
        exit 1
    fi
    sleep 1
done
echo "Docker daemon ready (took ${SECONDS}s)"

# Pull and start services
echo "Starting services..."
docker-compose pull
docker-compose up -d

# Wait for node to be ready (5 minutes to allow for image pulls)
echo "Waiting for XMTP node on port 5556..."
SECONDS=0
while ! nc -z localhost 5556 2>/dev/null; do
    if [ $SECONDS -gt 300 ]; then
        echo "XMTP node failed to start within 300 seconds"
        echo "Docker container status:"
        docker-compose ps
        echo "Docker logs:"
        docker-compose logs --tail=50
        exit 1
    fi
    sleep 2
done
echo "XMTP node is ready! (took ${SECONDS}s)"

echo "All services started. Max lifetime: ${MAX_LIFETIME_SECONDS}s"

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
