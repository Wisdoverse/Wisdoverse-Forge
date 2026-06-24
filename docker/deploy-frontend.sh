#!/bin/bash
# =============================================================================
# Deploy Wisdoverse Forge Frontend
# =============================================================================
# Usage: ./deploy-frontend.sh [webroot-path]
#
# Extracts frontend files from the container and deploys to webroot
# with correct ownership (1000:1000 for nginx/web server)
# =============================================================================

set -e

WEBROOT="${1:-/opt/agentforge/www}"
WEBROOT_UID="${WEBROOT_UID:-1000}"
WEBROOT_GID="${WEBROOT_GID:-1000}"
CONTAINER_NAME="agentforge"

echo "Deploying frontend to: $WEBROOT"
echo "Target ownership: $WEBROOT_UID:$WEBROOT_GID"

# Check container exists
if ! docker ps -q -f name="$CONTAINER_NAME" | grep -q .; then
    echo "Error: Container '$CONTAINER_NAME' not running"
    echo "Start it first: docker compose up -d"
    exit 1
fi

# Copy files from container
echo "Copying files from container..."
docker cp "$CONTAINER_NAME":/app/dist/. "$WEBROOT/"

# Set ownership
echo "Setting ownership..."
chown -R "$WEBROOT_UID:$WEBROOT_GID" "$WEBROOT/"

echo "Done! Frontend deployed to $WEBROOT"
ls -la "$WEBROOT/"
