FROM rust:bookworm AS builder

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM debian:bookworm-slim

RUN useradd --system --uid 10001 --create-home velaedge
COPY --from=builder /build/target/release/protocol-device-sim /usr/local/bin/protocol-device-sim

USER 10001:10001
ENTRYPOINT ["protocol-device-sim"]
