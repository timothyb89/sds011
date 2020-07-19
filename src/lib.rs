
use std::ffi::OsStr;
use std::sync::mpsc::{Sender, Receiver};
use std::thread;
use std::time::{Duration, Instant};
use std::io::Read;

#[macro_use] extern crate log;

use bytes::{BytesMut, BufMut};

use serialport::{
  open_with_settings,
  SerialPort, SerialPortSettings, DataBits, FlowControl, Parity, StopBits
};
use thread::JoinHandle;

mod error;
mod util;
mod command;
mod response;

pub use util::*;
pub use command::*;
pub use response::*;
pub use error::*;

fn parse_packet(packet: &[u8]) -> Result<Resp> {
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

  let buf = packet.to_owned();
  let command = buf[1];
  let command_extra = buf[2];

  Ok(match (command, command_extra) {
    (0xC0, _) => QueryResponse::parse(&buf),

    (0xC5, 0x02) => SetReportingModeResponse::parse(&buf),
    (0xC5, 0x05) => SetDeviceIdResponse::parse(&buf),
    (0xC5, 0x06) => SetSleepWorkResponse::parse(&buf),
    (0xC5, 0x08) => SetWorkingPeriodResponse::parse(&buf),
    (0xC5, 0x07) => GetFirmwareVersionResponse::parse(&buf),

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

fn read_thread(
  port: Box<dyn SerialPort>,
  tx: Sender<Resp>,
  control_tx: Sender<ControlMessage>,
) -> JoinHandle<()> {
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

        match packet.len() {
          10 => {
            match parse_packet(packet) {
              Ok(response) => tx.send(response).ok(),
              Err(e) => control_tx.send(ControlMessage::Error(e)).ok()
            };

            current_packet = None;
          },

          len if len > 10 => {
            control_tx.send(ControlMessage::Error(Error::PacketError(format!(
              "packet is too long, will discard: {:x?}", packet
            )))).ok();
          },

          _ => ()
        };
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

fn write_thread(
  mut port: Box<dyn SerialPort>,
  rx: Receiver<Cmd>,
  control_tx: Sender<ControlMessage>,
) -> JoinHandle<()> {
  thread::spawn(move || {
    debug!("started write_thread");

    for cmd in rx {
      match port.write_all(&cmd.data) {
        Ok(_) => debug!("sent command: {:x?}", cmd),
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
  response_tx: Sender<Resp>,
  control_tx: Sender<ControlMessage>
) -> Result<()> {
  // implementation note: writing commands to the sensor is unreliable
  // I tried a number of different implementations to reduce the issue, e.g.:
  //   - mutex while receiving a packet to prevent crosstalk from the write
  //     thread
  //   - merging the read and write threads to ensure the two operations were
  //     never running concurrently
  // ultimately I've kept this implementation since it feels cleaner and none of
  // the above helped anyway
  // probably related to active reporting

  let settings = SerialPortSettings {
    baud_rate: 9600,
    data_bits: DataBits::Eight,
    flow_control: FlowControl::None,
    parity: Parity::None,
    stop_bits: StopBits::One,

    // timeout longer than the worst-case working period
    timeout: Duration::from_secs(60 * 31)
  };

  let read_port = open_with_settings(device.as_ref(), &settings)
    .map_err(Error::SerialPortError)?;

  let write_port = read_port.try_clone()
    .map_err(Error::SerialPortError)?;

  read_thread(read_port, response_tx, control_tx.clone());
  write_thread(write_port, command_rx, control_tx);

  info!("opened sensor at {:?}", device.as_ref());

  Ok(())
}

pub struct RetryConfig {
  /// The maximum number of attempts before giving up
  pub retries: usize,

  /// The time to wait between each check for responses.
  pub sleep: Duration,

  /// The maximum time to wait before retrying (i.e. resending the command).
  pub timeout: Duration,
}

impl Default for RetryConfig {
  fn default() -> Self {
    RetryConfig {
      retries: 5,
      timeout: Duration::from_millis(500),
      sleep: Duration::from_millis(100),
    }
  }
}

/// Sends the given command and waits for a response, retrying up to 5 times if
/// necessary.
///
/// Returns the first matching response for the input command, as well as a list
/// of all other responses received.
pub fn retry_send<T: Response>(
  command: impl Command<ResponseType = T>,
  command_tx: &Sender<Cmd>,
  response_rx: &Receiver<Resp>,
  config: &RetryConfig
) -> Result<(T, Vec<Resp>)> {
  let mut other: Vec<Resp> = Vec::new();

  for i in 0..config.retries {
    let start = Instant::now();
    command_tx.send(command.to_cmd()).map_err(Error::ChannelSendError)?;

    while start.elapsed() < config.timeout {
      for resp in response_rx.try_iter() {
        match resp.clone().try_into_response::<T>() {
          Ok(r) => return Ok((r, other)),
          Err(Error::InvalidResponseConversion { .. }) => {
            other.push(resp);
            continue;
          },
          Err(e) => return Err(e)
        };
      }

      thread::sleep(config.sleep);
    }

    if i == 4 {
      debug!("giving up waiting for response to {:?}", command);
    } else {
      debug!("retrying command {:?}, attempt #{}", command, i + 1);
    }
  }

  Err(Error::RetriesExceeded { command: format!("{:?}", command) })
}

/// Sends the given command and waits for a response, using default retry
/// options. If no valid response to the given command is received in the
/// configured period, returns an error.
///
/// Returns the first matching response for the input command, as well as a list
/// of all other responses received.
pub fn retry_send_default<T: Response>(
  command: impl Command<ResponseType = T>,
  command_tx: &Sender<Cmd>,
  response_rx: &Receiver<Resp>,
) -> Result<(T, Vec<Resp>)> {
  retry_send(command, command_tx, response_rx, &RetryConfig::default())
}
