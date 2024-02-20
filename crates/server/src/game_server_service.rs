use std::fmt::Debug;
use std::sync::Arc;

use clap::ValueEnum;
use color_eyre::Result;
use uuid::Uuid;
use wire_protocol::GameServerInfo;

// TODO NOW make async
pub trait GameServerService: Sync + Send + Debug {
  fn acquire_game_server(&self) -> Result<GameServerInfo>;
}

#[derive(Debug, Clone)]
pub struct TestGameServerService;

impl GameServerService for TestGameServerService {
  fn acquire_game_server(&self) -> Result<GameServerInfo> {
    Ok(GameServerInfo { id: Uuid::new_v4() })
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
