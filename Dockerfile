FROM rust:1.70-bookworm as builder
RUN apt-get update \
    && apt-get install -y openssl ca-certificates tini libssl3 libssl3-dev build-essential \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

WORKDIR /app/src/reddark-remix/
COPY ./ ./
RUN cargo build --release

FROM debian:bookworm
WORKDIR /app
RUN apt-get update \
    && apt-get install -y openssl ca-certificates tini libssl3 \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

EXPOSE 4000

COPY ./public ./public
COPY ./templates ./templates
COPY --from=builder /app/src/reddark-remix/target/release/reddark-remix ./

ENTRYPOINT ["/usr/bin/tini-static", "--", "/app/reddark-remix"]
