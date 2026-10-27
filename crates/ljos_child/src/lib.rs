//! A child process that is killed and reaped when it is dropped.
//!
//! A `?` between a spawn and the wait that ends it would otherwise leave
//! the process running, and a zombie once it ends.

use std::{
    io,
    ops::{Deref, DerefMut},
    process::{Child, Command, ExitStatus},
};

/// A child that does not outlive its owner: dropped, it is killed and
/// waited for. One that should end on its own is ended with
/// [`Reaped::finish`]. Every method of [`Child`] is reached through it,
/// `Child::wait` among them, after which the drop has nothing to do.
#[derive(Debug)]
pub struct Reaped(Option<Child>);

impl Reaped {
    /// Starts `command` as a child that is reaped on drop.
    ///
    /// # Errors
    /// The command could not be started.
    pub fn spawn(command: &mut Command) -> io::Result<Self> {
        command.spawn().map(Self::new)
    }

    #[must_use]
    pub fn new(child: Child) -> Self {
        Self(Some(child))
    }

    /// Waits for the child to end of itself.
    ///
    /// # Errors
    /// As [`Child::wait`]; the child is then still killed on drop.
    pub fn finish(mut self) -> io::Result<ExitStatus> {
        let status = self.child().wait()?;
        self.0 = None;
        Ok(status)
    }

    /// Kills the child and waits for it, so that it is gone on return.
    ///
    /// # Errors
    /// As [`Child::wait`].
    pub fn stop(&mut self) -> io::Result<ExitStatus> {
        let child = self.child();
        // Already ended is not an error of `kill`; the wait says how.
        let _ = child.kill();
        child.wait()
    }

    /// The child, no longer killed on drop.
    #[must_use]
    pub fn into_inner(mut self) -> Child {
        self.0
            .take()
            .expect("a Reaped holds its child until dropped")
    }

    fn child(&mut self) -> &mut Child {
        self.0
            .as_mut()
            .expect("a Reaped holds its child until dropped")
    }
}

impl From<Child> for Reaped {
    fn from(child: Child) -> Self {
        Self::new(child)
    }
}

impl Deref for Reaped {
    type Target = Child;

    fn deref(&self) -> &Child {
        self.0
            .as_ref()
            .expect("a Reaped holds its child until dropped")
    }
}

impl DerefMut for Reaped {
    fn deref_mut(&mut self) -> &mut Child {
        self.child()
    }
}

impl Drop for Reaped {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Stdio;

    fn sleeper() -> Command {
        let mut command = Command::new("sleep");
        command.arg("60").stdin(Stdio::null());
        command
    }

    /// The process is gone from the table: neither running nor a zombie.
    fn gone(pid: u32) -> bool {
        !std::path::Path::new(&format!("/proc/{pid}")).exists()
    }

    #[test]
    fn a_dropped_child_is_killed_and_reaped() {
        let child = Reaped::spawn(&mut sleeper()).unwrap();
        let pid = child.id();
        assert!(!gone(pid));
        drop(child);
        assert!(gone(pid), "process {pid} outlived its guard");
    }

    #[test]
    fn a_child_left_by_a_question_mark_is_reaped() {
        fn fails_after_spawning() -> io::Result<u32> {
            let child = Reaped::spawn(&mut sleeper())?;
            let pid = child.id();
            Err(io::Error::other(pid.to_string()))
        }
        let pid: u32 = fails_after_spawning()
            .unwrap_err()
            .to_string()
            .parse()
            .unwrap();
        assert!(gone(pid), "process {pid} outlived the error");
    }

    #[test]
    fn a_child_that_ends_of_itself_says_how() {
        let child = Reaped::spawn(Command::new("true").stdin(Stdio::null())).unwrap();
        assert!(child.finish().unwrap().success());
    }

    #[test]
    fn a_stopped_child_is_gone_and_stopping_again_is_harmless() {
        let mut child = Reaped::spawn(&mut sleeper()).unwrap();
        let pid = child.id();
        assert!(!child.stop().unwrap().success());
        assert!(gone(pid));
        assert!(!child.stop().unwrap().success());
    }

    #[test]
    fn an_inner_child_taken_out_is_not_killed() {
        let child = Reaped::spawn(&mut sleeper()).unwrap();
        let mut inner = child.into_inner();
        assert_eq!(inner.try_wait().unwrap(), None);
        inner.kill().unwrap();
        inner.wait().unwrap();
    }
}
