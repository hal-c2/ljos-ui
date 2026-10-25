//! `app_metrics` `memory` with `mappings`: what is resident, split by the
//! mapping it is in, as the kernel's `/proc/self/smaps` tells it.
//!
//! The owners count what Ljós holds; the program's code, the libraries,
//! the GPU driver and what they allocate are outside them. Each mapping
//! names its file, so the resident pages of the program, of LLVM, of the
//! driver and of the fonts can be told apart; anonymous pages are split
//! into the allocator's heap, other anonymous maps, code compiled at run
//! time and the stack. Pages a library wrote into its own mapping (its
//! data, its relocations) are anonymous too and are counted with it.
//!
//! The file is read only when a client asks: walking it takes a few
//! milliseconds in a large program. Elsewhere than on Linux nothing is
//! known, and nothing is guessed.

use serde_json::{Value, json};

/// The groups, in the order they are listed when equal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Group {
    /// The program's own file.
    Program,
    /// libLLVM: the shader compiler of software and some GPU drivers.
    Llvm,
    /// The GPU driver and its loaders: Mesa, Vulkan, EGL, GL, DRM.
    Driver,
    Fonts,
    Libraries,
    /// Other mapped files: locale data, icon caches.
    Files,
    /// Shared memory: window buffers handed to the compositor.
    Shared,
    /// The GPU device's own memory mapped into the process.
    Device,
    /// The allocator's main heap (`[heap]`).
    Heap,
    /// Other anonymous maps: the allocator's other arenas and large
    /// blocks, thread stacks, what libraries map themselves.
    Anonymous,
    /// Anonymous and executable: code compiled at run time.
    Jit,
    /// The main thread's stack.
    Stack,
    /// What the kernel maps in (`[vdso]`, `[vvar]`).
    Kernel,
}

impl Group {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Group::Program => "program",
            Group::Llvm => "llvm",
            Group::Driver => "gpu_driver",
            Group::Fonts => "fonts",
            Group::Libraries => "libraries",
            Group::Files => "files",
            Group::Shared => "shared",
            Group::Device => "gpu_device",
            Group::Heap => "heap",
            Group::Anonymous => "anonymous",
            Group::Jit => "jit",
            Group::Stack => "stack",
            Group::Kernel => "kernel",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Group::Program => "the program",
            Group::Llvm => "LLVM",
            Group::Driver => "GPU driver",
            Group::Fonts => "fonts",
            Group::Libraries => "other libraries",
            Group::Files => "other files",
            Group::Shared => "shared memory",
            Group::Device => "GPU device maps",
            Group::Heap => "allocator heap",
            Group::Anonymous => "other anonymous maps",
            Group::Jit => "code compiled at run time",
            Group::Stack => "main stack",
            Group::Kernel => "kernel pages",
        }
    }

    /// Whether its pages are a file's or shared: what `statm` counts as
    /// shared, the program and its libraries.
    fn file_backed(self) -> bool {
        matches!(
            self,
            Group::Program
                | Group::Llvm
                | Group::Driver
                | Group::Fonts
                | Group::Libraries
                | Group::Files
                | Group::Shared
                | Group::Device
        )
    }
}

/// What strips a path of what the kernel adds to a file replaced or
/// removed since it was mapped.
fn trimmed(path: &str) -> &str {
    path.strip_suffix(" (deleted)").unwrap_or(path)
}

fn is_driver(name: &str, path: &str) -> bool {
    const PREFIXES: [&str; 18] = [
        "libgallium",
        "libvulkan",
        "libGL",
        "libEGL",
        "libGLX",
        "libGLES",
        "libOpenGL",
        "libglapi",
        "libgbm",
        "libdrm",
        "libnvidia",
        "libcuda",
        "libxatracker",
        "libmali",
        "libpvr",
        "libdxcore",
        "libd3d12",
        "libva",
    ];
    PREFIXES.iter().any(|p| name.starts_with(p))
        || name.contains("_dri.so")
        || name.contains("mesa")
        || path.contains("/dri/")
}

fn is_font(name: &str, path: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        ".ttf", ".otf", ".ttc", ".pfb", ".pfa", ".woff", ".woff2", ".pcf", ".pcf.gz",
    ]
    .iter()
    .any(|e| lower.ends_with(e))
        || path.contains("/fontconfig/")
}

/// The group of one mapping, by its path and permissions.
pub(crate) fn classify(path: &str, perms: &str, program: &str) -> Group {
    let path = trimmed(path);
    if path.is_empty() {
        return if perms.contains('x') {
            Group::Jit
        } else {
            Group::Anonymous
        };
    }
    match path {
        "[heap]" => return Group::Heap,
        "[stack]" => return Group::Stack,
        "[anon_shmem]" => return Group::Shared,
        _ => {}
    }
    if path.starts_with("[anon:") {
        return if perms.contains('x') {
            Group::Jit
        } else {
            Group::Anonymous
        };
    }
    if path.starts_with('[') {
        return Group::Kernel;
    }
    if !program.is_empty() && path == program {
        return Group::Program;
    }
    if path.starts_with("/dev/dri/")
        || path.starts_with("/dev/nvidia")
        || path.starts_with("/dev/kgsl")
        || path.starts_with("/dev/mali")
        || path.starts_with("/dmabuf")
        || path.starts_with("anon_inode:")
    {
        return Group::Device;
    }
    if path.starts_with("/memfd:") || path.starts_with("/dev/shm/") || path.starts_with("/SYSV") {
        return Group::Shared;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.starts_with("libLLVM") {
        Group::Llvm
    } else if is_driver(name, path) {
        Group::Driver
    } else if is_font(name, path) {
        Group::Fonts
    } else if name.contains(".so") {
        Group::Libraries
    } else {
        Group::Files
    }
}

/// One group's resident bytes.
#[derive(Clone, Debug, Default)]
pub(crate) struct Tally {
    pub(crate) resident: u64,
    /// Of `resident`, anonymous: what the group wrote into its mapping.
    pub(crate) anonymous: u64,
    pub(crate) mappings: usize,
    /// By file, the most resident first.
    pub(crate) files: Vec<(String, u64)>,
}

/// What is resident by group.
#[derive(Clone, Debug, Default)]
pub(crate) struct Mappings {
    pub(crate) groups: Vec<(Group, Tally)>,
}

/// The most files listed in one group.
const MAX_FILES: usize = 5;

impl Mappings {
    /// Reads the process's own map; `None` where there is none to read.
    #[cfg(target_os = "linux")]
    pub(crate) fn now() -> Option<Mappings> {
        let smaps = std::fs::read_to_string("/proc/self/smaps").ok()?;
        let program = std::env::current_exe()
            .ok()
            .map(|p| trimmed(&p.to_string_lossy()).to_owned())
            .unwrap_or_default();
        Some(Mappings::parse(&smaps, &program))
    }

    #[cfg(not(target_os = "linux"))]
    pub(crate) fn now() -> Option<Mappings> {
        None
    }

    /// The map as `smaps` writes it: a header line for each mapping, then
    /// its counts in kB.
    pub(crate) fn parse(smaps: &str, program: &str) -> Mappings {
        use std::collections::BTreeMap;
        let mut tallies: BTreeMap<Group, (Tally, BTreeMap<String, u64>)> = BTreeMap::new();
        let mut current: Option<(Group, String)> = None;
        let kb = |line: &str| -> u64 {
            line.split_whitespace()
                .nth(1)
                .and_then(|n| n.parse::<u64>().ok())
                .unwrap_or(0)
                * 1024
        };
        for line in smaps.lines() {
            let mut fields = line.split_whitespace();
            let Some(first) = fields.next() else { continue };
            if first.ends_with(':') {
                let Some((group, path)) = &current else {
                    continue;
                };
                let (tally, files) = tallies.entry(*group).or_default();
                match first {
                    "Rss:" => {
                        let b = kb(line);
                        tally.resident += b;
                        if !path.is_empty() && b > 0 {
                            *files.entry(path.clone()).or_default() += b;
                        }
                    }
                    "Anonymous:" => tally.anonymous += kb(line),
                    _ => {}
                }
                continue;
            }
            // A header: address range, permissions, offset, device, inode,
            // then the path, which may hold spaces.
            if !first.contains('-') {
                continue;
            }
            let perms = fields.next().unwrap_or("");
            let path = line
                .splitn(6, char::is_whitespace)
                .nth(5)
                .map_or("", str::trim);
            let group = classify(path, perms, program);
            tallies.entry(group).or_default().0.mappings += 1;
            let named = if path.starts_with('/') {
                trimmed(path).to_owned()
            } else {
                String::new()
            };
            current = Some((group, named));
        }
        let mut groups: Vec<(Group, Tally)> = tallies
            .into_iter()
            .map(|(g, (mut t, files))| {
                let mut files: Vec<(String, u64)> = files.into_iter().collect();
                files.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                files.truncate(MAX_FILES);
                t.files = files;
                (g, t)
            })
            .collect();
        groups.sort_by(|a, b| b.1.resident.cmp(&a.1.resident).then(a.0.cmp(&b.0)));
        Mappings { groups }
    }

    #[cfg(test)]
    pub(crate) fn get(&self, group: Group) -> Tally {
        self.groups
            .iter()
            .find(|(g, _)| *g == group)
            .map(|(_, t)| t.clone())
            .unwrap_or_default()
    }

    pub(crate) fn resident(&self) -> u64 {
        self.groups.iter().map(|(_, t)| t.resident).sum()
    }

    /// Resident pages that are a file's or shared, by group, with what the
    /// group wrote (anonymous) left out: the program and its libraries.
    pub(crate) fn file_backed(&self) -> Vec<(Group, u64)> {
        self.groups
            .iter()
            .filter(|(g, _)| g.file_backed())
            .map(|(g, t)| (*g, t.resident.saturating_sub(t.anonymous)))
            .filter(|&(_, b)| b > 0)
            .collect()
    }

    /// Anonymous pages, by group: what the program and its libraries hold
    /// of their own.
    pub(crate) fn anonymous(&self) -> Vec<(Group, u64)> {
        self.groups
            .iter()
            .map(|(g, t)| {
                (
                    *g,
                    if g.file_backed() {
                        t.anonymous
                    } else {
                        t.resident
                    },
                )
            })
            .filter(|&(_, b)| b > 0)
            .collect()
    }

    pub(crate) fn json(&self, driver: Option<&str>) -> Value {
        json!({
            "known": true,
            "resident_bytes": self.resident(),
            "driver": driver,
            "groups": self.groups.iter().map(|(g, t)| json!({
                "group": g.key(),
                "label": g.label(),
                "resident_bytes": t.resident,
                "anonymous_bytes": t.anonymous,
                "file_backed": g.file_backed(),
                "mappings": t.mappings,
                "files": t.files.iter().map(|(p, b)| json!({
                    "path": p,
                    "resident_bytes": b,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMAPS: &str = "\
55d0c0000000-55d0c1000000 r-xp 00000000 08:01 1234 /opt/app/bin/viewer
Rss:                1024 kB
Anonymous:             0 kB
55d0c1000000-55d0c1100000 rw-p 01000000 08:01 1234 /opt/app/bin/viewer
Rss:                  64 kB
Anonymous:            64 kB
55d0c2000000-55d0c3000000 rw-p 00000000 00:00 0 [heap]
Rss:                2048 kB
Anonymous:          2048 kB
7f0000000000-7f0004000000 rw-p 00000000 00:00 0
Rss:                 512 kB
Anonymous:           512 kB
7f0010000000-7f0010100000 rwxp 00000000 00:00 0
Rss:                 128 kB
Anonymous:           128 kB
7f0020000000-7f0024000000 r-xp 00000000 08:01 99 /usr/lib/libLLVM.so.20.1
Rss:                4096 kB
Anonymous:             0 kB
7f0030000000-7f0031000000 r-xp 00000000 08:01 98 /usr/lib/libgallium-25.2.so
Rss:                 800 kB
Anonymous:             0 kB
7f0031000000-7f0031100000 rw-p 01000000 08:01 98 /usr/lib/libgallium-25.2.so
Rss:                  40 kB
Anonymous:            40 kB
7f0040000000-7f0040100000 r--s 00000000 08:01 97 /usr/share/fonts/noto/Noto Sans.ttf
Rss:                 300 kB
Anonymous:             0 kB
7f0050000000-7f0050200000 rw-s 00000000 00:01 96 /memfd:wayland-shm (deleted)
Rss:                1000 kB
Anonymous:             0 kB
7f0060000000-7f0060100000 r-xp 00000000 08:01 95 /usr/lib/libc.so.6
Rss:                 900 kB
Anonymous:             0 kB
7ffd00000000-7ffd00021000 rw-p 00000000 00:00 0 [stack]
Rss:                  32 kB
Anonymous:            32 kB
7ffd00100000-7ffd00102000 r-xp 00000000 00:00 0 [vdso]
Rss:                   8 kB
Anonymous:             0 kB
";

    #[test]
    fn each_mapping_is_counted_in_its_group() {
        let m = Mappings::parse(SMAPS, "/opt/app/bin/viewer");
        let kib = |g| m.get(g).resident / 1024;
        assert_eq!(kib(Group::Program), 1088);
        assert_eq!(m.get(Group::Program).anonymous / 1024, 64);
        assert_eq!(kib(Group::Heap), 2048);
        assert_eq!(kib(Group::Anonymous), 512);
        assert_eq!(kib(Group::Jit), 128);
        assert_eq!(kib(Group::Llvm), 4096);
        assert_eq!(kib(Group::Driver), 840);
        assert_eq!(kib(Group::Fonts), 300);
        assert_eq!(kib(Group::Shared), 1000);
        assert_eq!(kib(Group::Libraries), 900);
        assert_eq!(kib(Group::Stack), 32);
        assert_eq!(kib(Group::Kernel), 8);
        assert_eq!(m.resident() / 1024, 10_952);
        // The largest first.
        assert_eq!(m.groups[0].0, Group::Llvm);
        assert_eq!(
            m.get(Group::Fonts).files[0].0,
            "/usr/share/fonts/noto/Noto Sans.ttf"
        );
        assert_eq!(m.get(Group::Shared).files[0].0, "/memfd:wayland-shm");
    }

    #[test]
    fn what_a_library_wrote_is_its_own_not_the_files() {
        let m = Mappings::parse(SMAPS, "/opt/app/bin/viewer");
        let file: u64 = m.file_backed().iter().map(|(_, b)| b).sum();
        let own: u64 = m.anonymous().iter().map(|(_, b)| b).sum();
        assert_eq!(file + own, m.resident());
        assert!(m.anonymous().contains(&(Group::Driver, 40 * 1024)));
        assert!(m.file_backed().contains(&(Group::Driver, 800 * 1024)));
    }

    #[test]
    fn a_replaced_program_is_still_the_program() {
        assert_eq!(
            classify(
                "/opt/app/bin/viewer (deleted)",
                "r-xp",
                "/opt/app/bin/viewer"
            ),
            Group::Program
        );
        assert_eq!(
            classify("/usr/lib/dri/radeonsi_dri.so", "r-xp", ""),
            Group::Driver
        );
        assert_eq!(
            classify("/usr/lib/libvulkan_lvp.so", "r-xp", ""),
            Group::Driver
        );
        assert_eq!(classify("/dev/dri/renderD128", "rw-s", ""), Group::Device);
        assert_eq!(
            classify("/home/u/.cache/fontconfig/abc-le64.cache-9", "r--p", ""),
            Group::Fonts
        );
        assert_eq!(classify("[anon:js heap]", "rw-p", ""), Group::Anonymous);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_process_own_map_adds_up_to_about_what_is_resident() {
        let m = Mappings::now().expect("Linux has /proc/self/smaps");
        assert!(m.get(Group::Program).resident > 0);
        assert!(m.resident() > 0);
    }
}
