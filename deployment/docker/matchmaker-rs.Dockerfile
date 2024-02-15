# Build

FROM rust:1.76.0 as builder

WORKDIR /usr/src/matchmaker-rs

COPY . .

WORKDIR /usr/src/matchmaker-rs/crates/server

RUN cargo install --path .

# Runtime

FROM rust:1.76.0 as runtime

COPY --from=builder /usr/local/cargo/bin/matchmaker-rs /usr/local/cargo/bin/matchmaker-rs

ENTRYPOINT ["/usr/local/cargo/bin/matchmaker-rs"]