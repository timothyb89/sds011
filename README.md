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

## Installation: Raspberry Pi (2/3/4)

 1. Use `Dockerfile.gnueabihf` to build 32-bit ARM binaries for targets with
    hardware floating point (at least the Pi 2/3/4, and some Zero Ws depending
    on software).

    ```bash
    docker build . -f Dockerfile.gnueabi -t sds011-exporter:build
    ```

 2. Extract the binaries:

    ```bash
    mkdir -p /tmp/sds011-exporter
    docker run \
      --rm \
      -v /tmp/sds011-exporter:/tmp/sds011-exporter \
      sds011:test \
      sh -c 'cp /project/target/arm-unknown-linux-gnu*/release/sds011-tool /project/target/arm-unknown-linux-gnu*/release/sds011-exporter /tmp/sds011-exporter/'
    ```

 3. Copy the two binaries, `sds011-tool` and `sds011-exporter` from your local
    `/tmp/sds011-exporter` to your Pi's `/usr/local/bin/`.

 4. Copy [`sds011-exporter.service`] to `/etc/systemd/system/` on your Pi.

    Make sure to replace `<DEVICE>` in the `ExecStart=` section with your serial
    port device, e.g. `/dev/ttyUSB0`.

 5. Add the `pi` user to the dialout group:

    ```bash
    usermod -a -G dialout pi
    ```

 6. Enable and start the exporter:
    ```bash
    sudo systemctl enable sds011-exporter
    sudo systemctl start sds011-exporter
    ```

[`sds011-exporter.service`]: ./sds011-exporter.service

### Serial notes

The USB serial adapter included with the SDS011 works fine, you can free up a
USB port using the Pi's hardware UART support on the GPIO pins.

On earlier Pi versions this may require disabling the Linux serial console or
reconfiguring Bluetooth devices, however the Pi 4 has 4 additional
hardware-supported UARTs on other pins. For more information, see:
 * https://www.raspberrypi.org/documentation/configuration/uart.md
 * https://www.raspberrypi.org/forums/viewtopic.php?p=1493743&sid=1907f4ae24060b1c01e451fcd491f90c#p1493743

This project has been tested using `dtoverlay=uart4` to use serial on GPIO 8
and 9.

## Alternatives

 * https://github.com/Vourhey/nova-sds011-rs
 * https://github.com/chrisballinger/sds011-rs
 * https://github.com/woofwoofinc/rust-nova-sds011
 * https://gitlab.com/frankrich/sds011_particle_sensor/-/blob/master/Code/sds011.py
