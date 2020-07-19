use std::io;

use err_derive::Error;

use crate::command::Cmd;
use crate::response::Resp;

#[derive(Debug, Error)]
#[error(no_from)]
pub enum Error {
  #[error(display = "error opening serial port: {:?}", _0)]
  SerialPortError(#[error(source)] serialport::Error),

  #[error(display = "error parsing packet: {}", _0)]
  PacketError(String),

  #[error(display = "error reading response: {}", _0)]
  ReadError(#[source] io::Error),

  #[error(display = "error sending command: {}", _0)]
  WriteError(#[source] io::Error),

  #[error(display = "invalid work mode: {}", _0)]
  InvalidWorkMode(String),

  #[error(display = "invalid reporting mode: {}", _0)]
  InvalidReportingMode(String),

  #[error(display = "invalid working period '{}': {}", period, reason)]
  InvalidWorkingPeriod {
    period: String,
    reason: String
  },

  #[error(display = "error sending to channel")]
  ChannelSendError(#[source] std::sync::mpsc::SendError<Cmd>),

  #[error(display = "never received response to command: {:?}", command)]
  RetriesExceeded {
    /// a debug-ified representation of the command being retried
    command: String
  },

  #[error(display = "response {:?} cannot be converted into {}", resp, target)]
  InvalidResponseConversion {
    resp: Resp,
    target: String
  }
}

pub type Result<T> = std::result::Result<T, Error>;
