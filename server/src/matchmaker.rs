use std::fmt;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::sync::Arc;
use std::time::Duration;

use color_eyre::Result;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use tracing::error;
use tracing::info;
use tracing::instrument;
use tracing::span;
use tracing::Level;

use crate::game_server_service::GameServer;
use crate::game_server_service::GameServerService;
use crate::protocol::JoinMatchRequest;

// TODO Feature: SBMM (Skill Based Match Making), depending on skill level sort
// into different buckets.  With backoff, expand range of adjacent bucket search
// of each skill bucket to try to find games if no games are found in the
// current bucket.

type MatchmakeResponder = oneshot::Sender<Option<Vec<JoinMatchRequestWithReply>>>;

pub async fn matchmaker(
  join_match_rx: mpsc::UnboundedReceiver<JoinMatchRequestWithReply>,
  game_server_service: Arc<dyn GameServerService>,
  match_size: u32,
) -> Result<()> {
  let (request_game_tx, request_game_rx) = tokio::sync::mpsc::channel::<MatchmakeResponder>(1);

  let user_aggregator = tokio::spawn(user_aggregator(join_match_rx, request_game_rx, match_size));
  let matchmaker = tokio::spawn(matchmaker_loop(request_game_tx, game_server_service));

  tokio::select! {
    r = user_aggregator => {
      r??;
    }
    r = matchmaker => {
      r??;
    }
  };

  Ok(())
}

#[instrument]
async fn user_aggregator(
  mut join_match_rx: mpsc::UnboundedReceiver<JoinMatchRequestWithReply>,
  mut request_game_rx: mpsc::Receiver<MatchmakeResponder>,
  match_size: u32,
) -> Result<()> {
  let mut matchmake_requests: Vec<JoinMatchRequestWithReply> = Vec::new();

  loop {
    tokio::select! {
      Some(join_match_request) = join_match_rx.recv() => {
        let _span = span!(
          Level::INFO,
          "matchmake request insertion",
          ?join_match_request.request
        )
        .entered();

        matchmake_requests.push(join_match_request)
      },

      Some(chan) = request_game_rx.recv() => {
        let _span = span!(Level::INFO, "matchmake request game").entered();

        if let Err(err) = if matchmake_requests.len() >= match_size as usize{
          let game: Vec<_> = matchmake_requests.drain(0..match_size as usize).collect();
          chan.send(Some(game))
        } else {
          chan.send(None)
        } {
          error!("failed to send game request: {:?}", err)
        };
      }
    }
  }
}

#[instrument]
async fn matchmaker_loop(
  request_game_tx: mpsc::Sender<MatchmakeResponder>,
  game_server_service: Arc<dyn GameServerService>,
) -> Result<()> {
  loop {
    if let Err(e) = matchmaker_iteration(request_game_tx.clone(), game_server_service.clone()).await
    {
      error!("failed to receive users from matchmaking service: {:?}", e);
    }

    tokio::time::sleep(Duration::from_secs(1)).await;
  }
}

#[instrument]
async fn matchmaker_iteration(
  request_game_tx: mpsc::Sender<MatchmakeResponder>,
  game_server_service: Arc<dyn GameServerService>,
) -> Result<()> {
  let (tx, rx) = oneshot::channel();

  if let Err(e) = request_game_tx.send(tx).await {
    error!("failed to send matchmaking request request: {:?}", e);
  }

  let users = rx.await?;

  if users.is_none() {
    info!("Not enough users to match yet, retrying...");
    return Ok(());
  }

  let game_server = game_server_service.acquire_game_server()?;
  for user in users.unwrap().into_iter() {
    user.tx.send(game_server.clone()).unwrap();
  }

  Ok(())
}

pub struct JoinMatchRequestWithReply {
  pub request: JoinMatchRequest,
  pub tx: oneshot::Sender<GameServer>,
}

impl Debug for JoinMatchRequestWithReply {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    f.debug_struct("JoinMatchRequestWithReply")
      .field("request", &self.request)
      .finish()
  }
}
