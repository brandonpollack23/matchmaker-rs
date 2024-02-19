use color_eyre::Result;
use std::net::SocketAddr;
use tokio::{
  io::AsyncWriteExt,
  net::{TcpListener, TcpStream},
  sync::{mpsc, mpsc::UnboundedSender, oneshot},
};
use tracing::{debug, error, info, instrument, Instrument};
use wire_protocol::{GameServerInfo, MatchmakeProtocolRequest, MatchmakeProtocolResponse};

use crate::{
  game_server_service, matchmaker,
  matchmaker::{JoinMatchRequestWithReply, UserAggregatorModeCli},
  Cli,
};

#[instrument]
pub(crate) async fn run_server(args: &Cli) -> Result<()> {
  let listener = TcpListener::bind(format!("127.0.0.1:{}", args.port))
    .await
    .unwrap();

  // TODO EXTRA: parallelize this with a tokio::select! macro and a concurrent
  // vector and benchmark.
  let (join_match_tx, join_match_rx) = mpsc::unbounded_channel::<JoinMatchRequestWithReply>();
  let (cancel_request_tx, cancel_request_rx) = tokio::sync::mpsc::unbounded_channel::<SocketAddr>();

  let game_server_service = game_server_service::from_type(&args.game_server_service);

  let listener = tokio::task::Builder::new()
    .name("server::listener")
    .spawn(listen_handler(listener, join_match_tx, cancel_request_tx))?;

  let user_aggregator_mode = match args.user_aggregator_mode {
    UserAggregatorModeCli::Local => matchmaker::UserAggregatorMode::Local,
  };
  let matchmaker = tokio::task::Builder::new()
    .name("matchmaker::supervisor")
    .spawn(matchmaker::matchmaker(
      join_match_rx,
      cancel_request_rx,
      game_server_service,
      user_aggregator_mode,
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

#[instrument]
pub(crate) async fn listen_handler(
  listener: TcpListener,
  join_match_tx: UnboundedSender<JoinMatchRequestWithReply>,
  cancel_request_tx: mpsc::UnboundedSender<SocketAddr>,
) -> Result<()> {
  loop {
    let (socket, addr) = listener.accept().await.unwrap();
    let span = tracing::info_span!("client_connection", %addr);

    let join_match_tx = join_match_tx.clone();
    let cancel_request_tx = cancel_request_tx.clone();
    tokio::task::Builder::new()
      .name(&format!("socket::{addr:?}"))
      // .instrument(span)
      .spawn(async move {
        if let Err(e) = handle_client_connection(socket, addr,  &join_match_tx, &cancel_request_tx).await {
          error!("client connection error: {:?}", e);
        }

        debug!("client connection closed: {:?}", addr);
      }.instrument(span))?;
  }
}

/// Handle client connection.
/// The protocol is byte based.
/// 4 bytes - payload length N -- this gives us a max length of 4GB
/// N bytes - payload in the form of a MessagePack object TODO consider changing
/// to typesafe language agnostic like protobufs or flatpaks or avro.
#[instrument]
pub(crate) async fn handle_client_connection(
  mut socket: TcpStream,
  socket_addr: SocketAddr,
  join_match_tx: &mpsc::UnboundedSender<JoinMatchRequestWithReply>,
  cancel_request_tx: &mpsc::UnboundedSender<SocketAddr>,
) -> Result<()> {
  info!(
    counter.active_connections = 1,
    monotonic_counter.total_connections = 1,
    "accepted connection from: {:?}",
    socket_addr
  );

  let client_start_time = std::time::Instant::now();
  loop {
    let message = wire_protocol::deserialize_async(&mut socket).await?;
    if let Continue::No = handle_message(
      message,
      &mut socket,
      socket_addr,
      join_match_tx,
      cancel_request_tx,
    )
    .await?
    {
      let reply = wire_protocol::serialize(&MatchmakeProtocolResponse::Goodbye)?;
      socket.write_all(&reply).await?;
      socket.shutdown().await.unwrap();

      info!(
        monotonic.active_connections = -1,
        histogram.client_connection_duration_ms = client_start_time.elapsed().as_millis() as u64,
        "closed connection from: {:?}",
        socket_addr
      );
      return Ok(());
    }
  }
}

#[instrument]
pub(crate) async fn handle_message(
  message: MatchmakeProtocolRequest,
  socket: &mut TcpStream,
  socket_addr: SocketAddr,
  join_match_tx: &mpsc::UnboundedSender<JoinMatchRequestWithReply>,
  cancel_request_tx: &mpsc::UnboundedSender<SocketAddr>,
) -> Result<Continue> {
  match message {
    MatchmakeProtocolRequest::JoinMatch(request) => {
      let (tx, rx) = oneshot::channel::<Option<GameServerInfo>>();
      let req_with_sock = JoinMatchRequestWithReply {
        socket_addr,
        request,
        tx,
      };
      join_match_tx.send(req_with_sock)?;

      let server = rx.await?;
      if server.is_none() {
        // Request was cancelled.
        info!("Request for {:?} was cancelled", socket_addr);
        return Ok(Continue::No);
      }

      let reply =
        wire_protocol::serialize(&MatchmakeProtocolResponse::GameServerInfo(server.unwrap()))?;
      socket.write_all(&reply).await?;
      return Ok(Continue::Yes);
    }
    MatchmakeProtocolRequest::Disconnect => {
      cancel_request_tx.send(socket_addr)?;
      return Ok(Continue::No);
    }
  }
}

pub(crate) enum Continue {
  Yes,
  No,
}
