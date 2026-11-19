//! A Windows Job Object that ends the engine when Joona exits for any reason,
//! including a crash or being killed from Task Manager.

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;

    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };
    use windows::core::PCWSTR;

    pub struct KillOnClose(HANDLE);

    // SAFETY: a job handle can be used from any thread.
    unsafe impl Send for KillOnClose {}
    unsafe impl Sync for KillOnClose {}

    impl KillOnClose {
        pub fn new() -> Option<Self> {
            // SAFETY: plain Win32 calls with a correctly sized info struct.
            unsafe {
                let job = CreateJobObjectW(None, PCWSTR::null()).ok()?;
                let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let set = SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if set.is_err() {
                    let _ = CloseHandle(job);
                    return None;
                }
                Some(Self(job))
            }
        }

        pub fn assign(&self, child: &tokio::process::Child) {
            if let Some(handle) = child.raw_handle() {
                // SAFETY: the handle belongs to a live child process.
                unsafe {
                    let _ = AssignProcessToJobObject(self.0, HANDLE(handle));
                }
            }
        }
    }

    impl Drop for KillOnClose {
        fn drop(&mut self) {
            // SAFETY: the handle came from CreateJobObjectW and is closed once.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub struct KillOnClose;

    impl KillOnClose {
        pub fn new() -> Option<Self> {
            Some(Self)
        }

        pub fn assign(&self, _child: &tokio::process::Child) {}
    }
}

pub use imp::KillOnClose;
