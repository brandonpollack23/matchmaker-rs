use std::fmt::Display;
use std::io::Read;
use std::io::Write;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
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

pub fn serialize(message: &MatchmakeProtocolMessage) -> Result<Vec<u8>> {
  let mut buf = VecWriter::new(4);

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

pub async fn deserialize_async<AR: AsyncRead + Unpin>(
  source: &mut AR,
) -> Result<MatchmakeProtocolMessage> {
  let size = source
    .read_u32()
    .await
    .map_err(|e| Error::DeserializeError(format!("could not read size: {:?}", e)))?;

  let mut buffer = vec![0; size as usize];
  source
    .read_exact(&mut buffer)
    .await
    .map_err(|e| Error::DeserializeError(format!("could not message into buffer: {:?}", e)))?;
  let message: MatchmakeProtocolMessage = rmp_serde::from_slice(&buffer)
    .map_err(|e| Error::DeserializeError(format!("could not rmp_serde deserialize: {:?}", e)))?;
  Ok(message)
}

pub fn deserialize_sync<R: Read>(source: &mut R) -> Result<MatchmakeProtocolMessage> {
  let size: u32 = 0;
  source
    .read_exact(&mut size.to_le_bytes())
    .map_err(|e| Error::DeserializeError(format!("could not read size: {:?}", e)))?;

  let mut buffer = vec![0; size as usize];
  source
    .read_exact(&mut buffer)
    .map_err(|e| Error::DeserializeError(format!("could not message into buffer: {:?}", e)))?;
  println!("size: {} buffer: {:?}", size, buffer);
  let message: MatchmakeProtocolMessage = rmp_serde::from_slice(&buffer)
    .map_err(|e| Error::DeserializeError(format!("could not rmp_serde deserialize: {:?}", e)))?;
  Ok(message)
}

/// Wire Protocol messages.
#[derive(PartialEq, Eq, Serialize, Deserialize, Debug)]
pub enum MatchmakeProtocolMessage {
  JoinMatch(JoinMatchRequest),
}

/// Protocol message representing a request to join a match, contains all the users to join.
#[derive(PartialEq, Eq, Serialize, Deserialize, Debug)]
pub struct JoinMatchRequest {
  // TODO ENHANCEMENT change to a vec of users, mark users that are trying to queue together
  // with a flag, when trying to make a match, if there isn't room for all
  // players in the game, skip over this group and leave it at the head for the
  // next game.
  pub user: User,
}

#[derive(PartialEq, Eq, Serialize, Deserialize, Debug)]
pub struct User(Uuid);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameServerInfo {
  pub id: Uuid,
}

#[derive(Clone, Debug)]
struct VecWriter {
  position: usize,
  inner: Vec<u8>,
}

impl VecWriter {
  fn new(offset: usize) -> Self {
    VecWriter {
      position: offset,
      inner: Vec::new(),
    }
  }

  fn into_inner(self) -> Vec<u8> {
    self.inner
  }
}

impl Write for VecWriter {
  fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
    if self.position + buf.len() > self.inner.len() {
      self.inner.resize(self.position + buf.len(), 0);
    }

    let end = self.position + buf.len();
    self.inner[self.position..end].copy_from_slice(buf);
    self.position = end;
    Ok(buf.len())
  }

  fn flush(&mut self) -> std::io::Result<()> {
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use std::io::Cursor;

  use super::*;

  #[test]
  fn test_serialize_sync() {
    let uuid = Uuid::new_v4();
    let message = MatchmakeProtocolMessage::JoinMatch(JoinMatchRequest { user: User(uuid) });

    let buf = serialize(&message).unwrap();

    let mut cursor = Cursor::new(buf);
    let message_readback = deserialize_sync(&mut cursor).unwrap();
    assert_eq!(message_readback, message);
  }

  #[tokio::test]
  async fn test_serialize_async() {
    let uuid = Uuid::new_v4();
    let message = MatchmakeProtocolMessage::JoinMatch(JoinMatchRequest { user: User(uuid) });

    let buf = serialize(&message).unwrap();

    let mut cursor = Cursor::new(buf);
    let message_readback = deserialize_async(&mut cursor).await.unwrap();
    assert_eq!(message_readback, message);
  }
}
