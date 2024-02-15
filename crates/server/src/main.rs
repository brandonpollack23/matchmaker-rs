use clap::Parser;
use color_eyre::Result;
use game_server_service::GameServerServiceTypes;
use tokio::{
  io::AsyncWriteExt,
  net::{TcpListener, TcpStream},
  sync::{
    mpsc::{self, UnboundedSender},
    oneshot,
  },
};
use tracing::{error, info, instrument, Instrument};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};
use wire_protocol::{GameServerInfo, MatchmakeProtocolRequest, MatchmakeProtocolResponse};

use crate::matchmaker::JoinMatchRequestWithReply;

mod game_server_service;
mod matchmaker;

// TODO MAIN GOALS: profile with a perf based tool
// TODO MAIN GOALS: trace with tracy, OTel/Jaeger, Chrome/Perfetto.
// TODO MAIN GOALS: redis distributed feature version and docker compose to
// stand up.

// TODO TESTS: server side

// TODO Docs: clap explanation string

#[derive(clap::Parser, Debug)]
#[command(
  name = "matchmaker-rs",
  version = "0.1.0",
  about = "Matchmaker server experiment, see the readme i guess if it exists."
)]
struct Cli {
  #[arg(short = 'l', long, default_value = "info")]
  print_log_level: tracing::Level,
  #[arg(short, long, default_value = "1337")]
  port: u16,
  #[arg(long, default_value = "test")]
  game_server_service: GameServerServiceTypes,
  #[arg(short, long, default_value = "60")]
  match_size: u32,
  /// Enable the tokio console (see <https://github.com/tokio-rs/console>)
  #[arg(short, long, default_value = "false")]
  tokio_console: bool,
}

#[tokio::main]
async fn main() {
  // Setup pretty error handling.
  color_eyre::install().unwrap();

  // Parse command line arguments.
  let args = Cli::parse();
  eprintln!("Running with args: {:#?}", args);

  setup_tracing(&args);

  run_server(&args).await.unwrap();
}

fn setup_tracing(args: &Cli) {
  tracing_subscriber::registry()
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
    })
    .init();
}

#[instrument]
async fn run_server(args: &Cli) -> Result<()> {
  let listener = TcpListener::bind(format!("127.0.0.1:{}", args.port))
    .await
    .unwrap();

  // TODO EXTRA: parallelize this with a tokio::select! macro and a concurrent
  // vector and benchmark.
  let (join_match_tx, join_match_rx) = mpsc::unbounded_channel::<JoinMatchRequestWithReply>();

  let game_server_service = game_server_service::from_type(&args.game_server_service);

  let listener = tokio::task::Builder::new()
    .name("server::listener")
    .spawn(listen_handler(listener, join_match_tx))?;

  let matchmaker = tokio::task::Builder::new()
    .name("matchmaker::supervisor")
    .spawn(matchmaker::matchmaker(
      join_match_rx,
      game_server_service,
      args.match_size,
    ))?;

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
  join_match_tx: UnboundedSender<JoinMatchRequestWithReply>,
) -> Result<()> {
  loop {
    let (socket, addr) = listener.accept().await.unwrap();
    let span = tracing::info_span!("client_connection", %addr);
    info!("accepted connection from: {:?}", addr);

    let tx = join_match_tx.clone();
    tokio::spawn(
      tokio::task::Builder::new()
        .name(&format!("socket::{addr:?}"))
        .spawn(async move {
          if let Err(e) = handle_client_connection(socket, &tx).await {
            error!("client connection error: {:?}", e);
          }

          info!("client connection closed: {:?}", addr);
        })?
        .instrument(span),
    );
  }
}

/// Handle client connection.
/// The protocol is byte based.
/// 4 bytes - payload length N -- this gives us a max length of 4GB
/// N bytes - payload in the form of a MessagePack object TODO consider changing
/// to typesafe language agnostic like protobufs or flatpaks or avro.
async fn handle_client_connection(
  mut socket: TcpStream,
  join_match_tx: &UnboundedSender<JoinMatchRequestWithReply>,
) -> Result<()> {
  loop {
    let message = wire_protocol::deserialize_async(&mut socket).await?;
    if let Continue::No = handle_message(message, &mut socket, join_match_tx).await? {
      let reply = wire_protocol::serialize(&MatchmakeProtocolResponse::Goodbye)?;
      socket.write_all(&reply).await?;
      socket.shutdown().await.unwrap();
      return Ok(());
    }
  }
}

async fn handle_message(
  message: MatchmakeProtocolRequest,
  socket: &mut TcpStream,
  join_match_tx: &UnboundedSender<JoinMatchRequestWithReply>,
) -> Result<Continue> {
  match message {
    MatchmakeProtocolRequest::JoinMatch(request) => {
      let (tx, rx) = oneshot::channel::<GameServerInfo>();
      let req_with_sock = JoinMatchRequestWithReply { request, tx };
      join_match_tx.send(req_with_sock)?;

      let server = rx.await?;
      let reply = wire_protocol::serialize(&MatchmakeProtocolResponse::GameServerInfo(server))?;
      socket.write_all(&reply).await?;
    }
    MatchmakeProtocolRequest::Disconnect => return Ok(Continue::No),
  }

  Ok(Continue::Yes)
}

enum Continue {
  Yes,
  No,
}
