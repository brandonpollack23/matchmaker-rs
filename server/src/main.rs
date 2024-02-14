use clap::Parser;
use color_eyre::Result;
use protocol::{JoinMatchRequest, MatchmakeProtocolMessage};
use tokio::{
  io::AsyncReadExt,
  net::{TcpListener, TcpStream},
  sync::mpsc::UnboundedSender,
};
use tracing::{error, Instrument};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

mod matchmaker;
mod protocol;

// TODO MAIN GOALS: profile with a perf based tool
// TODO MAIN GOALS: trace with tracy, OTel/Jaeger, Chrome/Perfetto.
// TODO MAIN GOALS: redis distributed feature version and docker compose to stand up.

#[derive(clap::Parser)]
struct Cli {
  #[arg(short = 'l', long, default_value = "info")]
  print_log_level: tracing::Level,
  #[arg(short, long, default_value = "1337")]
  port: u16,
  #[arg(short, long, default_value = "60")]
  match_size: u32,
}

#[tokio::main]
async fn main() {
  // Setup pretty error handling.
  color_eyre::install().unwrap();

  // Parse command line arguments.
  let args = Cli::parse();
  eprintln!("print log level: {}", args.print_log_level.as_str());

  // Setup tracing.
  tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer().with_filter(
      tracing_subscriber::filter::LevelFilter::from_level(args.print_log_level),
    ))
    .init();

  // Run the server.
  run_server(&args).await.unwrap();
}

async fn run_server(args: &Cli) -> Result<()> {
  let listener = TcpListener::bind(format!("127.0.0.1:{}", args.port))
    .await
    .unwrap();

  // TODO EXTRA: parallelize this with a tokio::select! macro and a concurrent vector and benchmark.
  let (join_match_tx, join_match_rx) = tokio::sync::mpsc::unbounded_channel::<JoinMatchRequest>();

  let matchmaker = tokio::spawn(matchmaker::matchmaker(join_match_rx, args.match_size));
  let listener = tokio::spawn(listen_handler(listener, join_match_tx));

  tokio::select! {
    r = listener => {
      r??;
    }
    r = matchmaker => {
      r??;
    }
  };

  Ok(())
}

async fn listen_handler(
  listener: TcpListener,
  join_match_tx: UnboundedSender<JoinMatchRequest>,
) -> Result<()> {
  loop {
    let (socket, addr) = listener.accept().await.unwrap();
    let span = tracing::info_span!("client_connection", %addr);

    let tx = join_match_tx.clone();
    if let Err(e) = tokio::spawn(async move {
      if let Err(e) = handle_client_connection(socket, &tx).await {
        error!("client connection error: {:?}", e);
      }
    })
    .instrument(span)
    .await
    {
      error!("failed to spawn task: {:?}", e);
    }
  }
}

/// Handle client connection.
/// The protocol is byte based.
/// 4 bytes - payload length N -- this gives us a max length of 4GB
/// N bytes - payload in the form of a MessagePack object TODO consider changing to typesafe language agnostic.
async fn handle_client_connection(
  socket: TcpStream,
  join_match_tx: &UnboundedSender<JoinMatchRequest>,
) -> Result<()> {
  let mut socket_buffered = tokio::io::BufReader::new(socket);

  loop {
    let size = socket_buffered.read_u32().await?;
    let mut buffer = Vec::with_capacity(size as usize);
    socket_buffered.read_exact(&mut buffer).await?;
    let message: MatchmakeProtocolMessage = rmp_serde::from_slice(&buffer)?;
    handle_message(message, join_match_tx).await?;
  }
}

async fn handle_message(
  message: MatchmakeProtocolMessage,
  join_match_tx: &UnboundedSender<JoinMatchRequest>,
) -> Result<()> {
  match message {
    MatchmakeProtocolMessage::JoinMatch(req) => {
      join_match_tx.send(req)?;
    }
  }

  Ok(())
}
