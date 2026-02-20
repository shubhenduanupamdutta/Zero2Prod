# Builder stage

FROM lukemathwalker/cargo-chef:latest-rust-1.93.1@sha256:e69ab50e4065ec115ac625309045fa93ff3ea7037cf40e960d1ee78e6582ba71 AS chef
WORKDIR /app
RUN apt update && apt install lld clang -y

FROM chef AS planner
COPY . .
# Compute a lockfile for our project
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build our project not our dependencies
RUN cargo chef cook --release --recipe-path recipe.json
# Up to this point, we have built all our dependencies and cached them. Now we copy our source code and build our project.

COPY . .
ENV SQLX_OFFLINE=true

RUN cargo build --release --bin zero2prod

# Runtime stage

FROM debian:bookworm-slim@sha256:98f4b71de414932439ac6ac690d7060df1f27161073c5036a7553723881bffbe AS runtime

WORKDIR /app

# Install some dependencies that are dynamically linked by some of our dependencies.
# Install ca-certificates to make sure our app can make TLS connections to the outside world.
RUN apt-get update -y \
    && apt-get install -y --no-install-recommends openssl ca-certificates \
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/zero2prod zero2prod
COPY configuration/ configuration/

ENV APP_ENVIRONMENT=production

ENTRYPOINT [ "./zero2prod" ]
