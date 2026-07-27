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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_job_succeeds() {
        let job = create_kill_on_close_job();
        assert!(
            job.is_ok(),
            "CreateJobObjectW should succeed outside a restrictive sandbox"
        );
        // Dropping the job here is safe — no processes are assigned to it.
    }

    #[test]
    fn assign_nonexistent_pid_fails() {
        let job = create_kill_on_close_job().expect("create job");
        // PID 0 is the System Idle Process and cannot be opened with our rights.
        let result = assign_process(&job, 0);
        assert!(
            result.is_err(),
            "assigning PID 0 should fail (cannot open System Idle Process)"
        );
    }

    #[test]
    fn job_object_kills_child_on_drop() {
        let job = create_kill_on_close_job().expect("create job");

        // Spawn a long-running child.
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/c", "ping -n 60 127.0.0.1 > nul"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn child");

        let child_pid = child.id();

        // Assign the child to the job.
        match assign_process(&job, child_pid) {
            Ok(()) => {}
            Err(e) => {
                // The test environment may already have the process in a job
                // (e.g., CI runners).  Skip rather than fail.
                eprintln!(
                    "SKIP: cannot assign process {child_pid} to job — test env may use jobs already: {e}"
                );
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
        }

        // Drop the job handle → KILL_ON_JOB_CLOSE fires.
        drop(job);

        // Give the OS a moment to terminate the process tree.
        std::thread::sleep(std::time::Duration::from_secs(3));

        match child.try_wait() {
            Ok(Some(_status)) => {
                // Child was terminated — success.
            }
            Ok(None) => {
                // Still running — the Job Object did not kill it.
                let _ = child.kill();
                let _ = child.wait();
                panic!("child process {child_pid} was NOT terminated by job close");
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("try_wait error on child {child_pid}: {e}");
            }
        }
    }
}
