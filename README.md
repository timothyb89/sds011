# sds011-async

This is a Rust library, command-line utility, and Prometheus exporter for the
Nova sds011 particulate matter sensor.

It aims to implement the entire protocol, and differs from other libraries in
that it can be used asynchronously<sup>1</sup> via [`mpsc` channels][mpsc], and
supports (and encourages) allowing the device to  report its own measurements
with the configured sleep/work period ("active reporting").

<small><sup>1</sup> unfortunately not implemented via `Futures`, but still
integrates easily with Tokio and friends.</small>

[mpsc]: https://doc.rust-lang.org/std/sync/mpsc/

## Usage: `sds011-tool`

Usage:

```bash
$ sds011-tool /dev/ttyUSB0 info
```

The [`sds011-tool`] can be used to inspect and configure the device:
  * `watch`: watches all incoming events, including actively-reported data
  * `info`: fetches current device configuration and firmware info
  * `set-reporting-mode [active|query]`: sets the device's reporting mode. If
    `active`, measurements will be sent proactively by the device at the
    interval set by `set-working-period`; if `query`, a query command must be
    sent to retrieve a measurement.
  * `set-work-mode [work|sleep]`: sets the device working mode, i.e. on or off.
    Note that while working physically moving parts are active and may
    contribute to wear over time.
  * `set-working-period [n]`: sets the working period when actively reporting
    data; 0 is continuous and reports every second, 1-30 (inclusive) is a period
    in minutes where the device sleeps for `(n minutes) - 30 seconds`, collects
    a measurement for 30 seconds, sends a measurement, and repeats.

Note that the sensor is remarkably bad at actually receiving messages,
particularly when active reporting is turned on, and particularly when reporting
continuously as messages tend to conflict with any commands being sent.
Be sure to check the return code to ensure the expected responses were received
and retry if necessary. The tool does retry automatically, but this doesn't
guarantee success.

[`sds011-tool`]: ./src/bin/sds011_tool.rs

## Usage: `sds011-exporter`

The [`sds011-exporter`] starts a web server that returns the current PM2.5 and
PM10 measurements as either JSON or Prometheus-compatible

[`sds011-exporter`]: ./src/bin/sds011_exporter.rs

## Alternatives

 * https://github.com/Vourhey/nova-sds011-rs
 * https://github.com/chrisballinger/sds011-rs
 * https://github.com/woofwoofinc/rust-nova-sds011
 * https://gitlab.com/frankrich/sds011_particle_sensor/-/blob/master/Code/sds011.py

