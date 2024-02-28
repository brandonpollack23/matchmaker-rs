use async_trait::async_trait;

use color_eyre::{eyre::OptionExt, Result};
use std::{
  collections::VecDeque,
  fmt::{self, Debug, Formatter},
  net::SocketAddr,
};
use tokio::sync::Mutex as TokioMutex;
use tokio::{
  sync::{mpsc, oneshot},
  time::Instant,
};
use tracing::Instrument;
use tracing::{error, info, span, trace, Level};
use wire_protocol::{GameServerInfo, JoinMatchRequest};

#[async_trait]
pub trait MatchmakingQueueService: Sync + Send + Debug {
  async fn retrieve_user_batch(&self) -> Result<Vec<JoinMatchRequestWithReply>>;
}

#[derive(Debug)]
pub struct InProcessMatchmakingQueueService {
  matches_rx: TokioMutex<mpsc::UnboundedReceiver<Vec<JoinMatchRequestWithReply>>>,
  _user_aggregator: tokio::task::JoinHandle<Result<()>>,
}

impl InProcessMatchmakingQueueService {
  pub fn new(
    join_match_rx: mpsc::UnboundedReceiver<JoinMatchRequestWithReply>,
    cancel_request_rx: mpsc::UnboundedReceiver<SocketAddr>,
    match_size: u32,
  ) -> Result<Self> {
    let (matches_tx, matches_rx) = mpsc::unbounded_channel();

    let user_aggregator_span = span!(tracing::Level::INFO, "in_memory_matchmaking_queue_service");
    let user_aggregator = tokio::task::Builder::new()
      .name("in_memory_matchmaking_queue_service")
      .spawn(
        Self::user_aggregator(join_match_rx, cancel_request_rx, matches_tx, match_size)
          .instrument(user_aggregator_span),
      )?;

    Ok(Self {
      matches_rx: TokioMutex::new(matches_rx),
      _user_aggregator: user_aggregator,
    })
  }

  async fn user_aggregator(
    mut join_match_rx: mpsc::UnboundedReceiver<JoinMatchRequestWithReply>,
    mut cancel_request_rx: mpsc::UnboundedReceiver<SocketAddr>,
    matches_tx: mpsc::UnboundedSender<Vec<JoinMatchRequestWithReply>>,
    match_size: u32,
  ) -> Result<()> {
    let mut matchmake_requests: VecDeque<JoinMatchRequestWithReply> = VecDeque::new();
    let mut time_between_game_matches = Instant::now();

    loop {
      tokio::select! {
        Some(join_match_request) = join_match_rx.recv() => {
          let _span = span!(
            Level::INFO,
            "matchmake request insertion",
            ?join_match_request.request
          )
          .entered();

          matchmake_requests.push_back(join_match_request);

          if matchmake_requests.len() < match_size as usize {
            trace!("Not yet enough players for a match: {}", matchmake_requests.len());
            continue;
          }

          let _span = span!(Level::INFO, "matchmake request game").entered();

          let game: Vec<_> = matchmake_requests.drain(0..match_size as usize).collect();
          if matches_tx.send(game).is_err() {
            error!("Somehow the matches queue has been closed...")
          }

          info!(
            histogram.time_between_game_matches = time_between_game_matches.elapsed().as_millis() as u64,
            monotonic_counter.in_memory_games_ready = 1,
            "Game group created"
          );

          time_between_game_matches = tokio::time::Instant::now();
        },

        Some(socket_addr) = cancel_request_rx.recv() => {
          let _span = span!(Level::INFO, "matchmake request cancel").entered();

          matchmake_requests.retain(| r| if r.socket_addr == socket_addr {
            info!("cancelling matchmake request for: {:?}", r.request.user);
            false
          } else {
            true
          });
        }
      }
    }
  }
}

#[async_trait]
impl MatchmakingQueueService for InProcessMatchmakingQueueService {
  async fn retrieve_user_batch(&self) -> Result<Vec<JoinMatchRequestWithReply>> {
    self
      .matches_rx
      .lock()
      .await
      .recv()
      .await
      .ok_or_eyre("Matchmaking queue has shut down")
  }
}

pub struct JoinMatchRequestWithReply {
  pub socket_addr: SocketAddr,
  pub request: JoinMatchRequest,
  pub tx: oneshot::Sender<Option<GameServerInfo>>,
}

impl Debug for JoinMatchRequestWithReply {
  fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
    f.debug_struct("JoinMatchRequestWithReply")
      .field("request", &self.request)
      .finish()
  }
}
