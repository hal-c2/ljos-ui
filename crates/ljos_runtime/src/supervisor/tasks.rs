//! Supervised tokio tasks (ARCHITECTURE.md 9.4): a [`SupervisorTree`] whose
//! children are tasks in a `JoinSet`, each with its own `CancellationToken`.
//!
//! A [`TaskSupervisor`] is the `ServiceSup` or `PluginSup` of the standard
//! tree. It is driven by awaiting [`TaskSupervisor::run`] (or
//! [`TaskSupervisor::next`]), which sleeps in `JoinSet::join_next` until a
//! task ends; nothing polls. Restart intensity is measured on the
//! [`Clock`] it is given, so tests use a [`VirtualClock`](crate::VirtualClock).
//!
//! The supervisor is not `Send` (the tree is single-threaded): make it on
//! the thread that drives it, `Runtime::block_on(sup.run())` or a
//! `LocalSet`. The tasks themselves run wherever the runtime puts them.
//! When it gives up, `run` returns the reason; the owner hands that to the
//! application's supervisor through its own message path.
//!
//! Stopping a task cancels its token and does not wait: a task that is
//! restarted because a sibling failed may overlap its replacement until it
//! notices. [`TaskSupervisor::shutdown`] waits, and aborts what has not
//! ended after the grace time.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::task::{Id, JoinSet};
use tokio_util::sync::CancellationToken;

use crate::fault::Fault;
use crate::time::Clock;

use super::tree::{PLUGIN_INTENSITY, SERVICE_INTENSITY};
use super::{
    ChildId, ChildKind, ChildSpec, ExitReason, Intensity, Outcome, Restart, Strategy,
    SupervisorError, SupervisorTree,
};

/// What a supervised task is: it ends with `Ok` (a normal exit) or a fault,
/// and should end soon after its token is cancelled.
pub type TaskFuture = Pin<Box<dyn Future<Output = Result<(), Fault>> + Send + 'static>>;

/// What the tree starts and stops tasks with.
#[derive(Debug)]
pub struct TaskCx {
    set: JoinSet<Result<(), Fault>>,
    handle: Handle,
    /// Parent of every task's token, for [`TaskSupervisor::shutdown`].
    token: CancellationToken,
    /// The tasks whose end is news. A task that was stopped is taken out,
    /// so its end is not mistaken for an exit.
    live: HashMap<Id, ChildId>,
}

struct Task {
    factory: Box<dyn Fn(CancellationToken) -> TaskFuture>,
    child: Cell<Option<ChildId>>,
    running: RefCell<Option<(Id, CancellationToken)>>,
}

impl ChildSpec<TaskCx> for Task {
    fn kind(&self) -> ChildKind {
        ChildKind::Task
    }

    fn start(&self, cx: &mut TaskCx) -> Result<(), Fault> {
        let token = cx.token.child_token();
        let task = cx.set.spawn_on((self.factory)(token.clone()), &cx.handle);
        if let Some(child) = self.child.get() {
            cx.live.insert(task.id(), child);
        }
        *self.running.borrow_mut() = Some((task.id(), token));
        Ok(())
    }

    fn stop(&self, cx: &mut TaskCx, _reason: &ExitReason) {
        if let Some((id, token)) = self.running.borrow_mut().take() {
            cx.live.remove(&id);
            token.cancel();
        }
    }
}

/// One task ended and its supervisor decided.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskExit {
    pub child: ChildId,
    pub reason: ExitReason,
    pub outcome: Outcome,
}

/// See the module documentation.
pub struct TaskSupervisor {
    tree: SupervisorTree<TaskCx>,
    cx: TaskCx,
    clock: Rc<dyn Clock>,
    tasks: HashMap<ChildId, Rc<Task>>,
}

impl TaskSupervisor {
    /// A supervisor whose tasks run on `handle`'s runtime.
    pub fn new(
        name: &str,
        strategy: Strategy,
        intensity: Intensity,
        clock: Rc<dyn Clock>,
        handle: Handle,
    ) -> Self {
        Self {
            tree: SupervisorTree::new(name, strategy, intensity),
            cx: TaskCx {
                set: JoinSet::new(),
                handle,
                token: CancellationToken::new(),
                live: HashMap::new(),
            },
            clock,
            tasks: HashMap::new(),
        }
    }

    /// `ServiceSup`: one-for-one, 5 restarts in 30 s.
    pub fn services(clock: Rc<dyn Clock>, handle: Handle) -> Self {
        Self::new(
            "services",
            Strategy::OneForOne,
            SERVICE_INTENSITY,
            clock,
            handle,
        )
    }

    /// `PluginSup`: one-for-one, 3 restarts in 60 s. Spawn its tasks with
    /// [`Restart::Transient`]: a plugin that ends by itself stays ended.
    pub fn plugins(clock: Rc<dyn Clock>, handle: Handle) -> Self {
        Self::new(
            "plugins",
            Strategy::OneForOne,
            PLUGIN_INTENSITY,
            clock,
            handle,
        )
    }

    /// Starts a task. `make` is called for every start, the first and each
    /// restart, with a fresh token.
    pub fn spawn<F, Fut>(&mut self, name: &str, restart: Restart, make: F) -> ChildId
    where
        F: Fn(CancellationToken) -> Fut + 'static,
        Fut: Future<Output = Result<(), Fault>> + Send + 'static,
    {
        let task = Rc::new(Task {
            factory: Box::new(move |token| Box::pin(make(token))),
            child: Cell::new(None),
            running: RefCell::new(None),
        });
        // Added before it starts so that the start knows its id.
        let child = self
            .tree
            .add_child(self.tree.root(), name, restart, task.clone())
            .expect("the root exists and the id is new");
        task.child.set(Some(child));
        // Starting a task cannot fail: it only spawns.
        let _ = task.start(&mut self.cx);
        self.tasks.insert(child, task);
        child
    }

    /// Cancels a task and forgets it. It is not restarted.
    pub fn stop(&mut self, child: ChildId) -> Result<(), SupervisorError> {
        self.tasks.remove(&child);
        self.tree.terminate_child(child, &mut self.cx)
    }

    /// The token every task's token is a child of.
    pub fn token(&self) -> &CancellationToken {
        &self.cx.token
    }

    /// The tree, for its listing.
    pub fn tree(&self) -> &SupervisorTree<TaskCx> {
        &self.tree
    }

    /// Tasks that have not ended, the stopped ones that are still winding
    /// down included.
    pub fn running(&self) -> usize {
        self.cx.set.len()
    }

    /// Waits for a supervised task to end and applies the policy. `None`
    /// when no task is left to wait for.
    pub async fn next(&mut self) -> Option<TaskExit> {
        loop {
            let (id, reason) = match self.cx.set.join_next_with_id().await? {
                Ok((id, Ok(()))) => (id, ExitReason::Normal),
                Ok((id, Err(fault))) => (id, ExitReason::Fault(fault)),
                Err(e) => {
                    let id = e.id();
                    let reason = match e.try_into_panic() {
                        Ok(payload) => ExitReason::Fault(Fault::from_panic(payload)),
                        // Aborted by `shutdown`.
                        Err(_) => ExitReason::Shutdown,
                    };
                    (id, reason)
                }
            };
            // A task this supervisor stopped: its end is expected.
            let Some(child) = self.cx.live.remove(&id) else {
                continue;
            };
            if let Some(task) = self.tasks.get(&child) {
                task.running.borrow_mut().take();
            }
            let now = self.clock.now();
            let Ok(outcome) = self
                .tree
                .child_exited(child, reason.clone(), now, &mut self.cx)
            else {
                continue;
            };
            if !self
                .tree
                .children(self.tree.root())
                .iter()
                .any(|c| c.id == child)
            {
                // A temporary task: the tree has forgotten it.
                self.tasks.remove(&child);
            }
            return Some(TaskExit {
                child,
                reason,
                outcome,
            });
        }
    }

    /// Supervises until no task is left (`Ok`) or the supervisor gives up
    /// (`Err` with the reason of the last exit). After it gave up the
    /// tasks are cancelled; call [`TaskSupervisor::shutdown`] to wait for
    /// them.
    pub async fn run(&mut self) -> Result<(), ExitReason> {
        while let Some(exit) = self.next().await {
            if let Outcome::GaveUp { reason, .. } = exit.outcome {
                return Err(reason);
            }
        }
        Ok(())
    }

    /// Cancels every task and waits for them to end. What has not ended
    /// after `grace` is aborted. Returns how many had to be aborted.
    pub async fn shutdown(&mut self, grace: Duration) -> usize {
        self.tree.shutdown(&mut self.cx);
        self.cx.token.cancel();
        self.cx.live.clear();
        let drained = tokio::time::timeout(grace, async {
            while self.cx.set.join_next().await.is_some() {}
        })
        .await;
        if drained.is_ok() {
            return 0;
        }
        let aborted = self.cx.set.len();
        self.cx.set.shutdown().await;
        aborted
    }
}

impl std::fmt::Debug for TaskSupervisor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskSupervisor")
            .field("tree", &self.tree)
            .field("running", &self.running())
            .finish_non_exhaustive()
    }
}
