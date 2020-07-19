#[macro_use] extern crate log;

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;
use std::sync::mpsc::channel;

use anyhow::{Result};
use structopt::StructOpt;
use sds011_exporter::command::*;
use sds011_exporter::response::*;
use sds011_exporter::util::*;
use sds011_exporter::{retry_send_default, ControlMessage};
use serde_json::{self, json};
use simple_prometheus_exporter::{Exporter, export};
use warp::Filter;

#[derive(Debug, Clone, StructOpt)]
#[structopt(name = "sds011-exporter")]
struct Options {
  /// sensor serial device, e.g. /dev/ttyUSB0
  #[structopt(parse(from_os_str))]
  device: PathBuf,

  /// port for the http server
  #[structopt(long, short, default_value = "8082", env = "SDS011_PORT")]
  port: u16,

  /// device working period in minutes; 0 reports every second at the cost of
  /// accuracy, while 1-30 (inclusive) report once measurement every `n`
  /// minutes, with 30 seconds of data collection.
  #[structopt(long, default_value = "1", env = "SDS011_WORKING_PERIOD")]
  working_period: WorkingPeriod
}

type Reading = Option<QueryResponse>;

fn read_thread(
  reading_lock: Arc<RwLock<Reading>>,
  error_count: Arc<AtomicUsize>,
  fatal_error_count: Arc<AtomicUsize>,
  opts: &Options
) -> Result<()> {
  let (command_tx, command_rx) = channel();
  let (response_tx, response_rx) = channel();
  let (control_tx, control_rx) = channel();

  sds011_exporter::open_sensor(
    &opts.device,
    command_rx,
    response_tx,
    control_tx
  )?;

  retry_send_default(SetWorkingPeriod {
    query: false,
    working_period: opts.working_period,
  }, &command_tx, &response_rx)?;

  retry_send_default(SetReportingMode {
    query: false,
    mode: ReportingMode::Active
  }, &command_tx, &response_rx)?;

  info!(
    "configured device to actively report with working period: {:?}",
    opts.working_period
  );

  thread::spawn(move || {
    info!("started read thread");

    'outer: loop {
      for response in response_rx.try_iter() {
        if let Resp::Query(q) = response {
          match reading_lock.write() {
            Ok(mut latest) => *latest = Some(q),
            Err(e) => {
              error!("error acquiring lock: {}", e);
              break 'outer;
            }
          }
        }
      }

      for message in control_rx.try_iter() {
        match message {
          ControlMessage::Error(e) => {
            warn!("sensor warning: {:?}", e);
            error_count.fetch_add(1, Ordering::Relaxed);
          },
          ControlMessage::FatalError(e) => {
            error!("sensor fatal error: {:?}", e);
            fatal_error_count.fetch_add(1, Ordering::Relaxed);

            // clear the reading so charts don't report misleading data
            match reading_lock.write() {
              Ok(mut latest) => *latest = None,
              Err(e) => {
                error!("error acquiring lock while bailing anyway: {:?}", e);
              }
            }

            break 'outer;
          }
        }
      }

      thread::sleep(Duration::from_millis(1000));
    }

    error!("sensor thread exited unexpectedly; refer to the log for details");
    std::process::exit(1);
  });

  Ok(())
}

fn export_reading(
  exporter: &Exporter,
  reading: &Reading,
  error_count: &Arc<AtomicUsize>,
  fatal_error_count: &Arc<AtomicUsize>
) -> String {
  let mut s = exporter.session();

  match reading {
    Some(r) => {
      export!(s, "sds011_pm25", r.pm25, unit = "pm2.5");
      export!(s, "sds011_pm10", r.pm10, unit = "pm10");
    },
    None => ()
  };

  export!(s, "sds011_error_count", error_count.load(Ordering::Relaxed) as f64);
  export!(s, "sds011_fatal_error_count", fatal_error_count.load(Ordering::Relaxed) as f64);

  s.to_string()
}

#[tokio::main]
async fn main() -> Result<()> {
  let env = env_logger::Env::default()
    .filter_or("SDS011_LOG", "info")
    .write_style_or("SDS011_STYLE", "always");

  env_logger::Builder::from_env(env)
    .target(env_logger::Target::Stderr)
    .init();

  let opts = Options::from_args();
  let port = opts.port;

  let latest_reading_lock = Arc::new(RwLock::new(None));
  let error_count = Arc::new(AtomicUsize::new(0));
  let fatal_error_count = Arc::new(AtomicUsize::new(0));

  read_thread(
    latest_reading_lock.clone(),
    error_count.clone(),
    fatal_error_count.clone(),
    &opts
  )?;

  let json_lock = Arc::clone(&latest_reading_lock);
  let r_json = warp::path("json").map(move || {
    match *json_lock.read().unwrap() {
      Some(ref r) => warp::reply::json(&json!({
        "pm25": r.pm25,
        "pm10": r.pm10
      })),
      None => warp::reply::json(&json!(null))
    }
  });

  let exporter = Arc::new(Exporter::new());
  let metrics_lock = Arc::clone(&latest_reading_lock);
  let metrics_error_count = Arc::clone(&error_count);
  let metrics_fatal_error_count = Arc::clone(&fatal_error_count);
  let r_metrics = warp::path("metrics").map(move || {
    export_reading(
      &exporter,
      &*metrics_lock.read().unwrap(),
      &metrics_error_count,
      &metrics_fatal_error_count
    )
  });

  info!("starting exporter on port {}", port);

  let routes = warp::get().and(r_json).or(r_metrics);
  warp::serve(routes).run(([0, 0, 0, 0], port)).await;

  Ok(())
}
