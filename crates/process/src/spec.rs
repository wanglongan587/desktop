use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// Stdio policy used when spawning a child process.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProcessStdio {
    /// Create an owned async pipe that callers can take from the managed process.
    #[default]
    Piped,
    /// Inherit the corresponding stdio stream from the parent process.
    Inherit,
    /// Connect the corresponding stdio stream to the platform null device.
    Null,
}

impl ProcessStdio {
    pub(crate) fn as_stdio(self) -> Stdio {
        match self {
            Self::Piped => Stdio::piped(),
            Self::Inherit => Stdio::inherit(),
            Self::Null => Stdio::null(),
        }
    }
}

/// How a child process inherits environment variables from the parent (design-v3 §14.3).
///
/// Agent plugin launches must not silently inherit the whole Host environment; a typed policy avoids
/// a hard-to-read bool and makes "clear, then apply only the explicitly granted allowlist" explicit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EnvironmentPolicy {
    /// Inherit the parent environment, then apply the configured overrides (leaf/test behavior).
    #[default]
    Inherit,
    /// Clear the parent environment entirely, then apply ONLY the configured overrides as an
    /// explicit allowlist (agent-launch: no inherited `PATH`/user-dir/Host env unless granted).
    ClearAndAllowlist,
}

/// Spawn configuration for one OS child process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessSpec {
    program: OsString,
    args: Vec<OsString>,
    cwd: Option<PathBuf>,
    envs: Vec<(OsString, OsString)>,
    env_policy: EnvironmentPolicy,
    stdin: ProcessStdio,
    stdout: ProcessStdio,
    stderr: ProcessStdio,
    kill_on_drop: bool,
}

impl ProcessSpec {
    pub fn new(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            envs: Vec::new(),
            env_policy: EnvironmentPolicy::default(),
            stdin: ProcessStdio::Piped,
            stdout: ProcessStdio::Piped,
            stderr: ProcessStdio::Piped,
            kill_on_drop: true,
        }
    }

    /// Appends one argument to the child process command line.
    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Appends multiple arguments to the child process command line.
    pub fn args<Args, Arg>(mut self, args: Args) -> Self
    where
        Args: IntoIterator<Item = Arg>,
        Arg: Into<OsString>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Sets the working directory for the child process.
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Adds or overrides one environment variable for the child process.
    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.envs.push((key.into(), value.into()));
        self
    }

    /// Sets the environment policy (§14.3). Defaults to [`EnvironmentPolicy::Inherit`].
    pub fn with_env_policy(mut self, policy: EnvironmentPolicy) -> Self {
        self.env_policy = policy;
        self
    }

    /// Sets the stdin policy for the child process.
    pub fn stdin(mut self, stdin: ProcessStdio) -> Self {
        self.stdin = stdin;
        self
    }

    /// Sets the stdout policy for the child process.
    pub fn stdout(mut self, stdout: ProcessStdio) -> Self {
        self.stdout = stdout;
        self
    }

    /// Sets the stderr policy for the child process.
    pub fn stderr(mut self, stderr: ProcessStdio) -> Self {
        self.stderr = stderr;
        self
    }

    /// Configures the child process to be killed when the managed handle is dropped.
    pub fn kill_on_drop(mut self) -> Self {
        self.kill_on_drop = true;
        self
    }

    /// Configures the child process to keep running when the managed handle is dropped.
    pub fn keep_alive_on_drop(mut self) -> Self {
        self.kill_on_drop = false;
        self
    }

    /// Returns the executable path or name that will be passed to the OS.
    pub fn program(&self) -> &OsStr {
        &self.program
    }

    /// Returns the configured command-line arguments in insertion order.
    pub fn args_iter(&self) -> impl Iterator<Item = &OsStr> {
        self.args.iter().map(OsString::as_os_str)
    }

    /// Returns the configured working directory, if one was set.
    pub fn cwd_path(&self) -> Option<&Path> {
        self.cwd.as_deref()
    }

    /// Returns the configured environment overrides in insertion order.
    pub fn envs(&self) -> impl Iterator<Item = (&OsStr, &OsStr)> {
        self.envs
            .iter()
            .map(|(key, value)| (key.as_os_str(), value.as_os_str()))
    }

    /// Returns the environment policy (§14.3).
    pub fn env_policy(&self) -> EnvironmentPolicy {
        self.env_policy
    }

    /// Returns the configured stdin policy.
    pub fn stdin_policy(&self) -> ProcessStdio {
        self.stdin
    }

    /// Returns the configured stdout policy.
    pub fn stdout_policy(&self) -> ProcessStdio {
        self.stdout
    }

    /// Returns the configured stderr policy.
    pub fn stderr_policy(&self) -> ProcessStdio {
        self.stderr
    }

    /// Returns whether the child process should be killed when the handle is dropped.
    pub fn should_kill_on_drop(&self) -> bool {
        self.kill_on_drop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_policy_defaults_to_inherit_and_is_settable() {
        let spec = ProcessSpec::new("bun");
        assert_eq!(spec.env_policy(), EnvironmentPolicy::Inherit);
        let spec = spec.with_env_policy(EnvironmentPolicy::ClearAndAllowlist);
        assert_eq!(spec.env_policy(), EnvironmentPolicy::ClearAndAllowlist);
    }

    #[test]
    fn env_overrides_are_preserved_alongside_policy() {
        let spec = ProcessSpec::new("bun")
            .with_env_policy(EnvironmentPolicy::ClearAndAllowlist)
            .env("ORA_PLUGIN_API", "1")
            .env("PATH", "/granted/bin");
        assert_eq!(spec.env_policy(), EnvironmentPolicy::ClearAndAllowlist);
        let envs: Vec<(&OsStr, &OsStr)> = spec.envs().collect();
        assert_eq!(envs.len(), 2);
        assert_eq!(envs[0].0, OsStr::new("ORA_PLUGIN_API"));
    }
}
