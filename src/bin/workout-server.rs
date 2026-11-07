//! workout-server: serves the sync API for the master log.
//!
//! ```sh
//! workout-server --log ~/workout-log/Workout.md
//! workout-server --log copy/Workout.md --listen 127.0.0.1:8484 --app ../app/.dist --allow-host workout.example.com
//! ```

use std::net::TcpListener;
use workout_syntax::server::{serve, Config};
use workout_syntax::sync::LogStore;

const USAGE: &str = "usage: workout-server --log <Workout.md> [--listen 127.0.0.1:8484] [--app <dir>] [--allow-host <name>]...";

fn main() {
    let mut log = None;
    let mut listen = "127.0.0.1:8484".to_string();
    let mut app = None;
    let mut hosts = vec!["127.0.0.1".to_string(), "localhost".to_string()];
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| fail(&format!("{a} needs a value\n{USAGE}")));
        match a.as_str() {
            "--log" => log = Some(value()),
            "--listen" => listen = value(),
            "--app" => app = Some(value().into()),
            "--allow-host" => hosts.push(value()),
            "-h" | "--help" => {
                println!("{USAGE}");
                return;
            }
            _ => fail(&format!("unknown argument {a}\n{USAGE}")),
        }
    }
    let log = log.unwrap_or_else(|| fail(USAGE));
    let store = LogStore::open(&log).unwrap_or_else(|e| fail(&e));
    let ip = listen.rsplit_once(':').map_or(listen.as_str(), |(h, _)| h);
    if !(ip == "127.0.0.1" || ip == "::1" || ip == "[::1]" || ip.starts_with("100.")) {
        eprintln!("warning: listening on {ip}; the server is meant for 127.0.0.1 or a Tailscale (100.x) address");
    }
    let listener = TcpListener::bind(&listen).unwrap_or_else(|e| fail(&format!("cannot listen on {listen}: {e}")));
    eprintln!("workout-server: {} on http://{listen}", store.path().display());
    if let Err(e) = serve(listener, Config { store, allowed_hosts: hosts, app_dir: app }) {
        fail(&format!("server stopped: {e}"));
    }
}

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    std::process::exit(2);
}
