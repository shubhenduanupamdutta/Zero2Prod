# Builder stage

FROM rust:1.93.1@sha256:80302520b7199f0504975bca59a914015e9fee088f759875dbbc238ca9509ee1 AS builder

WORKDIR /app

RUN apt update && apt install lld clang -y

COPY . .

ENV SQLX_OFFLINE=true

RUN cargo build --release

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