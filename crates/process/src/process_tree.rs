//! Windows process-tree lifecycle traits (design-v3 §11.4).
//!
//! Agent plugins spawn Claude Code / Codex / OpenCode child processes. The existing
//! [`ManagedProcess::kill`](crate::ManagedProcess) only terminates the direct child, so descendants
//! can outlive it (§1.3). The runtime therefore needs a contained process *tree*: every descendant
//! is bound to a Windows Job Object with `KILL_ON_JOB_CLOSE` before any plugin code executes, so a
//! Host crash or forced stop reaps the whole tree.
//!
//! This module defines the static-dispatch trait surface for that tree. The Windows Job Object
//! implementation (FFI via `PROC_THREAD_ATTRIBUTE_JOB_LIST`, named pipes, completion-port tree-empty
//! watcher) is a separate, E2E-gated piece; until it exists, `spawn_tree` must fail closed with
//! [`ProcessTreeError::TreeKillUnavailable`] rather than degrade to a plain `Command::spawn`
//! (§11.4 invariant 5). `TokioProcessSpawner` remains for leaf processes and tests.

use std::future::Future;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::ProcessSpec;

/// A observed process exit (§11.1): the direct child's exit code, when known.
///
/// `None` covers the "killed without a code" / "not yet reaped" cases used by the drain state
/// machine in §11.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessExit {
    /// The platform exit code, or `None` if the process was killed without yielding one.
    pub exit_code: Option<i32>,
}

impl ProcessExit {
    /// Constructs a process exit from a known code.
    pub const fn from_code(exit_code: i32) -> Self {
        Self {
            exit_code: Some(exit_code),
        }
    }

    /// Constructs a process exit with no known code.
    pub const fn unknown() -> Self {
        Self { exit_code: None }
    }
}

/// Failures produced while spawning, terminating or waiting on a managed process tree.
///
/// These are the tree-operation-level failures; the manager surfaces them as the
/// `TreeKillUnavailable` / `TreeCleanupTimeout` variants of its `PluginError` (§16.1).
#[derive(Debug, thiserror::Error)]
pub enum ProcessTreeError {
    /// The host platform cannot satisfy the containment path (no Job Object / attribute API / async
    /// pipe). `spawn_tree` returns this before any plugin code can run (§11.4 fail-closed).
    #[error("process tree containment is unavailable on this host: {0}")]
    TreeKillUnavailable(String),
    /// Spawning the contained tree failed.
    #[error("failed to spawn process tree: {0}")]
    SpawnFailed(String),
    /// Terminating the Job failed.
    #[error("failed to terminate process tree: {0}")]
    TerminateFailed(String),
    /// The tree did not become empty (`ActiveProcesses == 0`) before the cleanup deadline.
    #[error("process tree cleanup timed out for generation {generation}")]
    TreeEmptyTimeout { generation: u64 },
}

/// The three stdio pipe endpoints transferred exactly once to the dedicated I/O tasks (§11.5).
pub struct PluginStdio<Stdin, Stdout, Stderr> {
    /// The single stdin writer (§3.13: exactly one stdin writer per connection).
    pub stdin: Stdin,
    /// The frame reader's stdout endpoint.
    pub stdout: Stdout,
    /// The stderr drain's endpoint.
    pub stderr: Stderr,
}

/// Terminates a generation without taking exclusive ownership away from exit watchers (§11.4).
///
/// `Clone` must share the same RAII inner (the Job handle); it must not create an unmanaged
/// duplicate. The controller remains usable after `terminate_tree` so the tree-empty watcher can
/// still observe `ActiveProcesses == 0`.
pub trait ProcessTreeController: Clone + Send + Sync + 'static {
    /// Requests termination of every process currently assigned to this Job.
    fn terminate_tree(&self) -> Result<(), ProcessTreeError>;
}

/// Separates one tree into capabilities the supervisor can drive concurrently (§11.4).
///
/// The shared inner Job handle stays alive until every capability is dropped; splitting transfers
/// ownership of the stdio pipes, the controller, and the two owned futures exactly once.
pub struct ProcessTreeParts<Stdin, Stdout, Stderr, Controller, DirectExit, TreeEmpty> {
    /// Pipe endpoints transferred exactly once to the three dedicated I/O tasks.
    pub stdio: PluginStdio<Stdin, Stdout, Stderr>,
    /// Actor-owned controller used for graceful escalation and fatal termination.
    pub controller: Controller,
    /// Owned future that reports and reaps the direct Bun process independently.
    pub direct_exit: DirectExit,
    /// Owned future that proves the Job has no active processes.
    pub tree_empty: TreeEmpty,
}

/// Spawns contained process trees (§11.4).
///
/// Upper layers depend on this trait with static dispatch so tests can inject a fake tree spawner
/// without starting real child processes. Implementations must contain the tree (bind it to a Job
/// with `KILL_ON_JOB_CLOSE`) before returning, or fail with
/// [`ProcessTreeError::TreeKillUnavailable`].
pub trait ProcessTreeSpawner {
    /// Process tree type returned by this spawner.
    type ProcessTree: ManagedProcessTree;

    /// Creates the managed tree or fails without allowing an uncontained child to run.
    fn spawn_tree(&self, spec: ProcessSpec) -> Result<Self::ProcessTree, ProcessTreeError>;
}

/// Owns one generation's direct process, complete Job hierarchy, and stdio pipes (§11.4).
///
/// Implementations split exactly once into concurrently usable, owned capabilities via
/// [`into_parts`](ManagedProcessTree::into_parts); the shared inner Job handle remains alive until
/// every capability is dropped.
pub trait ManagedProcessTree {
    /// Stdin pipe type (single writer).
    type Stdin: AsyncWrite + Unpin + Send + 'static;
    /// Stdout pipe type (frame reader).
    type Stdout: AsyncRead + Unpin + Send + 'static;
    /// Stderr pipe type (drain).
    type Stderr: AsyncRead + Unpin + Send + 'static;
    /// Controller capable of terminating the whole tree.
    type Controller: ProcessTreeController;
    /// Future that reports and reaps the direct Bun process independently of descendants.
    type DirectExit: Future<Output = Result<ProcessExit, ProcessTreeError>> + Send + 'static;
    /// Future that proves the Job has no active processes (`ActiveProcesses == 0`).
    type TreeEmpty: Future<Output = Result<(), ProcessTreeError>> + Send + 'static;

    /// Consumes the aggregate owner and transfers stdio, termination, direct-exit, and tree-empty
    /// capabilities exactly once to the generation supervisor.
    #[allow(clippy::type_complexity)]
    fn into_parts(
        self,
    ) -> Result<
        ProcessTreeParts<
            Self::Stdin,
            Self::Stdout,
            Self::Stderr,
            Self::Controller,
            Self::DirectExit,
            Self::TreeEmpty,
        >,
        ProcessTreeError,
    >;
}
