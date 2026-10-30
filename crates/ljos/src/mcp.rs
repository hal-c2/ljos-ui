//! The MCP inspector of a running application (the `mcp` feature,
//! development only; ARCHITECTURE.md §19.4): what `ljos run --mcp` serves
//! for plain QML, for any [`App`](crate::App).
//!
//! With no code, the environment asks for it when the program starts:
//!
//! ```text
//! LJOS_MCP=inspect,input LJOS_MCP_DIR=/path/to/project cargo run
//! ```
//!
//! `LJOS_MCP` names the capabilities as `--mcp=` does; the token file goes
//! to `target/ljos/mcp-<pid>.json` below `LJOS_MCP_DIR`, or below the
//! working directory, where a client started in the same place looks. In
//! code, [`App::inspector`](crate::App::inspector) asks for it; the
//! variable, when set, wins, so that whoever runs the program decides.
//! A program built without the feature says so when the variable is set.
//! `LJOS_MCP_RATE` grants more calls a second than the default, as
//! `ljos run --mcp-rate` does, for a program that starts the application
//! as its own child and drives it as part of its own interface.

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

pub use ljos_dev::mcp::{
    CALLS_PER_SECOND, Cap, CapSet, DEFAULT_CALLS_PER_SECOND, ParseCapError, RateError,
};
pub use ljos_shell::{InspectorAccess, InspectorAddress, InspectorOptions};

/// The capabilities, as `ljos run --mcp=` takes them.
pub const CAPS_VAR: &str = "LJOS_MCP";
/// The directory the token file goes below.
pub const DIR_VAR: &str = "LJOS_MCP_DIR";
/// The calls a second a client is answered, as `ljos run --mcp-rate`
/// takes them.
pub const RATE_VAR: &str = "LJOS_MCP_RATE";

/// Why an inspector cannot be served as asked.
#[derive(Debug, thiserror::Error)]
pub enum InspectorError {
    #[error(transparent)]
    Caps(#[from] ParseCapError),
    #[error(
        "{0} belong to the authoring server (`ljos mcp`), not to a running application: \
         name each capability to grant, as in inspect,input,mutate,eval"
    )]
    NotForApplication(String),
    #[error(
        "`inspect` has to be granted: the other tools take the references it gives \
         (inspect, or inspect,input,mutate,eval)"
    )]
    NoInspect,
    #[error("the working directory, for the token file: {0}")]
    Dir(#[from] std::io::Error),
    #[error(transparent)]
    Rate(#[from] RateError),
}

/// An inspector to serve: who may do what, and where the token goes.
#[derive(Clone, Debug)]
pub struct Inspector {
    caps: CapSet,
    dir: Option<PathBuf>,
    calls_per_second: u32,
}

impl Inspector {
    /// An inspector whose clients hold `caps`, under the rules of
    /// `ljos run --mcp`: `inspect` is among them, and `fs` and `run`,
    /// which are the authoring server's, are not.
    pub fn new(caps: CapSet) -> Result<Self, InspectorError> {
        let apart: Vec<String> = caps
            .iter()
            .filter(|c| matches!(c, Cap::Fs | Cap::Run))
            .map(|c| c.to_string())
            .collect();
        if !apart.is_empty() {
            return Err(InspectorError::NotForApplication(apart.join(", ")));
        }
        if !caps.contains(Cap::Inspect) {
            return Err(InspectorError::NoInspect);
        }
        Ok(Self {
            caps,
            dir: None,
            calls_per_second: DEFAULT_CALLS_PER_SECOND,
        })
    }

    /// As [`new`](Self::new), from names: `inspect,input`.
    pub fn parse(list: &str) -> Result<Self, InspectorError> {
        Self::new(CapSet::parse_list(list)?)
    }

    /// What [`CAPS_VAR`], [`DIR_VAR`] and [`RATE_VAR`] ask for; `None`
    /// when the first is unset or empty.
    pub fn from_env() -> Result<Option<Self>, InspectorError> {
        Self::from_vars(
            std::env::var_os(CAPS_VAR).as_deref(),
            std::env::var_os(DIR_VAR).as_deref(),
        )?
        .map(|i| i.rate_var(std::env::var_os(RATE_VAR).as_deref()))
        .transpose()
    }

    /// [`from_env`](Self::from_env) with the values of the variables given.
    pub fn from_vars(
        caps: Option<&OsStr>,
        dir: Option<&OsStr>,
    ) -> Result<Option<Self>, InspectorError> {
        let Some(caps) = caps.filter(|c| !c.is_empty()) else {
            return Ok(None);
        };
        let inspector = Self::parse(&caps.to_string_lossy())?;
        Ok(Some(match dir.filter(|d| !d.is_empty()) {
            Some(dir) => inspector.below(dir),
            None => inspector,
        }))
    }

    /// The token file goes below `dir` rather than the working directory.
    #[must_use]
    pub fn below(mut self, dir: impl AsRef<Path>) -> Self {
        self.dir = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Clients are answered `n` calls a second rather than the default,
    /// `n` within [`CALLS_PER_SECOND`]. It grants no capability.
    pub fn calls_per_second(mut self, n: u32) -> Result<Self, InspectorError> {
        self.calls_per_second = ljos_dev::mcp::check_calls_per_second(n)?;
        Ok(self)
    }

    /// The rate [`RATE_VAR`]'s value `rate` asks for, when it is set and
    /// not empty; the rate given before otherwise.
    pub fn rate_var(mut self, rate: Option<&OsStr>) -> Result<Self, InspectorError> {
        if let Some(rate) = rate.filter(|r| !r.is_empty()) {
            self.calls_per_second = ljos_dev::mcp::parse_calls_per_second(&rate.to_string_lossy())?;
        }
        Ok(self)
    }

    pub fn caps(&self) -> CapSet {
        self.caps
    }

    /// The calls a second a client is answered.
    pub fn rate(&self) -> u32 {
        self.calls_per_second
    }

    /// What the shell is handed: a loopback port the system picks, the
    /// token file below the directory.
    pub fn options(&self) -> Result<(InspectorOptions, InspectorAccess), InspectorError> {
        let dir = match &self.dir {
            Some(dir) => dir.clone(),
            None => std::env::current_dir()?,
        };
        Ok((
            InspectorOptions::below(dir),
            InspectorAccess {
                caps: self.caps,
                calls_per_second: self.calls_per_second,
                ..InspectorAccess::default()
            },
        ))
    }

    /// Serves it on the window `setup` opens, and says where on stderr
    /// as `ljos run --mcp` does.
    pub fn serve<M: Send + 'static>(
        &self,
        setup: &mut crate::Setup<'_, M>,
    ) -> Result<InspectorAddress, crate::SetupError> {
        let (options, access) = self.options()?;
        let at = setup.serve_inspector_as(options, access, |_| {})?;
        let granted: Vec<String> = self.caps.iter().map(|c| c.to_string()).collect();
        if let Some(warning) = &at.warning {
            eprintln!("ljos: WARNING: {warning}");
        }
        eprintln!(
            "ljos: inspector ({}) at {}, token in {}",
            granted.join(","),
            at.url,
            at.token_file.display()
        );
        Ok(at)
    }
}
