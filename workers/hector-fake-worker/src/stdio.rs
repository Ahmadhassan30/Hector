use std::{
    env,
    ffi::OsString,
    io::{self, Read, Write},
};

use hector_protocol::{
    Envelope, FrameHeader, ProtocolError, ProtocolMessage, WorkerRole, decode_frame,
    decode_payload, encode_frame,
};

use crate::{
    CliError, CliFlag, FakeDisposition, FakeLifecycle, FakeScenario, FakeWorker, FakeWorkerConfig,
    FakeWorkerError, FakeWorkerExitCode, MalformedFrameKind,
};

const CRASH_DIAGNOSTIC: &str = "hector-fake-worker: intentional crash scenario\n";
const HANG_DIAGNOSTIC: &str = "hector-fake-worker: intentional hang scenario\n";

/// A successful transport-loop outcome.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RunOutcome {
    Graceful,
    InputClosed,
    IntentionalCrash,
}

/// Runs the fake worker using process arguments and locked standard streams.
pub fn run_from_environment() -> FakeWorkerExitCode {
    let config = match parse_cli_args(env::args_os().skip(1)) {
        Ok(config) => config,
        Err(error) => {
            write_process_diagnostic(&FakeWorkerError::Cli(error));
            return FakeWorkerExitCode::InvalidConfiguration;
        }
    };

    let stdin = io::stdin();
    let stdout = io::stdout();
    let stderr = io::stderr();
    match run_with_io(config, stdin.lock(), stdout.lock(), stderr.lock()) {
        Ok(RunOutcome::Graceful | RunOutcome::InputClosed) => FakeWorkerExitCode::Success,
        Ok(RunOutcome::IntentionalCrash) => FakeWorkerExitCode::IntentionalCrash,
        Err(error) => {
            write_process_diagnostic(&error);
            error.exit_code()
        }
    }
}

fn write_process_diagnostic(error: &FakeWorkerError) {
    let _ = writeln!(io::stderr().lock(), "hector-fake-worker: {error}");
}

fn parse_cli_args<I, S>(args: I) -> Result<FakeWorkerConfig, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut arguments = args.into_iter().map(Into::into).peekable();
    let mut role = None;
    let mut scenario = None;

    while let Some(argument) = arguments.next() {
        let argument = argument
            .into_string()
            .map_err(|_| CliError::UnexpectedArgument)?;
        let flag = match argument.as_str() {
            "--role" => CliFlag::Role,
            "--scenario" => CliFlag::Scenario,
            _ if argument.starts_with("--") => return Err(CliError::UnknownFlag),
            _ => return Err(CliError::UnexpectedArgument),
        };

        let value = arguments.next().ok_or(CliError::MissingValue(flag))?;
        let value = value
            .into_string()
            .map_err(|_| CliError::UnexpectedArgument)?;
        if value.starts_with("--") {
            return Err(CliError::MissingValue(flag));
        }

        match flag {
            CliFlag::Role => {
                if role.is_some() {
                    return Err(CliError::DuplicateFlag(CliFlag::Role));
                }
                role = Some(parse_role(&value)?);
            }
            CliFlag::Scenario => {
                if scenario.is_some() {
                    return Err(CliError::DuplicateFlag(CliFlag::Scenario));
                }
                scenario =
                    Some(FakeScenario::from_name(&value).map_err(|_| CliError::UnknownScenario)?);
            }
        }
    }

    let role = role.ok_or(CliError::MissingFlag(CliFlag::Role))?;
    let scenario = scenario.ok_or(CliError::MissingFlag(CliFlag::Scenario))?;
    Ok(FakeWorkerConfig::new(role, scenario))
}

fn parse_role(value: &str) -> Result<WorkerRole, CliError> {
    match value {
        "asr" => Ok(WorkerRole::Asr),
        "llm" => Ok(WorkerRole::Llm),
        "tts" => Ok(WorkerRole::Tts),
        _ => Err(CliError::UnknownRole),
    }
}

fn run_with_io<R, W, E>(
    config: FakeWorkerConfig,
    mut input: R,
    mut output: W,
    mut diagnostics: E,
) -> Result<RunOutcome, FakeWorkerError>
where
    R: Read,
    W: Write,
    E: Write,
{
    let mut worker = FakeWorker::new(config)?;
    loop {
        let Some(envelope) = read_envelope(&mut input)? else {
            return match worker.lifecycle() {
                FakeLifecycle::Idle => Ok(RunOutcome::InputClosed),
                FakeLifecycle::AwaitingHello | FakeLifecycle::Active => {
                    Err(FakeWorkerError::InputClosed(worker.lifecycle()))
                }
                FakeLifecycle::Terminated => Ok(RunOutcome::Graceful),
            };
        };
        let result = worker.handle(envelope)?;
        let (emissions, disposition) = result.into_parts();
        for emission in emissions {
            write_envelope(&mut output, &emission)?;
        }

        match disposition {
            FakeDisposition::Continue => {}
            FakeDisposition::TerminateCleanly => return Ok(RunOutcome::Graceful),
            FakeDisposition::EmitMalformedFrame(kind) => {
                let frame = injected_frame(kind, config.role())?;
                output
                    .write_all(&frame)
                    .map_err(|_| FakeWorkerError::StdoutIo)?;
                output.flush().map_err(|_| FakeWorkerError::StdoutIo)?;
                return Ok(RunOutcome::Graceful);
            }
            FakeDisposition::Hang => {
                let _ = diagnostics.write_all(HANG_DIAGNOSTIC.as_bytes());
                let _ = diagnostics.flush();
                hang_forever();
            }
            FakeDisposition::Crash => {
                let _ = diagnostics.write_all(CRASH_DIAGNOSTIC.as_bytes());
                let _ = diagnostics.flush();
                return Ok(RunOutcome::IntentionalCrash);
            }
        }
    }
}

fn read_envelope<R: Read>(input: &mut R) -> Result<Option<Envelope>, FakeWorkerError> {
    let mut prefix = [0_u8; 4];
    let mut prefix_read = 0;
    while prefix_read < prefix.len() {
        match input.read(&mut prefix[prefix_read..]) {
            Ok(0) if prefix_read == 0 => return Ok(None),
            Ok(0) => {
                return Err(ProtocolError::TruncatedPrefix {
                    available: prefix_read,
                }
                .into());
            }
            Ok(count) => prefix_read += count,
            Err(_) => return Err(FakeWorkerError::StdinIo),
        }
    }

    let header = FrameHeader::decode(&prefix)?;
    let mut payload = vec![0_u8; header.payload_len()];
    let mut payload_read = 0;
    while payload_read < payload.len() {
        match input.read(&mut payload[payload_read..]) {
            Ok(0) => {
                return Err(ProtocolError::TruncatedPayload {
                    declared: payload.len(),
                    available: payload_read,
                }
                .into());
            }
            Ok(count) => payload_read += count,
            Err(_) => return Err(FakeWorkerError::StdinIo),
        }
    }
    Ok(Some(decode_payload(header, &payload)?))
}

fn write_envelope<W: Write>(output: &mut W, envelope: &Envelope) -> Result<(), FakeWorkerError> {
    let frame = encode_frame(envelope)?;
    output
        .write_all(&frame)
        .map_err(|_| FakeWorkerError::StdoutIo)?;
    output.flush().map_err(|_| FakeWorkerError::StdoutIo)
}

fn injected_frame(kind: MalformedFrameKind, role: WorkerRole) -> Result<Vec<u8>, ProtocolError> {
    match kind {
        MalformedFrameKind::MalformedJson => Ok(frame_literal(b"{")),
        MalformedFrameKind::UnsupportedVersion => unsupported_version_frame(role),
    }
}

fn frame_literal(payload: &[u8]) -> Vec<u8> {
    let payload_len = u32::try_from(payload.len()).expect("private payload is bounded");
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&payload_len.to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

fn unsupported_version_frame(role: WorkerRole) -> Result<Vec<u8>, ProtocolError> {
    let valid = Envelope::new(ProtocolMessage::Hello { worker: role });
    let encoded = encode_frame(&valid)?;
    let mut payload = encoded[4..].to_vec();
    const VERSION_ONE: &[u8] = b"\"protocol_version\":1";
    let Some(position) = payload
        .windows(VERSION_ONE.len())
        .position(|window| window == VERSION_ONE)
    else {
        return Err(ProtocolError::SerializationFailure);
    };
    let version_digit = position + VERSION_ONE.len() - 1;
    payload[version_digit] = b'2';
    let frame = frame_literal(&payload);
    if decode_frame(&frame) != Err(ProtocolError::UnsupportedProtocolVersion { received: 2 }) {
        return Err(ProtocolError::SerializationFailure);
    }
    Ok(frame)
}

fn hang_forever() -> ! {
    loop {
        std::thread::park();
    }
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        io::{self, Cursor, Read, Write},
    };

    use hector_core::{GenerationEpoch, RequestId};
    use hector_protocol::{
        Envelope, GenerationCorrelation, ProtocolError, ProtocolMessage, WorkPayload,
        WorkSubmission, WorkerRole, decode_frame, encode_frame,
    };

    use crate::{
        CliError, CliFlag, FakeScenario, FakeWorkerConfig, FakeWorkerError, FakeWorkerExitCode,
        RunOutcome, ScriptError,
    };

    use super::{
        CRASH_DIAGNOSTIC, injected_frame, parse_cli_args, read_envelope, run_with_io,
        unsupported_version_frame,
    };

    #[test]
    fn cli_accepts_both_exact_flag_orders() {
        for args in [
            vec!["--role", "asr", "--scenario", "normal"],
            vec!["--scenario", "normal", "--role", "asr"],
        ] {
            assert_eq!(
                parse_cli_args(args),
                Ok(FakeWorkerConfig::new(WorkerRole::Asr, FakeScenario::Normal))
            );
        }
    }

    #[test]
    fn cli_rejects_missing_duplicate_unknown_and_malformed_arguments() {
        let cases = [
            (vec![], CliError::MissingFlag(CliFlag::Role)),
            (
                vec!["--role", "asr"],
                CliError::MissingFlag(CliFlag::Scenario),
            ),
            (vec!["--role"], CliError::MissingValue(CliFlag::Role)),
            (
                vec!["--role", "--scenario", "normal"],
                CliError::MissingValue(CliFlag::Role),
            ),
            (
                vec!["--role", "asr", "--role", "llm", "--scenario", "normal"],
                CliError::DuplicateFlag(CliFlag::Role),
            ),
            (
                vec!["--role=asr", "--scenario", "normal"],
                CliError::UnknownFlag,
            ),
            (
                vec!["--role", "ASR", "--scenario", "normal"],
                CliError::UnknownRole,
            ),
            (
                vec!["--role", "asr", "--scenario", "unknown"],
                CliError::UnknownScenario,
            ),
            (
                vec!["asr", "--scenario", "normal"],
                CliError::UnexpectedArgument,
            ),
            (
                vec!["--role", "asr", "--scenario", "normal", "extra"],
                CliError::UnexpectedArgument,
            ),
        ];
        for (args, expected) in cases {
            assert_eq!(parse_cli_args(args), Err(expected));
        }
    }

    #[cfg(windows)]
    #[test]
    fn cli_rejects_non_unicode_arguments() {
        use std::os::windows::ffi::OsStringExt;

        assert_eq!(
            parse_cli_args([OsString::from_wide(&[0xd800])]),
            Err(CliError::UnexpectedArgument)
        );
    }

    #[test]
    fn frame_reader_handles_fragmentation_clean_eof_and_partial_input() {
        let hello = Envelope::new(ProtocolMessage::Hello {
            worker: WorkerRole::Asr,
        });
        let frame = encode_frame(&hello).expect("valid frame");
        let mut fragmented = OneByteReader::new(frame.clone());
        assert_eq!(read_envelope(&mut fragmented), Ok(Some(hello)));
        assert_eq!(read_envelope(&mut Cursor::new(Vec::<u8>::new())), Ok(None));

        assert_eq!(
            read_envelope(&mut Cursor::new(frame[..2].to_vec())),
            Err(FakeWorkerError::Protocol(ProtocolError::TruncatedPrefix {
                available: 2,
            }))
        );
        assert!(matches!(
            read_envelope(&mut Cursor::new(frame[..frame.len() - 1].to_vec())),
            Err(FakeWorkerError::Protocol(
                ProtocolError::TruncatedPayload { .. }
            ))
        ));
    }

    #[test]
    fn unsupported_version_changes_only_version_and_has_exact_length() {
        for role in [WorkerRole::Asr, WorkerRole::Llm, WorkerRole::Tts] {
            let valid = encode_frame(&Envelope::new(ProtocolMessage::Hello { worker: role }))
                .expect("valid envelope");
            let injected = unsupported_version_frame(role).expect("injection");
            assert_eq!(valid.len(), injected.len());
            assert_eq!(
                u32::from_be_bytes(injected[..4].try_into().expect("prefix")) as usize,
                injected.len() - 4
            );
            let differences = valid
                .iter()
                .zip(&injected)
                .filter(|(left, right)| left != right)
                .collect::<Vec<_>>();
            assert_eq!(differences, vec![(&b'1', &b'2')]);
            assert_eq!(
                decode_frame(&injected),
                Err(ProtocolError::UnsupportedProtocolVersion { received: 2 })
            );
        }
    }

    #[test]
    fn malformed_json_frame_uses_computed_complete_prefix() {
        let frame = injected_frame(super::MalformedFrameKind::MalformedJson, WorkerRole::Asr)
            .expect("bounded literal");
        assert_eq!(frame, vec![0, 0, 0, 1, b'{']);
        assert_eq!(decode_frame(&frame), Err(ProtocolError::InvalidJson));
    }

    #[test]
    fn transport_flushes_each_normal_emission_and_accepts_idle_eof() {
        let input = framed_messages([
            Envelope::new(ProtocolMessage::Hello {
                worker: WorkerRole::Llm,
            }),
            work(WorkerRole::Llm),
        ]);
        let output = CountingWriter::default();
        let outcome = run_with_io(
            FakeWorkerConfig::new(WorkerRole::Llm, FakeScenario::PartialThenComplete),
            Cursor::new(input),
            output.clone(),
            Vec::new(),
        )
        .expect("script completes and idle EOF is clean");
        assert_eq!(outcome, RunOutcome::InputClosed);
        assert_eq!(output.flushes(), 3);
        assert_eq!(decode_all(&output.bytes()).len(), 3);
    }

    #[test]
    fn transport_maps_eof_lifecycle_and_intentional_crash_exactly() {
        assert_eq!(
            run_with_io(
                FakeWorkerConfig::new(WorkerRole::Asr, FakeScenario::Normal),
                Cursor::new(Vec::new()),
                Vec::new(),
                Vec::new(),
            ),
            Err(FakeWorkerError::InputClosed(
                super::FakeLifecycle::AwaitingHello
            ))
        );

        let input = framed_messages([
            Envelope::new(ProtocolMessage::Hello {
                worker: WorkerRole::Tts,
            }),
            work(WorkerRole::Tts),
        ]);
        let mut diagnostics = Vec::new();
        assert_eq!(
            run_with_io(
                FakeWorkerConfig::new(WorkerRole::Tts, FakeScenario::Crash),
                Cursor::new(input),
                Vec::new(),
                &mut diagnostics,
            ),
            Ok(RunOutcome::IntentionalCrash)
        );
        assert_eq!(diagnostics, CRASH_DIAGNOSTIC.as_bytes());

        let active_input = framed_messages([
            Envelope::new(ProtocolMessage::Hello {
                worker: WorkerRole::Llm,
            }),
            work(WorkerRole::Llm),
        ]);
        assert_eq!(
            run_with_io(
                FakeWorkerConfig::new(WorkerRole::Llm, FakeScenario::Cancelled),
                Cursor::new(active_input),
                Vec::new(),
                Vec::new(),
            ),
            Err(FakeWorkerError::InputClosed(super::FakeLifecycle::Active))
        );
    }

    #[test]
    fn transport_distinguishes_stdin_stdout_and_protocol_failures() {
        assert_eq!(
            read_envelope(&mut FailingReader),
            Err(FakeWorkerError::StdinIo)
        );

        let input = framed_messages([Envelope::new(ProtocolMessage::Hello {
            worker: WorkerRole::Asr,
        })]);
        assert_eq!(
            run_with_io(
                FakeWorkerConfig::new(WorkerRole::Asr, FakeScenario::Normal),
                Cursor::new(input),
                FailingWriter,
                Vec::new(),
            ),
            Err(FakeWorkerError::StdoutIo)
        );

        let input = framed_messages([Envelope::new(ProtocolMessage::Hello {
            worker: WorkerRole::Asr,
        })]);
        assert_eq!(
            run_with_io(
                FakeWorkerConfig::new(WorkerRole::Asr, FakeScenario::Normal),
                Cursor::new(input),
                FlushFailingWriter,
                Vec::new(),
            ),
            Err(FakeWorkerError::StdoutIo)
        );

        let malformed = vec![0, 0, 0, 1, b'{'];
        assert_eq!(
            run_with_io(
                FakeWorkerConfig::new(WorkerRole::Asr, FakeScenario::Normal),
                Cursor::new(malformed),
                Vec::new(),
                Vec::new(),
            ),
            Err(FakeWorkerError::Protocol(ProtocolError::InvalidJson))
        );
    }

    #[test]
    fn exit_codes_are_stable_and_script_errors_are_configuration_failures() {
        assert_eq!(FakeWorkerExitCode::Success.code(), 0);
        assert_eq!(FakeWorkerExitCode::InvalidConfiguration.code(), 2);
        assert_eq!(FakeWorkerExitCode::LifecycleViolation.code(), 3);
        assert_eq!(FakeWorkerExitCode::ProtocolInputFailure.code(), 4);
        assert_eq!(FakeWorkerExitCode::StdinIoFailure.code(), 5);
        assert_eq!(FakeWorkerExitCode::StdoutIoFailure.code(), 6);
        assert_eq!(FakeWorkerExitCode::IntentionalCrash.code(), 70);
        assert_eq!(
            FakeWorkerError::Script(ScriptError::InvalidTypedPayload).exit_code(),
            FakeWorkerExitCode::InvalidConfiguration
        );
    }

    #[test]
    fn raw_scenarios_write_one_complete_injection_after_ready_and_flush_each_frame() {
        for (scenario, expected_error) in [
            (FakeScenario::MalformedFrame, ProtocolError::InvalidJson),
            (
                FakeScenario::UnsupportedVersion,
                ProtocolError::UnsupportedProtocolVersion { received: 2 },
            ),
        ] {
            let input = framed_messages([
                Envelope::new(ProtocolMessage::Hello {
                    worker: WorkerRole::Llm,
                }),
                work(WorkerRole::Llm),
            ]);
            let output = CountingWriter::default();
            assert_eq!(
                run_with_io(
                    FakeWorkerConfig::new(WorkerRole::Llm, scenario),
                    Cursor::new(input),
                    output.clone(),
                    Vec::new(),
                ),
                Ok(RunOutcome::Graceful)
            );
            assert_eq!(output.flushes(), 2);

            let bytes = output.bytes();
            let first_len = u32::from_be_bytes(bytes[..4].try_into().expect("prefix")) as usize;
            let first_end = 4 + first_len;
            assert!(matches!(
                decode_frame(&bytes[..first_end])
                    .expect("ready frame")
                    .message(),
                ProtocolMessage::Ready {
                    worker: WorkerRole::Llm
                }
            ));
            assert_eq!(decode_frame(&bytes[first_end..]), Err(expected_error));
        }
    }

    fn work(role: WorkerRole) -> Envelope {
        let correlation = GenerationCorrelation::new(
            GenerationEpoch::from_raw(2).expect("nonzero"),
            RequestId::from_raw(3).expect("nonzero"),
        );
        let payload = match role {
            WorkerRole::Asr => unreachable!("stdio tests do not need ASR work"),
            WorkerRole::Llm => WorkPayload::Llm {
                prompt: "prompt".to_owned(),
            },
            WorkerRole::Tts => WorkPayload::Tts {
                text: "text".to_owned(),
            },
        };
        Envelope::new(ProtocolMessage::Work(WorkSubmission::new(
            correlation,
            payload,
        )))
    }

    fn framed_messages<const N: usize>(messages: [Envelope; N]) -> Vec<u8> {
        messages
            .into_iter()
            .flat_map(|message| encode_frame(&message).expect("valid message"))
            .collect()
    }

    fn decode_all(bytes: &[u8]) -> Vec<Envelope> {
        let mut remaining = bytes;
        let mut envelopes = Vec::new();
        while !remaining.is_empty() {
            let length = u32::from_be_bytes(remaining[..4].try_into().expect("prefix")) as usize;
            let frame_len = 4 + length;
            envelopes.push(decode_frame(&remaining[..frame_len]).expect("valid output frame"));
            remaining = &remaining[frame_len..];
        }
        envelopes
    }

    struct OneByteReader {
        bytes: Cursor<Vec<u8>>,
    }

    impl OneByteReader {
        fn new(bytes: Vec<u8>) -> Self {
            Self {
                bytes: Cursor::new(bytes),
            }
        }
    }

    impl Read for OneByteReader {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let limit = output.len().min(1);
            self.bytes.read(&mut output[..limit])
        }
    }

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("injected input failure"))
        }
    }

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("injected output failure"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("injected flush failure"))
        }
    }

    struct FlushFailingWriter;

    impl Write for FlushFailingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::other("injected flush failure"))
        }
    }

    #[derive(Clone, Default)]
    struct CountingWriter {
        state: std::sync::Arc<std::sync::Mutex<CountingWriterState>>,
    }

    #[derive(Default)]
    struct CountingWriterState {
        bytes: Vec<u8>,
        flushes: usize,
    }

    impl CountingWriter {
        fn bytes(&self) -> Vec<u8> {
            self.state.lock().expect("test mutex").bytes.clone()
        }

        fn flushes(&self) -> usize {
            self.state.lock().expect("test mutex").flushes
        }
    }

    impl Write for CountingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.state
                .lock()
                .expect("test mutex")
                .bytes
                .extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.state.lock().expect("test mutex").flushes += 1;
            Ok(())
        }
    }
}
