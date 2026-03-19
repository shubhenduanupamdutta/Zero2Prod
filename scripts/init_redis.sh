#!/usr/bin/env bash
set -x # Print commands and their arguments as they are executed.
set -eo pipefail # Exit immediately if a command exits with a non-zero status, and prevent errors in a pipeline from being masked.

# If a redis container is running, print instructions to kill it and exit
RUNNING_CONTAINER=$(docker ps --filter 'name=redis' --format '{{.ID}}')

if [ -n "$RUNNING_CONTAINER" ]; then
  echo >&2 "Error: A Redis container is already running."
  echo >&2 "To stop it, run:"
  echo >&2 "    docker stop $RUNNING_CONTAINER"
  exit 1
fi

# Launch Redis using Docker
docker run \
    --publish 6379:6379 \
    --detach \
    --name "redis_$(date '+%s')" \
    redis:7.0-alpine
>&2 echo "Redis is ready to go!"
