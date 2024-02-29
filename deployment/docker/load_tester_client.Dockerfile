# Build

FROM rust:1.76.0 as builder

WORKDIR /usr/src/matchmaker-rs

COPY . .

WORKDIR /usr/src/matchmaker-rs/crates/load_tester_client

RUN cargo install --path .

# Runtime

FROM rust:1.76.0 as runtime

COPY --from=builder /usr/local/cargo/bin/load_tester_client /usr/local/cargo/bin/load_tester_client

ENTRYPOINT ["/usr/local/cargo/bin/load_tester_client"]