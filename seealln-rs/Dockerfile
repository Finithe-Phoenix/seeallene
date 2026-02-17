# syntax=docker/dockerfile:1

FROM rust:latest AS build
WORKDIR /app

COPY Cargo.toml Cargo.lock* ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/seealln /usr/local/bin/seealln
EXPOSE 8765
ENV RUST_LOG=info
ENTRYPOINT ["/usr/local/bin/seealln"]
