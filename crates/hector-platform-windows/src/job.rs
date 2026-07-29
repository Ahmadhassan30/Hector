use std::{
    ffi::c_void,
    mem::{size_of, zeroed},
    ptr, thread,
    time::{Duration, Instant},
};

use windows_sys::Win32::System::JobObjects::{
    CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectBasicAccountingInformation,
    JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
    TerminateJobObject,
};

use crate::{
    PlatformFailure, Win32Operation,
    handle::{OwnedHandle, last_error},
};

pub(crate) const FORCED_TERMINATION_EXIT_CODE: u32 = 0x4845_0016;

#[derive(Debug)]
pub(crate) struct Job {
    handle: OwnedHandle,
}

impl Job {
    pub(crate) fn new() -> Result<Self, PlatformFailure> {
        // SAFETY: null security attributes and name request a private unnamed
        // job and require no borrowed memory.
        let raw = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        let handle = OwnedHandle::new(raw, Win32Operation::CreateJob)?;

        // SAFETY: zero is the documented neutral value for all extended job
        // limits; only `LimitFlags` is then enabled.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let size = u32::try_from(size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>())
            .expect("Win32 job structure fits in u32");
        // SAFETY: `limits` is fully initialized for the exact information
        // class and remains alive for the duration of the call.
        let configured = unsafe {
            SetInformationJobObject(
                handle.raw(),
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast::<c_void>(),
                size,
            )
        };
        if configured == 0 {
            return Err(last_error(Win32Operation::ConfigureJob));
        }

        Ok(Self { handle })
    }

    pub(crate) const fn raw(&self) -> windows_sys::Win32::Foundation::HANDLE {
        self.handle.raw()
    }

    pub(crate) fn terminate(&self) -> Result<(), PlatformFailure> {
        // SAFETY: the handle is a live Job handle uniquely owned by `self`.
        if unsafe { TerminateJobObject(self.raw(), FORCED_TERMINATION_EXIT_CODE) } == 0 {
            Err(last_error(Win32Operation::TerminateJob))
        } else {
            Ok(())
        }
    }

    pub(crate) fn active_processes(&self) -> Result<u32, PlatformFailure> {
        // SAFETY: zero is a valid initial state for this output structure.
        let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
        let size = u32::try_from(size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>())
            .expect("Win32 job structure fits in u32");
        // SAFETY: the output buffer matches the selected information class.
        let queried = unsafe {
            QueryInformationJobObject(
                self.raw(),
                JobObjectBasicAccountingInformation,
                (&raw mut accounting).cast::<c_void>(),
                size,
                ptr::null_mut(),
            )
        };
        if queried == 0 {
            Err(last_error(Win32Operation::QueryJob))
        } else {
            Ok(accounting.ActiveProcesses)
        }
    }

    pub(crate) fn wait_empty(&self, timeout: Duration) -> Result<bool, PlatformFailure> {
        let deadline = Instant::now() + timeout;
        loop {
            if self.active_processes()? == 0 {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FORCED_TERMINATION_EXIT_CODE;

    #[test]
    fn forced_exit_code_is_private_and_structured() {
        assert_eq!(FORCED_TERMINATION_EXIT_CODE >> 16, 0x4845);
        assert_eq!(FORCED_TERMINATION_EXIT_CODE & 0xffff, 0x0016);
    }
}
