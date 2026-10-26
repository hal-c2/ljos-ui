//! `app_metrics` `memory` with `mappings`: what is resident, split by the
//! mapping it is in, as the kernel's `/proc/self/smaps` tells it, or on
//! macOS the task's map as Mach walks it.
//!
//! The owners count what Ljós holds; the program's code, the libraries,
//! the GPU driver and what they allocate are outside them. Each mapping
//! names its file, so the resident pages of the program, of LLVM, of the
//! driver and of the fonts can be told apart; anonymous pages are split
//! into the allocator's heap, other anonymous maps, code compiled at run
//! time and the stack. Pages a library wrote into its own mapping (its
//! data, its relocations) are anonymous too and are counted with it.
//!
//! The map is read only when a client asks: walking it takes a few
//! milliseconds in a large program. Elsewhere than on Linux and macOS
//! nothing is known, and nothing is guessed.

use std::collections::BTreeMap;

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
    } else if name.contains(".so")
        || name.ends_with(".dylib")
        || path.contains(".framework/")
        || name.starts_with("dyld_shared_cache")
    {
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

#[cfg(any(target_os = "macos", test))]
/// One region of a macOS task's map. Mach names no anonymous region, but
/// tags what the allocator and the stacks map.
#[derive(Clone, Debug, Default)]
pub(crate) struct Region {
    /// The file mapped, or empty.
    pub(crate) path: String,
    pub(crate) exec: bool,
    /// Who mapped it: `VM_MEMORY_*`.
    pub(crate) tag: u32,
    /// Shared with other processes rather than copied on write.
    pub(crate) shared: bool,
    pub(crate) resident: u64,
    /// Of `resident`, what was written.
    pub(crate) dirtied: u64,
}

#[cfg(any(target_os = "macos", test))]
/// `VM_MEMORY_*` tags of `<mach/vm_statistics.h>`.
mod tag {
    pub(super) const MALLOC: std::ops::RangeInclusive<u32> = 1..=13;
    pub(super) const IOKIT: u32 = 21;
    pub(super) const STACK: u32 = 30;
    pub(super) const IOSURFACE: u32 = 88;
    pub(super) const IOACCELERATOR: u32 = 100;
}

#[cfg(any(target_os = "macos", test))]
/// The group of a macOS region: its file's, as on Linux, or by its tag.
pub(crate) fn region_group(region: &Region, program: &str) -> Group {
    if !region.path.is_empty() {
        let perms = if region.exec { "r-x" } else { "r--" };
        return classify(&region.path, perms, program);
    }
    match region.tag {
        t if tag::MALLOC.contains(&t) => Group::Heap,
        tag::STACK => Group::Stack,
        tag::IOKIT | tag::IOSURFACE | tag::IOACCELERATOR => Group::Device,
        _ if region.shared => Group::Shared,
        _ if region.exec => Group::Jit,
        _ => Group::Anonymous,
    }
}

/// Tallies mappings into groups.
#[derive(Default)]
struct Builder {
    tallies: BTreeMap<Group, (Tally, BTreeMap<String, u64>)>,
}

impl Builder {
    fn mapping(&mut self, group: Group) {
        self.tallies.entry(group).or_default().0.mappings += 1;
    }

    /// Resident bytes of a mapping of `path` (empty when anonymous), and
    /// how many of them are anonymous.
    fn add(&mut self, group: Group, path: &str, resident: u64, anonymous: u64) {
        let (tally, files) = self.tallies.entry(group).or_default();
        tally.resident += resident;
        tally.anonymous += anonymous;
        if !path.is_empty() && resident > 0 {
            *files.entry(path.to_owned()).or_default() += resident;
        }
    }

    fn finish(self) -> Mappings {
        let mut groups: Vec<(Group, Tally)> = self
            .tallies
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
}

/// The program's own path, as its mappings name it.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn program() -> String {
    std::env::current_exe()
        .ok()
        .map(|p| trimmed(&p.to_string_lossy()).to_owned())
        .unwrap_or_default()
}

impl Mappings {
    /// Reads the process's own map; `None` where there is none to read.
    #[cfg(target_os = "linux")]
    pub(crate) fn now() -> Option<Mappings> {
        let smaps = std::fs::read_to_string("/proc/self/smaps").ok()?;
        Some(Mappings::parse(&smaps, &program()))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn now() -> Option<Mappings> {
        let regions = super::process::regions()?;
        Some(Mappings::of_regions(&regions, &program()))
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub(crate) fn now() -> Option<Mappings> {
        None
    }

    /// A macOS task's map. A file's pages that were written are copies of
    /// the process's own, as Linux counts them anonymous.
    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn of_regions(regions: &[Region], program: &str) -> Mappings {
        let mut builder = Builder::default();
        for region in regions {
            let group = region_group(region, program);
            builder.mapping(group);
            let anonymous = if region.path.is_empty() {
                region.resident
            } else if region.shared {
                0
            } else {
                region.dirtied.min(region.resident)
            };
            builder.add(group, &region.path, region.resident, anonymous);
        }
        builder.finish()
    }

    /// The map as `smaps` writes it: a header line for each mapping, then
    /// its counts in kB.
    #[cfg(any(target_os = "linux", test))]
    pub(crate) fn parse(smaps: &str, program: &str) -> Mappings {
        let mut builder = Builder::default();
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
                match first {
                    "Rss:" => builder.add(*group, path, kb(line), 0),
                    "Anonymous:" => builder.add(*group, "", 0, kb(line)),
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
            builder.mapping(group);
            let named = if path.starts_with('/') {
                trimmed(path).to_owned()
            } else {
                String::new()
            };
            current = Some((group, named));
        }
        builder.finish()
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

    #[test]
    fn a_macos_region_is_grouped_by_its_file_or_its_tag() {
        let region = |path: &str, tag, exec, shared| Region {
            path: path.into(),
            exec,
            tag,
            shared,
            resident: 64 << 10,
            dirtied: 16 << 10,
        };
        let m = Mappings::of_regions(
            &[
                region(
                    "/Applications/Viewer.app/Contents/MacOS/viewer",
                    0,
                    true,
                    false,
                ),
                region("/usr/lib/libz.1.dylib", 0, true, false),
                region(
                    "/System/Library/dyld/dyld_shared_cache_arm64e",
                    0,
                    true,
                    true,
                ),
                region("/Library/Fonts/Arial.ttf", 0, false, true),
                region("", tag::MALLOC.start() + 6, false, false),
                region("", tag::STACK, false, false),
                region("", tag::IOSURFACE, false, true),
                region("", 0, false, true),
                region("", 0, true, false),
                region("", 0, false, false),
            ],
            "/Applications/Viewer.app/Contents/MacOS/viewer",
        );
        for group in [
            Group::Program,
            Group::Fonts,
            Group::Heap,
            Group::Stack,
            Group::Device,
            Group::Shared,
            Group::Jit,
            Group::Anonymous,
        ] {
            assert_eq!(m.get(group).resident, 64 << 10, "{group:?}");
        }
        let libraries = m.get(Group::Libraries);
        assert_eq!((libraries.resident, libraries.mappings), (128 << 10, 2));
        // What a private mapping of a file wrote is the process's own; a
        // shared one's is the file's.
        assert_eq!(m.get(Group::Program).anonymous, 16 << 10);
        assert_eq!(libraries.anonymous, 16 << 10);
        assert_eq!(m.get(Group::Heap).anonymous, 64 << 10);
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn the_process_own_map_adds_up_to_about_what_is_resident() {
        let m = Mappings::now().expect("the platform lists the process's map");
        assert!(m.get(Group::Program).resident > 0);
        assert!(m.resident() > 0);
    }
}
