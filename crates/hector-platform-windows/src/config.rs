use std::{
    ffi::{OsStr, OsString},
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    time::Duration,
};

use hector_protocol::WorkerRole;

use crate::{ConfigError, ConfigField, TimeoutKind};

const MAX_FINITE_WAIT_MILLIS: u128 = u32::MAX as u128 - 1;

/// Bounded supervisor timing configuration.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SupervisorTimeouts {
    startup: Duration,
    transport_silence: Option<Duration>,
    graceful_shutdown: Duration,
    forced_termination: Duration,
}

impl SupervisorTimeouts {
    /// Creates checked timeouts.
    pub fn new(
        startup: Duration,
        transport_silence: Option<Duration>,
        graceful_shutdown: Duration,
        forced_termination: Duration,
    ) -> Result<Self, ConfigError> {
        validate_timeout(startup, TimeoutKind::Startup)?;
        if let Some(timeout) = transport_silence {
            validate_timeout(timeout, TimeoutKind::TransportSilence)?;
        }
        validate_timeout(graceful_shutdown, TimeoutKind::GracefulShutdown)?;
        validate_timeout(forced_termination, TimeoutKind::ForcedTermination)?;
        Ok(Self {
            startup,
            transport_silence,
            graceful_shutdown,
            forced_termination,
        })
    }

    /// Returns the process-start-to-ready timeout.
    pub const fn startup(self) -> Duration {
        self.startup
    }

    /// Returns the optional complete-protocol-silence timeout.
    pub const fn transport_silence(self) -> Option<Duration> {
        self.transport_silence
    }

    /// Returns the graceful shutdown timeout.
    pub const fn graceful_shutdown(self) -> Duration {
        self.graceful_shutdown
    }

    /// Returns the forced process-tree termination timeout.
    pub const fn forced_termination(self) -> Duration {
        self.forced_termination
    }
}

impl Default for SupervisorTimeouts {
    fn default() -> Self {
        // Five seconds is deliberately conservative for a local process and
        // protocol handshake. Real model adapters may explicitly raise it.
        Self {
            startup: Duration::from_secs(5),
            // H13 has no heartbeat. Silence therefore has no health meaning
            // unless a later adapter explicitly establishes that contract.
            transport_silence: None,
            // Local H13 shutdown should be prompt, while two seconds still
            // gives a loaded Windows machine room to schedule the worker.
            graceful_shutdown: Duration::from_secs(2),
            // Job termination is a kernel operation; two seconds is a bounded
            // allowance for process teardown and pipe closure.
            forced_termination: Duration::from_secs(2),
        }
    }
}

/// Cumulative restart allowance for one supervisor lifetime.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct RestartPolicy {
    max_restarts: u8,
}

impl RestartPolicy {
    /// Creates a policy with an exact maximum restart count.
    pub const fn new(max_restarts: u8) -> Self {
        Self { max_restarts }
    }

    /// Returns the maximum number of launches after the initial attempt.
    pub const fn max_restarts(self) -> u8 {
        self.max_restarts
    }
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self::new(2)
    }
}

/// Immutable configuration for one replaceable worker process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerProcessConfig {
    executable: PathBuf,
    arguments: Vec<OsString>,
    role: WorkerRole,
    working_directory: Option<PathBuf>,
    timeouts: SupervisorTimeouts,
    restart_policy: RestartPolicy,
}

impl WorkerProcessConfig {
    /// Creates a checked process configuration.
    pub fn new(
        executable: PathBuf,
        arguments: Vec<OsString>,
        role: WorkerRole,
        working_directory: Option<PathBuf>,
        timeouts: SupervisorTimeouts,
        restart_policy: RestartPolicy,
    ) -> Result<Self, ConfigError> {
        if executable.as_os_str().is_empty() {
            return Err(ConfigError::EmptyExecutable);
        }
        if !executable.is_absolute() {
            return Err(ConfigError::RelativeExecutable);
        }
        reject_nul(executable.as_os_str(), ConfigField::Executable)?;
        for argument in &arguments {
            reject_nul(argument, ConfigField::Argument)?;
        }
        if let Some(directory) = &working_directory {
            if !directory.is_absolute() {
                return Err(ConfigError::RelativeWorkingDirectory);
            }
            reject_nul(directory.as_os_str(), ConfigField::WorkingDirectory)?;
        }

        Ok(Self {
            executable,
            arguments,
            role,
            working_directory,
            timeouts,
            restart_policy,
        })
    }

    /// Returns the explicit executable path.
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    /// Returns arguments excluding `argv[0]`.
    pub fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    /// Returns the expected worker role.
    pub const fn role(&self) -> WorkerRole {
        self.role
    }

    /// Returns the optional explicit working directory.
    pub fn working_directory(&self) -> Option<&Path> {
        self.working_directory.as_deref()
    }

    /// Returns timeout policy.
    pub const fn timeouts(&self) -> SupervisorTimeouts {
        self.timeouts
    }

    /// Returns restart policy.
    pub const fn restart_policy(&self) -> RestartPolicy {
        self.restart_policy
    }
}

pub(crate) fn duration_to_wait_millis(
    duration: Duration,
    kind: TimeoutKind,
) -> Result<u32, ConfigError> {
    validate_timeout(duration, kind)?;
    let millis = duration.as_millis().max(1);
    u32::try_from(millis).map_err(|_| ConfigError::TimeoutTooLarge(kind))
}

fn validate_timeout(duration: Duration, kind: TimeoutKind) -> Result<(), ConfigError> {
    if duration.is_zero() {
        return Err(ConfigError::ZeroTimeout(kind));
    }
    if duration.as_millis().max(1) > MAX_FINITE_WAIT_MILLIS {
        return Err(ConfigError::TimeoutTooLarge(kind));
    }
    Ok(())
}

fn reject_nul(value: &OsStr, field: ConfigField) -> Result<(), ConfigError> {
    if value.encode_wide().any(|unit| unit == 0) {
        Err(ConfigError::EmbeddedNul(field))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, path::PathBuf, time::Duration};

    use hector_protocol::WorkerRole;

    use super::{RestartPolicy, SupervisorTimeouts, WorkerProcessConfig};
    use crate::{ConfigError, ConfigField, TimeoutKind};

    #[test]
    fn restart_policy_is_configurable_and_defaults_to_two() {
        assert_eq!(RestartPolicy::default().max_restarts(), 2);
        for value in [0, 1, 3, u8::MAX] {
            assert_eq!(RestartPolicy::new(value).max_restarts(), value);
        }
    }

    #[test]
    fn timeout_defaults_disable_silence_and_are_explicit() {
        let timeouts = SupervisorTimeouts::default();
        assert_eq!(timeouts.startup(), Duration::from_secs(5));
        assert_eq!(timeouts.transport_silence(), None);
        assert_eq!(timeouts.graceful_shutdown(), Duration::from_secs(2));
        assert_eq!(timeouts.forced_termination(), Duration::from_secs(2));
    }

    #[test]
    fn timeout_validation_rejects_zero_and_unrepresentable_values() {
        assert_eq!(
            SupervisorTimeouts::new(
                Duration::ZERO,
                None,
                Duration::from_secs(1),
                Duration::from_secs(1)
            ),
            Err(ConfigError::ZeroTimeout(TimeoutKind::Startup))
        );
        assert_eq!(
            SupervisorTimeouts::new(
                Duration::from_secs(1),
                Some(Duration::ZERO),
                Duration::from_secs(1),
                Duration::from_secs(1)
            ),
            Err(ConfigError::ZeroTimeout(TimeoutKind::TransportSilence))
        );
        let too_large = Duration::from_millis(u64::from(u32::MAX));
        assert_eq!(
            SupervisorTimeouts::new(
                too_large,
                None,
                Duration::from_secs(1),
                Duration::from_secs(1)
            ),
            Err(ConfigError::TimeoutTooLarge(TimeoutKind::Startup))
        );
    }

    #[test]
    fn process_configuration_rejects_unsafe_paths_and_nuls() {
        let defaults = SupervisorTimeouts::default();
        let restart = RestartPolicy::default();
        assert_eq!(
            WorkerProcessConfig::new(
                PathBuf::new(),
                Vec::new(),
                WorkerRole::Asr,
                None,
                defaults,
                restart
            ),
            Err(ConfigError::EmptyExecutable)
        );
        assert_eq!(
            WorkerProcessConfig::new(
                PathBuf::from("worker.exe"),
                Vec::new(),
                WorkerRole::Asr,
                None,
                defaults,
                restart
            ),
            Err(ConfigError::RelativeExecutable)
        );
        assert_eq!(
            WorkerProcessConfig::new(
                PathBuf::from(r"C:\worker.exe"),
                vec![OsString::from("bad\0argument")],
                WorkerRole::Asr,
                None,
                defaults,
                restart
            ),
            Err(ConfigError::EmbeddedNul(ConfigField::Argument))
        );
    }
}
