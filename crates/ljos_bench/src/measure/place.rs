//! Putting the gate inside the limit, by whichever mechanism holds it
//! ([`crate::enforce`]), and reading back what holds it.

use std::{
    ffi::{CString, OsString},
    fs::File,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use ljos_child::Reaped;

use super::{Error, Setup, account::Accounting, process};
use crate::{
    cgroup::{self, Cgroup},
    enforce::{
        self, CpusBy, Enforcement, LimitBy, Mechanism, cpu_mask, set_affinity, set_data_limit,
    },
    procfs,
    profile::{Profile, cpu_set},
};

/// The gate, inside the limit, not yet open.
pub(super) struct Placed {
    pub child: Reaped,
    pub acct: Accounting,
    pub enforcement: Enforcement,
    /// The systemd unit a scope is.
    pub unit: Option<String>,
    /// A cgroup the bench made, removed once the application is stopped.
    pub made: Option<Made>,
}

/// Why a mechanism did not place the gate.
enum Fail {
    /// The launcher made a scope that was not the one asked for: on a busy
    /// machine now and then, so the launch is tried again.
    Refused(String),
    /// The launcher could not start a scope at all.
    Launch(String),
    /// This mechanism cannot hold the limit here.
    Cannot(String),
}

/// What a placement is given.
pub(super) struct Ask<'a> {
    pub setup: &'a Setup,
    pub profile: &'a Profile,
    /// What follows the gate's program: `bench-gate --fifo …`.
    pub gate_args: &'a [OsString],
    pub fifo: &'a Path,
    pub log: &'a File,
    pub log_path: &'a Path,
    /// The application, for messages.
    pub what: &'a str,
    /// Unique to this launch, for a cgroup's name.
    pub tag: &'a str,
}

/// Places the gate by the mechanism `--limit-by` asks for, the one chosen
/// for the profile before, or the first that holds the limit.
///
/// # Errors
/// [`Error::Refused`] for a scope that was not isolated, which may be
/// tried again; [`Error::Unheld`] when nothing asked for can hold the
/// limit; [`Error::Launch`] for a launcher that cannot be started.
pub(super) fn place(ask: &Ask<'_>) -> Result<Placed, Error> {
    let setup = ask.setup;
    let chosen = setup.chosen(&ask.profile.name);
    let auto = setup.limit_by == LimitBy::Auto && chosen.is_none();
    let order: Vec<Mechanism> = match (chosen, setup.limit_by) {
        (Some(m), _) | (None, LimitBy::Only(m)) => vec![m],
        (None, LimitBy::Auto) => Mechanism::AUTO.to_vec(),
    };
    let mut why = Vec::new();
    for m in order {
        let placed = match m {
            Mechanism::Scope => scope(ask),
            Mechanism::Cgroup => own_cgroup(ask, auto),
            Mechanism::Container => container(ask, auto),
            Mechanism::Process => process_limits(ask),
        };
        match placed {
            Ok(p) => {
                if setup.choose(&ask.profile.name, m) {
                    eprintln!("ljos bench: {}", p.enforcement.describe(&ask.profile.name));
                }
                return Ok(p);
            }
            Err(Fail::Refused(s)) => return Err(Error::Refused(s)),
            Err(Fail::Launch(s)) if !auto => return Err(Error::Launch(s)),
            Err(Fail::Launch(s) | Fail::Cannot(s)) => why.push(format!("by {}: {s}", m.by())),
        }
    }
    let hint = if auto {
        "; --limit-by process measures under limits on the process alone, which hold less (docs/BENCHMARKS.md)"
    } else {
        ""
    };
    Err(Error::Unheld(format!(
        "{} cannot be held to {}: {}{hint}",
        ask.what,
        ask.profile.name,
        why.join("; ")
    )))
}

/// The gate started directly, not through a launcher.
fn gate_command(ask: &Ask<'_>) -> Result<Command, Fail> {
    let mut command = Command::new(&ask.setup.gate);
    command
        .args(ask.gate_args)
        .stdin(Stdio::null())
        .stdout(
            ask.log
                .try_clone()
                .map_err(|e| Fail::Cannot(e.to_string()))?,
        )
        .stderr(
            ask.log
                .try_clone()
                .map_err(|e| Fail::Cannot(e.to_string()))?,
        );
    Ok(command)
}

/// The profile's CPUs as a set for the affinity call.
fn mask(profile: &Profile) -> Result<Option<libc::cpu_set_t>, Fail> {
    profile
        .limits
        .cpus
        .as_deref()
        .map(|list| {
            cpu_mask(list).ok_or_else(|| Fail::Cannot(format!("{list:?} is not a CPU list")))
        })
        .transpose()
}

/// Whether `pid` runs only on the profile's CPUs.
fn check_affinity(pid: u32, profile: &Profile) -> Result<Option<String>, String> {
    let Some(want) = &profile.limits.cpus else {
        return Ok(None);
    };
    let have = enforce::affinity_of(pid).ok_or("its CPU affinity cannot be read")?;
    if cpu_set(&have) == cpu_set(want) {
        Ok(Some(have))
    } else {
        Err(format!("it may run on CPUs {have}, not only {want}"))
    }
}

fn spawn(command: &mut Command, what: &str) -> Result<Reaped, Fail> {
    Reaped::spawn(command)
        .map_err(|e| Fail::Cannot(format!("cannot start the gate for {what}: {e}")))
}

/// A systemd scope, made by the launcher.
fn scope(ask: &Ask<'_>) -> Result<Placed, Fail> {
    let setup = ask.setup;
    let mut command = Command::new(&setup.launcher);
    command
        .args(ask.profile.launcher_args())
        .arg("--")
        .arg(&setup.gate)
        .args(ask.gate_args)
        .stdin(Stdio::null())
        .stdout(
            ask.log
                .try_clone()
                .map_err(|e| Fail::Cannot(e.to_string()))?,
        )
        .stderr(
            ask.log
                .try_clone()
                .map_err(|e| Fail::Cannot(e.to_string()))?,
        );
    let mut child = Reaped::spawn(&mut command)
        .map_err(|e| Fail::Launch(format!("{}: {e}", setup.launcher.display())))?;
    let pid = child.id();
    let gate_exe = std::fs::canonicalize(&setup.gate).unwrap_or_else(|_| setup.gate.clone());
    let what = ask.what;

    // Not one instruction of the application runs until its cgroup is the
    // one asked for.
    let launched = Instant::now();
    let deadline = launched + setup.isolation_timeout;
    let cg = loop {
        if let Ok(Some(status)) = child.try_wait() {
            let said = std::fs::read_to_string(ask.log_path).unwrap_or_default();
            return Err(Fail::Launch(format!(
                "{what}: the launcher ended ({}) before the application started: {}",
                process::describe(status),
                said.trim()
            )));
        }
        // The gate first: the launcher moves itself before it execs the
        // gate, so a cgroup read after the gate is seen is the final one.
        // The gate is this program, and so is the child between its fork
        // and the launcher's exec; its own fifo tells the gate apart.
        let handed = procfs::exe(pid).is_some_and(|e| e == gate_exe)
            && procfs::has_arg(pid, ask.fifo.as_os_str());
        let cg =
            cgroup::of_pid(&setup.cgroup_root, pid).map_err(|e| Fail::Cannot(e.to_string()))?;
        match process::check_isolated(&cg, pid, ask.profile, CpusBy::Cpuset) {
            Ok(()) => break cg,
            Err(reason) if handed || Instant::now() > deadline => {
                let _ = child.stop();
                return Err(Fail::Refused(format!(
                    "{what} is not inside the limit of {} {} ms after its launch{}: {reason}",
                    ask.profile.name,
                    launched.elapsed().as_millis(),
                    if handed { ", in the gate" } else { "" },
                )));
            }
            Err(_) => thread::sleep(Duration::from_millis(5)),
        }
    };
    Ok(Placed {
        unit: process::unit(setup, &cg),
        enforcement: Enforcement::of_cgroup(
            &setup.cgroup_root,
            &cg,
            Mechanism::Scope,
            CpusBy::Cpuset,
        ),
        acct: Accounting::Own(Cgroup { dir: cg }),
        child,
        made: None,
    })
}

/// Where a leaf is made for the processes of a cgroup that must hand its
/// controllers down: a cgroup with children holds no processes itself.
const LEAF: &str = "ljos-bench-leaf";

/// The cgroup the bench's own cgroups are made under.
fn parent(setup: &Setup) -> Result<PathBuf, String> {
    if let Some(p) = &setup.cgroup_parent {
        let rel = p.strip_prefix("/").unwrap_or(p);
        return Ok(setup.cgroup_root.join(rel));
    }
    let own = cgroup::of_pid(&setup.cgroup_root, std::process::id()).map_err(|e| e.to_string())?;
    // Once the bench has moved itself into the leaf, the cgroup it hands
    // controllers down from is still the one above.
    Ok(match (own.file_name(), own.parent()) {
        (Some(name), Some(up)) if name == LEAF => up.to_owned(),
        _ => own,
    })
}

/// A cgroup the bench made: what is left in it is killed when it is
/// dropped, and it is removed.
#[derive(Debug)]
pub(super) struct Made(Cgroup);

impl Drop for Made {
    fn drop(&mut self) {
        let c = &self.0;
        if c.write("cgroup.kill", "1").is_err() {
            for pid in c.procs().unwrap_or_default() {
                if let Ok(pid) = i32::try_from(pid) {
                    // SAFETY: kill(2) on a process in the bench's own cgroup.
                    unsafe {
                        libc::kill(pid, libc::SIGKILL);
                    }
                }
            }
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while c.procs().is_some_and(|p| !p.is_empty()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let _ = std::fs::remove_dir(&c.dir);
    }
}

/// How [`hand_down`] writes a cgroup's file: the kernel's own, or a test's
/// stand-in that answers as the kernel would.
type WriteFile<'a> = &'a dyn Fn(&Path, &str) -> std::io::Result<()>;

fn write_file(path: &Path, value: &str) -> std::io::Result<()> {
    std::fs::write(path, value)
}

/// The controllers of `want` that `have` does not hand down yet.
fn missing<'a>(want: &[&'a str], have: &[String]) -> Vec<&'a str> {
    want.iter()
        .copied()
        .filter(|c| !have.iter().any(|h| h == c))
        .collect()
}

/// Whether the processes of `dir` may be moved into a leaf below it: when
/// the bench (`me`) is all it holds, or it is the root of the container's
/// tree, whose processes stay under the container's limits.
fn may_move(dir: &Path, procs: &[u32], me: u32, is_root: bool) -> Result<(), String> {
    if is_root || procs.iter().all(|&p| p == me) {
        return Ok(());
    }
    Err(format!(
        "{} holds processes other than the bench ({procs:?}), so it cannot hand its controllers down; give --cgroup-parent a cgroup the bench may make cgroups in",
        dir.display()
    ))
}

fn processes(n: usize) -> String {
    if n == 1 {
        "1 process".to_owned()
    } else {
        format!("{n} processes")
    }
}

/// Hands `want` down from `parent`, moving its processes into a leaf if it
/// must and [`may_move`] lets it. When the controllers still cannot be
/// handed down, the processes are moved back and the leaf removed.
fn hand_down(
    root: &Path,
    parent: &Cgroup,
    want: &[&str],
    me: u32,
    write: WriteFile<'_>,
    notes: &mut Vec<String>,
) -> Result<(), String> {
    let missing = missing(want, &parent.subtree_control());
    if missing.is_empty() {
        return Ok(());
    }
    let line: Vec<String> = missing.iter().map(|c| format!("+{c}")).collect();
    let line = line.join(" ");
    let file = parent.dir.join("cgroup.subtree_control");
    match write(&file, &line) {
        Ok(()) => return Ok(()),
        Err(e) if e.raw_os_error() == Some(libc::EBUSY) => {}
        Err(e) => return Err(format!("{} cannot be given {line:?}: {e}", file.display())),
    }
    let procs = parent.procs().unwrap_or_default();
    may_move(&parent.dir, &procs, me, parent.dir == root)?;
    let leaf = parent.dir.join(LEAF);
    std::fs::create_dir_all(&leaf).map_err(|e| format!("cannot make {}: {e}", leaf.display()))?;
    let into = leaf.join("cgroup.procs");
    // One that ended meanwhile is not in the way any more.
    let moved: Vec<u32> = procs
        .into_iter()
        .filter(|pid| write(&into, &pid.to_string()).is_ok())
        .collect();
    if let Err(e) = write(&file, &line) {
        let back = parent.dir.join("cgroup.procs");
        let stuck: Vec<u32> = moved
            .iter()
            .copied()
            .filter(|pid| write(&back, &pid.to_string()).is_err())
            .collect();
        let undone = if stuck.is_empty() {
            let _ = std::fs::remove_dir(&leaf);
            format!(
                "the {} moved into {LEAF} to make room were moved back",
                processes(moved.len())
            )
        } else {
            format!(
                "{} moved into {LEAF} to make room could not be moved back ({stuck:?}); they are still under {}'s limits",
                processes(stuck.len()),
                enforce::relative(root, &parent.dir)
            )
        };
        return Err(format!(
            "{} cannot be given {line:?}: {e}; {undone}",
            file.display()
        ));
    }
    let verb = if moved.len() == 1 { "was" } else { "were" };
    notes.push(format!(
        "{} {verb} moved from {} into {LEAF} below it, so it could hand its controllers down; they keep its limits",
        processes(moved.len()),
        enforce::relative(root, &parent.dir)
    ));
    Ok(())
}

/// Has `command`'s process write itself into `procs` (a cgroup's
/// `cgroup.procs`) and take `affinity` before it execs, so not even it
/// runs outside the limit.
fn join_on_exec(
    command: &mut Command,
    procs: &Path,
    affinity: Option<libc::cpu_set_t>,
) -> Result<(), String> {
    let procs = CString::new(procs.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?;
    // SAFETY: open, write, close and sched_setaffinity between fork and
    // exec; the path and the set were made before the fork.
    unsafe {
        command.pre_exec(move || {
            let fd = libc::open(procs.as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC);
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let wrote = libc::write(fd, b"0".as_ptr().cast(), 1);
            libc::close(fd);
            if wrote != 1 {
                return Err(std::io::Error::last_os_error());
            }
            if let Some(mask) = &affinity {
                set_affinity(mask)?;
            }
            Ok(())
        });
    }
    Ok(())
}

/// A cgroup the bench makes under one it may write.
fn own_cgroup(ask: &Ask<'_>, auto: bool) -> Result<Placed, Fail> {
    let setup = ask.setup;
    let root = &setup.cgroup_root;
    if auto && !cgroup::is_cgroup2(root) {
        return Err(Fail::Cannot(format!(
            "{} is not a cgroup v2 filesystem",
            root.display()
        )));
    }
    let parent = Cgroup {
        dir: parent(setup).map_err(Fail::Cannot)?,
    };
    let l = &ask.profile.limits;
    let available = parent.controllers();
    let has = |c: &str| available.iter().any(|a| a == c);
    let mut want = Vec::new();
    for (needed, controller) in [
        (l.memory_max.is_some() || l.no_swap, "memory"),
        (l.cpu_quota_percent.is_some(), "cpu"),
    ] {
        if needed && !has(controller) {
            return Err(Fail::Cannot(format!(
                "{} is not given the {controller} controller (cgroup.controllers: {:?})",
                parent.dir.display(),
                available.join(" ")
            )));
        }
        if needed {
            want.push(controller);
        }
    }
    // Without cpuset the CPUs are held by affinity instead.
    let cpus_by = if l.cpus.is_some() && has("cpuset") {
        want.push("cpuset");
        CpusBy::Cpuset
    } else {
        CpusBy::Affinity
    };
    let mut notes = Vec::new();
    hand_down(
        root,
        &parent,
        &want,
        std::process::id(),
        &write_file,
        &mut notes,
    )
    .map_err(Fail::Cannot)?;

    let dir = parent.dir.join(format!("ljos-bench-{}", ask.tag));
    std::fs::create_dir(&dir).map_err(|e| {
        Fail::Cannot(format!(
            "cannot make a cgroup in {}: {e}",
            parent.dir.display()
        ))
    })?;
    let made = Made(Cgroup { dir: dir.clone() });
    let set = |file: &str, value: &str| {
        made.0
            .write(file, value)
            .map_err(|e| Fail::Cannot(format!("{}/{file} cannot be {value}: {e}", dir.display())))
    };
    if let (Some(cpus), CpusBy::Cpuset) = (&l.cpus, cpus_by) {
        set("cpuset.cpus", cpus)?;
    }
    if let Some(bytes) = l.memory_max {
        set("memory.max", &bytes.to_string())?;
    }
    if l.no_swap {
        if dir.join("memory.swap.max").exists() {
            set("memory.swap.max", "0")?;
        } else if enforce::machine_has_swap() {
            return Err(Fail::Cannot(format!(
                "{} has no memory.swap.max, and the machine has swap",
                dir.display()
            )));
        } else {
            notes.push("the kernel accounts no swap here, and the machine has none".to_owned());
        }
    }
    if let Some((quota, period)) = ask.profile.cpu_max() {
        set("cpu.max", &format!("{quota} {period}"))?;
    }

    let affinity = match cpus_by {
        CpusBy::Affinity => mask(ask.profile)?,
        CpusBy::Cpuset => None,
    };
    let mut command = gate_command(ask)?;
    join_on_exec(&mut command, &dir.join("cgroup.procs"), affinity).map_err(Fail::Cannot)?;
    let mut child = spawn(&mut command, ask.what)?;
    let pid = child.id();
    let check = || -> Result<(), String> {
        let cg = cgroup::of_pid(root, pid).map_err(|e| e.to_string())?;
        if cg != dir {
            return Err(format!("it is in {}, not {}", cg.display(), dir.display()));
        }
        process::check_isolated(&dir, pid, ask.profile, cpus_by)
    };
    if let Err(why) = check() {
        let _ = child.stop();
        return Err(Fail::Cannot(format!(
            "{} is not inside its cgroup: {why}",
            ask.what
        )));
    }
    let mut enforcement = Enforcement::of_cgroup(root, &dir, Mechanism::Cgroup, cpus_by);
    if cpus_by == CpusBy::Affinity {
        enforcement.cpus = enforce::affinity_of(pid).filter(|_| l.cpus.is_some());
        if l.cpus.is_some() {
            enforcement.notes.push(format!(
                "{} is not given the cpuset controller, so the CPUs are held by the application's affinity",
                enforce::relative(root, &parent.dir)
            ));
        }
    }
    enforcement.notes.extend(notes);
    Ok(Placed {
        child,
        acct: Accounting::Own(made.0.clone()),
        enforcement,
        unit: None,
        made: Some(made),
    })
}

/// The cgroup the bench is in, when it holds the application at or below
/// the profile.
fn container(ask: &Ask<'_>, auto: bool) -> Result<Placed, Fail> {
    let setup = ask.setup;
    let root = &setup.cgroup_root;
    if auto && !cgroup::is_cgroup2(root) {
        return Err(Fail::Cannot(format!(
            "{} is not a cgroup v2 filesystem",
            root.display()
        )));
    }
    let own = cgroup::of_pid(root, std::process::id()).map_err(|e| Fail::Cannot(e.to_string()))?;
    let eff = enforce::effective(root, &own);
    enforce::check_container(&own, &eff, ask.profile).map_err(Fail::Cannot)?;
    let affinity = mask(ask.profile)?;
    let mut command = gate_command(ask)?;
    if let Some(mask) = affinity {
        // SAFETY: one system call between fork and exec, on a set made
        // before the fork.
        unsafe {
            command.pre_exec(move || set_affinity(&mask));
        }
    }
    let mut child = spawn(&mut command, ask.what).map_err(|e| match e {
        Fail::Cannot(s) if affinity.is_some() => Fail::Cannot(format!(
            "{s} (the container may not let it have CPUs {})",
            ask.profile.limits.cpus.as_deref().unwrap_or_default()
        )),
        other => other,
    })?;
    let pid = child.id();
    let cpus = match check_affinity(pid, ask.profile) {
        Ok(c) => c,
        Err(why) => {
            let _ = child.stop();
            return Err(Fail::Cannot(why));
        }
    };
    let cg = cgroup::of_pid(root, pid).map_err(|e| Fail::Cannot(e.to_string()))?;
    let holder = |b: Option<&Path>| b.map(|d| Cgroup { dir: d.to_owned() });
    let memory = holder(eff.memory.holder());
    let cpu = holder(eff.cpu.holder());
    let mut notes = vec![
        "the container's cgroup holds the bench and whatever else runs in the container too: what they use counts against the application's limit".to_owned(),
        "a kill for want of memory is counted only when it ended the application itself".to_owned(),
    ];
    if ask.profile.limits.memory_max.is_some() {
        notes.push("the least memory an application starts in cannot be searched for: the container's limit is not the bench's to change".to_owned());
    }
    let enforcement = Enforcement {
        mechanism: Mechanism::Container,
        cgroup: Some(enforce::relative(root, &cg)),
        memory_max: eff.memory.value(),
        swap_max: eff.swap.value(),
        cpu_max: eff.cpu.value(),
        cpus,
        cpus_by: ask.profile.limits.cpus.as_ref().map(|_| CpusBy::Affinity),
        accounting: enforce::Accounting::Processes,
        not_enforced: Vec::new(),
        notes,
    };
    Ok(Placed {
        child,
        acct: Accounting::tree(pid, memory, cpu, None),
        enforcement,
        unit: None,
        made: None,
    })
}

/// Limits on the application's process, asked for by name only.
fn process_limits(ask: &Ask<'_>) -> Result<Placed, Fail> {
    let l = &ask.profile.limits;
    let affinity = mask(ask.profile)?;
    let data = l.memory_max;
    let mut command = gate_command(ask)?;
    // Its own group, so the throttle stops all of it and nothing else.
    command.process_group(0);
    // SAFETY: two system calls between fork and exec, on values made
    // before the fork.
    unsafe {
        command.pre_exec(move || {
            if let Some(bytes) = data {
                set_data_limit(bytes)?;
            }
            if let Some(mask) = &affinity {
                set_affinity(mask)?;
            }
            Ok(())
        });
    }
    let mut child = spawn(&mut command, ask.what)?;
    let pid = child.id();
    let check = || -> Result<Option<String>, String> {
        if let Some(bytes) = data {
            let have = enforce::data_limit_of(pid);
            if have != Some(bytes) {
                return Err(format!(
                    "its RLIMIT_DATA is {}, not {bytes}",
                    have.map_or("unlimited".to_owned(), |b| b.to_string())
                ));
            }
        }
        check_affinity(pid, ask.profile)
    };
    let cpus = match check() {
        Ok(c) => c,
        Err(why) => {
            let _ = child.stop();
            return Err(Fail::Cannot(why));
        }
    };
    let mut not_enforced = Vec::new();
    if data.is_some() {
        not_enforced.push("memory is held for each process on its own (RLIMIT_DATA), not for the application's processes together, and an allocation over it fails rather than being killed for want of memory".to_owned());
    }
    if l.no_swap && enforce::machine_has_swap() {
        not_enforced
            .push("swap is not held back: the machine may swap the application out".to_owned());
    }
    if l.cpu_quota_percent.is_some() {
        not_enforced.push(format!(
            "CPU is held on average, by stopping and continuing the application's process group every {} ms, not by the scheduler; a process that leaves the group is not held",
            crate::throttle::EVERY.as_millis()
        ));
    }
    if l.cpus.is_some() {
        not_enforced
            .push("the CPUs are the application's affinity, which it may change".to_owned());
    }
    let quota = ask.profile.cpu_max();
    let enforcement = Enforcement {
        mechanism: Mechanism::Process,
        cgroup: None,
        memory_max: data,
        swap_max: None,
        cpu_max: quota.map(|(q, p)| [q, p]),
        cpus,
        cpus_by: l.cpus.as_ref().map(|_| CpusBy::Affinity),
        accounting: enforce::Accounting::Processes,
        not_enforced,
        notes: Vec::new(),
    };
    Ok(Placed {
        child,
        acct: Accounting::tree(pid, None, None, quota),
        enforcement,
        unit: None,
        made: None,
    })
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, fs, os::unix::process::ExitStatusExt};

    use super::*;

    /// Answers writes as cgroup v2 does: a cgroup holding processes cannot
    /// hand controllers down (unless `busy` says it never can), and a pid
    /// written to a `cgroup.procs` leaves the cgroup it was in.
    struct Kernel {
        root: PathBuf,
        busy: bool,
        writes: Cell<u32>,
    }

    fn procs_of(dir: &Path) -> Vec<String> {
        fs::read_to_string(dir.join("cgroup.procs"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn cgroups(dir: &Path, out: &mut Vec<PathBuf>) {
        out.push(dir.to_owned());
        for e in fs::read_dir(dir).unwrap().flatten() {
            if e.file_type().unwrap().is_dir() {
                cgroups(&e.path(), out);
            }
        }
    }

    impl Kernel {
        fn new(root: &Path, busy: bool) -> Self {
            Self {
                root: root.to_owned(),
                busy,
                writes: Cell::new(0),
            }
        }

        fn write(&self, path: &Path, value: &str) -> std::io::Result<()> {
            self.writes.set(self.writes.get() + 1);
            let dir = path.parent().unwrap();
            match path.file_name().unwrap().to_str().unwrap() {
                "cgroup.subtree_control" => {
                    if self.busy || !procs_of(dir).is_empty() {
                        return Err(std::io::Error::from_raw_os_error(libc::EBUSY));
                    }
                    let mut have = fs::read_to_string(path).unwrap_or_default();
                    for c in value.split_whitespace() {
                        have.push_str(c.trim_start_matches('+'));
                        have.push(' ');
                    }
                    fs::write(path, have)
                }
                "cgroup.procs" => {
                    let mut all = Vec::new();
                    cgroups(&self.root, &mut all);
                    for cg in all {
                        let left: Vec<String> =
                            procs_of(&cg).into_iter().filter(|p| p != value).collect();
                        let file = cg.join("cgroup.procs");
                        // An emptied leaf can be removed, as an empty cgroup can.
                        if left.is_empty() && cg.file_name().is_some_and(|n| n == LEAF) {
                            let _ = fs::remove_file(&file);
                        } else if file.exists() {
                            fs::write(
                                &file,
                                left.iter().map(|p| format!("{p}\n")).collect::<String>(),
                            )?;
                        }
                    }
                    let mut now = procs_of(dir);
                    now.push(value.to_owned());
                    fs::write(
                        path,
                        now.iter().map(|p| format!("{p}\n")).collect::<String>(),
                    )
                }
                other => panic!("hand_down wrote {other}"),
            }
        }
    }

    /// A cgroup `name` below a stand-in root, holding `procs` and handing
    /// down `subtree`.
    fn cgroup(root: &Path, name: &str, procs: &str, subtree: &str) -> Cgroup {
        let dir = if name.is_empty() {
            root.to_owned()
        } else {
            root.join(name)
        };
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("cgroup.procs"), procs).unwrap();
        fs::write(dir.join("cgroup.subtree_control"), subtree).unwrap();
        Cgroup { dir }
    }

    fn hand(k: &Kernel, parent: &Cgroup, me: u32, notes: &mut Vec<String>) -> Result<(), String> {
        hand_down(
            &k.root,
            parent,
            &["memory", "cpu"],
            me,
            &|p, v| k.write(p, v),
            notes,
        )
    }

    #[test]
    fn controllers_handed_down_already_are_not_written() {
        let t = tempfile::tempdir().unwrap();
        let k = Kernel::new(t.path(), false);
        let parent = cgroup(t.path(), "box", "42\n", "cpu memory\n");
        let mut notes = Vec::new();
        hand(&k, &parent, 42, &mut notes).unwrap();
        assert_eq!(k.writes.get(), 0);
        assert!(notes.is_empty());
        assert_eq!(
            missing(&["memory", "cpuset"], &["memory".to_owned()]),
            ["cpuset"]
        );
    }

    #[test]
    fn a_cgroup_holding_only_the_bench_moves_it_into_a_leaf() {
        let t = tempfile::tempdir().unwrap();
        let k = Kernel::new(t.path(), false);
        let parent = cgroup(t.path(), "box", "42\n", "");
        let mut notes = Vec::new();
        hand(&k, &parent, 42, &mut notes).unwrap();
        assert_eq!(procs_of(&parent.dir.join(LEAF)), ["42"]);
        assert!(procs_of(&parent.dir).is_empty());
        assert_eq!(parent.subtree_control(), ["memory", "cpu"]);
        assert_eq!(notes.len(), 1);
        assert!(
            notes[0].starts_with("1 process was moved from /box into ljos-bench-leaf"),
            "{notes:?}"
        );
    }

    #[test]
    fn a_cgroup_holding_others_is_turned_away_untouched() {
        let t = tempfile::tempdir().unwrap();
        let k = Kernel::new(t.path(), false);
        let parent = cgroup(t.path(), "box", "42\n7\n", "");
        let mut notes = Vec::new();
        let why = hand(&k, &parent, 42, &mut notes).unwrap_err();
        assert!(
            why.contains("holds processes other than the bench ([42, 7])"),
            "{why}"
        );
        assert!(!parent.dir.join(LEAF).exists());
        assert_eq!(procs_of(&parent.dir), ["42", "7"]);
        assert!(may_move(&parent.dir, &[42, 7], 42, true).is_ok());
    }

    #[test]
    fn the_containers_root_moves_all_it_holds() {
        let t = tempfile::tempdir().unwrap();
        let k = Kernel::new(t.path(), false);
        let root = cgroup(t.path(), "", "1\n7\n42\n", "");
        let mut notes = Vec::new();
        hand(&k, &root, 42, &mut notes).unwrap();
        assert_eq!(procs_of(&root.dir.join(LEAF)), ["1", "7", "42"]);
        assert!(notes[0].starts_with("3 processes were moved"), "{notes:?}");
    }

    #[test]
    fn a_hand_down_that_still_fails_moves_the_processes_back() {
        let t = tempfile::tempdir().unwrap();
        let k = Kernel::new(t.path(), true);
        let root = cgroup(t.path(), "", "1\n42\n", "");
        let mut notes = Vec::new();
        let why = hand(&k, &root, 42, &mut notes).unwrap_err();
        assert!(
            why.contains("the 2 processes moved into ljos-bench-leaf to make room were moved back"),
            "{why}"
        );
        assert_eq!(procs_of(&root.dir), ["1", "42"]);
        assert!(!root.dir.join(LEAF).exists());
        assert!(notes.is_empty());
    }

    /// On a machine that lets the test make a cgroup below its own: what
    /// joins on exec starts inside it, and dropping it kills what is left
    /// and removes it.
    #[test]
    fn a_made_cgroup_holds_what_joins_it_and_takes_it_away() {
        let root = Path::new("/sys/fs/cgroup");
        if !cgroup::is_cgroup2(root) {
            eprintln!("skipped: {} is not cgroup v2", root.display());
            return;
        }
        let own = cgroup::of_pid(root, std::process::id()).unwrap();
        let dir = own.join(format!("ljos-bench-test-{}", std::process::id()));
        if let Err(e) = fs::create_dir(&dir) {
            eprintln!("skipped: cannot make a cgroup in {}: {e}", own.display());
            return;
        }
        let made = Made(Cgroup { dir: dir.clone() });
        let mut command = Command::new("sleep");
        command.arg("30");
        join_on_exec(&mut command, &dir.join("cgroup.procs"), None).unwrap();
        let mut child = match command.spawn() {
            Ok(c) => c,
            // Moving needs the cgroup above to hand no controllers down
            // while it holds processes, and to be writable.
            Err(e)
                if matches!(
                    e.raw_os_error(),
                    Some(libc::EBUSY | libc::EACCES | libc::EPERM)
                ) =>
            {
                eprintln!("skipped: cannot move a process into {}: {e}", dir.display());
                return;
            }
            Err(e) => panic!("{e}"),
        };
        assert_eq!(cgroup::of_pid(root, child.id()).unwrap(), dir);
        drop(made);
        assert_eq!(child.wait().unwrap().signal(), Some(libc::SIGKILL));
        assert!(!dir.exists(), "{} is left", dir.display());
    }
}
