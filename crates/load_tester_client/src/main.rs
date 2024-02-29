use std::time::Duration;

use clap::Parser;
use color_eyre::{eyre::Context, Result};
use rand::Rng;
use rand_distr::Distribution;
use tokio::{io::AsyncWriteExt, sync::OnceCell};
use tracing::{debug, info, instrument};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};
use uuid::Uuid;

#[derive(clap::Parser, Debug)]
#[command(
  name = "matchmaker-rs-load-tester",
  version = "0.1.0",
  about = "Single node load tester for the matchmaker server."
)]
struct Cli {
  /// Number of simultaneous clients to simulate.
  #[arg(short, long, default_value = "30000")]
  number_of_clients: u32,
  #[arg(short, long, default_value = "127.0.0.1:1337")]
  server_address: String,
}

static ARGS: OnceCell<Cli> = OnceCell::const_new();

#[tokio::main]
async fn main() {
  color_eyre::install().unwrap();

  tracing_subscriber::registry()
    .with(
      tracing_subscriber::fmt::layer()
        .compact()
        .with_file(true)
        .with_line_number(true)
        .with_filter(tracing_subscriber::filter::LevelFilter::from_level(
          tracing::Level::DEBUG,
        )),
    )
    .init();

  ARGS.set(Cli::parse()).unwrap();
  eprintln!("Running with args: {:#?}", ARGS.get().unwrap());

  ensure_server_up(&ARGS.get().unwrap().server_address).unwrap();

  let mut rng = rand::thread_rng();
  let dist = rand_distr::SkewNormal::new(200f32, 200f32, 0f32).unwrap();
  let mut spawned = 0;
  while spawned < ARGS.get().unwrap().number_of_clients {
    let spawn_count = rng.gen_range(1..50);
    for _ in 0..spawn_count {
      tokio::spawn(simulate_client(&ARGS.get().unwrap().server_address));
    }
    spawned += spawn_count;

    let sleep_time = dist.sample(&mut rng).max(0f32);
    debug!(
      "Spawned {} reqeuests, sleeping for {}ms",
      spawn_count, sleep_time
    );
    tokio::time::sleep(Duration::from_millis(sleep_time as u64)).await;
  }

  // Hack to keep the program running.
  tokio::time::sleep(Duration::from_secs(60)).await;
}

fn ensure_server_up(server_address: &str) -> Result<()> {
  std::net::TcpStream::connect(server_address)?;
  Ok(())
}

#[instrument]
async fn simulate_client(server_address: &str) -> Result<()> {
  let mut stream = tokio::net::TcpStream::connect(server_address).await?;

  let message =
    wire_protocol::MatchmakeProtocolRequest::JoinMatch(wire_protocol::JoinMatchRequest {
      user: wire_protocol::User(Uuid::new_v4()),
    });
  let buffer = wire_protocol::serialize(&message)?;

  stream.write_all(&buffer).await?;

  let server_reply: wire_protocol::MatchmakeProtocolResponse =
    wire_protocol::deserialize_async(&mut stream).await?;

  info!("Server replied with: {:?}", server_reply);

  let buffer = wire_protocol::serialize(&wire_protocol::MatchmakeProtocolRequest::Disconnect)?;
  stream.write_all(&buffer).await?;

  let goodbye_reply: wire_protocol::MatchmakeProtocolResponse =
    wire_protocol::deserialize_async(&mut stream).await?;

  info!(
    "Server closed it's end of the connection: {:?}",
    goodbye_reply
  );

  stream.shutdown().await?;

  Ok(())
}
