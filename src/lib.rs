extern crate errno;
extern crate fs2;
extern crate libc;
extern crate tempfile;

use fs2::FileExt;
use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::ffi::CString;
use std::hash::{Hash, Hasher};
use std::fs::{ File, OpenOptions };
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub type SemalockError = String;

pub struct Semalock {
    pub file: File,
    sem: *mut libc::sem_t,
    sem_name_cstring: CString
}

pub fn testing(one: i32, two: i32) -> i32 {
    one + two
}

impl Semalock {
    /// Creates a new `Semalock`, opening or creating the file
    /// for reading and writing. A POSIX named semaphore is
    /// allocated (based on the hash of the path) and is used
    /// to reduce contention when acquiring exclusive file locks.
    /// On Linux, this is nearly FIFO in terms of acquiring the
    /// lock, though not always. Good (i.e. very minimal CPU usage)
    /// performance has been tested with upto 8192 simultaneous
    /// writers.
    pub fn new(path: &Path) -> Result<Semalock, SemalockError> {
        OpenOptions::new()
            .write(true)
            .read(true)
            .create(true)
            .open(path)
            .map_err(|e| format!("OpenOptions::open failed: {}", e.description()))
            .and_then(move |file| {
                let file_hash = {
                    let mut s = DefaultHasher::new();
                    path.to_string_lossy().hash(&mut s);
                    s.finish()
                };

                let sem_name = format!("fast-lock-{}-{:x}", 0, file_hash);

                CString::new(sem_name)
                    .map_err(|e| format!("CString::new failed: {}", e.description()))
                    .and_then(move |sem_name_cstring| {
                        let sem = unsafe {
                            let sem = libc::sem_open(sem_name_cstring.as_ptr(), libc::O_CREAT, 0o644, 1);

                            if sem == libc::SEM_FAILED {
                                let e = errno::errno();
                                Err(format!("sem_open {}: {}", e.0, e))
                            } else {
                                Ok(sem)
                            }
                        };

                        sem.map(|sem| Semalock { file, sem, sem_name_cstring })
                    })
            })
    }

    /// Unlinks the semaphore used by this `Semalock` instance. Future
    /// acquisitions will result in a new kernel object being created.
    /// This does not affect the data of the file that this lock is
    /// protecting. See POSIX `sem_unlink` for more details.
    pub fn unlink(mut self) -> Result<(), SemalockError> {
        self.with(|s| {
            let code = unsafe { libc::sem_unlink(s.sem_name_cstring.as_ptr()) };

            if code == 0 {
                Ok(())
            } else {
                let e = errno::errno();
                Err(format!("sem_unlink {}: {}", e.0, e))
            }
        }).and_then(|a| a)
    }

    /// Acquires the lock, runs the provided function, and releases the lock. If the provided
    /// function panics, the lock is not automatically released. In this case, the secondary
    /// level of exclusive file locks will take effect, temporarily affecting performance
    /// until a timeout occurs and normal behavior is restored (in other applications).
    pub fn with<A, B>(&mut self, a: A) -> Result<B, SemalockError> where A: Fn(&mut Self) -> B {
        self
            .acquire()
            .and_then(|_| {
                let result = a(self);

                self
                    .release()
                    .map(|_| result)
            })
    }

    fn acquire(&self) -> Result<(), SemalockError> {
        loop {
            // @TODO switch to sem timeout, and allow progress if we timeout
            //       assuming process died. we should probably unlink the old
            //       semaphore or something?

            // algo:
            //
            // acquire semaphore with a timeout (say 10 seconds?)
            // if acquired:
            //     acquire (blocking) the file lock
            // if timed out:
            //     try to acquire exclusive file lock (there is no do! only try!)
            //     if acquired:
            //         we're now critical, meaning other process has crashed.
            //         we can continue as normal
            //     if failed, repeat acquiring semaphore with timeout

            let sem_timeout_seconds = 10;
            let now_elapsed_epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
            let sem_timeout = libc::timespec {
                tv_sec: (now_elapsed_epoch.as_secs() + sem_timeout_seconds) as i64,
                tv_nsec: (now_elapsed_epoch.subsec_nanos()) as i64
            };
            let call_status = unsafe { libc::sem_timedwait(self.sem, &sem_timeout) };

            if call_status == 0 {
                self.file.lock_exclusive().unwrap(); // @TODO deal with fail, compose etc

                return Ok(());
            } else {
                let e = errno::errno();

                match e.0 {
                    libc::EINTR => {

                    },

                    libc::ETIMEDOUT => {
                        let file_locked = self.file.try_lock_exclusive().is_ok();

                        if file_locked {
                            return Ok(())
                        }
                    },

                    _ => {
                        return Err(format!("sem_timedwait {}: {}", e.0, e))
                    }
                }
            }
        }
    }

    fn release(&self) -> Result<(), SemalockError> {
        // @TODO obv compose, deal with failure, rollback et al
        self.file.unlock().unwrap();

        let mut value: i32 = 0;

        // @TODO uncomment when new release of libc with my PR is done
        //let code = unsafe { libc::sem_getvalue(self.sem, &mut value) };

        let code = 0;

        if code == 0 && value == 0 {
            let unlock_code = unsafe { libc::sem_post(self.sem) };

            if unlock_code == 0 {
                Ok(())
            } else {
                let e = errno::errno();

                Err(format!("sem_post {}: {}", e.0, e))
            }
        } else if code == 0 {
            // @TODO bug ? shouldn't happen unless another process increased it on us
            Ok(())
        } else {
            let e = errno::errno();

            Err(format!("sem_getvalue {}: {}", e.0, e))
        }
    }
}

// @TODO move over tests from other projects

#[test]
fn basic_usage() {
    use std::fs::remove_file;
    use std::io::prelude::*;
    use tempfile::NamedTempFile;

    let file = NamedTempFile::new().unwrap();
    let path = file.path();
    let mut lock = Semalock::new(path).unwrap();

    lock.with(|lock| {
        lock.file.write_all(b"hello world!").unwrap();
    }).unwrap();

    remove_file(path).unwrap();
}

#[test]
fn unlink_and_use_again() {
    use std::fs::remove_file;
    use std::io::prelude::*;
    use tempfile::NamedTempFile;

    let file = NamedTempFile::new().unwrap();
    let path = file.path();
    let mut lock = Semalock::new(path).unwrap();

    lock.with(|l| l.file.write_all(b"hello world!").unwrap()).unwrap();

    lock.unlink().unwrap();


    let mut lock = Semalock::new(path).unwrap();

    lock.with(|l| l.file.write_all(b"hello world 2!").unwrap()).unwrap();

    lock.unlink().unwrap();

    let mut file = File::open(path).unwrap();
    let mut result = String::new();
    file.read_to_string(&mut result).unwrap();

    assert_eq!(result, "hello world 2!");

    remove_file(path).unwrap();
}
