#[cfg(test)]
use std::sync::atomic::AtomicU8;
use std::{
    collections::VecDeque,
    ffi::c_void,
    mem::{size_of, zeroed},
    os::windows::io::AsRawHandle,
    ptr,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use hector_protocol::{Envelope, FrameHeader, ProtocolError, decode_payload};
use windows_sys::Win32::{
    Foundation::{
        ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF, ERROR_OPERATION_ABORTED, ERROR_WRITE_FAULT,
        GetLastError, HANDLE, HANDLE_FLAG_INHERIT, SetHandleInformation,
    },
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem::{ReadFile, WriteFile},
    System::{IO::CancelSynchronousIo, Pipes::CreatePipe},
};

use crate::{
    DiagnosticSnapshot, PipeStream, PlatformFailure, SupervisorFailureKind, ThreadKind,
    Win32Operation,
    handle::{OwnedHandle, last_error},
};

const FRAME_CHANNEL_CAPACITY: usize = 8;
const STDERR_CAPTURE_LIMIT: usize = 65_536;
#[cfg(test)]
static TEST_THREAD_SPAWN_FAILURE: AtomicU8 = AtomicU8::new(0);

pub(crate) struct ParentPipeHandles {
    pub(crate) stdin_write: OwnedHandle,
    pub(crate) stdout_read: OwnedHandle,
    pub(crate) stderr_read: OwnedHandle,
}

pub(crate) struct ChildPipeHandles {
    pub(crate) stdin_read: OwnedHandle,
    pub(crate) stdout_write: OwnedHandle,
    pub(crate) stderr_write: OwnedHandle,
}

impl ChildPipeHandles {
    pub(crate) fn as_array(&self) -> [HANDLE; 3] {
        [
            self.stdin_read.raw(),
            self.stdout_write.raw(),
            self.stderr_write.raw(),
        ]
    }
}

pub(crate) fn create_standard_pipes()
-> Result<(ParentPipeHandles, ChildPipeHandles), PlatformFailure> {
    let (stdin_read, stdin_write) = create_pipe()?;
    let (stdout_read, stdout_write) = create_pipe()?;
    let (stderr_read, stderr_write) = create_pipe()?;
    clear_inherit(&stdin_write)?;
    clear_inherit(&stdout_read)?;
    clear_inherit(&stderr_read)?;
    Ok((
        ParentPipeHandles {
            stdin_write,
            stdout_read,
            stderr_read,
        },
        ChildPipeHandles {
            stdin_read,
            stdout_write,
            stderr_write,
        },
    ))
}

fn create_pipe() -> Result<(OwnedHandle, OwnedHandle), PlatformFailure> {
    // SAFETY: zero is a valid base initialization and all fields are set
    // before the structure is passed to Win32.
    let mut attributes: SECURITY_ATTRIBUTES = unsafe { zeroed() };
    attributes.nLength =
        u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).expect("SECURITY_ATTRIBUTES fits in u32");
    attributes.bInheritHandle = 1;
    let mut read = ptr::null_mut();
    let mut write = ptr::null_mut();
    // SAFETY: both output pointers and the initialized security attributes are
    // valid for the duration of the call.
    if unsafe { CreatePipe(&raw mut read, &raw mut write, &raw const attributes, 0) } == 0 {
        return Err(last_error(Win32Operation::CreatePipe));
    }
    Ok((
        OwnedHandle::new(read, Win32Operation::CreatePipe)?,
        OwnedHandle::new(write, Win32Operation::CreatePipe)?,
    ))
}

fn clear_inherit(handle: &OwnedHandle) -> Result<(), PlatformFailure> {
    // SAFETY: `handle` remains live and uniquely owned; the operation only
    // clears its inheritance flag.
    if unsafe { SetHandleInformation(handle.raw(), HANDLE_FLAG_INHERIT, 0) } == 0 {
        Err(last_error(Win32Operation::SetHandleInformation))
    } else {
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(crate) enum IoTerminal {
    Eof(PipeStream),
    Platform(PipeStream, PlatformFailure),
    Protocol(ProtocolError),
    OutputBackpressure,
    ThreadPanicked(ThreadKind),
}

pub(crate) enum IoReceive {
    Envelope(Envelope),
    Timeout,
    Terminal(IoTerminal),
}

pub(crate) struct IoThreads {
    input: Option<SyncSender<Vec<u8>>>,
    output: Receiver<Envelope>,
    stop: Arc<AtomicBool>,
    terminal: Arc<Mutex<Option<IoTerminal>>>,
    diagnostics: Arc<Mutex<DiagnosticBuffer>>,
    last_activity: Arc<Mutex<Instant>>,
    joins: Vec<(ThreadKind, JoinHandle<()>)>,
}

impl IoThreads {
    pub(crate) fn start(
        pipes: ParentPipeHandles,
        initial_activity: Instant,
    ) -> Result<Self, SupervisorFailureKind> {
        let (input_sender, input_receiver) = mpsc::sync_channel(FRAME_CHANNEL_CAPACITY);
        let (output_sender, output_receiver) = mpsc::sync_channel(FRAME_CHANNEL_CAPACITY);
        let stop = Arc::new(AtomicBool::new(false));
        let terminal = Arc::new(Mutex::new(None));
        let diagnostics = Arc::new(Mutex::new(DiagnosticBuffer::new()));
        let last_activity = Arc::new(Mutex::new(initial_activity));
        let start_gate = Arc::new((Mutex::new(None), Condvar::new()));
        let mut joins = Vec::with_capacity(3);

        joins.push((
            ThreadKind::StdinWriter,
            spawn_thread(ThreadKind::StdinWriter, {
                let stop = Arc::clone(&stop);
                let terminal = Arc::clone(&terminal);
                let activity = Arc::clone(&last_activity);
                let start_gate = Arc::clone(&start_gate);
                move || {
                    if wait_for_start(&start_gate) {
                        writer_loop(
                            pipes.stdin_write,
                            input_receiver,
                            &stop,
                            &terminal,
                            &activity,
                        );
                    }
                }
            })?,
        ));
        joins.push((
            ThreadKind::StdoutReader,
            match spawn_thread(ThreadKind::StdoutReader, {
                let stop = Arc::clone(&stop);
                let terminal = Arc::clone(&terminal);
                let activity = Arc::clone(&last_activity);
                let start_gate = Arc::clone(&start_gate);
                move || {
                    if wait_for_start(&start_gate) {
                        reader_loop(
                            pipes.stdout_read,
                            output_sender,
                            &stop,
                            &terminal,
                            &activity,
                        );
                    }
                }
            }) {
                Ok(join) => join,
                Err(error) => {
                    stop.store(true, Ordering::Release);
                    drop(input_sender);
                    release_start_gate(&start_gate, false);
                    for (_, join) in joins {
                        let _ = join.join();
                    }
                    return Err(error);
                }
            },
        ));
        joins.push((
            ThreadKind::StderrReader,
            match spawn_thread(ThreadKind::StderrReader, {
                let stop = Arc::clone(&stop);
                let terminal = Arc::clone(&terminal);
                let diagnostics = Arc::clone(&diagnostics);
                let start_gate = Arc::clone(&start_gate);
                move || {
                    if wait_for_start(&start_gate) {
                        diagnostics_loop(pipes.stderr_read, &stop, &terminal, &diagnostics);
                    }
                }
            }) {
                Ok(join) => join,
                Err(error) => {
                    stop.store(true, Ordering::Release);
                    drop(input_sender);
                    release_start_gate(&start_gate, false);
                    for (_, join) in joins {
                        let _ = join.join();
                    }
                    return Err(error);
                }
            },
        ));
        release_start_gate(&start_gate, true);

        Ok(Self {
            input: Some(input_sender),
            output: output_receiver,
            stop,
            terminal,
            diagnostics,
            last_activity,
            joins,
        })
    }

    pub(crate) fn send(&self, frame: Vec<u8>) -> Result<(), SupervisorFailureKind> {
        let Some(input) = &self.input else {
            return Err(SupervisorFailureKind::InputBackpressure);
        };
        match input.try_send(frame) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(SupervisorFailureKind::InputBackpressure),
            Err(TrySendError::Disconnected(_)) => Err(SupervisorFailureKind::ThreadPanicked(
                ThreadKind::StdinWriter,
            )),
        }
    }

    pub(crate) fn receive(&self, timeout: Duration) -> IoReceive {
        match self.output.recv_timeout(timeout) {
            Ok(envelope) => IoReceive::Envelope(envelope),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let terminal = self
                    .terminal
                    .lock()
                    .expect("terminal mutex is not poisoned")
                    .clone();
                terminal.map_or(IoReceive::Timeout, IoReceive::Terminal)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let terminal = self
                    .terminal
                    .lock()
                    .expect("terminal mutex is not poisoned")
                    .clone()
                    .unwrap_or(IoTerminal::ThreadPanicked(ThreadKind::StdoutReader));
                IoReceive::Terminal(terminal)
            }
        }
    }

    pub(crate) fn diagnostics(&self) -> DiagnosticSnapshot {
        self.diagnostics
            .lock()
            .expect("diagnostic mutex is not poisoned")
            .snapshot()
    }

    pub(crate) fn silence_elapsed(&self) -> Duration {
        self.last_activity
            .lock()
            .expect("activity mutex is not poisoned")
            .elapsed()
    }

    pub(crate) fn stop_and_join(&mut self) -> Result<(), ThreadKind> {
        self.stop.store(true, Ordering::Release);
        self.input.take();
        for (_, join) in &self.joins {
            cancel_thread_io(join);
        }
        let mut panic = None;
        for (kind, join) in self.joins.drain(..) {
            if join.join().is_err() {
                panic.get_or_insert(kind);
            }
        }
        panic.map_or(Ok(()), Err)
    }
}

impl Drop for IoThreads {
    fn drop(&mut self) {
        // A dropped JoinHandle detaches its thread. Always drive the same
        // bounded cancellation-and-join path, including during unwinding and
        // cleanup-error propagation.
        let _ = self.stop_and_join();
    }
}

fn wait_for_start(gate: &(Mutex<Option<bool>>, Condvar)) -> bool {
    let (state, ready) = gate;
    let mut state = state.lock().expect("thread-start gate is not poisoned");
    while state.is_none() {
        state = ready
            .wait(state)
            .expect("thread-start gate is not poisoned");
    }
    state.expect("thread-start gate was resolved")
}

fn release_start_gate(gate: &(Mutex<Option<bool>>, Condvar), start: bool) {
    let (state, ready) = gate;
    *state.lock().expect("thread-start gate is not poisoned") = Some(start);
    ready.notify_all();
}

fn spawn_thread(
    kind: ThreadKind,
    body: impl FnOnce() + Send + 'static,
) -> Result<JoinHandle<()>, SupervisorFailureKind> {
    #[cfg(test)]
    if TEST_THREAD_SPAWN_FAILURE
        .compare_exchange(
            thread_kind_code(kind),
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_ok()
    {
        return Err(SupervisorFailureKind::ThreadSpawn(kind));
    }
    let name = match kind {
        ThreadKind::StdinWriter => "hector-worker-stdin",
        ThreadKind::StdoutReader => "hector-worker-stdout",
        ThreadKind::StderrReader => "hector-worker-stderr",
    };
    thread::Builder::new()
        .name(name.to_owned())
        .spawn(body)
        .map_err(|_| SupervisorFailureKind::ThreadSpawn(kind))
}

#[cfg(test)]
const fn thread_kind_code(kind: ThreadKind) -> u8 {
    match kind {
        ThreadKind::StdinWriter => 1,
        ThreadKind::StdoutReader => 2,
        ThreadKind::StderrReader => 3,
    }
}

fn writer_loop(
    handle: OwnedHandle,
    input: Receiver<Vec<u8>>,
    stop: &AtomicBool,
    terminal: &Mutex<Option<IoTerminal>>,
    activity: &Mutex<Instant>,
) {
    while !stop.load(Ordering::Acquire) {
        let Ok(frame) = input.recv() else {
            return;
        };
        if let Err(error) = write_all(handle.raw(), &frame, Win32Operation::WritePipe) {
            if !stop.load(Ordering::Acquire) {
                set_terminal(terminal, IoTerminal::Platform(PipeStream::Stdin, error));
            }
            return;
        }
        *activity.lock().expect("activity mutex is not poisoned") = Instant::now();
    }
}

fn reader_loop(
    handle: OwnedHandle,
    output: SyncSender<Envelope>,
    stop: &AtomicBool,
    terminal: &Mutex<Option<IoTerminal>>,
    activity: &Mutex<Instant>,
) {
    loop {
        if stop.load(Ordering::Acquire) {
            return;
        }
        let mut prefix = [0u8; 4];
        match read_exact(handle.raw(), &mut prefix, Win32Operation::ReadPipe) {
            Ok(ReadStatus::Complete) => {}
            Ok(ReadStatus::Eof { read: 0 }) => {
                set_terminal(terminal, IoTerminal::Eof(PipeStream::Stdout));
                return;
            }
            Ok(ReadStatus::Eof { read }) => {
                set_terminal(
                    terminal,
                    IoTerminal::Protocol(ProtocolError::TruncatedPrefix { available: read }),
                );
                return;
            }
            Err(_error) if stop.load(Ordering::Acquire) => return,
            Err(error) => {
                set_terminal(terminal, IoTerminal::Platform(PipeStream::Stdout, error));
                return;
            }
        }

        let header = match FrameHeader::decode(&prefix) {
            Ok(header) => header,
            Err(error) => {
                set_terminal(terminal, IoTerminal::Protocol(error));
                return;
            }
        };
        let mut payload = vec![0u8; header.payload_len()];
        match read_exact(handle.raw(), &mut payload, Win32Operation::ReadPipe) {
            Ok(ReadStatus::Complete) => {}
            Ok(ReadStatus::Eof { read }) => {
                set_terminal(
                    terminal,
                    IoTerminal::Protocol(ProtocolError::TruncatedPayload {
                        declared: payload.len(),
                        available: read,
                    }),
                );
                return;
            }
            Err(_error) if stop.load(Ordering::Acquire) => return,
            Err(error) => {
                set_terminal(terminal, IoTerminal::Platform(PipeStream::Stdout, error));
                return;
            }
        }
        let envelope = match decode_payload(header, &payload) {
            Ok(envelope) => envelope,
            Err(error) => {
                set_terminal(terminal, IoTerminal::Protocol(error));
                return;
            }
        };
        *activity.lock().expect("activity mutex is not poisoned") = Instant::now();
        match output.try_send(envelope) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                set_terminal(terminal, IoTerminal::OutputBackpressure);
                return;
            }
            Err(TrySendError::Disconnected(_)) => return,
        }
    }
}

fn diagnostics_loop(
    handle: OwnedHandle,
    stop: &AtomicBool,
    terminal: &Mutex<Option<IoTerminal>>,
    diagnostics: &Mutex<DiagnosticBuffer>,
) {
    let mut chunk = [0u8; 4096];
    loop {
        if stop.load(Ordering::Acquire) {
            return;
        }
        match read_some(handle.raw(), &mut chunk, Win32Operation::ReadPipe) {
            Ok(Some(read)) => diagnostics
                .lock()
                .expect("diagnostic mutex is not poisoned")
                .push(&chunk[..read]),
            Ok(None) => return,
            Err(_error) if stop.load(Ordering::Acquire) => return,
            Err(error) => {
                set_terminal(terminal, IoTerminal::Platform(PipeStream::Stderr, error));
                return;
            }
        }
    }
}

enum ReadStatus {
    Complete,
    Eof { read: usize },
}

fn read_exact(
    handle: HANDLE,
    output: &mut [u8],
    operation: Win32Operation,
) -> Result<ReadStatus, PlatformFailure> {
    let mut offset = 0usize;
    while offset < output.len() {
        match read_some(handle, &mut output[offset..], operation)? {
            Some(read) => offset += read,
            None => return Ok(ReadStatus::Eof { read: offset }),
        }
    }
    Ok(ReadStatus::Complete)
}

fn read_some(
    handle: HANDLE,
    output: &mut [u8],
    operation: Win32Operation,
) -> Result<Option<usize>, PlatformFailure> {
    let requested = u32::try_from(output.len()).unwrap_or(u32::MAX);
    let mut read = 0u32;
    // SAFETY: the output slice is writable for `requested` bytes, the count
    // pointer is valid, and synchronous operation uses no OVERLAPPED state.
    let success = unsafe {
        ReadFile(
            handle,
            output.as_mut_ptr(),
            requested,
            &raw mut read,
            ptr::null_mut(),
        )
    };
    if success != 0 {
        return if read == 0 {
            Ok(None)
        } else {
            Ok(Some(read as usize))
        };
    }
    // SAFETY: `GetLastError` reads thread-local error state immediately after
    // the failed call.
    let code = unsafe { GetLastError() };
    if matches!(
        code,
        ERROR_BROKEN_PIPE | ERROR_HANDLE_EOF | ERROR_OPERATION_ABORTED
    ) {
        Ok(None)
    } else {
        Err(PlatformFailure::new(operation, code))
    }
}

fn write_all(
    handle: HANDLE,
    mut input: &[u8],
    operation: Win32Operation,
) -> Result<(), PlatformFailure> {
    while !input.is_empty() {
        let requested = u32::try_from(input.len()).unwrap_or(u32::MAX);
        let mut written = 0u32;
        // SAFETY: the input slice is readable for `requested` bytes and the
        // count pointer is valid for synchronous `WriteFile`.
        if unsafe {
            WriteFile(
                handle,
                input.as_ptr(),
                requested,
                &raw mut written,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(last_error(operation));
        }
        if written == 0 {
            return Err(PlatformFailure::new(operation, ERROR_WRITE_FAULT));
        }
        input = &input[written as usize..];
    }
    Ok(())
}

fn set_terminal(terminal: &Mutex<Option<IoTerminal>>, value: IoTerminal) {
    let mut terminal = terminal.lock().expect("terminal mutex is not poisoned");
    if terminal.is_none() {
        *terminal = Some(value);
    }
}

fn cancel_thread_io(join: &JoinHandle<()>) {
    let raw = join.as_raw_handle().cast::<c_void>();
    // SAFETY: `raw` is the live OS thread handle borrowed from `JoinHandle`.
    // Cancellation is advisory and cleanup still joins the owned thread.
    unsafe {
        CancelSynchronousIo(raw);
    }
}

#[derive(Debug)]
struct DiagnosticBuffer {
    bytes: VecDeque<u8>,
    discarded: u64,
}

impl DiagnosticBuffer {
    fn new() -> Self {
        Self {
            bytes: VecDeque::with_capacity(STDERR_CAPTURE_LIMIT),
            discarded: 0,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.bytes.len() == STDERR_CAPTURE_LIMIT {
                self.bytes.pop_front();
                self.discarded = self.discarded.saturating_add(1);
            }
            self.bytes.push_back(byte);
        }
    }

    fn snapshot(&self) -> DiagnosticSnapshot {
        DiagnosticSnapshot::new(self.bytes.iter().copied().collect(), self.discarded)
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::atomic::Ordering, time::Instant};

    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};

    use super::{
        DiagnosticBuffer, IoThreads, STDERR_CAPTURE_LIMIT, TEST_THREAD_SPAWN_FAILURE,
        create_standard_pipes, thread_kind_code,
    };
    use crate::{SupervisorFailureKind, ThreadKind};

    #[test]
    fn diagnostics_keep_newest_bounded_bytes_and_count_discards() {
        let mut diagnostics = DiagnosticBuffer::new();
        let input = vec![7u8; STDERR_CAPTURE_LIMIT + 17];
        diagnostics.push(&input);
        let snapshot = diagnostics.snapshot();
        assert_eq!(snapshot.bytes().len(), STDERR_CAPTURE_LIMIT);
        assert_eq!(snapshot.discarded_bytes(), 17);
    }

    #[test]
    fn partial_thread_construction_joins_started_threads_and_closes_pipes() {
        // Warm thread-builder and standard-library process state before taking
        // the exact handle baseline.
        let (parent, child) = create_standard_pipes().expect("warmup pipes open");
        let io = IoThreads::start(parent, Instant::now()).expect("warmup threads start");
        drop(child);
        drop(io);

        for kind in [
            ThreadKind::StdinWriter,
            ThreadKind::StdoutReader,
            ThreadKind::StderrReader,
        ] {
            let handles_before = current_process_handle_count();
            TEST_THREAD_SPAWN_FAILURE.store(thread_kind_code(kind), Ordering::Release);
            let (parent, child) = create_standard_pipes().expect("test pipes open");
            let failure = match IoThreads::start(parent, Instant::now()) {
                Ok(_) => panic!("injected thread failure must stop construction"),
                Err(failure) => failure,
            };
            drop(child);
            assert_eq!(failure, SupervisorFailureKind::ThreadSpawn(kind));
            assert_eq!(
                current_process_handle_count(),
                handles_before,
                "partial thread startup must retain no handles for {kind:?}"
            );
        }
    }

    fn current_process_handle_count() -> u32 {
        let mut count = 0;
        // SAFETY: the pseudo-handle is valid for the current process and
        // `count` is a valid output pointer.
        assert_ne!(
            unsafe { GetProcessHandleCount(GetCurrentProcess(), &raw mut count) },
            0
        );
        count
    }
}
