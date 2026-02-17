# syntax=docker/dockerfile:1

FROM rust:latest AS build
WORKDIR /app

COPY Cargo.toml Cargo.lock* ./
COPY src ./src
# In Docker we disable the optional "capture" feature (scrap), because capture typically
# requires OS/window-system deps not available in minimal containers.
RUN cargo build --release --no-default-features

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/seealln /usr/local/bin/seealln
EXPOSE 8765
ENV RUST_LOG=info
ENTRYPOINT ["/usr/local/bin/seealln"]
