//! A Windows Job object that kills every process in it when its handle closes. Assigning the agent
//! right after spawn puts the whole tree (a `.cmd` shim, `node`, any grandchild) under one handle,
//! so `child.kill()` on the shim alone cannot leave orphans.

use std::io;
use std::os::windows::io::AsRawHandle;
use std::process::Child;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};

pub struct Job(HANDLE);

// SAFETY: a Job handle is only closed in Drop and the kernel object is safe to use from any thread.
unsafe impl Send for Job {}

impl Job {
    /// A new job with KILL_ON_JOB_CLOSE, holding `child`.
    pub fn with_child(child: &Child) -> io::Result<Job> {
        // SAFETY: plain Win32 calls with valid arguments; the handle is closed by `Job::drop`, and
        // by the early returns below through the `Job` value that already owns it.
        unsafe {
            let handle = CreateJobObjectW(None, None).map_err(io::Error::other)?;
            let job = Job(handle);
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .map_err(io::Error::other)?;
            AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle()))
                .map_err(io::Error::other)?;
            Ok(job)
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: the handle is owned by this value and closed once. Closing it kills the tree.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
