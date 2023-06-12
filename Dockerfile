FROM rust:1.70-bookworm as builder
WORKDIR /app/src/
RUN USER=root cargo new --bin reddark-remix
COPY Cargo.toml Cargo.lock ./reddark-remix/

WORKDIR /app/src/reddark-remix/
RUN cargo build --release

COPY ./src ./src
RUN cargo build --release

FROM debian:bookworm
WORKDIR /app
RUN apt-get update \
    && apt-get install -y openssl ca-certificates tini \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

EXPOSE 4000

COPY ./public ./public
COPY --from=builder /app/src/reddark-remix/target/release/reddark-remix ./

ENTRYPOINT ["/usr/bin/tini-static", "--", "/app/reddark-remix"]
