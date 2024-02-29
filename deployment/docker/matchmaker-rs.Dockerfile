# Build

FROM rust:1.76.0 AS builder

WORKDIR /usr/src/matchmaker-rs

COPY . .

WORKDIR /usr/src/matchmaker-rs/crates/server

RUN cargo install --path .

# Runtime

FROM rust:1.76.0 AS runtime

COPY --from=builder /usr/local/cargo/bin/matchmaker-rs /usr/local/cargo/bin/matchmaker-rs

RUN chmod +x /usr/local/cargo/bin/matchmaker-rs
ENTRYPOINT ["/usr/local/cargo/bin/matchmaker-rs"]