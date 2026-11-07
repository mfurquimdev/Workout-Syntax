//! The server side of a sync: merges a device's text into the master log
//! file and commits every change to git.
//!
//! Write order (from the design brief):
//! 1. Take a lock. Neovim does not honour it; it only serializes syncs.
//! 2. Read the file. If it is empty or much shorter than the last written
//!    text, wait briefly and read again: Neovim may be mid-save.
//! 3. If the file differs from the last commit, commit it as an outside edit.
//! 4. Merge. If the result would remove anything the device still has,
//!    write nothing and ask (the removal question). If the result equals
//!    the file, stop: no write and no commit.
//! 5. Write a temp file in the same directory and fsync it.
//! 6. Check the file again. If it changed meanwhile, start over.
//! 7. Rename over the file, then fsync the directory.
//! 8. Commit, naming the device and what changed.
//! 9. Reply.
//!
//! The log is never created: a missing file is an error. An empty body is
//! refused. A failed commit does not undo the write; it becomes a notice.

use crate::merge::{self, Notice};
use crate::removals::removals;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

/// The sync protocol this server speaks. A device sending another value is
/// told to update; its text stays safe on the device.
pub const PROTOCOL: u32 = 1;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SyncRequest {
    pub protocol: u32,
    pub device: String,
    /// The text the device last agreed with the server
    pub base: String,
    /// The device's text now
    pub text: String,
    /// Answer to a removal question: apply the removals with this ID
    #[serde(default)]
    pub confirm: Option<String>,
    /// Answer to a removal question: keep the device's version of the
    /// sessions the question listed
    #[serde(default)]
    pub keep: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SyncReply {
    /// The merge is done. `text` is the merged log, which the device adopts
    /// as its text and base.
    Merged { text: String, notices: Vec<String>, written: bool },
    /// The merge would remove these from the device. Nothing was written.
    /// Resend with `confirm: removal_id` to remove, or `keep: true` to keep.
    Confirm { removal_id: String, removals: Vec<String> },
    /// The device's text is not ready to merge, such as a half-typed date.
    Wait { reason: String },
    /// Nothing was merged. `http` is the HTTP status to answer with.
    Error { message: String, #[serde(skip)] http: u16 },
}

/// Answers for a `GET /api/log`: the whole log, for a device's first open.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogReply {
    pub protocol: u32,
    pub text: String,
}

/// The master log and its git repository.
pub struct LogStore {
    path: PathBuf,
    lock: Mutex<Option<String>>, // the last text this server wrote
    /// Pause between reads while the file looks mid-save
    pub settle: Duration,
}

impl LogStore {
    /// Opens an existing log. Never creates one.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        if !path.is_file() {
            return Err(format!("no log at {}; the server never creates one", path.display()));
        }
        Ok(LogStore { path, lock: Mutex::new(None), settle: Duration::from_millis(200) })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn read(&self) -> Result<LogReply, String> {
        let text = fs::read_to_string(&self.path).map_err(|e| format!("cannot read the log: {e}"))?;
        Ok(LogReply { protocol: PROTOCOL, text })
    }

    pub fn sync(&self, req: &SyncRequest) -> SyncReply {
        if req.protocol != PROTOCOL {
            return error(409, format!(
                "update needed: this device speaks protocol {}, the server speaks {PROTOCOL}; your text is safe on the device",
                req.protocol
            ));
        }
        if !merge::valid_device_name(&req.device) {
            return error(400, format!("device name `{}` must be one word without spaces or colons", req.device));
        }
        if req.text.trim().is_empty() {
            return error(400, "refused an empty log; the device's text is safe".into());
        }
        if crate::parse_document(&req.text).has_unfinished_date() {
            return SyncReply::Wait { reason: "a date line is unfinished or not yet yyyy-mm-dd".into() };
        }
        let mut last_written = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        for _ in 0..3 {
            match self.attempt(req, &mut last_written) {
                Attempt::Done(reply) => return reply,
                Attempt::Changed => continue, // the file changed during the write: start over
            }
        }
        error(503, "the log kept changing during the sync; try again".into())
    }

    fn attempt(&self, req: &SyncRequest, last_written: &mut Option<String>) -> Attempt {
        // 2. read, waiting out a save in progress
        let server = match self.settled_read(last_written.as_deref()) {
            Ok(t) => t,
            Err(reply) => return Attempt::Done(reply),
        };
        let mut notices = Vec::new();
        // 3. commit an outside edit
        let repo = self.repo();
        if let Some(repo) = &repo {
            if let Some(head) = git_show_head(repo, &self.path) {
                if head != server {
                    for r in removals(&head, &server) {
                        if let crate::removals::Removal::Session { date, .. } = r {
                            notices.push(format!("Session {date} was removed outside the app"));
                        }
                    }
                    if let Err(e) = git_commit(repo, &self.path, "Outside edit") {
                        notices.push(format!("the outside edit was not committed: {e}"));
                    }
                }
            } else if let Err(e) = git_commit(repo, &self.path, "Outside edit") {
                notices.push(format!("the log was not committed: {e}"));
            }
        } else {
            notices.push("the log is not in a git repository, so changes have no history".into());
        }
        // 4. merge, and ask before removing anything the device has
        let merged = match merge::merge(&req.base, &server, &req.text, &req.device) {
            Ok(m) => m,
            Err(e) => return Attempt::Done(error(400, format!("cannot merge: {e}"))),
        };
        let lost = removals(&req.text, &merged.text);
        let merged = if lost.is_empty() {
            merged
        } else {
            let id = removal_id(&req.base, &server, &req.text, &lost);
            if req.keep {
                let dates: Vec<&str> = lost.iter().map(|r| r.date()).collect();
                let base = with_sessions_from(&req.base, &server, &dates);
                match merge::merge(&base, &server, &req.text, &req.device) {
                    Ok(m) => m,
                    Err(e) => return Attempt::Done(error(400, format!("cannot merge: {e}"))),
                }
            } else if req.confirm.as_deref() == Some(id.as_str()) {
                merged
            } else {
                let removals = lost.iter().map(|r| r.to_string()).collect();
                return Attempt::Done(SyncReply::Confirm { removal_id: id, removals });
            }
        };
        notices.extend(merged.notices.iter().map(|n| n.to_string()));
        if merged.text == server {
            return Attempt::Done(SyncReply::Merged { text: merged.text, notices, written: false });
        }
        if merged.notices.iter().any(|n| matches!(n, Notice::Conflict { .. })) {
            if let Err(e) = self.keep_replay(req, &server) {
                notices.push(format!("the merge inputs were not saved for replay: {e}"));
            }
        }
        // 5.–7. write a temp file, check, rename
        match self.replace(&server, &merged.text) {
            Ok(true) => {}
            Ok(false) => return Attempt::Changed,
            Err(e) => return Attempt::Done(error(500, format!("cannot write the log: {e}"))),
        }
        *last_written = Some(merged.text.clone());
        // 8. commit
        if let Some(repo) = &repo {
            if let Err(e) = git_commit(repo, &self.path, &commit_message(&req.device, &merged.notices)) {
                notices.push(format!("the sync was written but not committed: {e}"));
            }
        }
        Attempt::Done(SyncReply::Merged { text: merged.text, notices, written: true })
    }

    fn settled_read(&self, last_written: Option<&str>) -> Result<String, SyncReply> {
        let mut text = String::new();
        for i in 0..5 {
            text = fs::read_to_string(&self.path).map_err(|e| error(500, format!("cannot read the log: {e}")))?;
            let short = last_written.is_some_and(|w| text.len() * 2 < w.len());
            if !text.trim().is_empty() && !short {
                return Ok(text);
            }
            if i < 4 {
                std::thread::sleep(self.settle);
            }
        }
        if text.trim().is_empty() {
            Err(error(503, "the log on the server is empty; it may be mid-save, so nothing was merged".into()))
        } else {
            Err(error(503, "the log on the server is much shorter than the last sync; it may be mid-save, so nothing was merged".into()))
        }
    }

    /// Writes `new` over the log if it still holds `expected`. `Ok(false)`
    /// when the file changed meanwhile.
    fn replace(&self, expected: &str, new: &str) -> std::io::Result<bool> {
        let dir = self.path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let name = self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let tmp = dir.join(format!(".{name}.sync-{}", std::process::id()));
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(new.as_bytes())?;
            f.sync_all()?;
        }
        if fs::read_to_string(&self.path)? != expected {
            fs::remove_file(&tmp)?;
            return Ok(false);
        }
        fs::rename(&tmp, &self.path)?;
        if let Ok(d) = fs::File::open(dir) {
            let _ = d.sync_all();
        }
        Ok(true)
    }

    /// Saves base, server and device text of a merge that wrote conflict
    /// comments, so a bad merge can be replayed. Kept next to the log, out
    /// of git.
    fn keep_replay(&self, req: &SyncRequest, server: &str) -> std::io::Result<()> {
        let dir = self.state_dir().join("replays");
        fs::create_dir_all(&dir)?;
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
        let body = serde_json::json!({ "device": req.device, "base": req.base, "server": server, "text": req.text });
        fs::write(dir.join(format!("{stamp}-{}.json", req.device)), body.to_string())
    }

    fn state_dir(&self) -> PathBuf {
        self.path.parent().unwrap_or(Path::new(".")).join(".workout-sync")
    }

    fn repo(&self) -> Option<PathBuf> {
        let dir = self.path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let out = Command::new("git").arg("-C").arg(dir).args(["rev-parse", "--show-toplevel"]).output().ok()?;
        out.status.success().then(|| PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
    }
}

enum Attempt {
    Done(SyncReply),
    Changed,
}

fn error(http: u16, message: String) -> SyncReply {
    SyncReply::Error { message, http }
}

/// A stable ID for one removal question: the same inputs give the same ID,
/// also after a server restart, so a resent confirmation still matches.
fn removal_id(base: &str, server: &str, device: &str, lost: &[crate::removals::Removal]) -> String {
    // FNV-1a, 64 bits
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |s: &str| {
        for b in s.bytes().chain([0xff]) {
            h ^= b as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    };
    feed(base);
    feed(server);
    feed(device);
    for r in lost {
        feed(&r.to_string());
    }
    format!("{h:016x}")
}

/// `base` with the sessions on `dates` taken from `server` (or dropped when
/// the server has none), so that in the next merge the device's version of
/// those sessions is the only change and wins.
fn with_sessions_from(base: &str, server: &str, dates: &[&str]) -> String {
    let base_blocks = blocks(base);
    let server_blocks = blocks(server);
    let mut out: Vec<String> = Vec::new();
    let mut done = Vec::new();
    for (date, text) in &base_blocks {
        match date {
            Some(d) if dates.contains(&d.as_str()) => {
                if let Some((_, t)) = server_blocks.iter().find(|(sd, _)| sd.as_deref() == Some(d)) {
                    out.push(t.clone());
                }
                done.push(d.clone());
            }
            _ => out.push(text.clone()),
        }
    }
    for (date, text) in &server_blocks {
        if let Some(d) = date {
            if dates.contains(&d.as_str()) && !done.contains(d) {
                out.push(text.clone());
            }
        }
    }
    out.concat()
}

/// The text cut at the lines that start a session, keeping line endings.
fn blocks(text: &str) -> Vec<(Option<String>, String)> {
    let mut out: Vec<(Option<String>, String)> = vec![(None, String::new())];
    for line in text.split_inclusive('\n') {
        if let Some(d) = merge::date_key(line.trim_end_matches('\n')) {
            out.push((Some(d.to_string()), String::new()));
        }
        out.last_mut().unwrap().1.push_str(line);
    }
    out
}

fn commit_message(device: &str, notices: &[Notice]) -> String {
    let mut added = Vec::new();
    let mut moved = Vec::new();
    let mut conflicts = 0;
    for n in notices {
        match n {
            Notice::Added { date, from: merge::Side::Device } => added.push(date.as_str()),
            Notice::Moved { date } => moved.push(date.as_str()),
            Notice::Conflict { .. } => conflicts += 1,
            _ => {}
        }
    }
    let mut parts = Vec::new();
    if !added.is_empty() {
        parts.push(format!("added {}", added.join(", ")));
    }
    if !moved.is_empty() {
        parts.push(format!("moved {}", moved.join(", ")));
    }
    if conflicts > 0 {
        parts.push(format!("{conflicts} conflict line{}", if conflicts == 1 { "" } else { "s" }));
    }
    let mut msg = format!("Sync from {device}");
    if !parts.is_empty() {
        msg.push_str(": ");
        msg.push_str(&parts.join("; "));
    }
    msg.push_str("\n\n");
    for n in notices {
        msg.push_str(&format!("- {n}\n"));
    }
    msg
}

fn git(repo: &Path) -> Command {
    let mut c = Command::new("git");
    c.arg("-C").arg(repo).args(["-c", "user.name=workout-server", "-c", "user.email=workout-server@localhost"]);
    c
}

/// The log's content at HEAD, or `None` when it has no commit yet.
fn git_show_head(repo: &Path, path: &Path) -> Option<String> {
    let rel = path.canonicalize().ok()?.strip_prefix(repo.canonicalize().ok()?).ok()?.to_path_buf();
    let out = git(repo).arg("show").arg(format!("HEAD:{}", rel.display())).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Commits the log if it changed. Ok when there was nothing to commit.
fn git_commit(repo: &Path, path: &Path, message: &str) -> Result<(), String> {
    let run = |c: &mut Command| -> Result<std::process::Output, String> { c.output().map_err(|e| e.to_string()) };
    let add = run(git(repo).arg("add").arg("--").arg(path))?;
    if !add.status.success() {
        return Err(String::from_utf8_lossy(&add.stderr).trim().to_string());
    }
    let staged = run(git(repo).args(["diff", "--cached", "--quiet", "--"]).arg(path))?;
    if staged.status.success() {
        return Ok(()); // nothing to commit
    }
    let commit = run(git(repo).args(["commit", "--quiet", "-m", message, "--"]).arg(path))?;
    if commit.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&commit.stderr).trim().to_string())
    }
}
