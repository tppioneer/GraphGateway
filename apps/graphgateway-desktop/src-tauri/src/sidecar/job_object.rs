//! Windows Job Object wrapper for sidecar process-tree cleanup.
//!
//! When the Tauri process exits (gracefully or crashes), the OS closes all
//! remaining handles to the Job Object.  With `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`,
//! every process in the job is terminated automatically — preventing orphan
//! sidecar, mcp-proxy, or GitNexus processes.

#![cfg(windows)]

use std::ffi::c_void;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

/// Opaque handle to a Windows Job Object.
///
/// Dropping this closes the handle.  With `KILL_ON_JOB_CLOSE`, closing the
/// last handle terminates all processes in the job.
pub(crate) struct JobHandle(HANDLE);

unsafe impl Send for JobHandle {}
unsafe impl Sync for JobHandle {}

impl Drop for JobHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Create a Job Object configured to kill all members on last handle close.
pub(crate) fn create_kill_on_close_job(
) -> Result<JobHandle, Box<dyn std::error::Error + Send + Sync>> {
    unsafe {
        let handle = CreateJobObjectW(Some(&SECURITY_ATTRIBUTES::default()), None)
            .map_err(|e| format!("CreateJobObjectW failed: {e}"))?;

        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

        SetInformationJobObject(
            handle,
            windows::Win32::System::JobObjects::JobObjectExtendedLimitInformation,
            &limits as *const _ as *const c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .map_err(|e| {
            let _ = CloseHandle(handle);
            format!("SetInformationJobObject failed: {e}")
        })?;

        Ok(JobHandle(handle))
    }
}

/// Assign a process (by PID) to the job object.
pub(crate) fn assign_process(
    job: &JobHandle,
    pid: u32,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    unsafe {
        let proc_handle = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid)
            .map_err(|e| format!("OpenProcess({pid}) failed: {e}"))?;

        let result = AssignProcessToJobObject(job.0, proc_handle);
        let _ = CloseHandle(proc_handle);
        result.map_err(|e| format!("AssignProcessToJobObject({pid}) failed: {e}"))?;
    }

    Ok(())
}
