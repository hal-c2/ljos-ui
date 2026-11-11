//! The one way in to making an object. A [`Request`] says what to make it
//! from, under what, with which initial properties and for whom.
//! [`Engine::make`] makes it at once; [`Engine::make_later`] gives a
//! [`CreationJob`] that makes it in slices. Both run the same steps
//! ([`sliced::Making`]) around the same request, and a hot reload keeps
//! the request to ask for the object again.

use std::ops::Deref;
use std::rc::Rc;

use ljos_runtime::{Lane, ProcId};

use super::initial::Settled;
use super::{Creation, CreationHandle, CreationJob, Initial, Template, sliced};
use crate::engine::Engine;
use crate::error::EngineError;
use crate::ids::{ContextId, ObjectId};
use crate::value::Value;

/// What a creation makes its object from.
#[derive(Clone)]
pub(crate) enum Source {
    /// A file: a [`crate::ComponentSource`] key, or `file:///key.qml`.
    Url(String),
    /// A `Component` object. `template` is the one to make it from when
    /// it is not the component's own now: the caller has it at hand, or a
    /// reload makes again what an old component made.
    Component {
        component: ObjectId,
        template: Option<Rc<Template>>,
    },
    /// A template no `Component` object holds: QML text, a singleton.
    Template(Rc<Template>),
}

/// Who asks for an object, which decides what its making belongs to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum By {
    /// Native code. A root it makes with no parent opens a singleton
    /// scope of its own.
    #[default]
    Native,
    /// A script (`createObject`, `incubateObject`). Its making is not the
    /// script's time, nor what the script reads. A root with no parent
    /// reads singletons in the scope of the script that asked. One made
    /// under an object of the tree is made again by a hot reload.
    Script,
}

/// Told the root of an object and its context once every object of it
/// exists, and before any binding of them has run.
pub trait Allocated {
    fn allocated(self, engine: &Engine, root: ObjectId, context: ContextId);
}

impl Allocated for () {
    fn allocated(self, _: &Engine, _: ObjectId, _: ContextId) {}
}

impl<F: FnOnce(&Engine, ObjectId, ContextId)> Allocated for F {
    fn allocated(self, engine: &Engine, root: ObjectId, context: ContextId) {
        self(engine, root, context);
    }
}

/// An object to make. See the module documentation.
#[must_use]
pub struct Request<A = ()> {
    pub(super) source: Source,
    pub(super) parent: Option<ObjectId>,
    /// See [`Request::in_scope`].
    pub(super) scope: Option<ObjectId>,
    pub(super) props: Vec<(String, Value)>,
    pub(super) how: Initial,
    pub(super) by: By,
    /// The root is a value of `parent`'s default property, as well as
    /// owned by it.
    pub(super) append: bool,
    pub(super) allocated: A,
}

impl<A> std::fmt::Debug for Request<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut d = f.debug_struct("Request");
        match &self.source {
            Source::Url(url) => d.field("url", url),
            Source::Component { component, .. } => d.field("component", component),
            Source::Template(_) => d.field("template", &true),
        };
        d.field("parent", &self.parent)
            .field("scope", &self.scope)
            .field("by", &self.by)
            .finish_non_exhaustive()
    }
}

impl Request {
    /// The file component at `url` (a [`crate::ComponentSource`] key, or
    /// `file:///key.qml`).
    pub fn url(url: impl Into<String>) -> Self {
        Self::from(Source::Url(url.into()))
    }

    /// What the `Component` object `component` makes.
    pub fn component(component: ObjectId) -> Self {
        Self::from(Source::Component {
            component,
            template: None,
        })
    }

    /// What `component` makes, from `template`, which the caller has at
    /// hand already.
    pub(crate) fn component_from(component: ObjectId, template: Rc<Template>) -> Self {
        Self::from(Source::Component {
            component,
            template: Some(template),
        })
    }

    /// What `template` makes, which no `Component` object holds.
    pub(crate) fn template(template: Rc<Template>) -> Self {
        Self::from(Source::Template(template))
    }

    fn from(source: Source) -> Self {
        Self {
            source,
            parent: None,
            scope: None,
            props: Vec::new(),
            how: Initial::Native,
            by: By::Native,
            append: true,
            allocated: (),
        }
    }
}

impl<A> Request<A> {
    /// Owned by `parent`, and a value of its default property when it has
    /// one.
    pub fn parent(mut self, parent: Option<ObjectId>) -> Self {
        self.parent = parent;
        self
    }

    /// `props` set before the object completes (they count for
    /// `required`), as `how` says.
    pub fn props(mut self, props: Vec<(String, Value)>, how: Initial) -> Self {
        self.props = props;
        self.how = how;
        self
    }

    /// A file made in a context of `scope`'s own, which a name the file
    /// cannot place finds the members of `scope` in, and then what `scope`
    /// finds: a Loader's item reads the Loader's members first (checked on
    /// qml6). The context goes with the object made in it. Only a file
    /// ([`Request::url`]) has a context to give.
    pub fn in_scope(mut self, scope: ObjectId) -> Self {
        self.scope = Some(scope);
        self
    }

    pub fn by(mut self, by: By) -> Self {
        self.by = by;
        self
    }

    /// A view's delegate, owned by `parent` but not its child. `setup` is
    /// the place to give the context its row and the root its visual
    /// parent.
    pub(crate) fn delegate<B: Allocated>(self, setup: B) -> Request<B> {
        let mut req = self.allocated(setup);
        req.append = false;
        req
    }

    /// `allocated` is told the root and its context once they exist.
    pub(crate) fn allocated<B: Allocated>(self, allocated: B) -> Request<B> {
        let Self {
            source,
            parent,
            scope,
            props,
            how,
            by,
            append,
            allocated: _,
        } = self;
        Request {
            source,
            parent,
            scope,
            props,
            how,
            by,
            append,
            allocated,
        }
    }

    /// The request without what it tells, and what it tells.
    pub(super) fn split(self) -> (Request, A) {
        let Self {
            source,
            parent,
            scope,
            props,
            how,
            by,
            append,
            allocated,
        } = self;
        (
            Request {
                source,
                parent,
                scope,
                props,
                how,
                by,
                append,
                allocated: (),
            },
            allocated,
        )
    }
}

impl Request {
    /// The request again, for a reload to ask with.
    pub(super) fn copy(&self) -> Self {
        Self {
            source: self.source.clone(),
            parent: self.parent,
            scope: self.scope,
            props: self.props.clone(),
            how: self.how.clone(),
            by: self.by,
            append: self.append,
            allocated: (),
        }
    }
}

/// A template a creation makes its object from: a file's, its own, or
/// one a component holds.
pub(super) enum Made {
    File(Template),
    Shared(Rc<Template>),
}

impl Deref for Made {
    type Target = Template;

    fn deref(&self) -> &Template {
        match self {
            Self::File(t) => t,
            Self::Shared(t) => t,
        }
    }
}

impl Made {
    pub(super) fn into_rc(self) -> Rc<Template> {
        match self {
            Self::File(t) => Rc::new(t),
            Self::Shared(t) => t,
        }
    }
}

impl Engine {
    /// Makes what `req` asks for at once. The object comes with what
    /// became of its initial properties.
    pub fn make<A: Allocated>(&self, req: Request<A>) -> Result<(ObjectId, Settled), EngineError> {
        match req.by {
            By::Native => self.make_now(req),
            By::Script => self.native_for_script(|| self.make_now(req)),
        }
    }

    /// A job that makes what `req` asks for in slices, for a scheduler
    /// ([`Engine::incubate`]).
    pub fn make_later<A: Allocated + 'static>(
        &self,
        req: Request<A>,
        lane: Lane,
        owner: ProcId,
    ) -> (CreationJob, CreationHandle) {
        let (mut req, allocated) = req.split();
        let record = self.reload_asked(&mut req);
        CreationJob::asked(self, req, allocated, record, lane, owner)
    }

    /// The template `source` makes its object from, and the context made
    /// for `scope`, which goes with the object once it exists.
    pub(super) fn template_of(
        &self,
        source: &Source,
        scope: Option<ObjectId>,
    ) -> Result<(Made, Option<ContextId>), EngineError> {
        match source {
            Source::Url(url) => {
                let mut t = self.file_template(url)?;
                let ctx = scope.map(|s| self.scope_context(s));
                t.context = ctx;
                Ok((Made::File(t), ctx))
            }
            Source::Component {
                template: Some(t), ..
            } => Ok((Made::Shared(t.clone()), None)),
            Source::Component {
                component,
                template: None,
            } => Ok((Made::Shared(self.component_template(*component)?), None)),
            Source::Template(t) => Ok((Made::Shared(t.clone()), None)),
        }
    }

    /// The steps of a [`CreationJob`] ([`sliced::Making`]) with nothing to
    /// stop them, on the stack.
    fn make_now<A: Allocated>(&self, req: Request<A>) -> Result<(ObjectId, Settled), EngineError> {
        let (mut req, allocated) = req.split();
        let record = self.reload_asked(&mut req);
        let scripted = record.as_ref().is_some_and(|r| r.by == By::Script);
        let Request {
            source,
            parent,
            scope,
            props,
            how,
            by,
            append,
            allocated: (),
        } = req;
        let made = self.not_script_time(|| {
            let errors = self.open_creation();
            let made = (|| {
                let (template, ctx) = self.template_of(&source, scope)?;
                let mut cr = Creation::default();
                let host = by == By::Native;
                let root = self
                    .allocate_template(&mut cr, &template, parent, append, host)
                    .inspect_err(|_| {
                        if let Some(ctx) = ctx {
                            self.destroy_context(ctx);
                        }
                    })?;
                if let Some(ctx) = ctx {
                    self.add_context(root, ctx);
                }
                if let Some(top) = &cr.top {
                    allocated.allocated(self, root, top.context);
                }
                // After `allocated`: a delegate's path has its row in it.
                self.reload_assign(&cr, root, scripted);
                let mut making = sliced::Making::new(cr, root, props, how);
                loop {
                    if let Some(r) = making.step(self, &|| false) {
                        let withheld = making.withheld();
                        return r.map(|root| (root, Settled { withheld }));
                    }
                }
            })();
            self.close_creation(errors);
            made
        });
        if let (Ok((root, _)), Some(record)) = (&made, record) {
            self.reload_note_asked(*root, record);
        }
        made
    }
}
