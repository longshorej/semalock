# semalock

[![Crates.io](https://img.shields.io/crates/v/semalock.svg?style=flat-square)](https://crates.io/crates/semalock)
[![Crates.io](https://img.shields.io/crates/d/semalock.svg?style=flat-square)](https://crates.io/crates/semalock)
[![Travis](https://img.shields.io/travis/longshorej/semalock.svg?style=flat-square)](https://travis-ci.org/longshorej/semalock)

semalock is a Rust library for controlling concurrent access to files on POSIX operating systems in an efficient manner.

It uses a combination of POSIX named semaphores and exclusive file locks to safely and efficiently acquire exclusive access to a file. This has been observed to be particularly efficient on Linux, with under 5% of CPU time spent on lock overhead with 8192 processes.

# Usage

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

# Supported Operating Systems

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

# Development

You'll need Cargo. More notes to come at a later date.

## Releasing

1) Sanity: ```cargo clean; cargo test```
2) Upgrade version in `Cargo.toml`
3) Commit changes
4) Create and push a tag: ```git tag v<version>; git push v<version>```
5) Release on crates.io: ```cargo publish```

# Author

Jason Longshore <hello@jasonlongshore.com>
