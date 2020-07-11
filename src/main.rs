#[macro_use] extern crate log;

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;
use std::sync::mpsc::channel;

use anyhow::{Result, Error, Context};
use structopt::StructOpt;
use warp::Filter;
use serde_json::{self, json};
use simple_prometheus_exporter::{Exporter, export};
use tokio::prelude::*;

use sds011_exporter::*;

#[derive(Debug, Clone, StructOpt)]
#[structopt(name = "sds011-exporter")]
struct Options {
  /// sensor serial device, e.g. /dev/ttyUSB0
  #[structopt(parse(from_os_str))]
  device: PathBuf,

  /// port for the http server
  #[structopt(long, short, default_value = "8080", env = "SDS011_PORT")]
  port: u16,
}

struct Reading {
  pm25: f32,
  pm10: f32
}

enum MaybeReading {
  Ok(Reading),
  Err(()),
  None
}

fn read_thread(
  reading_lock: Arc<RwLock<MaybeReading>>,
  error_count: Arc<AtomicUsize>
) {
  let sensor = ();

  thread::spawn(move || {
    loop {
      let reading = MaybeReading::None; // TODO

      thread::sleep(Duration::from_millis(1000));

      let mut latest_reading = match reading_lock.write() {
        Ok(r) => r,
        Err(e) => {
          error!("error acquiring lock: {}", e);
          break;
        }
      };

      *latest_reading = reading;
    }

    error!("sensor thread exited unexpectedly; refer to the log for details");
    std::process::exit(1);
  });
}

fn export_reading(
  exporter: &Exporter, reading: &MaybeReading, error_count: &Arc<AtomicUsize>
) -> String {
  let mut s = exporter.session();

  match reading {
    MaybeReading::Ok(r) => {
      // TODO
      export!(s, "sds011_pm25", 0, unit = "pm2.5");
      export!(s, "sds011_pm10", 0, unit = "pm10");
    },
    MaybeReading::Err(e) => {
      export!(s, "sds011_error", 1);
      //export!(s, "sds011_error", 1, kind = e.to_string());
    },
    MaybeReading::None => ()
  };

  export!(s, "co2_error_count", error_count.load(Ordering::Relaxed) as f64);

  s.to_string()
}

fn main() -> Result<()> {
  env_logger::init();

  let opts = Options::from_args();
  let port = opts.port;

  let (command_tx, command_rx) = channel();
  let (response_tx, response_rx) = channel();
  let (control_tx, control_rx) = channel();

  sds011_exporter::open_sensor(
    &opts.device,
    command_rx,
    response_tx,
    control_tx
  )?;

  loop {
    for response in response_rx.try_iter() {
      info!("response: {:?}", response);
    }

    for control in control_rx.try_iter() {
      info!("control msg: {:?}", control);

      match control {
        ControlMessage::Error(e) => {
          error!("error: {}", e);
        },
        ControlMessage::FatalError(e) => {
          error!("fatal error: {}", e);
          std::process::exit(1);
        }
      }
    }

    thread::sleep(Duration::from_micros(1000));
  }

  Ok(())
}

#[tokio::main]
async fn main_later() {
  env_logger::init();

  let opts = Options::from_args();
  let port = opts.port;



  let error_count = Arc::new(AtomicUsize::new(0));
  let latest_reading_lock = Arc::new(RwLock::new(MaybeReading::None));

  read_thread(latest_reading_lock.clone(), error_count.clone());

  let json_lock = Arc::clone(&latest_reading_lock);
  let r_json = warp::path("json").map(move || {
    match *json_lock.read().unwrap() {
      MaybeReading::Ok(ref r) => warp::reply::json(&json!({
        "todo": true,
      })),
      MaybeReading::Err(ref e) => warp::reply::json(&json!({
        //"error": e.to_string()
        "error": true
      })),
      MaybeReading::None => warp::reply::json(&json!(null))
    }
  });

  let exporter = Arc::new(Exporter::new());
  let metrics_lock = Arc::clone(&latest_reading_lock);
  let metrics_error_count = Arc::clone(&error_count);
  let r_metrics = warp::path("metrics").map(move || {
    export_reading(&exporter, &*metrics_lock.read().unwrap(), &metrics_error_count)
  });

  let routes = warp::get().and(r_json).or(r_metrics);
  warp::serve(routes).run(([0, 0, 0, 0], port)).await;
}
