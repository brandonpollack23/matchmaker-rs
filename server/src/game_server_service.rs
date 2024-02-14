use std::sync::Arc;

use clap::ValueEnum;
use color_eyre::Result;
use uuid::Uuid;

pub trait GameServerService: Sync + Send + std::fmt::Debug {
  fn acquire_game_server(&self) -> Result<GameServer>;
}

pub struct GameServer {
  pub id: Uuid,
}

#[derive(Debug, Clone)]
pub struct TestGameServerService;

impl GameServerService for TestGameServerService {
  fn acquire_game_server(&self) -> Result<GameServer> {
    Ok(GameServer { id: Uuid::new_v4() })
  }
}

#[derive(Debug, Clone, ValueEnum)]
pub enum GameServerServiceTypes {
  #[clap(name = "test")]
  TestGameServerService,
}

pub fn from_type(game_server_service_type: &GameServerServiceTypes) -> Arc<dyn GameServerService> {
  match game_server_service_type {
    GameServerServiceTypes::TestGameServerService => Arc::new(TestGameServerService),
  }
}
