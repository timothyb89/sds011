#[macro_use] extern crate log;

use std::str::FromStr;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::time::Duration;
use std::thread;

use chrono::{Utc, SecondsFormat};
use sds011_exporter::command::*;
use sds011_exporter::response::*;
use sds011_exporter::util::*;
use sds011_exporter::{retry_send_default, ControlMessage};
use serde_json::json;
use structopt::StructOpt;
use anyhow::{anyhow, Error, Result};

#[derive(Debug, Clone, StructOpt)]
struct SetWorkModeAction {
  /// If set, queries the current state and does not set a value.
  #[structopt(long, short)]
  query: bool,

  /// The working mode, one of: work (on), sleep (off)
  mode: WorkMode
}

#[derive(Debug, Clone, StructOpt)]
struct SetReportingModeAction {
  /// If set, queries the current state and does not set a value.
  #[structopt(long, short)]
  query: bool,

  /// the reporting mode, one of: active, query
  mode: ReportingMode
}

#[derive(Debug, Clone, StructOpt)]
struct SetWorkingPeriodAction {
  /// If set, queries the current state and does not set a value.
  #[structopt(long, short)]
  query: bool,

  /// the working period in minutes; 0 for continuous
  ///
  /// 0: continuous, actively reports every second{n}
  /// 1-30: actively reports every `n` minutes after 30s of measurement
  working_period: WorkingPeriod
}

#[derive(Debug, Copy, Clone)]
enum OutputMode {
  None,
  JSON,
  CSV
}

impl FromStr for OutputMode {
  type Err = Error;
  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_ascii_lowercase().as_str() {
      "" | "none" => Ok(OutputMode::None),
      "json" => Ok(OutputMode::JSON),
      "csv" => Ok(OutputMode::CSV),
      s => Err(anyhow!("invalid output mode '{}', expected one of: none, json, csv", s))
    }
  }
}

#[derive(Debug, Clone, StructOpt)]
struct WatchAction {
  /// If set, writes incoming queries to stdout in the given format. Note that
  /// log messages are always written to stderr. JSON messages are one JSON
  /// object per line. One of: none, json, csv
  #[structopt(long, short, default_value = "none")]
  output_mode: OutputMode
}

#[derive(Debug, Clone, StructOpt)]
#[structopt(rename_all = "kebab-case")]
enum Action {
  /// Fetches sensor information
  Info,

  /// Displays sensor events
  Watch(WatchAction),

  /// Sets the sensor's working mode (work / sleep)
  SetWorkMode(SetWorkModeAction),

  /// Sets the device reporting mode (active / query)
  SetReportingMode(SetReportingModeAction),

  /// Sets the device working period
  ///
  /// 0: continuous (actively reports every ~1s, never sleeps){n}
  /// 1-30: reports every `n` minutes
  SetWorkingPeriod(SetWorkingPeriodAction),
}

#[derive(Debug, Clone, StructOpt)]
#[structopt(name = "sds011-tool")]
struct Options {
  /// sensor serial device, e.g. /dev/ttyUSB0
  #[structopt(parse(from_os_str))]
  device: PathBuf,

  #[structopt(subcommand)]
  action: Action
}

fn info(
  command_tx: Sender<Cmd>,
  response_rx: Receiver<Resp>,
  control_rx: Receiver<ControlMessage>
) -> Result<()> {
  let (firmware, _) = retry_send_default(
    GetFirmwareVersion,
    &command_tx,
    &response_rx
  )?;

  let (reporting, _) = retry_send_default(
    SetReportingMode {
      query: true,
      mode: ReportingMode::Active
    },
    &command_tx,
    &response_rx
  )?;

  let (working, _) = retry_send_default(
    SetWorkingPeriod {
      query: true,
      working_period: WorkingPeriod::Continuous
    },
    &command_tx,
    &response_rx
  )?;

  let (sleeping, _) = retry_send_default(
    SetSleepWork {
      query: true,
      mode: WorkMode::Work
    },
    &command_tx,
    &response_rx
  )?;

  println!("Device ID:        0x{:x?} ({})", firmware.device, firmware.device);
  println!("Working mode:     {:?}", sleeping.mode);
  println!("Reporting mode:   {:?}", reporting.mode);
  println!("Working period:   {:?}", working.working_period);
  println!("Firmware version: {:?}", firmware);

  for message in control_rx.try_iter() {
    warn!("{:?}", message);
  }

  Ok(())
}

fn format_query(query: &QueryResponse, mode: &OutputMode) -> Result<()> {
  let datetime = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);

  match mode {
    OutputMode::None => (),
    OutputMode::CSV => println!("{},{},{}", datetime, query.pm25, query.pm10),
    OutputMode::JSON => println!("{}", serde_json::to_string(&json!({
      "datetime": datetime,
      "pm25": query.pm25,
      "pm10": query.pm10
    }))?)
  }

  Ok(())
}

fn watch(
  _command_tx: Sender<Cmd>,
  response_rx: Receiver<Resp>,
  control_rx: Receiver<ControlMessage>,
  action: WatchAction
) -> Result<()> {
  if let OutputMode::CSV = &action.output_mode {
    println!("datetime,pm25,pm10");
  }

  loop {
    for response in response_rx.try_iter() {
      info!("{:x?}", response);

      if let Resp::Query(q) = &response {
        format_query(q, &action.output_mode)?;
      }
    }

    for control in control_rx.try_iter() {
      match control {
        ControlMessage::Error(e) => error!("Error: {:?}", e),
        ControlMessage::FatalError(e) => {
          error!("Fatal error: {:?}", e);
          std::process::exit(1);
        }
      }
    }

    thread::sleep(Duration::from_millis(100));
  }
}

fn set_work_mode(
  command_tx: Sender<Cmd>,
  response_rx: Receiver<Resp>,
  control_rx: Receiver<ControlMessage>,
  action: SetWorkModeAction
) -> Result<()> {
  match (action.query, action.mode) {
    (true, _) => info!("sending working mode query..."),
    (false, mode) => info!("attempting to set working mode: {:?}", mode)
  };

  let (response, _) = retry_send_default(SetSleepWork {
    query: action.query,
    mode: action.mode,
  }, &command_tx, &response_rx)?;

  for message in control_rx.try_iter() {
    warn!("{:?}", message);
  }

  info!("working mode is now: {:?}", response);

  Ok(())
}

fn set_reporting_mode(
  command_tx: Sender<Cmd>,
  response_rx: Receiver<Resp>,
  control_rx: Receiver<ControlMessage>,
  action: SetReportingModeAction
) -> Result<()> {
  match (action.query, action.mode) {
    (true, _) => info!("sending reporting mode query..."),
    (false, mode) => info!("attempting to set reporting mode: {:?}", mode)
  };

  let (response, _) = retry_send_default(SetReportingMode {
    query: action.query,
    mode: action.mode
  }, &command_tx, &response_rx)?;

  info!("reporting mode is now: {:?}", response);

  for message in control_rx.try_iter() {
    warn!("{:?}", message);
  }

  Ok(())
}

fn set_working_period(
  command_tx: Sender<Cmd>,
  response_rx: Receiver<Resp>,
  control_rx: Receiver<ControlMessage>,
  action: SetWorkingPeriodAction
) -> Result<()> {
  match (action.query, action.working_period) {
    (true, _) => info!("sent working period query..."),
    (false, period) => info!("attempting to set working period: {:?}", period)
  };

  let (response, _) = retry_send_default(SetWorkingPeriod {
    query: action.query,
    working_period: action.working_period
  }, &command_tx, &response_rx)?;

  info!("working period is now: {:?}", response);

  for message in control_rx.try_iter() {
    warn!("{:?}", message);
  }

  Ok(())
}

fn main() -> Result<()> {
  let env = env_logger::Env::default()
    .filter_or("SDS011_LOG", "info")
    .write_style_or("SDS011_STYLE", "always");

  env_logger::Builder::from_env(env)
    .target(env_logger::Target::Stderr)
    .init();

  let opts = Options::from_args();

  let (command_tx, command_rx) = channel();
  let (response_tx, response_rx) = channel();
  let (control_tx, control_rx) = channel();

  sds011_exporter::open_sensor(
    &opts.device,
    command_rx,
    response_tx,
    control_tx
  )?;

  match opts.action {
    Action::Info => info(command_tx, response_rx, control_rx),
    Action::Watch(action) => watch(command_tx, response_rx, control_rx, action),
    Action::SetWorkMode(action) => set_work_mode(command_tx, response_rx, control_rx, action),
    Action::SetReportingMode(action) => set_reporting_mode(command_tx, response_rx, control_rx, action),
    Action::SetWorkingPeriod(action) => set_working_period(command_tx, response_rx, control_rx, action)
  }
}
