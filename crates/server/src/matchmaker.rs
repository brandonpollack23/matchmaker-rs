use std::{fmt::Debug, sync::Arc, time::Duration};

use clap::ValueEnum;
use color_eyre::Result;
use tokio::sync::oneshot;

use tracing::{error, info, instrument, warn};

use crate::{
  game_server_service::GameServerService,
  matchmaking_queue_service::{JoinMatchRequestWithReply, MatchmakingQueueService},
};

// TODO Feature: SBMM (Skill Based Match Making), depending on skill level sort
// into different buckets.  With backoff, expand range of adjacent bucket search
// of each skill bucket to try to find games if no games are found in the
// current bucket.

type MatchmakeResponder = oneshot::Sender<Option<Vec<JoinMatchRequestWithReply>>>;

pub async fn matchmaker(
  game_server_service: Arc<dyn GameServerService>,
  matchmaking_queue_service: Arc<dyn MatchmakingQueueService>,
  _match_size: u32,
) -> Result<()> {
  let (_request_game_tx, _request_game_rx) = tokio::sync::mpsc::channel::<MatchmakeResponder>(1);

  let matchmaker = tokio::task::Builder::new()
    .name("matchmaker::matchmaking_loop")
    .spawn(matchmaker_loop(
      matchmaking_queue_service,
      game_server_service,
    ))?;

  tokio::select! {
    r = matchmaker => {
      r??;
    }
  };

  Ok(())
}

#[instrument]
async fn matchmaker_loop(
  matchmaking_queue_service: Arc<dyn MatchmakingQueueService>,
  game_server_service: Arc<dyn GameServerService>,
) -> Result<()> {
  loop {
    let users = tokio::time::timeout(
      Duration::from_secs(5),
      matchmaking_queue_service.retrieve_user_batch(),
    )
    .await;

    if users.is_err() {
      warn!(
        monotonic_counter.slow_matchmaking = 1,
        "Taking a long time to get users!"
      );
      // TODO: backoff
      continue;
    }

    let users = users.unwrap();
    if users.is_err() {
      error!("Could not get users for matchmaking instance! \n\t{users:?}");
      // TODO: backoff
      continue;
    }

    let users = users.unwrap();
    let game_server = game_server_service.acquire_game_server();
    if game_server.is_err() {
      error!("Could not retrieve game server! {game_server:?}");
      // TODO: backoff
      continue;
    }

    info!(
      monotonic_counter.games_created_total = 1,
      "Found enough users to match, creating game server..."
    );

    for user in users.into_iter() {
      user
        .tx
        .send(Some(game_server.as_ref().unwrap().clone()))
        .unwrap();
    }
  }
}

#[derive(Debug, Clone, ValueEnum)]
pub enum UserAggregatorMode {
  Local,
}

#[cfg(test)]
mod tests {
  use super::*;

  use std::net::SocketAddr;

  use tokio::sync::mpsc;
  use uuid::Uuid;
  use wire_protocol::{GameServerInfo, JoinMatchRequest, User};

  use crate::matchmaking_queue_service::InProcessMatchmakingQueueService;

  #[allow(clippy::type_complexity)]
  fn setup_matchmaker() -> (
    mpsc::UnboundedSender<JoinMatchRequestWithReply>,
    mpsc::UnboundedSender<SocketAddr>,
    Arc<dyn GameServerService>,
    Arc<dyn MatchmakingQueueService>,
  ) {
    let (join_match_tx, join_match_rx) = mpsc::unbounded_channel::<JoinMatchRequestWithReply>();
    let (cancel_request_tx, cancel_request_rx) =
      tokio::sync::mpsc::unbounded_channel::<SocketAddr>();

    let game_server_service =
      crate::game_server_service::from_type(&crate::GameServerServiceTypes::TestGameServerService);

    let matchmaking_queue_service =
      Arc::new(InProcessMatchmakingQueueService::new(join_match_rx, cancel_request_rx, 5).unwrap());

    (
      join_match_tx,
      cancel_request_tx,
      game_server_service,
      matchmaking_queue_service,
    )
  }

  #[tokio::test]
  async fn matchmaker_aggregates_up_to_match_size() {
    // Setup matchmaker
    let match_size = 5;
    let (join_match_tx, _cancel_request_tx, game_server_service, matchmaking_queue_service) =
      setup_matchmaker();
    let _matchmaker = tokio::spawn(matchmaker(
      game_server_service,
      matchmaking_queue_service,
      match_size,
    ));

    // Send join match requests and ensure the join match notification is sent only
    // once per game_size batch.
    for _ in 0..5 {
      let mut join_match_notifications = Vec::new();
      for i in 0..match_size {
        let (tx, rx) = oneshot::channel();
        join_match_notifications.push(rx);

        let join_match_req = JoinMatchRequestWithReply {
          socket_addr: SocketAddr::new([123, 123, 255, 255].into(), 1234),
          request: JoinMatchRequest {
            user: User(Uuid::new_v4()),
          },
          tx,
        };

        join_match_tx.send(join_match_req).unwrap();

        if i != match_size - 1 {
          // Not a full game yet, should not have a notification.
          assert!(join_match_notifications[i as usize].try_recv().is_err());
        }
      }

      // Full game, should have a notification.
      for rx in join_match_notifications {
        assert!(matches!(rx.await, Ok(Some(GameServerInfo { .. }))));
      }
    }
  }

  // TODO cancel test
}
