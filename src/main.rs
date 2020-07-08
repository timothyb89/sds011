#[macro_use] extern crate log;

use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use structopt::StructOpt;
use warp::Filter;
use serde_json::{self, json};
use simple_prometheus_exporter::{Exporter, export};

#[derive(Debug, Clone, StructOpt)]
#[structopt(name = "sds011-exporter")]
struct Options {
  /// port for the http server
  #[structopt(long, short, default_value = "8080", env = "CO2_PORT")]
  port: u16,
}

enum MaybeReading {
  Ok(()),
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

fn c_to_f(temp: f32) -> f32 {
  temp * (9f32 / 5f32) + 32f32
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

#[tokio::main]
async fn main() {
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