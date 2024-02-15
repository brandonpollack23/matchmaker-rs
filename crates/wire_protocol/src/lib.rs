use std::{
  fmt::Display,
  io::{Cursor, Read, Write},
};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt};
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug)]
pub enum Error {
  DeserializeError(String),
  SerializeError(String),
}

impl Display for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Error::DeserializeError(e) => write!(f, "DeserializeError: {}", e),
      Error::SerializeError(e) => write!(f, "SerializeError: {}", e),
    }
  }
}

impl std::error::Error for Error {}

pub fn serialize<M: Serialize>(message: &M) -> Result<Vec<u8>> {
  let mut buf = Cursor::new(Vec::new());
  buf.set_position(4);

  // Split between two buffers, size and payload.
  let mut serializer = rmp_serde::Serializer::new(&mut buf);
  message
    .serialize(&mut serializer)
    .map_err(|e| Error::SerializeError(format!("failed to serialize with rmp_serde: {:?}", e)))?;

  let mut buf = buf.into_inner();
  let size = (buf.len() - 4) as u32;
  buf[0..4]
    .as_mut()
    .write_all(&size.to_be_bytes())
    .map_err(|e| Error::SerializeError(format!("failed to prepend size: {:?}", e)))?;

  Ok(buf)
}

pub async fn deserialize_async<M: for<'a> Deserialize<'a>, AR: AsyncRead + Unpin>(
  source: &mut AR,
) -> Result<M> {
  let mut size_buf = [0u8; 4];
  source
    .read_exact(&mut size_buf)
    .await
    .map_err(|e| Error::DeserializeError(format!("could not read size: {:?}", e)))?;
  let size = u32::from_be_bytes(size_buf);

  let mut buffer = vec![0; size as usize];
  source
    .read_exact(&mut buffer)
    .await
    .map_err(|e| Error::DeserializeError(format!("could not message into buffer: {:?}", e)))?;
  let message: M = rmp_serde::from_slice(&buffer)
    .map_err(|e| Error::DeserializeError(format!("could not rmp_serde deserialize: {:?}", e)))?;
  Ok(message)
}

pub fn deserialize_sync<M: for<'a> Deserialize<'a>, R: Read>(source: &mut R) -> Result<M> {
  let mut size_buf = [0u8; 4];
  source
    .read_exact(&mut size_buf)
    .map_err(|e| Error::DeserializeError(format!("could not read size: {:?}", e)))?;
  let size = u32::from_be_bytes(size_buf);

  let mut buffer = vec![0; size as usize];
  source
    .read_exact(&mut buffer)
    .map_err(|e| Error::DeserializeError(format!("could not message into buffer: {:?}", e)))?;
  let message: M = rmp_serde::from_slice(&buffer)
    .map_err(|e| Error::DeserializeError(format!("could not rmp_serde deserialize: {:?}", e)))?;
  Ok(message)
}

/// Wire Protocol messages.
#[derive(PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum MatchmakeProtocolRequest {
  JoinMatch(JoinMatchRequest),
  Disconnect,
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum MatchmakeProtocolResponse {
  GameServerInfo(GameServerInfo),
  Goodbye,
}

/// Protocol message representing a request to join a match, contains all the
/// users to join.
#[derive(PartialEq, Eq, Serialize, Deserialize, Debug)]
pub struct JoinMatchRequest {
  // TODO ENHANCEMENT change to a vec of users, mark users that are trying to queue together
  // with a flag, when trying to make a match, if there isn't room for all
  // players in the game, skip over this group and leave it at the head for the
  // next game.
  pub user: User,
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Debug)]
pub struct User(pub Uuid);

#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize)]
pub struct GameServerInfo {
  pub id: Uuid,
}

#[cfg(test)]
mod tests {
  use std::io::Cursor;

  use super::*;

  #[test]
  fn test_serialize_sync() {
    let uuid = Uuid::new_v4();
    let message = MatchmakeProtocolRequest::JoinMatch(JoinMatchRequest { user: User(uuid) });

    let buf = serialize(&message).unwrap();

    let mut cursor = Cursor::new(buf);
    let message_readback: MatchmakeProtocolRequest = deserialize_sync(&mut cursor).unwrap();
    assert_eq!(message_readback, message);
  }

  #[tokio::test]
  async fn test_serialize_async() {
    let uuid = Uuid::new_v4();
    let message = MatchmakeProtocolRequest::JoinMatch(JoinMatchRequest { user: User(uuid) });

    let buf = serialize(&message).unwrap();

    let mut cursor = Cursor::new(buf);
    let message_readback: MatchmakeProtocolRequest = deserialize_async(&mut cursor).await.unwrap();
    assert_eq!(message_readback, message);
  }
}
