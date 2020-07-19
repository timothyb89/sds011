use std::convert::TryFrom;
use std::str::FromStr;

use crate::error::*;

/// Computes a checksum for the given bytes.
///
/// Note that these must be data bytes and exclude the header, tail, etc.
pub fn checksum(bytes: &[u8]) -> u8 {
  let sum: u16 = bytes.iter().map(|b| *b as u16).sum();

  // per docs: checksum = lower 8 bits of sum
  sum.to_le_bytes()[0]
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum WorkMode {
  Sleep,
  Work
}

impl WorkMode {
  pub fn from_byte(byte: u8) -> Self {
    match byte {
      0x00 => WorkMode::Sleep,
      _ => WorkMode::Work
    }
  }

  pub fn as_byte(&self) -> u8 {
    match self {
      WorkMode::Sleep => 0x00,
      WorkMode::Work => 0x01
    }
  }
}

impl FromStr for WorkMode {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self> {
    Ok(match s.to_lowercase().as_str() {
      "work" | "on" => WorkMode::Work,
      "sleep" | "off" => WorkMode::Sleep,
      _ => return Err(Error::InvalidWorkMode(s.to_string()))
    })
  }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum WorkingPeriod {
  /// device operates continuously, reporting a new result roughly every second
  Continuous,

  /// device sleeps for some number of minutes (sans 30 seconds), wakes for 30
  /// seconds to collect data, and returns to sleep
  Periodic(u8)
}

impl WorkingPeriod {
  pub fn from_byte(byte: u8) -> WorkingPeriod {
    match byte {
      0 => WorkingPeriod::Continuous,
      n => WorkingPeriod::Periodic(n)
    }
  }

  pub fn as_byte(&self) -> u8 {
    match self {
      WorkingPeriod::Continuous => 0,
      WorkingPeriod::Periodic(n) => *n
    }
  }
}

impl TryFrom<usize> for WorkingPeriod {
  type Error = Error;

  fn try_from(value: usize) -> Result<Self> {
    match value {
      0 => Ok(WorkingPeriod::Continuous),
      1..=30 => Ok(WorkingPeriod::Periodic(value as u8)),
      _ => Err(Error::InvalidWorkingPeriod {
        period: value.to_string(),
        reason: "value out of range (0 <= n <= 30)".into()
      })
    }
  }
}

impl FromStr for WorkingPeriod {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self> {
    let value = s.parse::<usize>()
      .map_err(|e| Error::InvalidWorkingPeriod {
        period: s.into(),
        reason: format!("could not parse int: {}", e)
      })?;

    WorkingPeriod::try_from(value)
  }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum ReportingMode {
  /// Sensor reports measurements at a regular interval without being explicitly
  /// queried.
  ///
  /// The interval may be configured with the SetWorkingPeriod command.
  Active,

  /// Sensor only reports measurements when explicitly queried (via Query
  /// command)
  Query
}

impl ReportingMode {
  pub fn from_byte(byte: u8) -> Self {
    match byte {
      0x00 => ReportingMode::Active,
      _ => ReportingMode::Query
    }
  }

  pub fn as_byte(&self) -> u8 {
    match self {
      ReportingMode::Active => 0x00,
      ReportingMode::Query => 0x01
    }
  }
}

impl FromStr for ReportingMode {
  type Err = Error;

  fn from_str(s: &str) -> Result<Self> {
    Ok(match s.to_lowercase().as_str() {
      "active" => ReportingMode::Active,
      "query" => ReportingMode::Query,
      _ => return Err(Error::InvalidReportingMode(s.to_string()))
    })
  }
}

