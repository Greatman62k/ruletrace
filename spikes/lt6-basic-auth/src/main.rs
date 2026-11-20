//! Command line of the LT-6 spike.
//!
//! `serve [--port P]`: stand-in Laboratory; the launch secret is printed once to stderr.
//! `listen --port P --lab-port L [--log FILE]`: raw recorder for the browser protocol.

#![forbid(unsafe_code)]

use lt6_basic_auth::{lab, recorder};
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

fn parse_port(value: Option<&String>, flag: &str) -> Result<u16, String> {
    value
        .ok_or_else(|| format!("{flag} needs a value"))?
        .parse::<u16>()
        .map_err(|_| format!("{flag} needs a port number"))
}

fn run(args: &[String]) -> Result<(), String> {
    let mode = args.first().map(String::as_str);
    let mut port: Option<u16> = None;
    let mut lab_port: Option<u16> = None;
    let mut log = PathBuf::from("recorder.log");
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => port = Some(parse_port(args.get(i + 1), "--port")?),
            "--lab-port" => lab_port = Some(parse_port(args.get(i + 1), "--lab-port")?),
            "--log" => {
                log = PathBuf::from(args.get(i + 1).ok_or("--log needs a value")?);
            }
            other => return Err(format!("unknown argument {other}")),
        }
        i += 2;
    }
    match mode {
        Some("serve") => {
            let (addr, secret) =
                lab::start_with_fresh_secret(port.unwrap_or(0)).map_err(|e| e.to_string())?;
            eprintln!("Stand-in Laboratory: http://{addr}/");
            eprintln!(
                "Browser prompt: user `{}`, password (shown once):",
                lab::USER
            );
            eprintln!("{secret}");
            Ok(())
        }
        Some("listen") => {
            let port = port.ok_or("listen needs --port")?;
            let lab_port = lab_port.ok_or("listen needs --lab-port")?;
            let addr = recorder::start(port, lab_port, log.clone()).map_err(|e| e.to_string())?;
            eprintln!("Recorder: http://{addr}/ (log file {})", log.display());
            Ok(())
        }
        _ => Err("usage: serve [--port P] | listen --port P --lab-port L [--log FILE]".into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => loop {
            thread::sleep(Duration::from_secs(3600));
        },
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}
