
#[macro_use] extern crate log;

use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::time::{Duration, Instant};
use std::thread;

use anyhow::Result;
use structopt::StructOpt;

use sds011_exporter::*;

#[derive(Debug, Clone, StructOpt)]
struct SetWorkModeAction {
  /// if set, retrieves the current mode and does not set anything
  #[structopt(long, short)]
  get: bool,

  /// The working mode, one of: work (on), sleep (off)
  mode: WorkMode
}

#[derive(Debug, Clone, StructOpt)]
struct SetReportingModeAction {
  /// if set, retrieves the current mode and does not set anything
  #[structopt(long, short)]
  get: bool,

  /// the reporting mode, one of: active, query
  mode: ReportingMode
}

#[derive(Debug, Clone, StructOpt)]
struct SetWorkingPeriodAction {
  #[structopt(long, short)]
  get: bool,

  /// the working period in minutes; 0 for continuous
  ///
  /// 0: continuous, actively reports every second{n}
  /// 1-30: actively reports every `n` minutes after 30s of measurement
  working_period: WorkingPeriod
}

#[derive(Debug, Clone, StructOpt)]
#[structopt(rename_all = "kebab-case")]
enum Action {
  /// Fetches sensor information
  Info,

  /// Displays sensor events
  Watch,

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
  response_rx: Receiver<Response>,
  control_rx: Receiver<ControlMessage>
) -> Result<()> {
  command_tx.send(GetFirmwareVersion.to_cmd())?;

  command_tx.send(SetReportingMode {
    query: true,
    mode: ReportingMode::Active
  }.to_cmd())?;

  command_tx.send(SetSleepWork {
    query: true,
    mode: WorkMode::Work
  }.to_cmd())?;

  command_tx.send(SetWorkingPeriod {
    query: true,
    working_period: WorkingPeriod::Continuous
  }.to_cmd())?;

  thread::sleep(Duration::from_millis(100));

  let start = Instant::now();
  let timeout = Duration::from_millis(2000);

  let mut firmware = None;
  let mut reporting = None;
  let mut working = None;
  let mut sleeping = None;

  loop {
    for response in response_rx.try_iter() {
      match response {
        Response::GetFirmwareVersion(v) => firmware = Some(v),
        Response::SetReportingMode(m) =>   reporting = Some(m),
        Response::SetWorkingPeriod(p) =>   working = Some(p),
        Response::SetSleepWork(s) =>       sleeping = Some(s),
        r => debug!("ignoring: {:?}", r)
      }
    }

    match (&firmware, &reporting, &working, &sleeping) {
      (Some(f), Some(r), Some(w), Some(s)) => {
        println!("Device ID:        0x{:x?} ({})", f.device, f.device);
        println!("Working mode:     {:?}", s.mode);
        println!("Reporting mode:   {:?}", r.mode);
        println!("Working period:   {:?}", w.working_period);
        println!("Firmware version: {:?}", f);
        break;
      },
      _ => ()
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

    if start.elapsed() > timeout {
      error!("did not receive status in time");
      std::process::exit(1);
    } else {
      thread::sleep(Duration::from_millis(100));
    }
  }

  Ok(())
}

fn watch(
  _command_tx: Sender<Cmd>,
  response_rx: Receiver<Response>,
  control_rx: Receiver<ControlMessage>
) -> Result<()> {
  loop {
    for response in response_rx.try_iter() {
      info!("{:x?}", response);
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
  response_rx: Receiver<Response>,
  control_rx: Receiver<ControlMessage>,
  action: SetWorkModeAction
) -> Result<()> {
  command_tx.send(SetSleepWork {
    query: action.get,
    mode: action.mode,
  }.to_cmd())?;

  match (action.get, action.mode) {
    (true, _) => info!("sent working mode query..."),
    (false, mode) => info!("attempting to set working mode: {:?}", mode)
  };

  let start = Instant::now();
  let timeout = Duration::from_millis(1000);

  'outer: loop {
    for response in response_rx.try_iter() {
      match response {
        Response::SetSleepWork(r) => {
          info!("received response: {:?}", r);
          break 'outer;
        },
        r => debug!("ignoring: {:x?}", r)
      }
    }

    for control in control_rx.try_iter() {
      match control {
        ControlMessage::Error(e) => error!("error: {:?}", e),
        ControlMessage::FatalError(e) => {
          error!("Fatal error: {:?}", e);
          std::process::exit(1);
        }
      }
    }

    if start.elapsed() > timeout {
      error!("did not receive status in time; try again");
      std::process::exit(1);
    } else {
      thread::sleep(Duration::from_millis(100));
    }
  }

  Ok(())
}

fn set_reporting_mode(
  command_tx: Sender<Cmd>,
  response_rx: Receiver<Response>,
  control_rx: Receiver<ControlMessage>,
  action: SetReportingModeAction
) -> Result<()> {
  command_tx.send(SetReportingMode {
    query: action.get,
    mode: action.mode
  }.to_cmd())?;

  match (action.get, action.mode) {
    (true, _) => info!("sent reporting mode query..."),
    (false, mode) => info!("attempting to set reporting mode: {:?}", mode)
  };

  let start = Instant::now();
  let timeout = Duration::from_millis(1000);

  'outer: loop {
    for response in response_rx.try_iter() {
      match response {
        Response::SetReportingMode(r) => {
          info!("received response: {:x?}", r);
          break 'outer;
        },
        r => debug!("ignoring: {:x?}", r)
      }
    }

    for control in control_rx.try_iter() {
      match control {
        ControlMessage::Error(e) => error!("error: {:?}", e),
        ControlMessage::FatalError(e) => {
          error!("Fatal error: {:?}", e);
          std::process::exit(1);
        }
      }
    }

    if start.elapsed() > timeout {
      error!("did not receive status in time; try again");
      std::process::exit(1);
    } else {
      thread::sleep(Duration::from_millis(100));
    }
  }

  Ok(())
}

fn set_working_period(
  command_tx: Sender<Cmd>,
  response_rx: Receiver<Response>,
  control_rx: Receiver<ControlMessage>,
  action: SetWorkingPeriodAction
) -> Result<()> {
  command_tx.send(SetWorkingPeriod {
    query: action.get,
    working_period: action.working_period
  }.to_cmd())?;

  match (action.get, action.working_period) {
    (true, _) => info!("sent working period query..."),
    (false, period) => info!("attempting to set working period: {:?}", period)
  };

  let start = Instant::now();
  let timeout = Duration::from_millis(1000);

  'outer: loop {
    for response in response_rx.try_iter() {
      match response {
        Response::SetWorkingPeriod(r) => {
          info!("received response: {:x?}", r);
          break 'outer;
        },
        r => debug!("ignoring: {:x?}", r)
      }
    }

    for control in control_rx.try_iter() {
      match control {
        ControlMessage::Error(e) => error!("error: {:?}", e),
        ControlMessage::FatalError(e) => {
          error!("Fatal error: {:?}", e);
          std::process::exit(1);
        }
      }
    }

    if start.elapsed() > timeout {
      error!("did not receive status in time; try again");
      std::process::exit(1);
    } else {
      thread::sleep(Duration::from_millis(100));
    }
  }

  Ok(())
}

fn main() -> Result<()> {
  env_logger::Builder::from_default_env()
    .filter_level(log::LevelFilter::Debug)
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
    Action::Watch => watch(command_tx, response_rx, control_rx),
    Action::SetWorkMode(action) => set_work_mode(command_tx, response_rx, control_rx, action),
    Action::SetReportingMode(action) => set_reporting_mode(command_tx, response_rx, control_rx, action),
    Action::SetWorkingPeriod(action) => set_working_period(command_tx, response_rx, control_rx, action)
  }
}