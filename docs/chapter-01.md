# Chapter 1: launch a program

By the end of this chapter `rtrace ls -l` really runs `ls -l` and exits with the same
status `ls` did. Nothing is traced yet. A tracer is first of all a program that starts
another program and waits for it, and that is the part you build here. Chapter 2 adds
one line to the child (`PTRACE_TRACEME`) and the launcher becomes a tracer.

**You will learn**

- how Unix starts a program: `fork`, `exec` and `waitpid`
- what an exit status is, and what the shell reports when a signal kills a process
- why `fork` is `unsafe` in `nix`, and what the child may do before `exec`
- `CString`, and why a Rust `String` cannot be handed to the kernel as it is
- the TDD skill of this chapter: working **outside-in**, from one acceptance test down
  to unit tests

**Checkpoint**: `cargo test` reports 13 passing tests (7 unit, 6 integration), and
`cargo run -- sh -c 'exit 7'; echo $?` prints `7`.

## 1.1 How a program gets started

Chapter 0 used `std::process::Command` in the integration tests. It hides three system
calls. A tracer has to do something *between* them, so we call them ourselves.

```
   rtrace (parent)
      |
      | fork()                       one process becomes two
      +-----------------------+
      |                       |
      | waitpid(child)        | execvp("ls", ["ls", "-l"])
      |   (sleeps)            |   the child's program is replaced by ls
      |                       |   ... ls runs ...
      |                       | exit(0)
      | <---------------------+
      | status = Exited(child, 0)
      v
   exit(0)
```

**`fork`** copies the calling process. After it returns there are two processes running
the same code at the same line. The only difference is the return value: the parent
gets the child's process ID, the child gets 0. `nix` turns that into an enum,
`ForkResult::Parent { child }` or `ForkResult::Child`, so a `match` tells you who you are.

**`exec`** replaces the program running in the current process with a new one. The
process ID stays the same, but the code, the stack and the heap are thrown away and the
new program starts at its `main`. If `exec` succeeds it *never returns*: the code that
called it no longer exists. It only returns when it fails. There is a family of `exec`
functions. We use `execvp`: `v` means the arguments come as an array (a vector), `p`
means the command is searched in `PATH`, so `ls` finds `/usr/bin/ls`.

**`waitpid`** blocks the parent until the child changes state, and reports how. In this
chapter the only state changes are the two ways a process can end:

| `WaitStatus` | Meaning | What the shell shows in `$?` |
|--------------|---------|------------------------------|
| `Exited(pid, code)` | the program called `exit(code)` or returned from `main` | `code` |
| `Signaled(pid, signal, core_dumped)` | a signal killed it | `128 + signal number` |

`128 + n` is a shell convention, not a kernel one: an exit code is one byte, and the
shell uses the upper half for "killed by signal `n`". `rtrace` follows the same
convention, so `rtrace cmd; echo $?` prints what `cmd; echo $?` would.

### What survives `exec`

`exec` replaces the memory but keeps the process. So the new program inherits the
process ID, the parent, the working directory, the environment (with `execvp`), and the
open file descriptors. That last one is why `rtrace echo hi` prints on your terminal
without any extra work: `echo` inherits `rtrace`'s standard output. In chapter 2 one
more thing survives: the "I am being traced" flag.

### Zombies

When a process ends, the kernel keeps a small record with its exit status until the
parent collects it with `waitpid`. A process in that state is a *zombie*: it runs no
code and holds no memory, but it still has an entry in the process table. `waitpid` is
therefore not optional. It is how you get the status, and it is also what removes the
zombie.

## 1.2 The acceptance test (outer loop, red)

Outside-in means: before any design, write down what the user should see, as a test
that runs the real binary. This test is the definition of "done" for the chapter.

The plan suggests `rtrace false` exits 1, and not `rtrace true` exits 0. Look at
`tests/cli.rs` to see why: `a_command_exits_zero_for_now` already passes, because `run`
returns the hard-coded `Ok(0)`. A test that passes before you write the code proves
nothing. `false` is a program that does nothing and exits with 1, so the fake cannot
satisfy it.

Add to `tests/cli.rs`:

```rust
#[test]
fn false_exits_one() {
    let status = rtrace().arg("false").status().expect("failed to run rtrace");

    assert_eq!(status.code(), Some(1));
}
```

Run `cargo test` and read the failure:

```
assertion `left == right` failed
  left: Some(0)
 right: Some(1)
```

This is the right kind of red: the assertion failed, and it failed because the
behaviour is missing, not because of a typo. **Leave this test red.** It stays red
until section 1.7. Everything in between is driven by small unit tests.

## 1.3 The test list

To make the outer test pass, `run` needs to do four things: convert the arguments to
the form `execvp` wants, fork, exec, and turn the wait status into an exit code. Two of
those are pure logic (same input, same output, no side effects), and two talk to the
kernel. Pure logic is cheap to unit-test, so it goes first:

- [ ] unit: `to_cstrings(["ls", "-l"])` gives two `CString`s
- [ ] unit: an argument containing a NUL byte gives an error, not a panic
- [ ] unit: `exit_code(WaitStatus::Exited(pid, 3))` is `3`
- [ ] unit: `exit_code(WaitStatus::Signaled(pid, SIGKILL, false))` is `128 + 9`
- [ ] integration: `rtrace true` exits 0, `rtrace false` exits 1
- [ ] integration: `rtrace sh -c 'exit 7'` exits 7
- [ ] integration: `rtrace no-such-cmd` exits non-zero with `command not found: no-such-cmd`
- [ ] integration: `rtrace echo hi` prints `hi` on stdout

Copy the list somewhere you can tick it off (a comment at the top of the new file works).
When an idea comes up while you are in the middle of a cycle, add it to the list and
keep going.

## 1.4 A new module: `src/launch.rs`

The code of this chapter goes into its own module. Create an empty `src/launch.rs` and
declare it in `src/lib.rs`, below the existing `pub mod error;`:

```rust
pub mod error;
mod launch;
```

There is no `pub` this time. `error` is public because `main.rs` and the integration
tests need the `Error` type. `launch` is an implementation detail: the only thing
outside the library that matters is `run`. Private by default, public when something
needs it.

## 1.5 Cycle 1: `to_cstrings`

### Why `CString`

The kernel's `execve` takes C strings: a pointer to bytes that end with a zero byte
(NUL, written `\0`). A Rust `String` is a pointer plus a length, has no terminator, and
may legally contain `\0` in the middle. So the two do not fit, in both directions:

- a `String` must get a NUL appended before the kernel can use it
- a `String` with a NUL *inside* cannot be represented at all: C would see the string
  end at that byte

`std::ffi::CString` is an owned, NUL-terminated string. `CString::new(s)` copies the
bytes, appends the terminator, and returns `Err(NulError)` if it finds a NUL inside.
That error case is the second item on the test list.

### Red

Add to the bottom of `src/launch.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn arguments_become_cstrings() {
        let argv = to_cstrings(&strings(&["ls", "-l"])).unwrap();

        assert_eq!(argv.len(), 2);
        assert_eq!(argv[0].as_c_str(), c"ls");
        assert_eq!(argv[1].as_c_str(), c"-l");
    }
}
```

- `strings` is a test helper. `run` receives `&[String]`, so `to_cstrings` will too,
  and writing `"ls".to_string()` in every test hides what the test is about.
- `c"ls"` is a C string literal: a `&CStr` with the terminator added by the compiler.
- The test has the three visible parts: arrange (the input), act (`to_cstrings`),
  assert.

`cargo test` fails with `cannot find function to_cstrings`. A compile error counts as
red.

### Green

Add at the top of `src/launch.rs`:

```rust
use std::ffi::CString;

use crate::Error;

fn to_cstrings(args: &[String]) -> Result<Vec<CString>, Error> {
    let mut out = Vec::with_capacity(args.len());
    for arg in args {
        out.push(CString::new(arg.as_str()).unwrap());
    }
    Ok(out)
}
```

The test passes. (The compiler warns that `to_cstrings` is never used outside the
tests. That warning is your to-do list: it goes away in section 1.7.)

The `unwrap()` is on purpose. Rule three says: no more production code than the failing
test needs, and no test has asked for error handling yet. The next one does.

### Red again: the NUL byte

```rust
    #[test]
    fn nul_byte_in_argument_is_an_error() {
        let result = to_cstrings(&strings(&["ls", "a\0b"]));

        assert!(matches!(result, Err(Error::Nul(_))));
    }
```

This fails to compile because `Error::Nul` does not exist. Chapter 0 gave the rule for
this situation: a new kind of error means a new variant and a `From` impl. In
`src/error.rs`:

```rust
use std::ffi::NulError;
```

```rust
pub enum Error {
    Usage,
    Nix(nix::Error),
    Io(io::Error),
    CommandNotFound(String),
    Nul(NulError),
}
```

```rust
            Error::Nul(err) => write!(f, "invalid argument: {err}"),
```

```rust
impl From<NulError> for Error {
    fn from(err: NulError) -> Self {
        Error::Nul(err)
    }
}
```

Notice that the compiler forces the `Display` arm on you: the `match` in `fmt` no
longer covers every variant. Add only the variant first and read that error once.

Now the test compiles and fails the way we want: it *panics* inside `to_cstrings`, at
the `unwrap()`. That is exactly the behaviour the test list says must not happen.

### Green

Replace `.unwrap()` with `?`:

```rust
        out.push(CString::new(arg.as_str())?);
```

`CString::new` returns `Result<CString, NulError>`, the function returns
`Result<_, Error>`, and `?` converts one into the other through the `From` impl you
just wrote. Both tests pass. Tick two items off the list.

### Refactor (optional)

The loop can be written as an iterator chain. `collect` can build a
`Result<Vec<_>, _>` from an iterator of `Result`s, stopping at the first error:

```rust
fn to_cstrings(args: &[String]) -> Result<Vec<CString>, Error> {
    args.iter()
        .map(|arg| CString::new(arg.as_str()).map_err(Error::from))
        .collect()
}
```

Both versions are fine. If you change it, run the tests afterwards: that is what makes
it a refactor and not a rewrite.

## 1.6 Cycle 2: `exit_code`

`waitpid` returns a `WaitStatus`. `run` returns an `i32`. The function between them is
pure, so it gets unit tests, and those tests can describe a child killed by `SIGKILL`
without killing anything.

### Red

Add to the test module (the two `use` lines go next to `use super::*;`):

```rust
    use nix::sys::signal::Signal;
    use nix::unistd::Pid;

    #[test]
    fn normal_exit_gives_its_status() {
        let status = WaitStatus::Exited(Pid::from_raw(42), 3);

        assert_eq!(exit_code(status), 3);
    }
```

`Pid::from_raw(42)` builds a process ID out of a plain number. No process 42 is
involved: `exit_code` never looks at it.

### Green: fake it

```rust
use nix::sys::wait::WaitStatus;

fn exit_code(status: WaitStatus) -> i32 {
    3
}
```

This looks silly, and it is the method working as intended. The test passes, so by the
rules you are done, and it is obvious that the code is wrong. That means the *tests*
are incomplete. The next test fixes that.

### Triangulate

```rust
    #[test]
    fn death_by_signal_gives_128_plus_signal() {
        let status = WaitStatus::Signaled(Pid::from_raw(42), Signal::SIGKILL, false);

        assert_eq!(exit_code(status), 128 + 9);
    }
```

Red: `left: 3, right: 137`. No constant satisfies both tests, so now write the real
thing:

```rust
fn exit_code(status: WaitStatus) -> i32 {
    match status {
        WaitStatus::Exited(_, code) => code,
        WaitStatus::Signaled(_, signal, _) => 128 + signal as i32,
        _ => 1,
    }
}
```

- `_` inside a pattern ignores a field. We do not need the pid or the core-dump flag.
- `signal as i32` converts the `Signal` enum to its number. `SIGKILL` is 9.
- The last arm, `_ => 1`, is required because `WaitStatus` has more variants
  (`Stopped`, `PtraceEvent`, `Continued`, ...). `waitpid` cannot return them the way we
  call it in this chapter: they only show up with extra flags or under `ptrace`. They
  are the subject of chapter 2, where this arm is replaced by real handling.

Both tests pass. Tick two more items off the list.

## 1.7 The part with side effects: `launch`

All the pure pieces exist. What is left is the code that calls the kernel. It cannot
run inside a unit test in any useful way, so it is covered by the integration tests
only. The acceptance test from section 1.2 has been red all along, so there *is* a
failing test asking for this code.

Change the imports at the top of `src/launch.rs` to:

```rust
use std::ffi::CString;
use std::io;

use nix::errno::Errno;
use nix::sys::wait::{WaitStatus, waitpid};
use nix::unistd::{ForkResult, execvp, fork, write};

use crate::Error;
```

and add, above `to_cstrings`:

```rust
pub fn launch(args: &[String]) -> Result<i32, Error> {
    let argv = to_cstrings(args)?;
    let not_found = format!("rtrace: {}\n", Error::CommandNotFound(args[0].clone()));

    match unsafe { fork() }? {
        ForkResult::Child => {
            let Err(errno) = execvp(&argv[0], &argv);
            let code = match errno {
                Errno::ENOENT => {
                    let _ = write(io::stderr(), not_found.as_bytes());
                    127
                }
                _ => 126,
            };
            unsafe { libc::_exit(code) }
        }
        ForkResult::Parent { child } => {
            let status = waitpid(child, None)?;
            Ok(exit_code(status))
        }
    }
}
```

Then, in `src/lib.rs`, replace the final `Ok(0)` of `run` (keep the usage check and
your `-h` branch above it):

```rust
    launch::launch(args)
```

Run `cargo test`. `false_exits_one` is green: the outer loop is closed. One unit test
needs attention first, see the next section. Before that, the function line by line.

**`to_cstrings(args)?`** runs before `fork`. A bad argument is reported by the parent
as a normal `Err`, and no child is ever created.

**`not_found`** is the error message for a missing command, built *before* `fork` even
though it is only needed if `exec` fails. The reason is the rule about the child,
explained below. It uses the `CommandNotFound` variant you added in exercise 4 of
chapter 0, through its `Display` impl. `args[0]` cannot panic here: `run` has already
returned `Error::Usage` for an empty slice.

**`unsafe { fork() }?`** has two things on it. The `?` handles a failed fork (the
system is out of processes or memory): the errno becomes `Error::Nix`. The `unsafe`
block is the subject of the next subsection.

**`ForkResult::Child`** is the branch the new process runs.

- `execvp(&argv[0], &argv)` takes the program to run and the *full* argument list. By
  convention the list starts with the program's own name, which is why `argv[0]`
  appears twice. This is the same convention that made you write `.skip(1)` in
  `main.rs`.
- `execvp` returns `Result<Infallible, Errno>`. `Infallible` is a type with no values:
  an `Ok` can never be built, because on success `execvp` does not return. The compiler
  knows this, so `let Err(errno) = ...;` is accepted without an `else`: there is no
  other case to handle.
- `ENOENT` ("no such file or directory") means the command was not found in `PATH`.
  The child prints the message and exits with 127. Any other error (for example
  `EACCES`: the file exists but is not executable) exits with 126. Both numbers are
  what a shell uses for the same situations. Exercise 2 gives the 126 case a message.
- `write(io::stderr(), ...)` is the raw `write` system call from `nix`, not `eprintln!`.
  `let _ =` discards its result on purpose: if writing to stderr fails there is nothing
  left to report it to.
- `libc::_exit(code)` ends the child immediately. It is `unsafe` only because every
  function in `libc` is.

**`ForkResult::Parent { child }`** is the branch `rtrace` itself runs. `waitpid(child,
None)` sleeps until the child ends (`None` means no option flags), which also reaps
the zombie, and `exit_code` turns the status into the number `run` returns.

Note the shape: `launch` contains no logic that could have been tested separately. It
converts, forks, execs, waits and delegates. That is the refactor checkpoint of this
chapter: the pure helpers live apart from the `fork` code, so the part without unit
tests is as small as it can be.

### Why `fork` is `unsafe`, and the rule for the child

`fork` copies the memory of the process but only *one* thread: the one that called it.
Suppose another thread was in the middle of allocating memory and held the allocator's
lock. In the child that lock is copied in its locked state, and the thread that would
unlock it does not exist. The first allocation in the child waits forever.

Rust cannot check this, so `nix` marks `fork` as `unsafe`. The `unsafe` block is you
saying: I know the rule and I follow it. The rule is: **between `fork` and `exec`, the
child only makes simple system calls.** In practice:

| Not in the child | Why | What we do |
|------------------|-----|------------|
| allocate (`format!`, `String`, `Vec`, `CString::new`) | the allocator lock may be held | build `argv` and `not_found` before `fork` |
| `println!` / `eprintln!` | they take a lock, and may hold buffered data copied from the parent | the raw `write` system call |
| panic | unwinding allocates and runs the parent's cleanup code in the child | no `unwrap`, no indexing that can fail |
| `std::process::exit` | runs exit handlers and flushes buffers that belong to the parent | `libc::_exit` |
| `return` from `launch` | the child would carry on as a second `rtrace`, back into `main` | every path ends in `exec` or `_exit` |

`rtrace` has a single thread, so most of this could not actually go wrong today. The
rule is still worth following exactly: the child branch becomes the heart of the tracer
in chapter 2, and "it happens to work" is a poor foundation for it.

## 1.8 A unit test that now forks

Look at the unit tests in `src/lib.rs`. `a_command_is_accepted` calls
`run(&["true"])`. It still passes, but it no longer tests what it did: it now forks the
*test runner* and runs `/bin/true`. The test runner is multi-threaded (it runs tests in
parallel), which is the one situation where the `fork` rule really matters, and a unit
test should not start processes anyway.

The behaviour it checks belongs to the integration layer now. So:

1. Delete `a_command_is_accepted` from `src/lib.rs`.
2. In `tests/cli.rs`, rename `a_command_exits_zero_for_now` and make the assertion
   exact. "For now" is over:

```rust
#[test]
fn true_exits_zero() {
    let status = rtrace().arg("true").status().expect("failed to run rtrace");

    assert_eq!(status.code(), Some(0));
}
```

Your `-h` test and the usage test stay: both return before `launch` is reached.

Deleting or moving a test is a normal part of TDD. Tests are code, and a test that sits
at the wrong layer costs more than it gives. What matters is that the behaviour is
still covered, and it is, by `true_exits_zero` and `false_exits_one`.

## 1.9 The remaining integration tests

The rest of the list is integration tests. The code for them already exists, so each of
these passes on the first run. That breaks the "see it fail" rule, so make each one
fail once on purpose, as chapter 0 taught: break the code, watch the test catch it,
restore the code. A suggested break is given with each test.

**The exit code is passed on, whatever it is.** `true` and `false` only cover 0 and 1.
A third value triangulates: an implementation like `if failed { 1 } else { 0 }` would
pass both of the others.

```rust
#[test]
fn exit_status_of_the_child_is_passed_on() {
    let status = rtrace()
        .args(["sh", "-c", "exit 7"])
        .status()
        .expect("failed to run rtrace");

    assert_eq!(status.code(), Some(7));
}
```

Break it: change `WaitStatus::Exited(_, code) => code` to `=> code.min(1)`.

**An unknown command is reported.**

```rust
#[test]
fn unknown_command_is_reported() {
    let output = rtrace()
        .arg("no-such-cmd")
        .output()
        .expect("failed to run rtrace");

    assert_eq!(output.status.code(), Some(127));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("command not found: no-such-cmd"),
        "stderr was: {stderr}"
    );
}
```

Break it: remove the `write` line in the child.

Think about who prints this message. It is the *child*: only the child knows that
`exec` failed. The parent just sees a process that exited with 127, exactly as if the
command itself had returned 127. That is how shells behave too. (`strace` is stricter:
it checks that the command exists before it forks.)

**Arguments reach the child, and its output reaches us.**

```rust
#[test]
fn arguments_reach_the_child() {
    let output = rtrace()
        .args(["echo", "hi"])
        .output()
        .expect("failed to run rtrace");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "hi\n");
}
```

Break it: pass `&argv[..1]` as the second argument of `execvp`.

The assertion is `assert_eq!` on the whole of stdout, not `contains`. `rtrace` must add
nothing to the traced program's output. Chapter 2 uses this exact test as its
acceptance test, to prove that tracing does not change what the tracee does.

Every item on the test list is now ticked.

## 1.10 The TDD skill: outside-in

Look back at the order of the work:

1. One acceptance test, written first, in the user's terms: `rtrace false` exits 1.
   It said *what* to build and stayed red for most of the chapter.
2. A list of smaller behaviours needed to get there.
3. Unit tests for the pure parts, each a red-green cycle of a minute or two. They
   decided *how*: the signatures of `to_cstrings` and `exit_code` came out of writing
   their tests.
4. A thin piece of glue with the side effects, written last, which turned the
   acceptance test green.

Two habits make this work on any project:

- **Split pure logic from side effects.** `to_cstrings` and `exit_code` could have been
  three lines inline in `launch`. Pulled out, they are testable in microseconds,
  including cases that are awkward to produce for real, such as a child killed by
  `SIGKILL` or an argument with a NUL byte (try typing one in a shell).
- **Keep the untested-by-unit-tests part thin and boring.** `launch` has no decisions
  of its own beyond the errno `match`, so a handful of integration tests cover it.

The outer test also tells you when to stop. When it is green and the list is empty, the
chapter is done. Anything else you feel like adding is a new item for a new list.

### In Node or Vue

The same double loop for a Node CLI: the acceptance test spawns the real program,

```js
import { spawnSync } from 'node:child_process'
import { expect, test } from 'vitest'

test('exits with the status of the command', () => {
  const result = spawnSync('node', ['bin/cli.js', 'false'])
  expect(result.status).toBe(1)
})
```

and underneath it, plain Vitest unit tests drive the pure functions
(`parseArgs(['-v', 'ls'])`, `exitCodeFor({ signal: 'SIGKILL' })`), which never spawn
anything. In a Vue app the outer test is a Playwright or Cypress test that clicks
through the page, and the inner tests are Vitest tests of composables and pure
functions. The shape is the same: one slow test that says what, many fast tests that
drive how.

## 1.11 Checkpoint

Your tree should look like this:

```
Cargo.toml
src/
  error.rs
  launch.rs
  lib.rs
  main.rs
tests/
  cli.rs
```

Run the tests:

```sh
cargo test
```

You should see 7 unit tests from `src/lib.rs` (3 in `tests`, 4 in `launch::tests`) and
6 integration tests from `tests/cli.rs`. There should be no warnings: `to_cstrings` and
`exit_code` are used now.

Then try the binary by hand:

```sh
cargo run -- ls -l                 # the listing, as if you had run ls -l
cargo run -- sh -c 'exit 7'
echo $?                            # 7
cargo run -- no-such-cmd           # rtrace: command not found: no-such-cmd
echo $?                            # 127
cargo run -- sleep 100             # press Ctrl-C
echo $?                            # 130, which is 128 + SIGINT (2)
```

In the last one, Ctrl-C sends `SIGINT` to both `rtrace` and `sleep`, so `rtrace` itself
is killed and the shell reports 130 for it. Exercise 1 produces a case where only the
child dies.

## 1.12 Exercises

1. Add an integration test for the signal path: `rtrace sh -c 'kill -KILL $$'` exits
   with 137. (`$$` is the shell's own process ID, so the child kills itself.) It should
   pass straight away. Break `exit_code` to see it fail.
2. A command that exists but cannot be executed exits with 126 and prints nothing. Test
   first: `rtrace /etc/passwd` should print `cannot execute: /etc/passwd` on stderr.
   You need a new `Error` variant, and a second message built before `fork`.
3. Put `eprintln!("in the child")` just before `execvp` and run `cargo run -- true`. It
   works. Explain to yourself why it is still against the rule in section 1.7. Then
   remove it.
4. Remove the `waitpid` line and return `Ok(0)` from the parent branch. Which tests go
   red? Run `cargo run -- sleep 2` and look at where the shell prompt comes back. Then
   restore it.
5. `waitpid` fails with `EINTR` if a signal handler interrupts it. `rtrace` installs no
   handlers, so it cannot happen yet. Add "retry `waitpid` on `EINTR`" to your test
   list for chapter 8 and do not implement it now. Knowing what to postpone is part of
   the method.

## What comes next

Chapter 2 adds one call in the child, between `fork` and `exec`: `ptrace::traceme()`.
From then on `waitpid` reports more than exits. It reports *stops*, and the `_ => 1`
arm in `exit_code` is no longer enough. To test that logic without a real process you
will put the kernel behind a trait and write your first fake.
