FROM rust:1.93.1@sha256:80302520b7199f0504975bca59a914015e9fee088f759875dbbc238ca9509ee1

WORKDIR /app

RUN apt update && apt install lld clang -y

COPY . .

ENV SQLX_OFFLINE=true

RUN cargo build --release

ENV APP_ENVIRONMENT=production

ENTRYPOINT [ "./target/release/zero2prod" ]