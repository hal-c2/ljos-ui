//! `StandardPaths` (QtCore): where the system keeps a user's documents,
//! settings and caches. On Linux these follow the XDG base directories and
//! user directories, with the fallbacks Qt uses when a variable is unset.
//!
//! What is read from the process environment is a [`PathEnv`], which a
//! test replaces with a home of its own; `root` then puts every file the
//! application touches under a directory of the test's.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ljos_engine::{Atom, Cx, QmlObject, Value, qml_impl, qml_singleton};

use crate::platform::Platform;

/// Where the user's directories are, and what the environment says of
/// them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathEnv {
    pub home: PathBuf,
    /// `XDG_*`, `TMPDIR` and `PATH`, as the process had them.
    pub vars: HashMap<String, String>,
    /// Set by tests: a path the application names, `/home/user/x`, is
    /// the file `root/home/user/x`. URLs keep the named path.
    pub root: Option<PathBuf>,
}

/// Names the user's real home in a run that moved its directories (a test
/// harness, an off-screen sandbox): nothing the run does may write in that
/// home's configuration, data, state or cache directories.
pub const GUARD_HOME_VAR: &str = "LJOS_GUARD_HOME";

const VARS: [&str; 12] = [
    "XDG_CONFIG_HOME",
    "XDG_CONFIG_DIRS",
    "XDG_DATA_HOME",
    "XDG_DATA_DIRS",
    "XDG_CACHE_HOME",
    "XDG_STATE_HOME",
    "XDG_RUNTIME_DIR",
    "TMPDIR",
    "PATH",
    "XDG_DESKTOP_DIR",
    "XDG_DOCUMENTS_DIR",
    "XDG_DOWNLOAD_DIR",
];

impl PathEnv {
    /// What the running process has.
    pub fn from_process() -> PathEnv {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let vars = VARS
            .iter()
            .filter_map(|k| Some(((*k).to_owned(), std::env::var(k).ok()?)))
            .collect();
        PathEnv {
            home,
            vars,
            root: None,
        }
    }

    /// What the running process has, kept away from a home it must not
    /// write in: a test binary's own (one under `target/*/deps`) and the
    /// one a harness names in [`GUARD_HOME_VAR`].
    pub fn for_process() -> PathEnv {
        let guard = std::env::var_os(GUARD_HOME_VAR)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute());
        let test_binary = std::env::current_exe().is_ok_and(|exe| {
            exe.parent()
                .and_then(Path::file_name)
                .is_some_and(|d| d == "deps")
        });
        PathEnv::from_process().guarded(
            guard.as_deref(),
            test_binary,
            &std::env::temp_dir().join(format!("ljos-{}", std::process::id())),
        )
    }

    /// This environment, or a fresh home under `temp` when it would write
    /// in `guard`'s directories or belongs to a test. Only the
    /// directories a home falls back to are guarded, so a harness may
    /// keep its sandbox under the user's home; a relative or empty XDG
    /// variable, which falls back to them, is what leaked before.
    fn guarded(self, guard: Option<&Path>, test_binary: bool, temp: &Path) -> PathEnv {
        let reaches = guard.is_some_and(|home| {
            let own = [".config", ".local/share", ".local/state", ".cache"].map(|d| home.join(d));
            [
                self.config_home(),
                self.data_home(),
                self.cache_home(),
                self.state_home(),
            ]
            .iter()
            .any(|dir| own.iter().any(|o| dir.starts_with(o)))
        });
        if !reaches && !test_binary {
            return self;
        }
        if reaches {
            // Said once, where the harness's log keeps it: the run goes on,
            // but whatever set its directories left one in the user's home.
            eprintln!(
                "ljos: {GUARD_HOME_VAR} is {}, and this run would write there; it writes under {} instead",
                guard.unwrap_or(Path::new("")).display(),
                temp.display()
            );
        }
        let kept = [
            "PATH",
            "TMPDIR",
            "XDG_RUNTIME_DIR",
            "XDG_CONFIG_DIRS",
            "XDG_DATA_DIRS",
        ];
        PathEnv {
            home: temp.join("ljos-home"),
            vars: self
                .vars
                .into_iter()
                .filter(|(k, _)| kept.contains(&k.as_str()))
                .collect(),
            root: self.root,
        }
    }

    /// A user whose home is `home`, with no XDG variables set, whose files
    /// live under `root`.
    pub fn sandbox(home: impl Into<PathBuf>, root: impl Into<PathBuf>) -> PathEnv {
        PathEnv {
            home: home.into(),
            vars: HashMap::new(),
            root: Some(root.into()),
        }
    }

    fn var(&self, key: &str) -> Option<&str> {
        self.vars
            .get(key)
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    }

    /// An XDG directory: the variable if it holds an absolute path, else
    /// `home/fallback`.
    fn xdg(&self, key: &str, fallback: &str) -> PathBuf {
        match self.var(key).map(PathBuf::from) {
            Some(p) if p.is_absolute() => p,
            _ => self.home.join(fallback),
        }
    }

    /// A list variable, or its default.
    fn dirs(&self, key: &str, default: &str) -> Vec<PathBuf> {
        self.var(key)
            .unwrap_or(default)
            .split(':')
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .collect()
    }

    /// The file a path the application names is.
    pub fn real(&self, path: &Path) -> PathBuf {
        match &self.root {
            Some(root) => root.join(path.strip_prefix("/").unwrap_or(path)),
            None => path.to_path_buf(),
        }
    }

    pub fn config_home(&self) -> PathBuf {
        self.xdg("XDG_CONFIG_HOME", ".config")
    }

    pub fn data_home(&self) -> PathBuf {
        self.xdg("XDG_DATA_HOME", ".local/share")
    }

    pub fn cache_home(&self) -> PathBuf {
        self.xdg("XDG_CACHE_HOME", ".cache")
    }

    pub fn state_home(&self) -> PathBuf {
        self.xdg("XDG_STATE_HOME", ".local/state")
    }
}

/// Qt's `QStandardPaths::StandardLocation`, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Location {
    Desktop,
    Documents,
    Fonts,
    Applications,
    Music,
    Movies,
    Pictures,
    Temp,
    Home,
    AppLocalData,
    Cache,
    GenericData,
    Runtime,
    Config,
    Download,
    GenericCache,
    GenericConfig,
    AppData,
    AppConfig,
    PublicShare,
    Templates,
    State,
    GenericState,
}

const LOCATIONS: [(Location, &str, &str); 23] = [
    (Location::Desktop, "DesktopLocation", "Desktop"),
    (Location::Documents, "DocumentsLocation", "Documents"),
    (Location::Fonts, "FontsLocation", "Fonts"),
    (
        Location::Applications,
        "ApplicationsLocation",
        "Applications",
    ),
    (Location::Music, "MusicLocation", "Music"),
    (Location::Movies, "MoviesLocation", "Movies"),
    (Location::Pictures, "PicturesLocation", "Pictures"),
    (Location::Temp, "TempLocation", "Temporary Directory"),
    (Location::Home, "HomeLocation", "Home"),
    (
        Location::AppLocalData,
        "AppLocalDataLocation",
        "Application Data",
    ),
    (Location::Cache, "CacheLocation", "Cache"),
    (Location::GenericData, "GenericDataLocation", "Shared Data"),
    (Location::Runtime, "RuntimeLocation", "Runtime"),
    (Location::Config, "ConfigLocation", "Configuration"),
    (Location::Download, "DownloadLocation", "Downloads"),
    (
        Location::GenericCache,
        "GenericCacheLocation",
        "Shared Cache",
    ),
    (
        Location::GenericConfig,
        "GenericConfigLocation",
        "Shared Configuration",
    ),
    // Qt 6.11 calls application data "Application Configuration" here;
    // measured with qml6, and kept so that a label reads the same.
    (
        Location::AppData,
        "AppDataLocation",
        "Application Configuration",
    ),
    (
        Location::AppConfig,
        "AppConfigLocation",
        "Application Configuration",
    ),
    (Location::PublicShare, "PublicShareLocation", "Public"),
    (Location::Templates, "TemplatesLocation", "Templates"),
    (Location::State, "StateLocation", "State"),
    (
        Location::GenericState,
        "GenericStateLocation",
        "Shared State",
    ),
];

impl Location {
    pub fn from_qt(v: i32) -> Option<Location> {
        LOCATIONS.get(usize::try_from(v).ok()?).map(|(l, ..)| *l)
    }

    pub fn display_name(self) -> &'static str {
        LOCATIONS[self as usize].2
    }

    /// Where the application writes, given its organization and name.
    pub fn writable(self, env: &PathEnv, org: &str, app: &str) -> PathBuf {
        let own = |base: PathBuf| app_dir(base, org, app);
        match self {
            // Without a user-dirs file, Qt puts these in the home itself.
            Location::Desktop | Location::PublicShare | Location::Templates => {
                user_dir(env, self).unwrap_or_else(|| env.home.clone())
            }
            Location::Documents => {
                user_dir(env, self).unwrap_or_else(|| env.home.join("Documents"))
            }
            Location::Music => user_dir(env, self).unwrap_or_else(|| env.home.join("Music")),
            Location::Movies => user_dir(env, self).unwrap_or_else(|| env.home.join("Videos")),
            Location::Pictures => user_dir(env, self).unwrap_or_else(|| env.home.join("Pictures")),
            Location::Download => user_dir(env, self).unwrap_or_else(|| env.home.join("Downloads")),
            Location::Fonts => env.data_home().join("fonts"),
            Location::Applications => env.data_home().join("applications"),
            Location::Temp => PathBuf::from(env.var("TMPDIR").unwrap_or("/tmp")),
            Location::Home => env.home.clone(),
            Location::AppLocalData | Location::AppData => own(env.data_home()),
            Location::Cache => own(env.cache_home()),
            Location::GenericData => env.data_home(),
            Location::Runtime => env
                .var("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| env.home.join(".run")),
            Location::Config | Location::GenericConfig => env.config_home(),
            Location::GenericCache => env.cache_home(),
            Location::AppConfig => own(env.config_home()),
            Location::State => own(env.state_home()),
            Location::GenericState => env.state_home(),
        }
    }

    /// Every directory looked in, the writable one first.
    pub fn all(self, env: &PathEnv, org: &str, app: &str) -> Vec<PathBuf> {
        let mut out = vec![self.writable(env, org, app)];
        let data = || env.dirs("XDG_DATA_DIRS", "/usr/local/share:/usr/share");
        let config = || env.dirs("XDG_CONFIG_DIRS", "/etc/xdg");
        match self {
            Location::GenericData => out.extend(data()),
            Location::AppLocalData | Location::AppData => {
                out.extend(data().into_iter().map(|d| app_dir(d, org, app)));
            }
            Location::Fonts => out.extend(data().into_iter().map(|d| d.join("fonts"))),
            Location::Applications => {
                out.extend(data().into_iter().map(|d| d.join("applications")));
            }
            Location::Config | Location::GenericConfig => out.extend(config()),
            Location::AppConfig => out.extend(config().into_iter().map(|d| app_dir(d, org, app))),
            _ => {}
        }
        out
    }
}

fn app_dir(base: PathBuf, org: &str, app: &str) -> PathBuf {
    let mut p = base;
    if !org.is_empty() {
        p.push(org);
    }
    if !app.is_empty() {
        p.push(app);
    }
    p
}

/// A directory the user named in `XDG_*_DIR`, or in
/// `$XDG_CONFIG_HOME/user-dirs.dirs` as `xdg-user-dirs` writes it.
fn user_dir(env: &PathEnv, location: Location) -> Option<PathBuf> {
    let key = match location {
        Location::Desktop => "XDG_DESKTOP_DIR",
        Location::Documents => "XDG_DOCUMENTS_DIR",
        Location::Download => "XDG_DOWNLOAD_DIR",
        Location::Music => "XDG_MUSIC_DIR",
        Location::Pictures => "XDG_PICTURES_DIR",
        Location::Movies => "XDG_VIDEOS_DIR",
        Location::PublicShare => "XDG_PUBLICSHARE_DIR",
        Location::Templates => "XDG_TEMPLATES_DIR",
        _ => return None,
    };
    let file = env.real(&env.config_home().join("user-dirs.dirs"));
    let text = std::fs::read_to_string(file).ok()?;
    let line = text
        .lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix(key)?.trim_start().strip_prefix('='))?;
    let value = line.trim().trim_matches('"');
    let path = match value.strip_prefix("$HOME") {
        Some(rest) => env.home.join(rest.trim_start_matches('/')),
        None => PathBuf::from(value),
    };
    path.is_absolute().then_some(path)
}

// One way of writing a file's address for every crate: the engine's.
pub use ljos_engine::{encoded_url, file_url, url_path};

/// Looks for `name` on `PATH` (or in `paths`), as an executable file.
fn find_executable(env: &PathEnv, name: &str, paths: &[PathBuf]) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let candidates: Vec<PathBuf> = if Path::new(name).is_absolute() {
        vec![PathBuf::from(name)]
    } else {
        let dirs = if paths.is_empty() {
            env.dirs("PATH", "/usr/local/bin:/usr/bin:/bin")
        } else {
            paths.to_vec()
        };
        dirs.into_iter().map(|d| d.join(name)).collect()
    };
    candidates.into_iter().find(|p| is_executable(&env.real(p)))
}

#[cfg(unix)]
fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(p: &Path) -> bool {
    p.is_file()
}

/// `StandardPaths` (QtCore).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtCore", name = "StandardPaths", enums = path_enums)]
#[qml_singleton]
pub struct StandardPaths {}

fn path_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    let locations = LOCATIONS
        .iter()
        .enumerate()
        .map(|(i, (_, name, _))| (*name, i as i64))
        .collect();
    vec![
        ("StandardLocation", locations),
        (
            "LocateOption",
            vec![("LocateFile", 0), ("LocateDirectory", 1)],
        ),
    ]
}

fn location(v: &Value) -> Option<Location> {
    Location::from_qt(v.to_number() as i32)
}

fn with_env<R>(cx: &Cx, f: impl FnOnce(&PathEnv, &str, &str) -> R) -> R {
    let platform = Platform::of(cx.engine);
    let (org, app) = platform.application(cx.engine);
    f(&platform.env(), &org, &app)
}

fn url_value(p: Option<&Path>) -> Value {
    Value::Url(Atom::from(p.map(file_url).unwrap_or_default().as_str()))
}

fn locate_in(cx: &Cx, which: &Value, name: &Value, options: &Value, all: bool) -> Vec<PathBuf> {
    let Some(loc) = location(which) else {
        return Vec::new();
    };
    let name = name.to_js_string().to_string();
    let directory = options.to_number() as i32 & 1 != 0;
    with_env(cx, |env, org, app| {
        let found = loc
            .all(env, org, app)
            .into_iter()
            .map(|d| d.join(&name))
            .filter(|p| {
                let real = env.real(p);
                if directory {
                    real.is_dir()
                } else {
                    real.exists()
                }
            });
        if all {
            found.collect()
        } else {
            found.take(1).collect()
        }
    })
}

#[qml_impl]
impl StandardPaths {
    #[qml(invokable, name = "displayName")]
    fn display_name(_cx: &Cx, which: Value) -> Atom {
        location(&which)
            .map(|l| Atom::from(l.display_name()))
            .unwrap_or_default()
    }

    #[qml(invokable, name = "writableLocation")]
    fn writable_location(cx: &Cx, which: Value) -> Value {
        let path =
            location(&which).map(|l| with_env(cx, |env, org, app| l.writable(env, org, app)));
        url_value(path.as_deref())
    }

    #[qml(invokable, name = "standardLocations")]
    fn standard_locations(cx: &Cx, which: Value) -> Value {
        let all = location(&which)
            .map(|l| with_env(cx, |env, org, app| l.all(env, org, app)))
            .unwrap_or_default();
        Value::list(all.iter().map(|p| url_value(Some(p))).collect())
    }

    #[qml(invokable, name = "findExecutable")]
    fn find_executable(cx: &Cx, name: Value, paths: Value) -> Value {
        let paths: Vec<PathBuf> = match &paths {
            Value::List(l) => l
                .iter()
                .filter_map(|v| url_path(&v.to_js_string()))
                .collect(),
            _ => Vec::new(),
        };
        let name = name.to_js_string().to_string();
        let found = with_env(cx, |env, _, _| find_executable(env, &name, &paths));
        url_value(found.as_deref())
    }

    #[qml(invokable)]
    fn locate(cx: &Cx, which: Value, name: Value, options: Value) -> Value {
        url_value(
            locate_in(cx, &which, &name, &options, false)
                .first()
                .map(PathBuf::as_path),
        )
    }

    #[qml(invokable, name = "locateAll")]
    fn locate_all(cx: &Cx, which: Value, name: Value, options: Value) -> Value {
        let found = locate_in(cx, &which, &name, &options, true);
        Value::list(found.iter().map(|p| url_value(Some(p))).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user() -> PathEnv {
        PathEnv::sandbox("/home/user", "/nowhere")
    }

    #[test]
    fn the_xdg_fallbacks_are_under_the_home() {
        let env = user();
        let w = |l: Location| l.writable(&env, "Org", "App");
        assert_eq!(w(Location::GenericConfig), Path::new("/home/user/.config"));
        assert_eq!(
            w(Location::AppConfig),
            Path::new("/home/user/.config/Org/App")
        );
        assert_eq!(w(Location::Cache), Path::new("/home/user/.cache/Org/App"));
        assert_eq!(
            w(Location::GenericState),
            Path::new("/home/user/.local/state")
        );
        assert_eq!(w(Location::Desktop), Path::new("/home/user"));
        assert_eq!(w(Location::Movies), Path::new("/home/user/Videos"));
        assert_eq!(w(Location::Temp), Path::new("/tmp"));
    }

    #[test]
    fn a_variable_moves_a_directory() {
        let mut env = user();
        env.vars.insert("XDG_CONFIG_HOME".into(), "/cfg".into());
        env.vars
            .insert("XDG_CONFIG_DIRS".into(), "/a:relative:/b".into());
        assert_eq!(
            Location::Config.all(&env, "", "App"),
            vec![PathBuf::from("/cfg"), "/a".into(), "/b".into()]
        );
    }

    #[test]
    fn a_sandboxed_path_is_under_the_root() {
        let env = user();
        assert_eq!(
            env.real(Path::new("/home/user/.config/a")),
            Path::new("/nowhere/home/user/.config/a")
        );
    }

    fn process(home: &str, vars: &[(&str, &str)]) -> PathEnv {
        PathEnv {
            home: home.into(),
            vars: vars
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
            root: None,
        }
    }

    #[test]
    fn a_test_never_writes_in_the_users_home() {
        let real = process("/home/user", &[("XDG_DATA_HOME", "/home/user/data")]);
        let env = real.clone().guarded(None, true, Path::new("/scratch"));
        assert_eq!(env.home, Path::new("/scratch/ljos-home"));
        assert_eq!(
            env.data_home(),
            Path::new("/scratch/ljos-home/.local/share")
        );
        assert!(!env.data_home().starts_with("/home/user"));
    }

    #[test]
    fn a_harness_names_the_home_nothing_may_write_in() {
        let guard = Some(Path::new("/home/user"));
        let temp = Path::new("/scratch");
        // The harness moved every directory: nothing changes.
        let moved = process(
            "/home/user/dev/target/cua/home",
            &[("XDG_DATA_HOME", "/home/user/dev/target/cua/data")],
        );
        assert_eq!(moved.clone().guarded(guard, false, temp), moved);
        // A relative variable is ignored, and its fallback is the home's.
        let leaked = process("/home/user", &[("XDG_DATA_HOME", "xdg/data")]);
        let env = leaked.guarded(guard, false, temp);
        assert_eq!(
            env.data_home(),
            Path::new("/scratch/ljos-home/.local/share")
        );
        // An application run by hand keeps the user's directories.
        let user = process("/home/user", &[]);
        assert_eq!(user.clone().guarded(None, false, temp), user);
    }

    #[test]
    fn display_names_are_qts() {
        assert_eq!(Location::Temp.display_name(), "Temporary Directory");
        assert_eq!(Location::from_qt(16), Some(Location::GenericConfig));
        assert_eq!(Location::from_qt(23), None);
        assert_eq!(Location::from_qt(-1), None);
    }
}
