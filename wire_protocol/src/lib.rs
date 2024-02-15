use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Wire Protocol messages.
#[derive(Serialize, Deserialize, Debug)]
pub enum MatchmakeProtocolMessage {
  JoinMatch(JoinMatchRequest),
}

/// Protocol message representing a request to join a match, contains all the users to join.
#[derive(Serialize, Deserialize, Debug)]
pub struct JoinMatchRequest {
  // TODO change to a vec of users, mark users that are trying to queue together
  // with a flag, when trying to make a match, if there isn't room for all
  // players in the game, skip over this group and leave it at the head for the
  // next game.
  pub user: User,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct User(Uuid);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameServerInfo {
  pub id: Uuid,
}
