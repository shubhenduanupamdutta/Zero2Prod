#!/usr/bin/env bash
set -x # Print commands and their arguments as they are executed.
set -eo pipefail # Exit immediately if a command exits with a non-zero status, and prevent errors in a pipeline from being masked.

if ! [ -x "$(command -v sqlx)" ]; then
  echo >&2 "Error: sqlx is not installed."
  echo >&2 "Use:"
  echo >&2 "    cargo install --version='~0.8' sqlx-cli --no-default-features --features rustls,postgres"
  echo >&2 "to install it."
  exit 1
fi

# Check if custom parameter has been set, otherwise use default values
DB_PORT="${POSTGRES_PORT:=5432}"
SUPERUSER="${POSTGRES_USER:=postgres}"
SUPERUSER_PWD="${SUPERUSER_PASSWORD:=password}"
APP_USER="${APP_USER:=app}"
APP_USER_PWD="${APP_USER_PWD:=secret}"
APP_DB_NAME="${APP_DB_NAME:=newsletter}"

# Launch postgres using docker
CONTAINER_NAME="postgres_zero2prod"
docker run \
  --env POSTGRES_USER="$SUPERUSER" \
  --env POSTGRES_PASSWORD="$SUPERUSER_PWD" \
  --health-cmd="pg_isready -U $SUPERUSER || exit 1" \
  --health-interval=1s \
  --health-timeout=5s \
  --health-retries=5 \
  --publish "${DB_PORT}":5432 \
  --detach \
  --name "$CONTAINER_NAME" \
  postgres -N 1000

# Wait for postgres to be ready
until [ \
 "$(docker inspect -f "{{.State.Health.Status}}" ${CONTAINER_NAME})" == "healthy" \
]; do
  >&2 echo "Postgres is still unavailable - sleeping"
  sleep 1
done

>&2 echo "Postgres is up and running on port ${DB_PORT}!"

# Create the application user
CREATE_QUERY="CREATE USER $APP_USER WITH PASSWORD '$APP_USER_PWD';"
docker exec -it "$CONTAINER_NAME" psql --username "$SUPERUSER" --command "$CREATE_QUERY"

# Grant create db privileges to the application user
GRANT_QUERY="ALTER USER $APP_USER CREATEDB;"
docker exec -it "$CONTAINER_NAME" psql --username "$SUPERUSER" --command "$GRANT_QUERY"

DATABASE_URL=postgres://${APP_USER}:${APP_USER_PWD}@localhost:${DB_PORT}/${APP_DB_NAME}
export DATABASE_URL
sqlx database create
