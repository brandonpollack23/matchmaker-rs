use clap::Parser;
use color_eyre::Result;
use game_server_service::GameServerServiceTypes;
use matchmaker::UserAggregatorMode;
#[cfg(feature = "tracing_otel")]
use opentelemetry::KeyValue;
#[cfg(feature = "tracing_otel")]
use opentelemetry_otlp::WithExportConfig;
#[cfg(feature = "tracing_otel")]
use opentelemetry_sdk::{propagation::TraceContextPropagator, trace, Resource};

use tracing_subscriber::{layer::SubscriberExt, Layer};

#[cfg(feature = "tracing_pprof")]
use pprof::ProfilerGuard;
// enable on pprof or perfetto
#[cfg(feature = "tracing_pprof")]
use std::sync::OnceLock;

mod game_server_service;
mod matchmaker;
mod matchmaking_queue_service;
mod server;

// TODO MAIN GOALS: create a load test on GCP using pulumi/one shot jobs
// TODO MAIN GOALS: kafka/redpanda (redis streams are one key so will be
// overloaded) distributed feature version and docker compose to stand up.
// TODO MAIN GOALS: SBMM
// TODO MAIN GOALS: Generic SBMM that runs in web assembly runtime.

#[cfg(feature = "tracing_pprof")]
static PPROF_GUARD: OnceLock<ProfilerGuard<'static>> = OnceLock::new();

#[derive(clap::Parser, Debug)]
#[command(
  name = "matchmaker-rs",
  version = "0.1.0",
  about = "Matchmaker server experiment, see the readme i guess if it exists."
)]
struct Cli {
  #[arg(short = 'l', long, default_value = "warn")]
  print_log_level: tracing::Level,
  #[arg(short, long, default_value = "1337")]
  port: u16,
  #[arg(long, default_value = "test")]
  game_server_service: GameServerServiceTypes,
  /// Whether to use a local or redis based distributed user aggregator.
  #[arg(short = 'm', long, default_value = "local")]
  user_aggregator_mode: UserAggregatorMode,
  #[arg(short = 's', long, default_value = "60")]
  match_size: u32,
  /// Enable the tokio console (see <https://github.com/tokio-rs/console>)
  #[arg(short, long, default_value = "false")]
  tokio_console: bool,
  /// The otlp collector server (Probably Jaeger all in one...or a cloud something)
  #[arg(long, default_value = "http://localhost:4317")]
  otlp_endpoint: String,
  #[cfg(feature = "tracing_flame")]
  #[arg(short, long, default_value = "./tracing.folded")]
  tracing_flame_output_file: String,
}

#[tokio::main]
async fn main() {
  setup_pprof().await;

  // Setup pretty error handling.
  color_eyre::install().unwrap();

  // Parse command line arguments.
  let args = Cli::parse();
  eprintln!("Running with args: {:#?}", args);

  setup_tracing(&args).unwrap();

  server::run_server(&args).await.unwrap();

  #[cfg(feature = "tracing_otel")]
  opentelemetry::global::shutdown_tracer_provider();
}

#[cfg(feature = "tracing_pprof")]
async fn setup_pprof() {
  PPROF_GUARD
    .set(
      pprof::ProfilerGuardBuilder::default()
        .frequency(1000)
        .build()
        .unwrap(),
    )
    .map_err(|_| "Error creating pprof")
    .expect("Error creating pprof");

  tokio::task::Builder::new()
    .name("pprof reporter")
    .spawn(async {
      loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        let guard = PPROF_GUARD.get().unwrap();
        let report = guard.report().build().unwrap();
        let file = std::fs::File::create("flamegraph.svg").unwrap();
        report.flamegraph(file).unwrap();
      }
    })
    .unwrap();
}

#[cfg(not(feature = "tracing_pprof"))]
async fn setup_pprof() {}

fn setup_tracing(args: &Cli) -> Result<()> {
  // Setup propogator so that we can trace across services.
  #[cfg(feature = "tracing_otel")]
  opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

  let tracing_subscriber = tracing_subscriber::registry()
    .with(
      tracing_subscriber::fmt::layer()
        .compact()
        .with_file(true)
        .with_line_number(true)
        .with_filter(tracing_subscriber::filter::LevelFilter::from_level(
          args.print_log_level,
        )),
    )
    .with(if args.tokio_console {
      Some(console_subscriber::spawn())
    } else {
      None
    });

  #[cfg(feature = "tracing_otel")]
  let tracing_subscriber = {
    let otlp_exporter = opentelemetry_otlp::new_exporter()
      .tonic()
      .with_endpoint(&args.otlp_endpoint);
    let tracer = opentelemetry_otlp::new_pipeline()
      .tracing()
      .with_exporter(otlp_exporter)
      .with_trace_config(
        trace::config().with_resource(
          Resource::new(vec![
            KeyValue::new(
              opentelemetry_semantic_conventions::resource::SERVICE_NAMESPACE,
              "matchmaker-rs",
            ),
            KeyValue::new(
              opentelemetry_semantic_conventions::resource::SERVICE_NAME,
              "traces",
            ),
          ])
          .clone(),
        ),
      )
      .install_batch(opentelemetry_sdk::runtime::Tokio)?;
    let otel_tracer = tracing_opentelemetry::layer()
      .with_tracer(tracer)
      .with_filter(
        tracing_subscriber::filter::EnvFilter::try_from_default_env().unwrap_or(
          tracing_subscriber::filter::EnvFilter::new("info,[tokio::]=off"),
        ),
      );

    // The following links explains how to add metrics using tracing events.
    // Add monotonic_counter., counter., and histogram as a prefix to events and
    // the value is how much to change by.
    // https://docs.rs/tracing-opentelemetry/latest/tracing_opentelemetry/struct.MetricsLayer.html
    let otlp_exporter = opentelemetry_otlp::new_exporter()
      .tonic()
      .with_endpoint(&args.otlp_endpoint);
    let meter = opentelemetry_otlp::new_pipeline()
      .metrics(opentelemetry_sdk::runtime::Tokio)
      .with_exporter(otlp_exporter)
      .with_resource(Resource::new(vec![
        KeyValue::new(
          opentelemetry_semantic_conventions::resource::SERVICE_NAMESPACE,
          "matchmaker-rs",
        ),
        KeyValue::new(
          opentelemetry_semantic_conventions::resource::SERVICE_NAME,
          "metrics",
        ),
      ]))
      .build()?;
    let otel_metrics = tracing_opentelemetry::MetricsLayer::new(meter).with_filter(
      tracing_subscriber::filter::EnvFilter::try_from_default_env().unwrap_or(
        tracing_subscriber::filter::EnvFilter::new("info,[tokio::]=off"),
      ),
    );

    tracing_subscriber.with(otel_tracer).with(otel_metrics)
  };

  #[cfg(feature = "tracing_flame")]
  let tracing_subscriber = {
    let (flame_layer, guard) =
      tracing_flame::FlameLayer::with_file(&args.tracing_flame_output_file).unwrap();
    std::thread::Builder::new()
      .name("flame-trace-writer".to_string())
      .spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        guard.flush().unwrap();
      })
      .unwrap();

    tracing_subscriber.with(flame_layer)
  };

  #[cfg(feature = "tracing_tracy")]
  let tracing_subscriber = tracing_subscriber.with(tracing_tracy::TracyLayer::default());

  #[cfg(feature = "tracing_perfetto")]
  let tracing_subscriber = {
    let (chrome_layer, chrome_guard) = tracing_chrome::ChromeLayerBuilder::new().build();
    std::thread::Builder::new()
      .name("chrome-trace-writer".to_string())
      .spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        chrome_guard.flush();
      })
      .unwrap();
    tracing_subscriber.with(chrome_layer)
  };

  tracing::subscriber::set_global_default(tracing_subscriber).unwrap();

  Ok(())
}
