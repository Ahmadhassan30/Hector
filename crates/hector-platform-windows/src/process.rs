use std::{
    ffi::{OsStr, c_void},
    mem::{size_of, zeroed},
    os::windows::ffi::OsStrExt,
    ptr,
    time::Duration,
};

use windows_sys::Win32::{
    Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::{
        JobObjects::AssignProcessToJobObject,
        Threading::{
            CREATE_NO_WINDOW, CREATE_SUSPENDED, CreateProcessW, DeleteProcThreadAttributeList,
            EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, InitializeProcThreadAttributeList,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION, ResumeThread,
            STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute,
            WaitForSingleObject,
        },
    },
};

use crate::{
    PlatformFailure, Win32Operation, WorkerProcessConfig,
    handle::{OwnedHandle, last_error},
    job::{FORCED_TERMINATION_EXIT_CODE, Job},
    stdio::{ParentPipeHandles, create_standard_pipes},
};
#[cfg(test)]
use std::sync::atomic::{AtomicU8, Ordering};

#[cfg(test)]
static TEST_LAUNCH_FAILURE_POINT: AtomicU8 = AtomicU8::new(0);

pub(crate) struct SuspendedProcess {
    pub(crate) job: Job,
    pub(crate) process: Process,
    pub(crate) primary_thread: OwnedHandle,
    pub(crate) parent_pipes: ParentPipeHandles,
}

#[derive(Debug)]
pub(crate) struct Process {
    handle: OwnedHandle,
}

impl Process {
    pub(crate) const fn raw(&self) -> HANDLE {
        self.handle.raw()
    }

    pub(crate) fn wait(&self, timeout: Duration) -> Result<Option<u32>, PlatformFailure> {
        let millis = duration_to_millis_saturating(timeout);
        // SAFETY: the handle remains live throughout the wait.
        match unsafe { WaitForSingleObject(self.raw(), millis) } {
            WAIT_OBJECT_0 => self.exit_code().map(Some),
            WAIT_TIMEOUT => Ok(None),
            _ => Err(last_error(Win32Operation::WaitForProcess)),
        }
    }

    pub(crate) fn exit_code_if_exited(&self) -> Result<Option<u32>, PlatformFailure> {
        self.wait(Duration::ZERO)
    }

    fn exit_code(&self) -> Result<u32, PlatformFailure> {
        let mut code = 0;
        // SAFETY: `code` is a valid output pointer and the process handle is
        // live until the call returns.
        if unsafe { GetExitCodeProcess(self.raw(), &raw mut code) } == 0 {
            Err(last_error(Win32Operation::QueryExitCode))
        } else {
            Ok(code)
        }
    }
}

impl SuspendedProcess {
    pub(crate) fn create(config: &WorkerProcessConfig) -> Result<Self, PlatformFailure> {
        let job = Job::new()?;
        #[cfg(test)]
        inject_launch_failure(1, Win32Operation::CreateJob)?;
        let (parent_pipes, child_pipes) = create_standard_pipes()?;
        #[cfg(test)]
        inject_launch_failure(2, Win32Operation::CreatePipe)?;
        let mut attribute_list = AttributeList::new(child_pipes.as_array())?;
        #[cfg(test)]
        inject_launch_failure(3, Win32Operation::InitializeAttributeList)?;

        let executable = wide_null(config.executable().as_os_str());
        let mut command_line =
            build_command_line(config.executable().as_os_str(), config.arguments());
        command_line.push(0);
        let working_directory = config
            .working_directory()
            .map(|path| wide_null(path.as_os_str()));
        let working_directory_ptr = working_directory
            .as_ref()
            .map_or(ptr::null(), |path| path.as_ptr());

        // SAFETY: zero is the documented initialization for these output and
        // optional-field Win32 structures.
        let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
        startup.StartupInfo.cb =
            u32::try_from(size_of::<STARTUPINFOEXW>()).expect("STARTUPINFOEXW fits in u32");
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = child_pipes.stdin_read.raw();
        startup.StartupInfo.hStdOutput = child_pipes.stdout_write.raw();
        startup.StartupInfo.hStdError = child_pipes.stderr_write.raw();
        startup.lpAttributeList = attribute_list.raw();

        // SAFETY: zero is the required initial state for this output structure.
        let mut information: PROCESS_INFORMATION = unsafe { zeroed() };
        // SAFETY: all pointers reference initialized buffers that remain alive
        // for the call. The mutable command line is NUL-terminated. The
        // attribute list contains exactly the three valid inheritable standard
        // handles, and the explicit application name avoids path ambiguity.
        let created = unsafe {
            CreateProcessW(
                executable.as_ptr(),
                command_line.as_mut_ptr(),
                ptr::null(),
                ptr::null(),
                1,
                CREATE_SUSPENDED | CREATE_NO_WINDOW | EXTENDED_STARTUPINFO_PRESENT,
                ptr::null(),
                working_directory_ptr,
                (&raw const startup.StartupInfo),
                &raw mut information,
            )
        };
        if created == 0 {
            return Err(last_error(Win32Operation::CreateProcess));
        }

        let process_handle = OwnedHandle::new(information.hProcess, Win32Operation::CreateProcess)?;
        let primary_thread = OwnedHandle::new(information.hThread, Win32Operation::CreateProcess)?;
        let mut unassigned = UnassignedProcessGuard::new(
            Process {
                handle: process_handle,
            },
            primary_thread,
        );
        #[cfg(test)]
        inject_launch_failure(4, Win32Operation::AssignProcessToJob)?;

        // SAFETY: both handles are live and the process is still suspended, so
        // no child code or descendant can run before association succeeds.
        if unsafe { AssignProcessToJobObject(job.raw(), unassigned.process().raw()) } == 0 {
            return Err(last_error(Win32Operation::AssignProcessToJob));
        }
        #[cfg(test)]
        inject_launch_failure(5, Win32Operation::AssignProcessToJob)?;
        let (process, primary_thread) = unassigned.disarm();

        drop(child_pipes);
        attribute_list.delete();
        #[cfg(test)]
        inject_launch_failure(6, Win32Operation::CreateProcess)?;
        Ok(Self {
            job,
            process,
            primary_thread,
            parent_pipes,
        })
    }
}

#[cfg(test)]
fn inject_launch_failure(point: u8, operation: Win32Operation) -> Result<(), PlatformFailure> {
    if TEST_LAUNCH_FAILURE_POINT
        .compare_exchange(point, 0, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        Err(PlatformFailure::new(operation, 5))
    } else {
        Ok(())
    }
}

struct UnassignedProcessGuard {
    process: Option<Process>,
    primary_thread: Option<OwnedHandle>,
}

impl UnassignedProcessGuard {
    fn new(process: Process, primary_thread: OwnedHandle) -> Self {
        Self {
            process: Some(process),
            primary_thread: Some(primary_thread),
        }
    }

    fn process(&self) -> &Process {
        self.process
            .as_ref()
            .expect("unassigned process guard is armed")
    }

    fn disarm(&mut self) -> (Process, OwnedHandle) {
        (
            self.process.take().expect("process is still owned"),
            self.primary_thread
                .take()
                .expect("primary thread is still owned"),
        )
    }
}

impl Drop for UnassignedProcessGuard {
    fn drop(&mut self) {
        let Some(process) = &self.process else {
            return;
        };
        // SAFETY: the guard uniquely owns a live suspended process handle.
        // Terminating it prevents a failed Job assignment from leaving an
        // unassigned, permanently suspended process behind.
        unsafe {
            TerminateProcess(process.raw(), FORCED_TERMINATION_EXIT_CODE);
            WaitForSingleObject(process.raw(), 2_000);
        }
    }
}

pub(crate) fn resume_primary_thread(thread: &OwnedHandle) -> Result<(), PlatformFailure> {
    // SAFETY: `thread` is the live initial thread handle returned by
    // `CreateProcessW`, and the caller retains the suspended process until
    // assignment to its Job and I/O-thread creation have both succeeded.
    if unsafe { ResumeThread(thread.raw()) } == u32::MAX {
        Err(last_error(Win32Operation::ResumeThread))
    } else {
        Ok(())
    }
}

struct AttributeList {
    storage: Vec<usize>,
    // The handle-list attribute references this stable allocation through
    // `CreateProcessW`; keep it alive until the attribute list is deleted.
    handles: Box<[HANDLE; 3]>,
    deleted: bool,
}

impl AttributeList {
    fn new(handles: [HANDLE; 3]) -> Result<Self, PlatformFailure> {
        let mut bytes = 0usize;
        // SAFETY: a null buffer is the documented sizing query. Failure with
        // insufficient buffer is expected; `bytes` receives the required size.
        unsafe {
            InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &raw mut bytes);
        }
        if bytes == 0 {
            return Err(last_error(Win32Operation::InitializeAttributeList));
        }

        let words = bytes.div_ceil(size_of::<usize>());
        let mut storage = vec![0usize; words];
        let handles = Box::new(handles);
        let raw = storage.as_mut_ptr().cast::<c_void>();
        // SAFETY: storage is pointer-aligned and spans at least `bytes`; it
        // remains stable for the full attribute-list lifetime.
        if unsafe { InitializeProcThreadAttributeList(raw, 1, 0, &raw mut bytes) } == 0 {
            return Err(last_error(Win32Operation::InitializeAttributeList));
        }

        // SAFETY: `raw` is an initialized one-entry attribute list. `handles`
        // contains three live inheritable handles and is borrowed only for the
        // call, which copies the list value into the attribute storage.
        if unsafe {
            UpdateProcThreadAttribute(
                raw,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast::<c_void>(),
                size_of::<[HANDLE; 3]>(),
                ptr::null_mut(),
                ptr::null(),
            )
        } == 0
        {
            // SAFETY: initialization succeeded, so deletion is required once.
            unsafe {
                DeleteProcThreadAttributeList(raw);
            }
            return Err(last_error(Win32Operation::UpdateAttributeList));
        }

        Ok(Self {
            storage,
            handles,
            deleted: false,
        })
    }

    fn raw(&mut self) -> *mut c_void {
        self.storage.as_mut_ptr().cast()
    }

    fn delete(&mut self) {
        if !self.deleted {
            let _ = &self.handles;
            // SAFETY: the list was initialized successfully and has not yet
            // been deleted; both attribute storage and its referenced handle
            // array remain alive.
            unsafe {
                DeleteProcThreadAttributeList(self.raw());
            }
            self.deleted = true;
        }
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        self.delete();
    }
}

pub(crate) fn build_command_line(executable: &OsStr, arguments: &[std::ffi::OsString]) -> Vec<u16> {
    let mut command = Vec::new();
    append_quoted_argument(&mut command, executable);
    for argument in arguments {
        command.push(b' ' as u16);
        append_quoted_argument(&mut command, argument);
    }
    command
}

fn append_quoted_argument(output: &mut Vec<u16>, argument: &OsStr) {
    let units: Vec<u16> = argument.encode_wide().collect();
    let needs_quotes = units.is_empty()
        || units.iter().any(|unit| {
            *unit == u16::from(b' ') || *unit == u16::from(b'\t') || *unit == u16::from(b'"')
        });
    if !needs_quotes {
        output.extend_from_slice(&units);
        return;
    }

    output.push(b'"' as u16);
    let mut backslashes = 0usize;
    for unit in units {
        if unit == b'\\' as u16 {
            backslashes += 1;
        } else if unit == b'"' as u16 {
            output.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2 + 1));
            output.push(unit);
            backslashes = 0;
        } else {
            output.extend(std::iter::repeat_n(b'\\' as u16, backslashes));
            backslashes = 0;
            output.push(unit);
        }
    }
    output.extend(std::iter::repeat_n(b'\\' as u16, backslashes * 2));
    output.push(b'"' as u16);
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(Some(0)).collect()
}

fn duration_to_millis_saturating(duration: Duration) -> u32 {
    u32::try_from(duration.as_millis()).unwrap_or(u32::MAX - 1)
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::{OsStr, OsString},
        os::windows::ffi::OsStringExt,
        process::Command,
        sync::{Mutex, atomic::Ordering},
        thread,
        time::{Duration, Instant},
    };

    use hector_protocol::WorkerRole;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessHandleCount};

    use super::{
        SuspendedProcess, TEST_LAUNCH_FAILURE_POINT, build_command_line, resume_primary_thread,
    };
    use crate::{RestartPolicy, SupervisorTimeouts, WorkerProcessConfig};

    static PROCESS_AUDIT_LOCK: Mutex<()> = Mutex::new(());
    fn display(value: Vec<u16>) -> String {
        String::from_utf16(&value).expect("test command line is valid Unicode")
    }

    #[test]
    fn command_line_quotes_windows_arguments_exactly() {
        assert_eq!(
            display(build_command_line(
                OsStr::new(r"C:\Program Files\worker.exe"),
                &[
                    std::ffi::OsString::from(""),
                    std::ffi::OsString::from("plain"),
                    std::ffi::OsString::from("two words"),
                    std::ffi::OsString::from_wide(&[
                        b'a' as u16,
                        b'\\' as u16,
                        b'"' as u16,
                        b'b' as u16
                    ])
                ]
            )),
            r#""C:\Program Files\worker.exe" "" plain "two words" "a\\\"b""#
        );
    }

    #[test]
    fn process_tree_and_handle_leak_audit() {
        let _guard = PROCESS_AUDIT_LOCK
            .lock()
            .expect("process-audit lock is not poisoned");
        run_descendant_fixture(false);
        let handles_after_warmup = current_process_handle_count();
        run_descendant_fixture(false);
        assert_eq!(
            current_process_handle_count(),
            handles_after_warmup,
            "a repeated launch/termination cycle must not retain handles"
        );
    }

    #[test]
    #[ignore = "interactive Task Manager/Process Explorer acceptance check"]
    fn manual_forced_process_tree_observation() {
        let _guard = PROCESS_AUDIT_LOCK
            .lock()
            .expect("process-audit lock is not poisoned");
        run_descendant_fixture(true);
    }

    fn run_descendant_fixture(observe: bool) {
        let config = fixture_process_config();

        {
            let suspended =
                SuspendedProcess::create(&config).expect("suspended fixture process launches");
            resume_primary_thread(&suspended.primary_thread)
                .expect("fixture primary thread resumes");
            drop(suspended.primary_thread);
            drop(suspended.parent_pipes);

            let deadline = Instant::now() + Duration::from_secs(3);
            while suspended
                .job
                .active_processes()
                .expect("Job query succeeds")
                < 2
                && Instant::now() < deadline
            {
                thread::sleep(Duration::from_millis(10));
            }
            assert!(
                suspended
                    .job
                    .active_processes()
                    .expect("Job query succeeds")
                    >= 2,
                "the fixture and its descendant must both belong to the Job"
            );

            if observe {
                eprintln!("H16 manual forced case: parent and descendant are live for 12 seconds");
                thread::sleep(Duration::from_secs(12));
            }
            suspended.job.terminate().expect("Job termination succeeds");
            assert!(
                suspended
                    .job
                    .wait_empty(Duration::from_secs(2))
                    .expect("Job empty query succeeds"),
                "Job termination must remove every descendant"
            );
            if observe {
                eprintln!(
                    "H16 manual forced case: Job tree terminated; observing absence for 8 seconds"
                );
                thread::sleep(Duration::from_secs(8));
            }
        }
    }

    fn fixture_process_config() -> WorkerProcessConfig {
        WorkerProcessConfig::new(
            std::env::current_exe().expect("test executable path is available"),
            vec![
                OsString::from("--exact"),
                OsString::from("process::tests::descendant_fixture"),
                OsString::from("--ignored"),
            ],
            WorkerRole::Llm,
            None,
            SupervisorTimeouts::default(),
            RestartPolicy::new(0),
        )
        .expect("fixture process configuration is valid")
    }

    #[test]
    fn launch_milestone_failures_roll_back_every_acquired_resource() {
        let _guard = PROCESS_AUDIT_LOCK
            .lock()
            .expect("process-audit lock is not poisoned");
        let config = fixture_process_config();

        for point in 1..=6 {
            TEST_LAUNCH_FAILURE_POINT.store(point, Ordering::Release);
            assert!(SuspendedProcess::create(&config).is_err());
        }
        let handles_after_warmup = current_process_handle_count();

        for point in 1..=6 {
            TEST_LAUNCH_FAILURE_POINT.store(point, Ordering::Release);
            assert!(SuspendedProcess::create(&config).is_err());
            assert_eq!(
                current_process_handle_count(),
                handles_after_warmup,
                "launch milestone {point} must retain no process, thread, pipe, attribute, or Job handle"
            );
        }
    }

    #[test]
    #[ignore = "launched only as a supervised descendant-process fixture"]
    fn descendant_fixture() {
        let system_root = std::env::var_os("SystemRoot").expect("SystemRoot is defined");
        let ping = std::path::PathBuf::from(system_root).join(r"System32\ping.exe");
        let mut descendant = Command::new(ping)
            .args(["-n", "30", "127.0.0.1"])
            .spawn()
            .expect("descendant fixture launches");
        let _ = descendant.wait();
    }

    fn current_process_handle_count() -> u32 {
        let mut count = 0;
        // SAFETY: the pseudo-handle is always valid for the current process,
        // and `count` is a valid output pointer.
        assert_ne!(
            unsafe { GetProcessHandleCount(GetCurrentProcess(), &raw mut count) },
            0,
            "GetProcessHandleCount succeeds"
        );
        count
    }
}
