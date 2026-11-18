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

/// Hands `want` down from `parent`, moving its processes into a leaf if it
/// must and may: when the bench is all it holds, or it is the root of the
/// container's tree, whose processes stay under the container's limits.
fn hand_down(
    root: &Path,
    parent: &Cgroup,
    want: &[&str],
    notes: &mut Vec<String>,
) -> Result<(), String> {
    let have = parent.subtree_control();
    let missing: Vec<&str> = want
        .iter()
        .copied()
        .filter(|c| !have.iter().any(|h| h == c))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let line: Vec<String> = missing.iter().map(|c| format!("+{c}")).collect();
    let line = line.join(" ");
    let file = parent.dir.join("cgroup.subtree_control");
    match parent.write("cgroup.subtree_control", &line) {
        Ok(()) => return Ok(()),
        Err(e) if e.raw_os_error() == Some(libc::EBUSY) => {}
        Err(e) => return Err(format!("{} cannot be given {line:?}: {e}", file.display())),
    }
    let procs = parent.procs().unwrap_or_default();
    let me = std::process::id();
    let is_root = parent.dir == root;
    if !(is_root || procs.iter().all(|&p| p == me)) {
        return Err(format!(
            "{} holds processes other than the bench ({procs:?}), so it cannot hand its controllers down; give --cgroup-parent a cgroup the bench may make cgroups in",
            parent.dir.display()
        ));
    }
    let leaf = Cgroup {
        dir: parent.dir.join(LEAF),
    };
    std::fs::create_dir_all(&leaf.dir)
        .map_err(|e| format!("cannot make {}: {e}", leaf.dir.display()))?;
    let mut moved = 0;
    for pid in procs {
        // One that ended meanwhile is not in the way any more.
        if leaf.write("cgroup.procs", &pid.to_string()).is_ok() {
            moved += 1;
        }
    }
    parent
        .write("cgroup.subtree_control", &line)
        .map_err(|e| format!("{} cannot be given {line:?}: {e}", file.display()))?;
    let (count, verb) = if moved == 1 {
        ("1 process".to_owned(), "was")
    } else {
        (format!("{moved} processes"), "were")
    };
    notes.push(format!(
        "{count} {verb} moved from {} into {LEAF} below it, so it could hand its controllers down; they keep its limits",
        enforce::relative(root, &parent.dir)
    ));
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
    hand_down(root, &parent, &want, &mut notes).map_err(Fail::Cannot)?;

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

    // The gate moves itself in before it execs, so not even it runs
    // outside the limit.
    let procs = CString::new(dir.join("cgroup.procs").as_os_str().as_encoded_bytes())
        .map_err(|e| Fail::Cannot(e.to_string()))?;
    let affinity = match cpus_by {
        CpusBy::Affinity => mask(ask.profile)?,
        CpusBy::Cpuset => None,
    };
    let mut command = gate_command(ask)?;
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
