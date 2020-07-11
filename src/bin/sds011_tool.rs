
#[macro_use] extern crate log;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::time::{Duration, Instant};
use std::thread;

use anyhow::{Result, Error, Context};
use structopt::StructOpt;

use sds011_exporter::*;

#[derive(Debug, Clone, StructOpt)]
#[structopt(rename_all = "kebab-case")]
enum Action {
  /// Fetches sensor information
  Info,

  /// Displays sensor events
  Watch,
}

#[derive(Debug, Clone, StructOpt)]
#[structopt(name = "sds011-exporter")]
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
  thread::sleep(Duration::from_millis(100));

  command_tx.send(GetFirmwareVersion.to_cmd())?;

  command_tx.send(SetReportingMode {
    query: true,
    active: false
  }.to_cmd())?;

  thread::sleep(Duration::from_millis(100));

  command_tx.send(SetSleepWork {
    query: true,
    mode: WorkMode::Work
  }.to_cmd())?;

  thread::sleep(Duration::from_millis(100));

  command_tx.send(SetWorkingPeriod {
    query: true,
    working_period: WorkingPeriod::Continuous
  }.to_cmd())?;

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
        println!("Device ID:        {:?}", f.device);
        println!("Current Mode:     {:?}", s.mode);
        println!("Reporting mode:   {}", if r.active { "active" } else { "query" });
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
  command_tx: Sender<Cmd>,
  response_rx: Receiver<Response>,
  control_rx: Receiver<ControlMessage>
) -> Result<()> {

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
    Action::Watch => watch(command_tx, response_rx, control_rx)
  }
}