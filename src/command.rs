use bytes::{BytesMut, BufMut};

use crate::response::*;
use crate::util::*;

pub trait Command : std::fmt::Debug {
  type ResponseType: Response;

  fn id(&self) -> u8 {
    // B4 is used for all the commands and differentiated between via data byte
    // 1, because ??? (maybe they wanted it to be included in the checksum?)
    0xB4
  }

  fn data(&self, bytes: &mut BytesMut);

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

  fn to_cmd(&self) -> Cmd {
    let mut data = BytesMut::new();
    self.write(&mut data);

    Cmd { data }
  }
}

#[derive(Debug)]
pub struct Cmd {
  pub(crate) data: BytesMut
}

impl<C: Command> From<C> for Cmd {
  fn from(c: C) -> Self {
    c.to_cmd()
  }
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct SetReportingMode {
  /// if true, queries the reporting mode; if false, sets it
  pub query: bool,

  /// if true, actively reports measurements; if false, sets the mode to query
  pub mode: ReportingMode,
}

impl Command for SetReportingMode {
  type ResponseType = SetReportingModeResponse;

  fn data(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0x02);

    bytes.put_u8(if self.query { 0x00 } else { 0x01 });
    bytes.put_u8(self.mode.as_byte());

    // bytes 4-13 are reserved
    //bytes.put(&b"\0\0\0\0\0\0\0\0\0\0"[..]);
    bytes.put(&[0x00; 10][..]);

    // bytes 14, 15 are FF (or sensor id, but why?)
    bytes.put(&[0xFF; 2][..]);
  }
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct Query;

impl Command for Query {
  type ResponseType = QueryResponse;

  fn data(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0x04);
    bytes.put(&[0x00; 12][..]);
    bytes.put(&[0xFF; 2][..]);
  }
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct SetDeviceId {
  pub id: u16
}

impl Command for SetDeviceId {
  type ResponseType = SetDeviceIdResponse;

  fn data(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0x05);
    bytes.put(&[0x00; 10][..]);
    bytes.put_u16(self.id);
    bytes.put(&[0xFF; 2][..]);
  }
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub struct SetSleepWork {
  pub query: bool,
  pub mode: WorkMode
}

impl Command for SetSleepWork {
  type ResponseType = SetSleepWorkResponse;

  fn data(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0x06);
    bytes.put_u8(if self.query { 0x00 } else { 0x01 });
    bytes.put_u8(self.mode.as_byte());
    bytes.put(&[0x00; 10][..]);
    bytes.put(&[0xFF; 2][..]);
  }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct SetWorkingPeriod {
  pub query: bool,
  pub working_period: WorkingPeriod
}

impl Command for SetWorkingPeriod {
  type ResponseType = SetWorkingPeriodResponse;

  fn data(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0x08);
    bytes.put_u8(if self.query { 0x00 } else {0x01 });
    bytes.put_u8(self.working_period.as_byte());
    bytes.put(&[0x00; 10][..]);
    bytes.put(&[0xFF; 2][..]);
  }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct GetFirmwareVersion;

impl Command for GetFirmwareVersion {
  type ResponseType = GetFirmwareVersionResponse;

  fn data(&self, bytes: &mut BytesMut) {
    bytes.put_u8(0x07);
    bytes.put(&[0x00; 12][..]);
    bytes.put(&[0xFF; 2][..]);
  }
}
