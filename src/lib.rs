extern crate errno; /* @TODO really need a dep for this? */
extern crate libc;
extern crate tempfile;

use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::ffi::CString;
use std::hash::{Hash, Hasher};
use std::fs::{ File, OpenOptions };
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub type SemalockError = String;

pub struct Semalock {
    fd: i32,
    pub file: File,
    sem: *mut libc::sem_t,
    sem_name_cstring: CString
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
                let fd = file.as_raw_fd();

                let file_hash = {
                    let mut s = DefaultHasher::new();
                    path.to_string_lossy().hash(&mut s);
                    s.finish()
                };

                let sem_name = format!("fast-lock-{}-{:x}", 0, file_hash);

                CString::new(sem_name)
                    .map_err(|e| format!("CString::new failed: {}", e.description()))
                    .and_then(move |sem_name_cstring| {
                        // @TODO move most of this out of unsafe
                        let sem = unsafe {
                            let sem = libc::sem_open(sem_name_cstring.as_ptr(), libc::O_CREAT, 0o644, 1);

                            if sem == libc::SEM_FAILED {
                                let e = errno::errno();
                                Err(format!("sem_open {}: {}", e.0, e))
                            } else {
                                Ok(sem)
                            }
                        };

                        sem.map(|sem| Semalock { fd, file, sem, sem_name_cstring })
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
            let now_elapsed_epoch = match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(r) => r,
                Err(e) => return Err(e.to_string())
            };
            let sem_timeout = libc::timespec {
                tv_sec: (now_elapsed_epoch.as_secs() + sem_timeout_seconds) as i64,
                tv_nsec: (now_elapsed_epoch.subsec_nanos()) as i64
            };
            let call_status = unsafe { libc::sem_timedwait(self.sem, &sem_timeout) };

            if call_status == 0 {
                let flock_code = unsafe { libc::flock(self.fd, libc::LOCK_EX) };

                if flock_code != 0 {
                    let e = errno::errno();
                    return Err(format!("flock {}: {}", e.0, e));
                }

                return Ok(());
            } else {
                let e = errno::errno();

                match e.0 {
                    libc::EINTR => {},

                    libc::ETIMEDOUT => {
                        let flock_code = unsafe { libc::flock(self.fd, libc::LOCK_EX) };

                        if flock_code != 0 {
                            let e = errno::errno();

                            match e.0 {
                                libc::EINTR => {},

                                libc::EWOULDBLOCK => {},

                                _ => {
                                    return Err(format!("flock {}: {}", e.0, e))
                                }
                            }
                        }

                        return Ok(());
                    },

                    _ => {
                        return Err(format!("sem_timedwait {}: {}", e.0, e))
                    }
                }
            }
        }
    }

    fn release(&self) -> Result<(), SemalockError> {
        let flock_code = unsafe { libc::flock(self.fd, libc::LOCK_UN) };

        if flock_code != 0 {
            let e = errno::errno();
            return Err(format!("flock {}: {}", e.0, e));
        }

        // @TODO uncomment when new release of libc with my PR is done
        //let mut value: i32 = 0;
        //let sem_getvalue_code = unsafe { libc::sem_getvalue(self.sem, &mut value) };
        let sem_value: i32 = 0;
        let sem_getvalue_code = 0;

        if sem_getvalue_code != 0 {
            let e = errno::errno();
            return Err(format!("sem_getvalue {}: {}", e.0, e));
        }

        // @TODO sem_value greater than 0, race with other process or bug
        if sem_value == 0 {
            let sem_post_code = unsafe { libc::sem_post(self.sem) };

            if sem_post_code != 0 {
                let e = errno::errno();

                return Err(format!("sem_post {}: {}", e.0, e));
            }
        }

        Ok(())
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

// @TODO concurrency_processes (should just work)

#[test]
fn concurrency_threads() {
    use std::fs;
    use std::io::prelude::*;
    use std::io::{ SeekFrom, Write };

    let path_str = {
        // immediately goes out of scope and gets deleted,
        // then we manage it ourselves
        let path_str = tempfile::NamedTempFile::new()
            .unwrap()
            .path()
            .to_str()
            .unwrap()
            .to_string();

        assert!(!Path::new(&path_str).exists());

        path_str
    };

    let num_threads = 512;

    let threads: Vec<std::thread::JoinHandle<()>> =
        (0..num_threads).map(|n| {
            let n = n.clone();
            let path_str = path_str.clone();
            std::thread::spawn(move || {
                let mut lock = Semalock::new(Path::new(&path_str)).unwrap();
                lock.with(|lock| {
                    lock.file.seek(SeekFrom::End(0)).unwrap();
                    lock.file.write_all(format!("{}\n", n).as_bytes()).unwrap();
                }).unwrap();
            })
        }).collect();

    for t in threads {
        t.join().unwrap();
    }

    let path = Path::new(&path_str);
    let mut result = String::new();
    let mut file = File::open(&path).unwrap();
    file.read_to_string(&mut result).unwrap();
    let lock = Semalock::new(Path::new(&path)).unwrap();
    lock.unlink().unwrap();
    fs::remove_file(Path::new(&path)).unwrap();

    let sum = result
        .lines()
        .map(|l| l.parse::<i32>().unwrap())
        .sum::<i32>();

    let expected = num_threads * (num_threads - 1) / 2;

    assert_eq!(sum, expected);
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
