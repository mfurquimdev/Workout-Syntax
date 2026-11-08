//! The sync server against a real file and git repository in a temporary
//! directory, and the HTTP layer on a local port.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use workout_syntax::server::{serve, Config};
use workout_syntax::sync::{LogStore, SyncReply, SyncRequest, PROTOCOL};

const LOG: &str = "2026-11-05\n18h00~19h00\n- Row 12x20kg\n- Bench 10x40kg\n\n2026-11-06\n- Squat 8x60kg\n";

static N: AtomicUsize = AtomicUsize::new(0);

/// A fresh directory with `Workout.md` holding `text`, in a git repository
/// with one commit when `git` is true.
fn setup(text: &str, git: bool) -> (PathBuf, LogStore) {
    let dir = std::env::temp_dir().join(format!("workout-sync-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("Workout.md");
    std::fs::write(&log, text).unwrap();
    if git {
        run(&dir, &["init", "-q"]);
        run(&dir, &["add", "Workout.md"]);
        run(&dir, &["commit", "-q", "-m", "Start"]);
    }
    let mut store = LogStore::open(&log).unwrap();
    store.settle = Duration::from_millis(5);
    (dir, store)
}

fn run(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn commits(dir: &Path) -> Vec<String> {
    run(dir, &["log", "--format=%s"]).lines().map(str::to_string).collect()
}

fn req(base: &str, text: &str) -> SyncRequest {
    SyncRequest { protocol: PROTOCOL, device: "phone".into(), base: base.into(), text: text.into(), confirm: None, keep: false }
}

fn file(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("Workout.md")).unwrap()
}

#[test]
fn a_device_addition_is_written_and_committed() {
    let (dir, store) = setup(LOG, true);
    let text = format!("{LOG}\n2026-11-07\n- Curl 12x10kg\n");
    let SyncReply::Merged { text: merged, notices, written } = store.sync(&req(LOG, &text)) else { panic!() };
    assert!(written);
    assert_eq!(merged, text);
    assert_eq!(file(&dir), text);
    assert!(notices.iter().any(|n| n == "Session 2026-11-07 added from this device"), "{notices:?}");
    assert_eq!(commits(&dir)[0], "Sync from phone: added 2026-11-07");
}

#[test]
fn a_repeated_sync_writes_nothing() {
    let (dir, store) = setup(LOG, true);
    let text = format!("{LOG}\n2026-11-07\n- Curl 12x10kg\n");
    store.sync(&req(LOG, &text));
    // the reply was lost, so the device sends the same request again
    let SyncReply::Merged { text: merged, written, .. } = store.sync(&req(LOG, &text)) else { panic!() };
    assert!(!written);
    assert_eq!(merged, text);
    assert_eq!(commits(&dir).len(), 2);
}

#[test]
fn changes_from_both_sides_merge() {
    let (dir, store) = setup(LOG, true);
    // Neovim adds a session on the server
    let server = format!("{LOG}\n2026-11-08\n- Deadlift 5x100kg\n");
    std::fs::write(dir.join("Workout.md"), &server).unwrap();
    let device = format!("{LOG}\n2026-11-07\n- Curl 12x10kg\n");
    let SyncReply::Merged { text, .. } = store.sync(&req(LOG, &device)) else { panic!() };
    assert!(text.contains("2026-11-07") && text.contains("2026-11-08"), "{text}");
    let c = commits(&dir);
    assert_eq!(c[0], "Sync from phone: added 2026-11-07");
    assert_eq!(c[1], "Outside edit");
}

/// The worst case from the reviews: a Neovim buffer opened before a phone
/// sync is saved afterwards, dropping the phone's session.
fn stale_save() -> (PathBuf, LogStore, String) {
    let (dir, store) = setup(LOG, true);
    let synced = format!("{LOG}\n2026-11-07\n- Curl 12x10kg\n- Press 8x30kg\n");
    store.sync(&req(LOG, &synced));
    std::fs::write(dir.join("Workout.md"), LOG).unwrap(); // the stale save
    (dir, store, synced)
}

#[test]
fn a_stale_save_asks_before_removing_and_writes_nothing() {
    let (dir, store, synced) = stale_save();
    let SyncReply::Confirm { removal_id, removals } = store.sync(&req(&synced, &synced)) else { panic!() };
    assert_eq!(removals, ["removes session 2026-11-07 (2 exercises, 2 sets)"]);
    assert_eq!(removal_id.len(), 16);
    assert_eq!(file(&dir), LOG, "nothing written while the question is open");
    assert_eq!(commits(&dir)[0], "Outside edit", "the stale save itself is in history");
}

#[test]
fn confirming_applies_the_removal() {
    let (dir, store, synced) = stale_save();
    let SyncReply::Confirm { removal_id, .. } = store.sync(&req(&synced, &synced)) else { panic!() };
    let mut r = req(&synced, &synced);
    r.confirm = Some(removal_id);
    let SyncReply::Merged { text, .. } = store.sync(&r) else { panic!() };
    assert_eq!(text, LOG);
    // a wrong ID asks again
    let (_, store, synced) = stale_save();
    let mut r = req(&synced, &synced);
    r.confirm = Some("0000000000000000".into());
    assert!(matches!(store.sync(&r), SyncReply::Confirm { .. }));
    let _ = dir;
}

#[test]
fn keeping_restores_the_device_sessions() {
    let (dir, store, synced) = stale_save();
    let mut r = req(&synced, &synced);
    r.keep = true;
    let SyncReply::Merged { text, written, .. } = store.sync(&r) else { panic!() };
    assert!(written);
    // every line comes back, with the blank line before the restored
    // session that the stale save dropped along with it
    assert_eq!(text, synced);
    assert_eq!(file(&dir), text);
}

#[test]
fn keeping_restores_lost_sets_but_keeps_other_server_changes() {
    let (dir, store) = setup(LOG, true);
    // Neovim drops the bench sets and adds a new session
    let server = LOG.replace("- Bench 10x40kg\n", "") + "\n2026-11-08\n- Deadlift 5x100kg\n";
    std::fs::write(dir.join("Workout.md"), &server).unwrap();
    assert!(matches!(store.sync(&req(LOG, LOG)), SyncReply::Confirm { .. }));
    let mut r = req(LOG, LOG);
    r.keep = true;
    let SyncReply::Merged { text, .. } = store.sync(&r) else { panic!() };
    assert!(text.contains("- Bench 10x40kg"), "{text}");
    assert!(text.contains("2026-11-08"), "{text}");
}

#[test]
fn the_device_own_deletions_are_not_questioned() {
    let (dir, store) = setup(LOG, true);
    let text = LOG.replace("- Bench 10x40kg\n", "");
    let SyncReply::Merged { written, .. } = store.sync(&req(LOG, &text)) else { panic!() };
    assert!(written);
    assert_eq!(file(&dir), text);
}

#[test]
fn refusals() {
    let (dir, store) = setup(LOG, true);
    let mut r = req(LOG, LOG);
    r.protocol = 0;
    let SyncReply::Error { message, http } = store.sync(&r) else { panic!() };
    assert_eq!(http, 409);
    assert!(message.starts_with("update needed") && message.ends_with("your text is safe on the device"));

    let SyncReply::Error { http, .. } = store.sync(&req(LOG, "  \n")) else { panic!() };
    assert_eq!(http, 400);

    let mut r = req(LOG, LOG);
    r.device = "my phone".into();
    assert!(matches!(store.sync(&r), SyncReply::Error { http: 400, .. }));

    let text = format!("{LOG}\n2026-11-0\n- Curl 12x10kg\n");
    assert!(matches!(store.sync(&req(LOG, &text)), SyncReply::Wait { .. }));

    // the server's file is empty: maybe mid-save, so nothing is merged
    std::fs::write(dir.join("Workout.md"), "").unwrap();
    let SyncReply::Error { http, message } = store.sync(&req(LOG, LOG)) else { panic!() };
    assert_eq!(http, 503, "{message}");
    assert_eq!(file(&dir), "");

    assert!(LogStore::open(dir.join("missing.md")).is_err(), "a missing log is never created");
    assert!(!dir.join("missing.md").exists());
}

#[test]
fn without_git_the_sync_still_works_with_a_notice() {
    let (dir, store) = setup(LOG, false);
    let text = format!("{LOG}\n2026-11-07\n- Curl 12x10kg\n");
    let SyncReply::Merged { notices, written, .. } = store.sync(&req(LOG, &text)) else { panic!() };
    assert!(written);
    assert!(notices.iter().any(|n| n.contains("not in a git repository")), "{notices:?}");
    assert_eq!(file(&dir), text);
}

#[test]
fn conflicts_keep_the_inputs_for_replay() {
    let (dir, store) = setup(LOG, true);
    std::fs::write(dir.join("Workout.md"), LOG.replace("12x20kg", "12x25kg")).unwrap();
    let SyncReply::Merged { text, .. } = store.sync(&req(LOG, &LOG.replace("12x20kg", "12x30kg"))) else { panic!() };
    assert!(text.contains("; conflict 2026-11-05 phone: - Row 12x30kg"), "{text}");
    let replays: Vec<_> = std::fs::read_dir(dir.join(".workout-sync/replays")).unwrap().collect();
    assert_eq!(replays.len(), 1);
    assert!(commits(&dir)[0].contains("1 conflict line"));
}

// ---------- HTTP ----------

fn start(store: LogStore) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = Config { store, allowed_hosts: vec!["127.0.0.1".into(), "localhost".into()], app_dir: None };
    std::thread::spawn(move || serve(listener, config));
    port
}

fn http(port: u16, raw: String) -> (u16, String, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(raw.as_bytes()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    let (head, body) = out.split_once("\r\n\r\n").unwrap();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, head.to_string(), body.to_string())
}

fn post(port: u16, host: &str, extra: &str, body: &str) -> (u16, String, String) {
    http(port, format!("POST /api/sync HTTP/1.1\r\nHost: {host}\r\n{extra}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()))
}

#[test]
fn http_api() {
    let (dir, store) = setup(LOG, true);
    let port = start(store);
    let host = format!("127.0.0.1:{port}");

    let (status, _, body) = http(port, format!("GET /api/health HTTP/1.1\r\nHost: {host}\r\n\r\n"));
    assert_eq!((status, body.as_str()), (200, "ok"));

    let (status, _, body) = http(port, format!("GET /api/log HTTP/1.1\r\nHost: {host}\r\n\r\n"));
    assert_eq!(status, 200);
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!((v["protocol"].as_u64(), v["text"].as_str()), (Some(PROTOCOL as u64), Some(LOG)));

    let text = format!("{LOG}\n2026-11-07\n- Curl 12x10kg\n");
    let body = serde_json::json!({ "protocol": PROTOCOL, "device": "phone", "base": LOG, "text": text }).to_string();
    let (status, head, reply) = post(port, &host, &format!("Origin: http://{host}\r\n"), &body);
    assert_eq!(status, 200, "{reply}");
    assert!(!head.to_ascii_lowercase().contains("access-control"), "no CORS headers: {head}");
    let v: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert_eq!((v["status"].as_str(), v["written"].as_bool()), (Some("merged"), Some(true)));
    assert_eq!(file(&dir), text);

    // another site's page, or a name this server does not answer for
    assert_eq!(post(port, &host, "Origin: https://evil.example\r\n", &body).0, 403);
    assert_eq!(post(port, "evil.example", "", &body).0, 421);
    assert_eq!(http(port, format!("GET /api/log HTTP/1.1\r\n\r\n")).0, 400);
    assert_eq!(post(port, &host, "", "{not json").0, 400);

    // the removal question travels as 409 with the list
    std::fs::write(dir.join("Workout.md"), LOG).unwrap();
    let body = serde_json::json!({ "protocol": PROTOCOL, "device": "phone", "base": text, "text": text }).to_string();
    let (status, _, reply) = post(port, &host, "", &body);
    assert_eq!(status, 409);
    let v: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(v["status"].as_str(), Some("confirm"));
    assert_eq!(v["removals"][0].as_str(), Some("removes session 2026-11-07 (1 exercise, 1 set)"));

    // no app directory: nothing else is served, and paths cannot escape
    assert_eq!(http(port, format!("GET /../Workout.md HTTP/1.1\r\nHost: {host}\r\n\r\n")).0, 404);
}

#[test]
fn static_app_files_stay_inside_the_app_directory() {
    let (dir, store) = setup(LOG, false);
    let app = dir.join("app");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::write(app.join("index.html"), "<p>app</p>").unwrap();
    std::fs::write(app.join("main.js"), "1").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = Config { store, allowed_hosts: vec!["127.0.0.1".into()], app_dir: Some(app) };
    std::thread::spawn(move || serve(listener, config));
    let get = |p: &str| http(port, format!("GET {p} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"));
    let (s, head, body) = get("/");
    assert_eq!((s, body.as_str()), (200, "<p>app</p>"));
    assert!(head.contains("text/html"));
    assert!(get("/main.js").1.contains("text/javascript"));
    assert_eq!(get("/some/route").2, "<p>app</p>", "single page app fallback");
    assert_eq!(get("/../Workout.md").0, 404);
    assert_eq!(get("/%2e%2e/Workout.md").2, "<p>app</p>", "encoded dots are only a file name");
}

#[test]
fn a_stale_save_of_an_unreadable_line_asks_too() {
    let (dir, store) = setup(LOG, true);
    let synced = format!("{LOG}- Pec Fly 39kg\n");
    let SyncReply::Merged { written, .. } = store.sync(&req(LOG, &synced)) else { panic!() };
    assert!(written);
    std::fs::write(dir.join("Workout.md"), LOG).unwrap(); // the stale save
    let SyncReply::Confirm { removals, .. } = store.sync(&req(&synced, &synced)) else { panic!("must ask") };
    assert_eq!(removals, ["removes the line `- Pec Fly 39kg` from 2026-11-06"]);
    assert_eq!(file(&dir), LOG);
}

#[test]
fn a_stale_save_that_loses_a_timed_set_asks() {
    let (dir, store) = setup(LOG, true);
    let synced = format!("{LOG}- Plank bw 60s^2\n");
    let SyncReply::Merged { written, .. } = store.sync(&req(LOG, &synced)) else { panic!() };
    assert!(written);
    // a stale save on the server keeps one of the two holds
    let stale = format!("{LOG}- Plank bw 60s\n");
    std::fs::write(dir.join("Workout.md"), &stale).unwrap();
    let SyncReply::Confirm { removals, .. } = store.sync(&req(&synced, &synced)) else { panic!("must ask") };
    assert_eq!(removals, ["removes 1 set of Plank on 2026-11-06 (2 to 1)"]);
    assert_eq!(file(&dir), stale);
}
