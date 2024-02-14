use color_eyre::Result;
use tokio::sync::mpsc;
use tokio::sync::oneshot;

use tracing::error;
use tracing::instrument;

use tracing::span;

use tracing::Level;

use crate::protocol::JoinMatchRequest;
use crate::protocol::User;

// TODO Feature: SBMM (Skill Based Match Making), depending on skill level sort
// into different buckets.  With backoff, expand range of adjacent bucket search
// of each skill bucket to try to find games if no games are found in the
// current bucket.

type MatchmakeResponder = oneshot::Sender<Option<Vec<User>>>;

pub async fn matchmaker(
  join_match_rx: mpsc::UnboundedReceiver<JoinMatchRequest>,
  match_size: u32,
) -> Result<()> {
  let (request_game_tx, request_game_rx) = tokio::sync::mpsc::channel::<MatchmakeResponder>(1);

  let user_aggregator = tokio::spawn(user_aggregator(join_match_rx, request_game_rx, match_size));
  let matchmaker = tokio::spawn(matchmaker_loop(request_game_tx));

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
  mut join_match_rx: mpsc::UnboundedReceiver<JoinMatchRequest>,
  mut request_game_rx: mpsc::Receiver<MatchmakeResponder>,
  match_size: u32,
) -> Result<()> {
  let mut matchmake_requests: Vec<User> = Vec::new();

  loop {
    tokio::select! {
      Some(join_match_request) = join_match_rx.recv() => {
        let _span = span!(
          Level::INFO,
          "matchmake request insertion",
          ?join_match_request
        )
        .entered();

        matchmake_requests.push(join_match_request.user)
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
async fn matchmaker_loop(request_game_tx: mpsc::Sender<MatchmakeResponder>) -> Result<()> {
  let (tx, rx) = oneshot::channel();

  if let Err(e) = request_game_tx.send(tx).await {
    error!("failed to send matchmaking request request: {:?}", e);
  }

  let users = rx.await;
  if let Err(e) = users {
    error!("failed to receive users from matchmaking service: {:?}", e);
  }

  // TODO MAIN GOAL: Create a "game server" and send it to each user, this can simply be behind a polymorphic trait.
  Ok(())
}
