//! [`App`]: the repeated start of an application behind one builder.
//!
//! Every application that runs on the shell today writes the same
//! lines: make a window configuration from a title and a size,
//! register each native module, hand the Engine its compiled
//! components, put any context state in scope, and create the root
//! component. [`App`] is those lines once. What varies stays visible:
//! the modules, the components and the root are the parameters, the
//! message handler is a parameter, and [`App::run_with`] hands the
//! caller the shell's full [`Setup`](crate::Setup) after the repeated
//! part, for everything the builder does not cover (a watcher, an
//! inspector, a timer hook — development hosts keep those).
//!
//! ```no_run
//! # mod my_module { pub fn register(_: &ljos::Engine) {} }
//! # fn components() -> ljos::MapSource { todo!() }
//! ljos::App::new("Hello")
//!     .size(640.0, 420.0)
//!     .register(my_module::register)
//!     .source(components())
//!     .root("Main")
//!     .run()
//!     .unwrap();
//! ```
//!
//! The same builder builds headlessly ([`App::build`]), which is how
//! an application tests its own start: the test crosses the same
//! interface as the running program, minus the window.
//!
//! A shipped application gives [`App::source`] components compiled
//! ahead of time (a `build.rs`, as the toolkit's own styles do), so
//! it links no QML parser. Compiling a directory of `.qml` files at
//! start-up is development-only: it is behind the non-default
//! `dev-qml` feature as [`crate::dev::compile_dir`]. On the builder,
//! [`App::qml_dir`] compiles against the application's own native
//! types too — the ones [`App::register`] collected before the
//! `qml_dir` call — so the QML may `import` the application's
//! modules; the bare `compile_dir` knows the table of types alone.
//!
//! Neither the builder nor its parameters name an egui type; the
//! shell's loop, and its 0 % CPU idle, stay the shell's business.

use ljos_engine::{Engine, EngineError, MapSource, ObjectId, Value};

/// Puts one context property in scope on the Engine being built.
type PutContext = Box<dyn FnOnce(&Engine)>;
/// Handles one message from a worker thread, on the Engine thread.
type MessageHandler<M> = Box<dyn FnMut(&Engine, M) + 'static>;

/// The start of an application: configuration, native modules,
/// compiled components, context state and the root component.
///
/// `M` is the type of message other threads post to the Engine
/// through the handle the shell's setup hands out; applications
/// without worker threads leave it as `()`.
pub struct App<M = ()> {
    title: String,
    size: [f32; 2],
    #[cfg(feature = "shell")]
    shell_config: Option<crate::ShellConfig>,
    modules: Vec<fn(&Engine)>,
    source: Option<MapSource>,
    root: Option<String>,
    context: Vec<PutContext>,
    on_message: Option<MessageHandler<M>>,
}

impl<M> std::fmt::Debug for App<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("title", &self.title)
            .field("size", &self.size)
            .field("modules", &self.modules.len())
            .field("source", &self.source.is_some())
            .field("root", &self.root)
            .field("context", &self.context.len())
            .field("on_message", &self.on_message.is_some())
            .finish_non_exhaustive()
    }
}

impl App<()> {
    /// An application whose window (for a root that is a plain item,
    /// or one that leaves it unsaid) is called `title`. It starts
    /// with no worker-thread messages; [`on_message`](App::on_message)
    /// chooses their type.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            size: [800.0, 600.0],
            #[cfg(feature = "shell")]
            shell_config: None,
            modules: Vec::new(),
            source: None,
            root: None,
            context: Vec::new(),
            on_message: None,
        }
    }
}

impl<M> App<M> {
    /// The window's initial size in logical pixels, for a root that
    /// does not say its own.
    #[must_use]
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = [width, height];
        self
    }

    /// A whole shell configuration, replacing the title and size the
    /// builder would make one from. For the settings with no builder
    /// shorthand: the background colour, the inbox bounds, fonts.
    #[cfg(feature = "shell")]
    #[must_use]
    pub fn shell_config(mut self, config: crate::ShellConfig) -> Self {
        self.shell_config = Some(config);
        self
    }

    /// Registers one native module when the Engine is built. A
    /// module is a `register` function — the one seam every native
    /// module sits at, the toolkit's own included — and an
    /// application calls this once per module it is made of.
    #[must_use]
    pub fn register(mut self, module: fn(&Engine)) -> Self {
        self.modules.push(module);
        self
    }

    /// The application's compiled components, by key (`"Main"` for
    /// `Main.qml`). A shipped application fills the [`MapSource`]
    /// ahead of time; see the module documentation.
    #[must_use]
    pub fn source(mut self, source: MapSource) -> Self {
        self.source = Some(source);
        self
    }

    /// The component to make as the root: the key [`App::source`]
    /// knows it by.
    #[must_use]
    pub fn root(mut self, key: impl Into<String>) -> Self {
        self.root = Some(key.into());
        self
    }

    /// Compiles the QML directory `dir` now (development only, the
    /// `dev-qml` feature) and makes `root` its root component.
    ///
    /// The compile knows the application's own native types, not
    /// only the table of types, so the QML may `import` a module
    /// the application registers and create objects of its types —
    /// with no oracle named by the user. [`register`](App::register)
    /// calls must come before this call for their types to be
    /// importable: the types are snapshotted when `qml_dir` runs
    /// (on a throwaway Engine holding the standard modules and
    /// every module collected so far, so a module's `register`
    /// function runs once for the snapshot and again on the real
    /// Engine). A module registered afterwards is, to this
    /// compile, an unknown type and fails as compile errors do.
    #[cfg(feature = "dev-qml")]
    pub fn qml_dir(
        mut self,
        dir: impl AsRef<std::path::Path>,
        root: impl Into<String>,
    ) -> Result<Self, String> {
        // The same throwaway snapshot the BDD backend and the
        // gallery's `--example` compile against: the standard
        // registrations the final Engine gets (`crate::engine`),
        // plus every module this builder has collected so far.
        let snapshot = crate::engine();
        for module in &self.modules {
            module(&snapshot);
        }
        self.source = Some(crate::dev::compile_dir_with(dir.as_ref(), &snapshot)?);
        self.root = Some(root.into());
        Ok(self)
    }

    /// Puts `name` in scope for the root and everything made after
    /// it (`setContextProperty`): the state a thin QML view reads
    /// before it has items of its own. The value is made when the
    /// Engine is built, after the modules are registered, so it can
    /// be an object of a native type:
    ///
    /// ```no_run
    /// # use ljos::prelude::*;
    /// # mod backend { pub fn register(_: &ljos::Engine) {} }
    /// let app = ljos::App::new("Hello")
    ///     .register(backend::register)
    ///     .context_property("backend", |engine| {
    ///         Value::Object(engine.create_by_name("Example", "Backend", None).unwrap())
    ///     });
    /// ```
    #[must_use]
    pub fn context_property(
        mut self,
        name: impl Into<String>,
        value: impl FnOnce(&Engine) -> Value + 'static,
    ) -> Self {
        let name = name.into();
        self.context.push(Box::new(move |engine: &Engine| {
            let value = value(engine);
            engine.set_context_property(engine.root_context(), &name, value);
        }));
        self
    }

    /// Runs on the Engine thread for every message a worker thread
    /// posts through the shell's handle, and so chooses the message
    /// type `N` of the application. Only the shell has an inbox; a
    /// headless [`build`](App::build) drops the handler.
    #[must_use]
    pub fn on_message<N>(self, handler: impl FnMut(&Engine, N) + 'static) -> App<N> {
        let Self {
            title,
            size,
            #[cfg(feature = "shell")]
            shell_config,
            modules,
            source,
            root,
            context,
            ..
        } = self;
        App {
            title,
            size,
            #[cfg(feature = "shell")]
            shell_config,
            modules,
            source,
            root,
            context,
            on_message: Some(Box::new(handler)),
        }
    }

    /// The Engine the application starts with, without a root: the
    /// standard modules, then this application's, then its
    /// components and context state.
    pub fn build_engine(self) -> Engine {
        let Self {
            modules,
            source,
            context,
            ..
        } = self;
        let engine = crate::engine();
        prepare(&engine, modules, source, context);
        engine
    }

    /// The Engine and its root object, without a window: what a
    /// test or a host with a loop of its own opens the application
    /// with. Fails as creating the root fails, and when no root was
    /// named.
    pub fn build(self) -> Result<(Engine, ObjectId), EngineError> {
        let root = self.root.clone();
        let engine = self.build_engine();
        let root = root.ok_or_else(|| {
            EngineError::Load("the application names no root component (App::root)".to_owned())
        })?;
        let object = engine.create_from_url(&root, None, Vec::new())?;
        Ok((engine, object))
    }

    /// Opens the application's windows and runs until the last of
    /// them is closed: the modules, components and context state go
    /// in, and the root component is created.
    ///
    /// Call it on the main thread: some platforms allow a loop
    /// nowhere else.
    #[cfg(feature = "shell")]
    pub fn run(self) -> Result<(), crate::ShellError>
    where
        M: Send + 'static,
    {
        let Some(root) = self.root.clone() else {
            return Err(crate::ShellError::Setup(
                "the application names no root component (App::root)".into(),
            ));
        };
        self.run_with(move |setup| {
            setup
                .engine()
                .create_from_url(&root, None, Vec::new())
                .map_err(|e| -> crate::SetupError { e.to_string().into() })
        })
    }

    /// As [`run`](App::run), with the root built by `setup` from the
    /// shell's full setup — the escape hatch. The builder's part
    /// (modules, components, context state, message handler) is
    /// already in when `setup` runs; what it makes is the root, and
    /// anything it sets on the setup afterwards wins over the
    /// builder's — a second `on_message` replaces the handler.
    #[cfg(feature = "shell")]
    pub fn run_with(
        self,
        setup: impl FnOnce(&mut crate::Setup<'_, M>) -> Result<ObjectId, crate::SetupError> + 'static,
    ) -> Result<(), crate::ShellError>
    where
        M: Send + 'static,
    {
        let Self {
            title,
            size,
            shell_config,
            modules,
            source,
            context,
            mut on_message,
            ..
        } = self;
        let config = shell_config
            .unwrap_or_else(|| crate::ShellConfig::new(title).with_size(size[0], size[1]));
        crate::run(config, move |s| {
            // The shell's Engine already holds the standard modules;
            // `prepare` must not register them a second time, so it
            // is the same function `build_engine` uses on its own.
            prepare(s.engine(), modules, source, context);
            if let Some(handler) = on_message.take() {
                s.on_message(handler);
            }
            setup(s)
        })
    }
}

/// Registers the application's modules, hands over its components
/// and puts its context state in scope, in that order: a context
/// value may be an object of a type a module just registered, and
/// the root — made next — sees all of it.
fn prepare(
    engine: &Engine,
    modules: Vec<fn(&Engine)>,
    source: Option<MapSource>,
    context: Vec<PutContext>,
) {
    for module in modules {
        module(engine);
    }
    if let Some(source) = source {
        engine.set_component_source(source);
    }
    for put in context {
        put(engine);
    }
}

/// Gives `engine` the components `source` and creates the one at
/// `root`: the two lines every setup closure that shows compiled
/// QML writes, for a caller that stays with [`crate::run`] and its
/// setup rather than the builder.
pub fn create_root(
    engine: &Engine,
    source: MapSource,
    root: &str,
) -> Result<ObjectId, EngineError> {
    engine.set_component_source(source);
    engine.create_from_url(root, None, Vec::new())
}
