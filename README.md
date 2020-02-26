# semalock

[![Crates.io](https://img.shields.io/crates/v/semalock.svg?style=flat-square)](https://crates.io/crates/semalock)
[![Crates.io](https://img.shields.io/crates/d/semalock.svg?style=flat-square)](https://crates.io/crates/semalock)
[![Travis](https://img.shields.io/travis/longshorej/semalock.svg?style=flat-square)](https://travis-ci.org/longshorej/semalock)

semalock is a Rust library for controlling concurrent access to files on POSIX operating systems in an efficient manner.

It uses a combination of POSIX named semaphores and exclusive file locks to safely and efficiently acquire exclusive access to a file. This has been observed to be particularly efficient on Linux, with under 5% of CPU time spent on lock overhead with 8192 processes.

## Usage

The following shows usage of semalock. This program opens `/some/file` and appends some text to it. Try it with GNU parallel to measure performance amongst multiple competing processes.

```rust
// Acquire and open a file and semaphore
let mut lock = Semalock::new(Path::new("/some/file"));

// Do some stuff to the file
lock.with(|lock| {
    lock.file
        .seek(SeekFrom::End())
        .and_then(|_| lock.file.write(b"hello world\n"))
});
```

## Supported Operating Systems

The following operating systems have been tested:

* GNU/Linux 4.16

The following operating systems have not been tested but should work:

* FreeBSD
* GNU/Linux 2.6+
* macOS 10.4+
* NetBSD
* OpenBSD

Supported operating systems must support provide the following:

* flock
* sem_get_value
* sem_open
* sem_post
* sem_timedwait
* sem_unlink

The following will not work:

* Windows NT

## Release Notes

### 0.3.1 - 2020-02-25

* Fix a compilation error on x86

### 0.3.0 - 2019-09-13

* `Semalock::with` now takes an `FnOnce` instead of an `Fn`.
* Various project hygiene changes

### 0.2.0 - 2018-05-14

* Initial release.

## Developer Notes

To run the tests, execute the following:

```bash
cargo test
```

To release the create, perform the following:

1. Edit `Cargo.toml`, bumping the version as appropriate.
2. Edit `README.md`, adding an entry to the Release Notes.
3. Commit these changes and push them to `master`.
4. Create and push a tag that starts with "v" -- e.g. "v0.4.0"

## Author

Jason Longshore <hello@jasonlongshore.com>
