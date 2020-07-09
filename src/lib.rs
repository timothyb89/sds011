use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use std::ops::Deref;

use bytes::{Bytes, BytesMut, BufMut, Buf};
use err_derive::Error;
use serde_json::{self, json};
use tokio::sync::mpsc::{channel, Sender, Receiver};
use tokio::prelude::*;
use tokio_serial::*;
use tokio_util::codec::{Encoder, Decoder, FramedRead, FramedWrite};
use futures::{Sink, SinkExt};

// tokio-serial example:
// https://github.com/berkowski/tokio-serial/blob/master/examples/serial_println.rs

// sds011-rs:
// https://github.com/chrisballinger/sds011-rs/tree/master/src

// rust-nova-sds011:
// https://github.com/woofwoofinc/rust-nova-sds011/blob/master/src/main.rs

#[derive(Debug, Error)]
pub enum Error {
  #[error(display = "error opening serial port: {:?}", _0)]
  SerialPortError(#[error(source, no_from)] io::Error),

  #[error(display = "codec error: {:?}", _0)]
  CodecError(#[source] io::Error)
}

type Result<T> = std::result::Result<T, Error>;


#[derive(Debug)]
struct SetReportingModeResponse;

#[derive(Debug)]
struct QueryResponse;

#[derive(Debug)]
struct SetDeviceIdResponse;

#[derive(Debug)]
struct SetSleepWorkResponse;

#[derive(Debug)]
struct SetWorkingPeriodResponse;

#[derive(Debug)]
struct GetFirmwareVersionResponse;


#[derive(Debug)]
enum Response {
  SetReportingMode(SetReportingModeResponse),
  Query(QueryResponse),
  SetDeviceId(SetDeviceIdResponse),
  SetSleepWork(SetSleepWorkResponse),
  SetWorkingPeriod(SetWorkingPeriodResponse),
  GetFirmwareVersion(GetFirmwareVersionResponse)
}

struct Sds011;

impl Decoder for Sds011 {
  type Item = Response;
  type Error = Error;

  fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>> {
    todo!();

    // Ok(None) -> partially correct but not finished (or at least, not
    // blatantly invalid)

    Ok(None)
  }
}

fn checksum(bytes: &[u8]) -> u8 {
  let sum: u16 = bytes.iter().map(|b| *b as u16).sum();

  // per docs: checksum = lower 8 bits of sum
  sum.to_le_bytes()[0]
}

trait Command {
  fn id(&self) -> u8 {
    // B4 is used for all the commands and differentiated between via data byte
    // 1, because ??? (maybe they wanted it to be included in the checksum?)
    0xB4
  }

  fn data(&self, bytes: &mut BytesMut);

  fn to_cmd(self) -> Cmd;

  fn write(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0xAA);

    let mut data_bytes = BytesMut::new();
    self.data(&mut data_bytes);
    let sum = checksum(&data_bytes[..]);

    bytes.put(data_bytes);
    bytes.put_u8(sum);
    bytes.put_u8(0xAB);
  }
}

#[derive(Debug)]
pub struct SetReportingMode {
  pub query: bool,
  pub active: bool
}

impl Command for SetReportingMode {
  fn data(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0x02);

    bytes.put_u8(if self.query { 0x00 } else { 0x01 });
    bytes.put_u8(if self.active { 0x00 } else { 0x01 });

    // bytes 4-13 are reserved
    //bytes.put(&b"\0\0\0\0\0\0\0\0\0\0"[..]);
    bytes.put(&[0x00; 10][..]);

    // bytes 14, 15 are FF (or sensor id, but why?)
    bytes.put(&[0xFF; 2][..]);
  }

  fn to_cmd(self) -> Cmd {
    Cmd::SetReportingMode(self)
  }
}

#[derive(Debug)]
pub struct Query;

impl Command for Query {
  fn data(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0x04);
    bytes.put(&[0x00; 12][..]);
    bytes.put(&[0xFF; 2][..]);
  }

  fn to_cmd(self) -> Cmd {
    Cmd::Query(self)
  }
}

#[derive(Debug)]
pub struct SetDeviceId {
  id: u16
}

impl Command for SetDeviceId {
  fn data(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0x05);
    bytes.put(&[0x00; 10][..]);
    bytes.put_u16(self.id);
    bytes.put(&[0xFF; 2][..]);
  }

  fn to_cmd(self) -> Cmd {
    Cmd::SetDeviceId(self)
  }
}

#[derive(Debug)]
pub enum WorkMode {
  Sleep,
  Work
}

impl WorkMode {
  fn from_byte(byte: u8) -> Self {
    match byte {
      0x00 => WorkMode::Sleep,
      _ => WorkMode::Work
    }
  }

  fn as_byte(&self) -> u8 {
    match self {
      WorkMode::Sleep => 0x00,
      WorkMode::Work => 0x01
    }
  }
}

#[derive(Debug)]
pub struct SetSleepWork {
  query: bool,
  mode: WorkMode
}

impl Command for SetSleepWork {
  fn data(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0x06);
    bytes.put_u8(if self.query { 0x00 } else { 0x01 });
    bytes.put_u8(self.mode.as_byte());
    bytes.put(&[0x00; 10][..]);
    bytes.put(&[0xFF; 2][..]);
    todo!();
  }

  fn to_cmd(self) -> Cmd {
    Cmd::SetSleepWork(self)
  }
}

#[derive(Debug)]
pub struct SetWorkingPeriod {

}

impl Command for SetWorkingPeriod {
  fn data(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0x08);
    todo!();
  }

  fn to_cmd(self) -> Cmd {
    Cmd::SetWorkingPeriod(self)
  }
}

#[derive(Debug)]
pub struct GetFirmwareVersion {

}

impl Command for GetFirmwareVersion {
  fn data(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0x07);
    todo!();
  }

  fn to_cmd(self) -> Cmd {
    Cmd::GetFirmwareVersion(self)
  }
}

#[derive(Debug)]
enum Cmd {
  SetReportingMode(SetReportingMode),
  Query(Query),
  SetDeviceId(SetDeviceId),
  SetSleepWork(SetSleepWork),
  SetWorkingPeriod(SetWorkingPeriod),
  GetFirmwareVersion(GetFirmwareVersion)
}

impl Deref for Cmd {
  type Target = dyn Command;

  fn deref(&self) -> &Self::Target {
    match self {
      Cmd::SetReportingMode(c) => c,
      Cmd::Query(c) => c,
      Cmd::SetDeviceId(c) => c,
      Cmd::SetSleepWork(c) => c,
      Cmd::SetWorkingPeriod(c) => c,
      Cmd::GetFirmwareVersion(c) => c,
    }
  }

}

impl<C: Command> From<C> for Cmd {
  fn from(c: C) -> Self {
    c.to_cmd()
  }
}

impl Encoder<Cmd> for Sds011 {
  type Error = Error;

  fn encode(&mut self, item: Cmd, dst: &mut BytesMut) -> Result<()> {
    item.write(dst);

    Ok(())
  }
}

struct Reading {

}

async fn sensor_read(s: Sender<Reading>, device: PathBuf) -> Result<()> {
  let settings = SerialPortSettings {
    baud_rate: 9600,
    data_bits: DataBits::Eight,
    flow_control: FlowControl::None,
    parity: Parity::None,
    stop_bits: StopBits::One,
    timeout: Duration::from_millis(10000),
  };

  let port = Serial::from_path(&device, &settings).map_err(|e| Error::SerialPortError(e))?;

  let mut writer = FramedWrite::new(port, Sds011);
  writer.send(Query.into()).await.unwrap();


  tokio::task::spawn(async {
    //port.
  });

  Ok(())
}