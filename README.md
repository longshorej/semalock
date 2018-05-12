# semalock

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

# Usage (JVM)

A subproject, `semalock-jvm`, provides bindings for the Java Virtual Machine so that you can use it in your Java and Scala programs.

# Development

The project is split into multiple subdirectories. You can find the main project in `semalock`.

# Author

Jason Longshore <hello@jasonlongshore.com>
