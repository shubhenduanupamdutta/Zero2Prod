#!/usr/bin/env bash
set -x # Print commands and their arguments as they are executed.
set -eo pipefail # Exit immediately if a command exits with a non-zero status, and prevent errors in a pipeline from being masked.

docker stop postgres_zero2prod
docker rm postgres_zero2prod