# Chapter 0: the skeleton

By the end of this chapter you have a project that compiles, a `run` function that the
whole tracer will grow out of, an error type, and five passing tests. Nothing is traced
yet. The point is to set up the shape of the project so that every later chapter only
adds code and never has to rearrange it.

**You will learn**

- the difference between a library crate and a binary crate, and why we use both
- how `mod`, `pub` and `use` organise code
- how to define your own error type as an `enum`
- how the `?` operator converts errors through the `From` trait
- the two kinds of test Cargo runs: unit tests and integration tests

**Checkpoint**: `cargo test` reports 5 passing tests, and `cargo run` prints a usage
message and exits with status 1.

## 0.1 Why split into a library and a binary

Right now the project has one file, `src/main.rs`. Cargo treats that as a *binary
crate*: it compiles to an executable, and nothing outside it can call its functions.

That is a problem for testing. An integration test in `tests/` is a separate crate, and
it cannot `use` anything from a binary crate. If all the logic lives in `main.rs`, the
only way to test it is to run the executable and inspect its output.

The standard fix is to put the logic in a *library crate* and keep the binary as a thin
wrapper:

| File | Crate | Job |
|------|-------|-----|
| `src/lib.rs` | library, named `rtrace` | all the logic, testable |
| `src/main.rs` | binary, also named `rtrace` | read arguments, call the library, exit |

Cargo detects both automatically. If `src/lib.rs` exists, the package has a library. If
`src/main.rs` exists, it has a binary. The binary depends on the library as if it were
an external crate, so `main.rs` refers to it by the package name: `rtrace::run(...)`.

## 0.2 Add the dependencies

A tracer talks to the kernel through system calls such as `fork`, `execve`, `waitpid`
and `ptrace`. Rust's standard library does not expose these directly, so we use two crates:

- **`libc`**: raw declarations of the C library's functions, constants and structs.
  Everything in it is `unsafe` and uses C types. We need it later for things like
  `user_regs_struct` (the CPU registers) and constants such as `O_RDONLY`.
- **`nix`**: safe wrappers around `libc`. Where `libc::fork()` returns a bare integer
  that you must check yourself, `nix::unistd::fork()` returns a `Result` holding an
  enum that says whether you are the parent or the child.

Run:

```sh
cargo add nix@0.31 --features ptrace,process,signal,uio
cargo add libc
```

`nix` is split into *features* so you only compile the parts you use. The four we enable:

| Feature | Gives us | First used in |
|---------|----------|---------------|
| `process` | `fork`, `execvp`, `waitpid` | chapter 1 |
| `ptrace` | `nix::sys::ptrace` | chapter 2 |
| `signal` | `Signal`, `raise`, `kill` | chapter 2 |
| `uio` | `process_vm_readv` | chapter 6 |

Your `Cargo.toml` should now end like this:

```toml
[dependencies]
libc = "0.2.191"
nix = { version = "0.31", features = ["ptrace", "process", "signal", "uio"] }
```

## 0.3 The error type: `src/error.rs`

Things will go wrong in three different ways in this project:

1. The user ran `rtrace` incorrectly (no command given).
2. A system call failed (`fork` returned an error, `ptrace` was refused).
3. An I/O operation failed (writing the trace to a file, in chapter 9).

In Rust a function that can fail returns `Result<T, E>`, and `E` must be one single
type. An `enum` lets one type cover all three cases. Create `src/error.rs`:

```rust
use std::fmt;
use std::io;

#[derive(Debug)]
pub enum Error {
    Usage,
    Nix(nix::Error),
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Usage => write!(f, "usage: rtrace <command> [args...]"),
            Error::Nix(errno) => write!(f, "system call failed: {errno}"),
            Error::Io(err) => write!(f, "I/O error: {err}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<nix::Error> for Error {
    fn from(errno: nix::Error) -> Self {
        Error::Nix(errno)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::Io(err)
    }
}
```

Going through it piece by piece:

**The enum.** `Usage` carries no data. `Nix(nix::Error)` and `Io(io::Error)` each wrap
the underlying error, so no information is lost. `nix::Error` is an alias for
`nix::errno::Errno`, an enum of the kernel's error numbers (`ENOENT`, `EPERM` and so on).

**`#[derive(Debug)]`** generates the `{:?}` formatting used by `assert!` failures and
`unwrap` panics. It is for programmers.

**`impl fmt::Display`** is the `{}` formatting, for users. We write it by hand because
only we know what message a user should see. The `match` must cover every variant. If
you add a fourth variant later and forget to handle it here, the code will not compile.
That exhaustiveness check is one of the main reasons to model errors as an enum.

**`impl std::error::Error for Error {}`** marks the type as a standard error. The body
is empty because the trait's only requirements are `Debug` and `Display`, which we
already have. It lets the type work with the rest of the ecosystem, such as
`Box<dyn Error>`.

**The two `From` impls** are what make `?` work. This is the part of the chapter most
worth understanding.

### How `?` uses `From`

Suppose a function returns `Result<i32, Error>` (our type) and calls something from
`nix`, which returns `Result<Pid, nix::Error>`. The error types do not match. Without
`?` you would write:

```rust
let pid = match some_nix_call() {
    Ok(pid) => pid,
    Err(errno) => return Err(Error::from(errno)),
};
```

The `?` operator is shorthand for exactly that:

```rust
let pid = some_nix_call()?;
```

On `Ok`, it unwraps the value. On `Err`, it calls `From::from` to convert the error into
the function's return error type, and returns early. Because we implemented
`From<nix::Error> for Error`, the conversion exists and the code compiles. Without that
impl you get the error "`?` couldn't convert the error to `Error`".

So the rule for the rest of the tutorial is: whenever a new kind of error shows up, add
a variant and a `From` impl, and every `?` in the project picks it up.

## 0.4 The library root: `src/lib.rs`

Replace the contents of `src/lib.rs` with:

```rust
pub mod error;

pub use error::Error;

pub fn run(args: &[String]) -> Result<i32, Error> {
    if args.is_empty() {
        return Err(Error::Usage);
    }
    Ok(0)
}
```

**`pub mod error;`** tells the compiler that a module named `error` exists and that its
code is in `src/error.rs`. A file is not part of the crate until some `mod` line
declares it. Forgetting this line is a common mistake: the file sits in `src/`, but
the compiler ignores it. `pub` makes the module visible outside the library.

**`pub use error::Error;`** re-exports the type at the crate root, so callers can write
`rtrace::Error` where they would otherwise need `rtrace::error::Error`.

**`run`** is the entry point of the tracer. Its signature is a contract that will not
change for the rest of the tutorial:

- `args: &[String]` is the command to trace and its arguments, for example
  `["ls", "-l"]`. It is a *slice*: a borrowed view of a sequence. Taking `&[String]`
  and not `Vec<String>` means `run` only reads the arguments and does not take
  ownership of them, and callers can pass a `Vec`, an array or part of either.
- `Result<i32, Error>` is the exit code the process should finish with, or the reason
  we could not trace at all. From chapter 1 on, the `i32` is the traced program's own
  exit code.

For now `run` only checks that a command was given. `Ok(0)` is a placeholder that
chapter 1 replaces with a real `fork` and `exec`.

The plan lists more modules (`tracer`, `syscalls`, `decode`, and others). Do not create
them yet. Each one is added in the chapter that first needs it.

## 0.5 The binary: `src/main.rs`

Replace `src/main.rs` with:

```rust
use std::process;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match rtrace::run(&args) {
        Ok(code) => process::exit(code),
        Err(err) => {
            eprintln!("rtrace: {err}");
            process::exit(1);
        }
    }
}
```

**`std::env::args()`** returns an iterator over the command-line arguments. The first
one is always the program's own name (`rtrace`), so `.skip(1)` drops it.
`.collect()` gathers the rest into a collection. `collect` can build many different
collection types, so the annotation `Vec<String>` tells it which one.

**`rtrace::run(&args)`** calls into the library. `&args` borrows the vector, and Rust
converts `&Vec<String>` to `&[String]` automatically.

**The `match`** handles both outcomes. On success the process exits with the code
`run` returned. On failure it prints the error with `{err}`, which uses the `Display`
impl from section 0.3, and exits with 1.

**`eprintln!`** writes to standard error. This matters for a tracer: the traced
program writes to standard output, and our messages must not mix with it. If you run
`rtrace ls > files.txt`, the file should contain only what `ls` printed.

This is everything `main.rs` will ever do. All later work happens in the library.

## 0.6 Unit tests

Unit tests live in the same file as the code they test, inside a module that is only
compiled when testing. Add this to the bottom of `src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_command_is_a_usage_error() {
        let result = run(&[]);
        assert!(matches!(result, Err(Error::Usage)));
    }

    #[test]
    fn a_command_is_accepted() {
        let args = vec!["true".to_string()];
        assert!(matches!(run(&args), Ok(0)));
    }

    #[test]
    fn question_mark_converts_errno_into_our_error() {
        fn fails() -> Result<(), Error> {
            Err(nix::errno::Errno::ENOENT)?;
            Ok(())
        }
        assert!(matches!(fails(), Err(Error::Nix(nix::errno::Errno::ENOENT))));
    }
}
```

- **`#[cfg(test)]`** means "compile this module only for `cargo test`". It adds
  nothing to the real binary.
- **`use super::*;`** imports everything from the parent module, so the tests can call
  `run` and name `Error` directly.
- **`#[test]`** marks a function as a test. A test passes if it returns without panicking.
- **`matches!(value, pattern)`** returns `true` if the value fits the pattern. We use
  it in place of `assert_eq!` because `assert_eq!` needs `PartialEq`, and our `Error`
  cannot derive it: `io::Error` does not implement `PartialEq`.

The third test demonstrates section 0.3. `Err(Errno::ENOENT)?` has error type
`nix::Error`, the function returns our `Error`, and the test confirms that `?` wrapped
it in `Error::Nix`. Try deleting the `From<nix::Error>` impl and running `cargo test` to
see the compiler error, then put it back.

## 0.7 An integration test

Integration tests live in the top-level `tests/` directory. Each file there is compiled
as its own crate that can only use the library's public API, or run the built
executable. From chapter 1 on, this is where the tracer gets tested against real programs.

Create `tests/cli.rs`:

```rust
use std::process::Command;

fn rtrace() -> Command {
    Command::new(env!("CARGO_BIN_EXE_rtrace"))
}

#[test]
fn no_arguments_prints_usage_and_fails() {
    let output = rtrace().output().expect("failed to run rtrace");

    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("usage: rtrace"), "stderr was: {stderr}");
}

#[test]
fn a_command_exits_zero_for_now() {
    let status = rtrace().arg("true").status().expect("failed to run rtrace");
    assert!(status.success());
}
```

- **`env!("CARGO_BIN_EXE_rtrace")`** is a path that Cargo provides at compile time,
  pointing at the freshly built `rtrace` executable. Cargo builds the binary before
  running integration tests, so the path is always valid and up to date.
- **`Command`** is the standard library's way to start a process. `.output()` runs it
  and captures stdout, stderr and the exit status. `.status()` runs it and returns only
  the status. Chapter 1 does by hand, with `fork` and `exec`, what `Command` does here.
- **`output.status.code()`** returns `Option<i32>`. It is `None` when the process was
  killed by a signal and so has no exit code. That case becomes important in chapter 8.
- The extra argument to `assert!` is a message shown on failure, so a failing test tells
  you what stderr actually contained.

## 0.8 Checkpoint

Your tree should look like this:

```
Cargo.toml
src/
  error.rs
  lib.rs
  main.rs
tests/
  cli.rs
```

Run the tests:

```sh
cargo test
```

You should see three groups of results: 3 unit tests from `src/lib.rs`, 0 from
`src/main.rs`, and 2 from `tests/cli.rs`.

Then try the binary by hand:

```sh
cargo run            # prints "rtrace: usage: rtrace <command> [args...]"
echo $?              # 1
cargo run -- true    # prints nothing
echo $?              # 0
```

The `--` separates Cargo's own arguments from the ones passed to your program.

## 0.9 Exercises

1. Add a `-h` flag: if the first argument is `-h`, print the usage text to stdout and
   exit 0. Decide whether that belongs in `run` or in `main`, and write a test for it.
2. Remove `pub` from `pub fn run` and read the compiler error from `main.rs`. Then
   restore it.
3. Delete the `pub mod error;` line and read the errors. This is what a file that is
   not declared as a module looks like to the compiler.
4. Add a variant `Error::CommandNotFound(String)` with a `Display` message. Nothing
   uses it yet, but chapter 1 will.

## What comes next

Chapter 1 replaces the `Ok(0)` placeholder in `run` with a real `fork`, `execvp` and
`waitpid`, so that `rtrace ls -l` actually runs `ls -l` and returns its exit status.
