use bytes::buf::Buf;

use crate::error::*;
use crate::util::*;

#[derive(Debug, PartialEq, Clone)]
pub enum Resp {
  SetReportingMode(SetReportingModeResponse),
  Query(QueryResponse),
  SetDeviceId(SetDeviceIdResponse),
  SetSleepWork(SetSleepWorkResponse),
  SetWorkingPeriod(SetWorkingPeriodResponse),
  GetFirmwareVersion(GetFirmwareVersionResponse)
}

impl Resp {
  /// Attempts to convert this Resp into the inner Response matching the return
  /// type. Returns `Error::InvalidResponseConversion` if conversion is
  /// impossible.
  pub fn try_into_response<T: Response>(self) -> Result<T> {
    T::unpack_resp(self)
  }
}

pub(crate) trait ResponseParser {
  fn parse(buf: &[u8]) -> Resp;
}

pub trait Response : Sized {
  /// Attempts to unpack the Response from the given Resp, returning
  /// `Error::InvalidResponseConversion` if doing so is impossible (i.e.
  /// incorrect type).
  fn unpack_resp(resp: Resp) -> Result<Self>;
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct SetReportingModeResponse {
  pub query: bool,
  pub mode: ReportingMode,
  pub device: u16
}

impl ResponseParser for SetReportingModeResponse {
  fn parse(mut buf: &[u8]) -> Resp {
    buf.advance(3);
    let query = buf.get_u8() == 0x00;
    let mode = ReportingMode::from_byte(buf.get_u8());
    buf.advance(1);
    let device = buf.get_u16();

    Resp::SetReportingMode(SetReportingModeResponse {
      query,
      mode,
      device,
    })
  }
}

impl Response for SetReportingModeResponse {
  fn unpack_resp(resp: Resp) -> Result<Self> {
    match resp {
      Resp::SetReportingMode(r) => Ok(r),
      resp => Err(Error::InvalidResponseConversion {
        resp,
        target: "SetReportingModeResponse".into()
      })
    }
  }
}

#[derive(Debug, PartialEq, Clone)]
pub struct QueryResponse {
  // PM2.5 reading in micrograms per cubic meter
  pub pm25: f32,

  // PM10 reading in micrograms per cubic meter
  pub pm10: f32,

  // 2-byte device ID
  pub device: u16
}

impl ResponseParser for QueryResponse {
  fn parse(mut buf: &[u8]) -> Resp {
    buf.advance(2);

    Resp::Query(QueryResponse {
      pm25: buf.get_u16_le() as f32 / 10f32,
      pm10: buf.get_u16_le() as f32 / 10f32,
      device: buf.get_u16(),
    })
  }
}

impl Response for QueryResponse {
  fn unpack_resp(resp: Resp) -> Result<Self> {
    match resp {
      Resp::Query(r) => Ok(r),
      resp => Err(Error::InvalidResponseConversion {
        resp,
        target: "QueryResponse".into()
      })
    }
  }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct SetDeviceIdResponse {
  // 2-byte device ID
  device: u16
}

impl ResponseParser for SetDeviceIdResponse {
  fn parse(mut buf: &[u8]) -> Resp {
    buf.advance(6); // bytes 3-5 are reserved

    Resp::SetDeviceId(SetDeviceIdResponse {
      device: buf.get_u16()
    })
  }
}

impl Response for SetDeviceIdResponse {
  fn unpack_resp(resp: Resp) -> Result<Self> {
    match resp {
      Resp::SetDeviceId(r) => Ok(r),
      resp => Err(Error::InvalidResponseConversion {
        resp,
        target: "SetDeviceIdResponse".into()
      })
    }
  }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct SetSleepWorkResponse {
  pub query: bool,
  pub mode: WorkMode,
  pub device: u16
}

impl ResponseParser for SetSleepWorkResponse {
  fn parse(mut buf: &[u8]) -> Resp {
    buf.advance(3);
    let query = buf.get_u8() == 0x00;
    let mode = WorkMode::from_byte(buf.get_u8());
    buf.advance(1);
    let device = buf.get_u16();

    Resp::SetSleepWork(SetSleepWorkResponse {
      query,
      mode,
      device
    })
  }
}

impl Response for SetSleepWorkResponse {
  fn unpack_resp(resp: Resp) -> Result<Self> {
    match resp {
      Resp::SetSleepWork(r) => Ok(r),
      resp => Err(Error::InvalidResponseConversion {
        resp,
        target: "SetSleepWorkResponse".into()
      })
    }
  }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct SetWorkingPeriodResponse {
  /// if true, queries the current state; if false, sets the working period
  pub query: bool,
  pub working_period: WorkingPeriod,
  pub device: u16
}

impl ResponseParser for SetWorkingPeriodResponse {
  fn parse(mut buf: &[u8]) -> Resp {
    buf.advance(3);
    let query = buf.get_u8() == 0x00;
    let working_period = WorkingPeriod::from_byte(buf.get_u8());
    buf.advance(1);
    let device = buf.get_u16();

    Resp::SetWorkingPeriod(SetWorkingPeriodResponse {
      query,
      working_period,
      device
    })
  }
}

impl Response for SetWorkingPeriodResponse {
  fn unpack_resp(resp: Resp) -> Result<Self> {
    match resp {
      Resp::SetWorkingPeriod(r) => Ok(r),
      resp => Err(Error::InvalidResponseConversion {
        resp,
        target: "SetWorkingPeriodResponse".into()
      })
    }
  }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct GetFirmwareVersionResponse {
  /// year in some mystery format (presumably 2000 + year)
  pub year: u8,
  pub month: u8,
  pub day: u8,
  pub device: u16
}

impl ResponseParser for GetFirmwareVersionResponse {
  fn parse(mut buf: &[u8]) -> Resp {
    buf.advance(3);

    Resp::GetFirmwareVersion(GetFirmwareVersionResponse {
      year: buf.get_u8(),
      month: buf.get_u8(),
      day: buf.get_u8(),
      device: buf.get_u16()
    })
  }
}

impl Response for GetFirmwareVersionResponse {
  fn unpack_resp(resp: Resp) -> Result<Self> {
    match resp {
      Resp::GetFirmwareVersion(r) => Ok(r),
      resp => Err(Error::InvalidResponseConversion {
        resp,
        target: "GetFirmwareVersionResponse".into()
      })
    }
  }
}

