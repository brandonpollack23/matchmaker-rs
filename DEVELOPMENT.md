# Development Environment Setup

## Prerequisites

Install cargo/rust and docker, and the redis cli

## Setup

run: 

```bash
./dev_setup.sh
```

When you're done you can tear it all down with

```bash
./dev_setup.sh down
```
## Tracing

There are various tracing_* features to enable, give them a try.

OpenTelemetry (for metrics and logs/spans) is on by default.

The flame one specifically needs to be processed by inferno, which is a separate tool.

```bash
cargo install inferno
```

Then you can generate a flamegraph with

```bash
# flamegraph
cat tracing.folded | inferno-flamegraph > tracing-flamegraph.svg

# flamechart
cat tracing.folded | inferno-flamegraph --flamechart > tracing-flamechart.svg
```

[reference](https://github.com/tokio-rs/tracing/tree/master/tracing-flame#generating-the-image)