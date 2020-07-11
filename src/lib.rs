use std::io::{self, Read};
use std::ops::Deref;
use std::ffi::OsStr;
use std::sync::mpsc::{Sender, Receiver};
use std::thread;
use std::time::Duration;

#[macro_use] extern crate log;

use bytes::{BytesMut, BufMut, Buf};
use err_derive::Error;
use serialport::*;
use thread::JoinHandle;

// tokio-serial example:
// https://github.com/berkowski/tokio-serial/blob/master/examples/serial_println.rs

// sds011-rs:
// https://github.com/chrisballinger/sds011-rs/tree/master/src

// rust-nova-sds011:
// https://github.com/woofwoofinc/rust-nova-sds011/blob/master/src/main.rs

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
  WriteError(#[source] io::Error)
}

type Result<T> = std::result::Result<T, Error>;

fn checksum(bytes: &[u8]) -> u8 {
  let sum: u16 = bytes.iter().map(|b| *b as u16).sum();

  // per docs: checksum = lower 8 bits of sum
  sum.to_le_bytes()[0]
}

pub trait Command {
  fn id(&self) -> u8 {
    // B4 is used for all the commands and differentiated between via data byte
    // 1, because ??? (maybe they wanted it to be included in the checksum?)
    0xB4
  }

  fn data(&self, bytes: &mut BytesMut);

  fn to_cmd(self) -> Cmd;

  fn write(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0xAA);
    bytes.put_u8(self.id());

    let mut data_bytes = BytesMut::new();
    self.data(&mut data_bytes);
    let sum = checksum(&data_bytes[..]);

    bytes.put(data_bytes);
    bytes.put_u8(sum);
    bytes.put_u8(0xAB);
  }
}

/// A command that can be sent to the sensor
#[derive(Debug)]
pub enum Cmd {
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


#[derive(Debug)]
pub enum Response {
  SetReportingMode(SetReportingModeResponse),
  Query(QueryResponse),
  SetDeviceId(SetDeviceIdResponse),
  SetSleepWork(SetSleepWorkResponse),
  SetWorkingPeriod(SetWorkingPeriodResponse),
  GetFirmwareVersion(GetFirmwareVersionResponse)
}

trait ResponseParser {
  fn parse(buf: &[u8]) -> Response;
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
pub enum WorkingPeriod {
  /// device operates continuously, reporting a new result roughly every second
  Continuous,

  /// device sleeps for some number of minutes (sans 30 seconds), wakes for 30
  /// seconds to collect data, and returns to sleep
  Periodic(u8)
}

impl WorkingPeriod {
  fn from_byte(byte: u8) -> WorkingPeriod {
    match byte {
      0 => WorkingPeriod::Continuous,
      n => WorkingPeriod::Periodic(n)
    }
  }

  fn as_byte(&self) -> u8 {
    match self {
      WorkingPeriod::Continuous => 0,
      WorkingPeriod::Periodic(n) => *n
    }
  }
}

#[derive(Debug)]
pub struct SetReportingModeResponse {
  pub query: bool,
  pub active: bool,
  pub device: u16
}

impl ResponseParser for SetReportingModeResponse {
  fn parse(mut buf: &[u8]) -> Response {
    buf.advance(3);
    let query = buf.get_u8() == 0x00;
    let active = buf.get_u8() == 0x00;
    buf.advance(1);
    let device = buf.get_u16();

    Response::SetReportingMode(SetReportingModeResponse {
      query,
      active,
      device,
    })
  }
}

#[derive(Debug)]
pub struct QueryResponse {
  // PM2.5 reading in micrograms per cubic meter
  pub pm25: f32,

  // PM10 reading in micrograms per cubic meter
  pub pm10: f32,

  // 2-byte device ID
  pub device: u16
}

impl ResponseParser for QueryResponse {
  fn parse(mut buf: &[u8]) -> Response {
    buf.advance(2);

    Response::Query(QueryResponse {
      pm25: buf.get_u16_le() as f32 / 10f32,
      pm10: buf.get_u16_le() as f32 / 10f32,
      device: buf.get_u16(),
    })
  }
}

#[derive(Debug)]
pub struct SetDeviceIdResponse {
  // 2-byte device ID
  device: u16
}

impl ResponseParser for SetDeviceIdResponse {
  fn parse(mut buf: &[u8]) -> Response {
    buf.advance(6); // bytes 3-5 are reserved

    Response::SetDeviceId(SetDeviceIdResponse {
      device: buf.get_u16()
    })
  }
}

#[derive(Debug)]
pub struct SetSleepWorkResponse {
  pub query: bool,
  pub mode: WorkMode,
  pub device: u16
}

impl ResponseParser for SetSleepWorkResponse {
  fn parse(mut buf: &[u8]) -> Response {
    buf.advance(3);
    let query = buf.get_u8() == 0x00;
    let mode = WorkMode::from_byte(buf.get_u8());
    buf.advance(1);
    let device = buf.get_u16();

    Response::SetSleepWork(SetSleepWorkResponse {
      query,
      mode,
      device
    })
  }
}

#[derive(Debug)]
pub struct SetWorkingPeriodResponse {
  /// if true, queries the current state; if false, sets the working period
  pub query: bool,
  pub working_period: WorkingPeriod,
  pub device: u16
}

impl ResponseParser for SetWorkingPeriodResponse {
  fn parse(mut buf: &[u8]) -> Response {
    buf.advance(3);
    let query = buf.get_u8() == 0x00;
    let working_period = WorkingPeriod::from_byte(buf.get_u8());
    buf.advance(1);
    let device = buf.get_u16();

    Response::SetWorkingPeriod(SetWorkingPeriodResponse {
      query,
      working_period,
      device
    })
  }
}


#[derive(Debug)]
pub struct GetFirmwareVersionResponse {
  /// year in some mystery format
  pub year: u8,
  pub month: u8,
  pub day: u8,
  pub device: u16
}

impl ResponseParser for GetFirmwareVersionResponse {
  fn parse(mut buf: &[u8]) -> Response {
    buf.advance(3);

    Response::GetFirmwareVersion(GetFirmwareVersionResponse {
      year: buf.get_u8(),
      month: buf.get_u8(),
      day: buf.get_u8(),
      device: buf.get_u16()
    })
  }
}


#[derive(Debug)]
pub struct SetReportingMode {
  /// if true, queries the reporting mode; if false, sets it
  pub query: bool,

  /// if true, actively reports measurements; if false, sets the mode to query
  pub active: bool,
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
  pub id: u16
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
pub struct SetSleepWork {
  pub query: bool,
  pub mode: WorkMode
}

impl Command for SetSleepWork {
  fn data(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0x06);
    bytes.put_u8(if self.query { 0x00 } else { 0x01 });
    bytes.put_u8(self.mode.as_byte());
    bytes.put(&[0x00; 10][..]);
    bytes.put(&[0xFF; 2][..]);
  }

  fn to_cmd(self) -> Cmd {
    Cmd::SetSleepWork(self)
  }
}

#[derive(Debug)]
pub struct SetWorkingPeriod {
  pub query: bool,
  pub working_period: WorkingPeriod
}

impl Command for SetWorkingPeriod {
  fn data(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0x08);
    bytes.put_u8(if self.query { 0x00 } else {0x01 });
    bytes.put_u8(self.working_period.as_byte());
    bytes.put(&[0x00; 10][..]);
    bytes.put(&[0xFF; 2][..]);
  }

  fn to_cmd(self) -> Cmd {
    Cmd::SetWorkingPeriod(self)
  }
}

#[derive(Debug)]
pub struct GetFirmwareVersion;

impl Command for GetFirmwareVersion {
  fn data(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0x07);
    bytes.put(&[0x00; 12][..]);
    bytes.put(&[0xFF; 2][..]);
  }

  fn to_cmd(self) -> Cmd {
    Cmd::GetFirmwareVersion(self)
  }
}

fn parse_packet(packet: &[u8]) -> Result<Response> {
  // this parse implementation makes some protocol assumptions based on the docs
  // note: buf is &packet[1..9]; head and tail are stripped during read
  //  - all packets are 10 bytes long (8, excluding head/tail)
  //  - &packet[1] (&buf[0]) is command id
  //  - &packet[2..=7] (&buf[1..=6]) are data bytes, for checksum purposes
  //  - &packet[2..=5] (&buf[1..=4]) is actual data (&packet[3] is usually
  //    constant)
  //  - &packet[6..=7] is device id (u16)
  //  - &packet[8] (&buf[]) is checksum(&packet[2..=7]) (or checksum(&buf[1..=6]))

  if packet.len() != 10 {
    return Err(Error::PacketError(format!(
      "packet has invalid length: {:x?}", packet
    )));
  }

  let checksum_received = packet[8];
  let checksum_bytes = &packet[2..=7];
  let checksum_calculated = checksum(checksum_bytes);
  if checksum_calculated != checksum_received {
    return Err(Error::PacketError(format!(
      "packet ({:x?}) has invalid checksum: expected={:x?} received={:x?}",
      packet, checksum_calculated, checksum_received
    )));
  }

  debug!(
    "packet ({:x?}) checksum is valid: expected={:x?} received={:x?}",
    packet, checksum_calculated, checksum_received
  );

  let buf = packet.clone();
  let command = buf[1];
  let command_extra = buf[2];

  Ok(match (command, command_extra) {
    (0xC0, _) => QueryResponse::parse(buf),

    (0xC5, 0x02) => SetReportingModeResponse::parse(buf),
    (0xC5, 0x05) => SetDeviceIdResponse::parse(buf),
    (0xC5, 0x06) => SetSleepWorkResponse::parse(buf),
    (0xC5, 0x08) => SetWorkingPeriodResponse::parse(buf),
    (0xC5, 0x07) => GetFirmwareVersionResponse::parse(buf),

    (other, other_extra) => return Err(Error::PacketError(format!(
      "packet ({:x?}) has invalid command: {:x?}/{:x?}",
      buf, other, other_extra
    )))
  })
}

#[derive(Debug)]
pub enum ControlMessage {
  /// A non-fatal error, e.g. a single bad packet
  Error(Error),

  /// An error that halts either of the read or write threads
  FatalError(Error),
}

fn read_thread(port: Box<dyn SerialPort>, tx: Sender<Response>, control_tx: Sender<ControlMessage>) -> JoinHandle<()> {
  thread::spawn(move || {
    debug!("started read_thread");

    let mut current_packet: Option<BytesMut> = None;

    for byte in port.bytes() {
      let byte = match byte {
        Ok(byte) => byte,
        Err(e) => {
          control_tx.send(ControlMessage::FatalError(Error::ReadError(e))).ok();
          break;
        }
      };

      // packet format (10 bytes):
      // header:    1 byte (0xAA)
      // command:   1 byte
      // data:      4 bytes
      // device id: 2 bytes (counts as data for checksum purposes)
      // checksum:  1 byte
      // tail:      1 byte (0xAB)

      if let Some(packet) = current_packet.as_mut() {
        packet.put_u8(byte);

        if packet.len() == 10 {
          match parse_packet(packet) {
            Ok(response) => tx.send(response).ok(),
            Err(e) => control_tx.send(ControlMessage::Error(e)).ok()
          };

          current_packet = None;
        } else if packet.len() > 10 {
          control_tx.send(ControlMessage::Error(Error::PacketError(format!(
            "packet is too long, will discard: {:x?}", packet
          )))).ok();
        }
      } else if byte == 0xAA {
        let mut packet = BytesMut::with_capacity(10);
        packet.put_u8(byte);

        current_packet = Some(packet);
      } else {
        debug!("garbage byte: {:x?}", byte);
      }
    }
  })
}

fn write_thread(mut port: Box<dyn SerialPort>, rx: Receiver<Cmd>, control_tx: Sender<ControlMessage>) -> JoinHandle<()> {
  thread::spawn(move || {
    debug!("started write_thread");

    for cmd in rx {
      let mut buf = BytesMut::new();
      cmd.write(&mut buf);

      match port.write(&buf) {
        Ok(_) => debug!("sent command: {:?} = {:x?}", cmd, &buf[..]),
        Err(e) => {
          control_tx.send(ControlMessage::FatalError(Error::WriteError(e))).ok();
          break;
        }
      }
    }
  })
}

/// Opens a sensor at the given path
///
/// Requires three channels:
///  - a Receiver to which device commands can be sent via the connected Sender
///  - a Sender to which parsed device responses can be written (including
///    query results and automatic readings)
///  - a Sender to which informational messages can be written, e.g. errors, EoF
pub fn open_sensor<P: AsRef<OsStr>>(
  device: P,
  command_rx: Receiver<Cmd>,
  response_tx: Sender<Response>,
  control_tx: Sender<ControlMessage>
) -> Result<()> {
  let settings = SerialPortSettings {
    baud_rate: 9600,
    data_bits: DataBits::Eight,
    flow_control: FlowControl::None,
    parity: Parity::None,
    stop_bits: StopBits::One,
    timeout: Duration::from_millis(10000),
  };

  let read_port = open_with_settings(device.as_ref(), &settings)
    .map_err(|e| Error::SerialPortError(e))?;

  let write_port = read_port.try_clone()
    .map_err(|e| Error::SerialPortError(e))?;

  read_thread(read_port, response_tx, control_tx.clone());
  write_thread(write_port, command_rx, control_tx.clone());

  info!("opened sensor at {:?}", device.as_ref());

  Ok(())
}
