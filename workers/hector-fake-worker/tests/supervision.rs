use std::{ffi::OsString, path::PathBuf, sync::Mutex, time::Duration};

use hector_core::{GenerationEpoch, RequestId};
use hector_platform_windows::{
    ConfigError, ForcedTerminationReason, ReceiveOutcome, RestartCause, RestartPolicy,
    ShutdownOutcome, SupervisorEvent, SupervisorFailureKind, SupervisorState, SupervisorTimeouts,
    TerminationOutcome, TimeoutKind, WorkerProcessConfig, WorkerSupervisor,
};
use hector_protocol::{
    Envelope, GenerationCorrelation, ProtocolMessage, WorkPayload, WorkSubmission, WorkerRole,
};

static PROCESS_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn launch_handshake_and_clean_shutdown() {
    let _guard = serial_guard();
    let mut supervisor = launch("normal", RestartPolicy::new(0), None);
    assert_eq!(supervisor.state(), SupervisorState::Ready);
    assert_eq!(supervisor.role(), WorkerRole::Llm);

    let outcome = supervisor.shutdown().expect("clean shutdown succeeds");
    let ShutdownOutcome::Graceful(report) = outcome else {
        panic!("expected graceful shutdown, got {outcome:?}");
    };
    assert!(report.drained_messages().is_empty());
    assert!(report.diagnostics().bytes().is_empty());
    assert_eq!(supervisor.state(), SupervisorState::Stopped);
    assert_eq!(
        supervisor.shutdown().expect("shutdown is idempotent"),
        ShutdownOutcome::AlreadyStopped
    );
}

#[test]
#[ignore = "interactive Task Manager/Process Explorer acceptance check"]
fn manual_graceful_process_observation() {
    let _guard = serial_guard();
    let mut supervisor = launch("normal", RestartPolicy::new(0), None);
    eprintln!("H16 manual graceful case: worker is live for 12 seconds");
    std::thread::sleep(Duration::from_secs(12));
    assert!(matches!(
        supervisor.shutdown().expect("clean shutdown succeeds"),
        ShutdownOutcome::Graceful(_)
    ));
    eprintln!("H16 manual graceful case: worker stopped; observing absence for 8 seconds");
    std::thread::sleep(Duration::from_secs(8));
}

#[test]
fn shutdown_while_active_does_not_require_cancellation() {
    let _guard = serial_guard();
    let mut supervisor = launch("cancelled", RestartPolicy::new(0), None);
    supervisor.send(&work()).expect("work frame queues");
    assert!(matches!(
        supervisor.shutdown().expect("active shutdown succeeds"),
        ShutdownOutcome::Graceful(_)
    ));
}

#[test]
fn zero_receive_wait_is_rejected_without_mutating_the_worker() {
    let _guard = serial_guard();
    let mut supervisor = launch("normal", RestartPolicy::new(0), None);
    let failure = supervisor
        .receive(Duration::ZERO)
        .expect_err("zero wait is outside the bounded receive contract");
    assert_eq!(
        failure.kind(),
        &SupervisorFailureKind::InvalidReceiveWait(ConfigError::ZeroTimeout(
            TimeoutKind::ReceiveWait
        ))
    );
    assert_eq!(supervisor.state(), SupervisorState::Ready);
    assert!(matches!(
        supervisor.shutdown().expect("worker remains usable"),
        ShutdownOutcome::Graceful(_)
    ));
}

#[test]
fn crash_restarts_with_configurable_budget_and_then_exhausts() {
    let _guard = serial_guard();
    let mut supervisor = launch("crash", RestartPolicy::new(1), None);
    supervisor.send(&work()).expect("first crash work queues");
    let restarted = supervisor
        .receive(Duration::from_secs(2))
        .expect("first crash is restartable");
    let ReceiveOutcome::Event(SupervisorEvent::Restarted(report)) = restarted else {
        panic!("expected restart report, got {restarted:?}");
    };
    assert_eq!(report.restart_index(), 1);
    assert_eq!(report.cause(), RestartCause::ProcessExited { code: 70 });
    assert!(String::from_utf8_lossy(report.diagnostics().bytes()).contains("intentional crash"));

    supervisor
        .send(&work())
        .expect("replacement crash work queues");
    let failure = supervisor
        .receive(Duration::from_secs(2))
        .expect_err("second crash exhausts the configured budget");
    assert!(matches!(
        failure.kind(),
        SupervisorFailureKind::RestartExhausted {
            restarts_used: 1,
            last_cause: RestartCause::ProcessExited { code: 70 }
        }
    ));
    assert_eq!(supervisor.state(), SupervisorState::Stopped);
}

#[test]
fn zero_restart_policy_preserves_intentional_crash_code() {
    let _guard = serial_guard();
    let mut supervisor = launch("crash", RestartPolicy::new(0), None);
    supervisor.send(&work()).expect("crash work queues");
    let failure = supervisor
        .receive(Duration::from_secs(2))
        .expect_err("zero budget prevents replacement");
    assert!(matches!(
        failure.kind(),
        SupervisorFailureKind::RestartExhausted {
            restarts_used: 0,
            last_cause: RestartCause::ProcessExited { code: 70 }
        }
    ));
}

#[test]
fn restart_allowance_never_resets_after_successful_handshakes() {
    let _guard = serial_guard();
    let mut supervisor = launch("crash", RestartPolicy::new(3), None);
    for expected_index in 1..=3 {
        supervisor.send(&work()).expect("crash work queues");
        let outcome = supervisor
            .receive(Duration::from_secs(2))
            .expect("remaining allowance replaces the worker");
        let ReceiveOutcome::Event(SupervisorEvent::Restarted(report)) = outcome else {
            panic!("expected restart report, got {outcome:?}");
        };
        assert_eq!(report.restart_index(), expected_index);
    }

    supervisor.send(&work()).expect("final crash work queues");
    let failure = supervisor
        .receive(Duration::from_secs(2))
        .expect_err("the cumulative allowance remains exhausted");
    assert!(matches!(
        failure.kind(),
        SupervisorFailureKind::RestartExhausted {
            restarts_used: 3,
            last_cause: RestartCause::ProcessExited { code: 70 }
        }
    ));
}

#[test]
fn disabled_transport_silence_returns_no_traffic_until_forced_cleanup() {
    let _guard = serial_guard();
    let mut supervisor = launch("hang", RestartPolicy::new(0), None);
    supervisor.send(&work()).expect("hang work queues");
    assert_eq!(
        supervisor
            .receive(Duration::from_millis(100))
            .expect("bounded receive succeeds"),
        ReceiveOutcome::NoTraffic
    );
    let TerminationOutcome::Terminated(report) = supervisor
        .force_terminate()
        .expect("forced Job cleanup succeeds")
    else {
        panic!("expected forced termination");
    };
    assert_eq!(report.reason(), ForcedTerminationReason::Explicit);
    assert_eq!(supervisor.state(), SupervisorState::Stopped);
}

#[test]
fn explicit_transport_silence_policy_restarts_hung_worker() {
    let _guard = serial_guard();
    let mut supervisor = launch(
        "hang",
        RestartPolicy::new(1),
        Some(Duration::from_millis(150)),
    );
    supervisor.send(&work()).expect("hang work queues");
    let outcome = supervisor
        .receive(Duration::from_millis(250))
        .expect("configured silence triggers replacement");
    let ReceiveOutcome::Event(SupervisorEvent::Restarted(report)) = outcome else {
        panic!("expected restart report, got {outcome:?}");
    };
    assert_eq!(report.restart_index(), 1);
    assert_eq!(report.cause(), RestartCause::TransportSilenceTimeout);
    assert!(matches!(
        supervisor.shutdown().expect("replacement shuts down"),
        ShutdownOutcome::Graceful(_)
    ));
}

#[test]
fn malformed_and_unsupported_protocol_are_not_restarted() {
    let _guard = serial_guard();
    for scenario in ["malformed_frame", "unsupported_version"] {
        let mut supervisor = launch(scenario, RestartPolicy::new(2), None);
        supervisor.send(&work()).expect("adversarial work queues");
        let failure = supervisor
            .receive(Duration::from_secs(2))
            .expect_err("protocol violation is fatal");
        assert!(
            matches!(failure.kind(), SupervisorFailureKind::Protocol(_)),
            "{scenario} must fail at the H13 protocol boundary: {failure:?}"
        );
        assert_eq!(failure.attempts_used(), 1);
        assert_eq!(supervisor.state(), SupervisorState::Stopped);
    }
}

fn launch(
    scenario: &str,
    restart_policy: RestartPolicy,
    transport_silence: Option<Duration>,
) -> WorkerSupervisor {
    let config = config(scenario, restart_policy, transport_silence);
    WorkerSupervisor::launch(config).expect("fake worker launches and handshakes")
}

fn config(
    scenario: &str,
    restart_policy: RestartPolicy,
    transport_silence: Option<Duration>,
) -> WorkerProcessConfig {
    let timeouts = SupervisorTimeouts::new(
        Duration::from_secs(5),
        transport_silence,
        Duration::from_secs(2),
        Duration::from_secs(2),
    )
    .expect("test timeouts are valid");
    WorkerProcessConfig::new(
        PathBuf::from(env!("CARGO_BIN_EXE_hector-fake-worker")),
        vec![
            OsString::from("--role"),
            OsString::from("llm"),
            OsString::from("--scenario"),
            OsString::from(scenario),
        ],
        WorkerRole::Llm,
        None,
        timeouts,
        restart_policy,
    )
    .expect("test worker configuration is valid")
}

fn work() -> Envelope {
    let correlation = GenerationCorrelation::new(
        GenerationEpoch::from_raw(7).expect("generation is nonzero"),
        RequestId::from_raw(11).expect("request is nonzero"),
    );
    Envelope::new(ProtocolMessage::Work(WorkSubmission::new(
        correlation,
        WorkPayload::Llm {
            prompt: "H16 integration".to_owned(),
        },
    )))
}

fn serial_guard() -> std::sync::MutexGuard<'static, ()> {
    PROCESS_TEST_LOCK
        .lock()
        .expect("process-test lock is not poisoned")
}
