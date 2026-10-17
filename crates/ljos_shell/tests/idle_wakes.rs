//! Nothing polls (ARCHITECTURE.md principle 6): once the session bus is
//! connected and followed the way the shell follows it (the screen reader,
//! the settings portal, the badge, the accessibility bridge), no thread of
//! the process wakes until a message comes. The thread pool that opens a bus
//! connection lets its threads end once idle; a pool that keeps one parked
//! with a timeout wakes the application twice a second for nothing.
#![cfg(target_os = "linux")]

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// How long the process is watched.
const WATCH: Duration = Duration::from_secs(3);

/// A bus of the test's own that starts no services: an accessibility bus
/// the desktop would start answers messages, which is not the process
/// waking by itself.
struct Bus {
    daemon: Child,
    address: String,
    _dir: tempfile::TempDir,
}

impl Bus {
    fn start() -> Option<Self> {
        let dir = tempfile::tempdir().ok()?;
        let socket = dir.path().join("bus");
        let config = dir.path().join("bus.conf");
        std::fs::write(
            &config,
            format!(
                "<busconfig><type>session</type>\
                 <listen>unix:path={}</listen><auth>EXTERNAL</auth>\
                 <policy context=\"default\"><allow send_destination=\"*\"/>\
                 <allow receive_sender=\"*\"/><allow own=\"*\"/></policy>\
                 </busconfig>",
                socket.display()
            ),
        )
        .ok()?;
        let mut daemon = Command::new("dbus-daemon")
            .args(["--nofork", "--print-address"])
            .arg(format!("--config-file={}", config.display()))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut address = String::new();
        BufReader::new(daemon.stdout.take()?)
            .read_line(&mut address)
            .ok()?;
        Some(Bus {
            daemon,
            address: address.trim().to_owned(),
            _dir: dir,
        })
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

/// Every thread of the process but the one asking, with how often it has
/// slept and been woken.
fn switches() -> HashMap<String, (String, u64)> {
    let me = std::fs::read_link("/proc/thread-self").expect("/proc/thread-self");
    let me = me.file_name().expect("a thread id").to_owned();
    let mut threads = HashMap::new();
    for task in std::fs::read_dir("/proc/self/task").expect("/proc/self/task") {
        let task = task.expect("a thread");
        if task.file_name() == me {
            continue;
        }
        let read = |name: &str| std::fs::read_to_string(task.path().join(name));
        let (Ok(status), Ok(name)) = (read("status"), read("comm")) else {
            continue; // it ended
        };
        let count = status
            .lines()
            .filter_map(|l| {
                l.strip_prefix("voluntary_ctxt_switches:")
                    .or_else(|| l.strip_prefix("nonvoluntary_ctxt_switches:"))
            })
            .filter_map(|v| v.trim().parse::<u64>().ok())
            .sum();
        let id = task.file_name().to_string_lossy().into_owned();
        threads.insert(id, (name.trim().to_owned(), count));
    }
    threads
}

/// The threads that woke between two counts, by name.
fn woken(
    before: &HashMap<String, (String, u64)>,
    after: &HashMap<String, (String, u64)>,
) -> Vec<(String, u64)> {
    let mut woke: Vec<(String, u64)> = after
        .iter()
        .filter_map(|(id, (name, n))| {
            let was = before.get(id).map_or(0, |(_, n)| *n);
            (*n > was).then(|| (format!("{name} ({id})"), n - was))
        })
        .collect();
    woke.sort();
    woke
}

#[test]
fn a_process_following_the_session_bus_sleeps_until_a_message_comes() {
    if !Path::new("/proc/thread-self").exists() {
        eprintln!("skipped: no /proc");
        return;
    }
    let Some(bus) = Bus::start() else {
        eprintln!("skipped: no dbus-daemon to start");
        return;
    };
    // Connecting is what hands work to the pool; following a signal is what
    // the shell's watch threads then sleep in.
    let conn = zbus::blocking::connection::Builder::address(bus.address.as_str())
        .and_then(zbus::blocking::connection::Builder::build)
        .expect("the test's bus");
    let names = zbus::blocking::fdo::DBusProxy::new(&conn).expect("the bus's own interface");
    let changes = names.receive_name_owner_changed().expect("a signal stream");
    let follower = std::thread::Builder::new()
        .name("follower".into())
        .spawn(move || changes.count())
        .expect("a thread");

    // Past the start: a pool thread ends after half a second idle.
    std::thread::sleep(Duration::from_millis(1500));
    let before = switches();
    std::thread::sleep(WATCH);
    let after = switches();
    let woke = woken(&before, &after);
    assert!(
        woke.is_empty(),
        "threads woke in {WATCH:?} with nothing to do: {woke:?}"
    );

    drop(names);
    drop(conn);
    drop(bus);
    drop(follower);
}
