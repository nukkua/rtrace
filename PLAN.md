# rtrace: build an strace in Rust, one chapter at a time

This is the implementation plan for `rtrace`, a small `strace` clone. The project is a
tutorial: each chapter adds one working slice of the tracer, introduces a few Rust
concepts and a few process/kernel concepts, and ends with a checkpoint.

## Ground rules

- **Target**: Linux on x86_64 only. Other architectures are an optional exercise in chapter 10.
- **Dependencies**: `nix` (safe wrappers over `ptrace`, `fork`, `waitpid`, signals) and
  `libc` (raw constants and `user_regs_struct`). Nothing else until chapter 9, which adds `clap`.
- **Checkpoint for every chapter**: `cargo test` passes, and `cargo run -- <some command>`
  shows the new behaviour.
- **Layout**: a library plus a thin binary, so the logic is testable without running a tracer.

```
src/
  main.rs       argument handling, calls into the library
  lib.rs        module declarations and the public `run` function
  tracer.rs     fork/exec, the wait loop, ptrace calls
  syscalls.rs   syscall number -> name and argument-kind table
  decode.rs     turning register values into printable arguments
  memory.rs     reading strings and buffers out of the tracee
  format.rs     building the output line
  error.rs      the project's error type
tests/
  cli.rs        integration tests that run the built binary against real programs
```

- **Testing strategy**:
  - Unit tests (`#[cfg(test)]` next to the code) for pure logic: table lookups, flag
    decoding, string escaping, line formatting.
  - Integration tests in `tests/` that run `rtrace` against `/bin/true`, `/bin/echo` and
    similar, then assert on the output. They use `env!("CARGO_BIN_EXE_rtrace")`.
  - Some sandboxes and containers forbid `ptrace`. Integration tests should detect
    `EPERM` on the first attach and skip with a message, not fail.

## Chapter overview

| # | You build | Rust concepts | Process concepts |
|---|-----------|---------------|------------------|
| 0 | Project skeleton | crates, modules, `cargo test` | none |
| 1 | Launch a child program | `Result`, `?`, `CString`, `match` | `fork`, `exec`, `waitpid`, exit status |
| 2 | Stop the child under ptrace | enums with data, pattern matching | `PTRACE_TRACEME`, stops, `SIGTRAP` |
| 3 | Print raw syscall numbers | structs, loops, state machines | syscall entry/exit stops, registers |
| 4 | Syscall names | `const` tables, slices, `Option` | the x86_64 syscall ABI |
| 5 | Arguments and return values | enums, traits, `impl Display` | argument registers, `-errno` returns |
| 6 | Strings and buffers from the tracee | `unsafe`, byte slices, `Vec<u8>`, lossy UTF-8 | virtual memory, `PTRACE_PEEKDATA`, `process_vm_readv` |
| 7 | Flags and structured arguments | bit operations, iterators, closures | `open` flags, `mmap` protection bits, errno names |
| 8 | Signals and exit | exhaustive `match`, newtypes | signal delivery stops, signal injection, exit codes |
| 9 | CLI options: `-o`, `-e trace=`, `-c`, `-p` | `clap` derive, `HashMap`, `Box<dyn Write>`, `Instant` | `PTRACE_ATTACH`/`SEIZE`, detaching |
| 10 | Follow children: `-f` | `HashMap<Pid, State>`, ownership of per-process state | `fork`/`clone` events, process trees, threads |

## Chapter 0: skeleton

**Goal**: a library crate and a binary crate that compile, with one passing test.

Steps:
1. Add `src/lib.rs` with empty modules and a `run(args) -> Result<i32, Error>` stub.
2. Make `main.rs` collect `std::env::args()`, call `run`, and exit with the returned code.
3. Add `error.rs` with an `Error` enum (`Usage`, `Nix(nix::Error)`, `Io(std::io::Error)`)
   and `From` impls so `?` works.
4. `cargo add nix --features ptrace,process,signal,uio` and `cargo add libc`.

Tests: `run` with no command returns `Error::Usage`.

Learn: binary versus library crates, `mod`, `pub`, how `?` uses `From`.

## Chapter 1: launch a program

**Goal**: `rtrace ls -l` runs `ls -l` and exits with the same status. No tracing yet.

Steps:
1. Convert the arguments to `CString`s before forking.
2. `fork()`. In the child call `execvp`. In the parent call `waitpid`.
3. Map `WaitStatus::Exited(_, code)` to the exit code and `WaitStatus::Signaled` to `128 + signal`.

Tests: integration tests that `rtrace true` exits 0, `rtrace false` exits 1, and a
missing command gives a clear error and a non-zero exit.

Learn: why `fork` is `unsafe` in `nix`, what survives `exec`, why the child must not
allocate or panic between `fork` and `exec`, and zombie processes.

## Chapter 2: become a tracer

**Goal**: the child stops before running its first instruction, and the parent resumes it.

Steps:
1. In the child, call `ptrace::traceme()` and then `raise(SIGSTOP)` before `execvp`.
2. In the parent, wait for the stop and set `PTRACE_O_TRACESYSGOOD | PTRACE_O_EXITKILL`.
3. Resume with `ptrace::cont` and wait for exit.

Tests: the chapter 1 tests still pass; add a test that the tracee's stdout is unchanged.

Learn: the tracer/tracee relationship, the kinds of stop `waitpid` reports,
and why `TRACESYSGOOD` makes syscall stops distinguishable from a real `SIGTRAP`.

## Chapter 3: see the syscalls

**Goal**: print one line per syscall with its raw number, for example `syscall 59 = 0`.

Steps:
1. Replace `cont` with `ptrace::syscall` in a loop.
2. On each `WaitStatus::PtraceSyscall`, read the registers with `ptrace::getregs`.
3. Track entry versus exit with a small `enum Phase { Entry, Exit }`, toggled per stop.
4. On entry record `orig_rax`. On exit print the number and `rax`.
5. Write trace output to stderr, so it does not mix with the tracee's stdout.

Tests: the output of `rtrace true` contains a line for syscall 59 (`execve`) and ends
with one for 231 (`exit_group`), which has no exit stop.

Learn: every syscall produces two stops, `orig_rax` versus `rax`, and why `exit_group`
and a successful `execve` are special cases.

## Chapter 4: names

**Goal**: `execve(...) = 0` in place of numbers.

Steps:
1. Build `syscalls.rs`: a `const` table of `SyscallInfo { nr, name, args: &[ArgKind] }`.
   Start with about 40 common calls and fall back to `syscall_<nr>` for the rest.
2. Add `fn lookup(nr: u64) -> Option<&'static SyscallInfo>`.
3. Optional exercise: generate the full table from `syscall_64.tbl` in a `build.rs`.

Tests: unit tests for known numbers (0 `read`, 1 `write`, 59 `execve`, 257 `openat`)
and for the unknown-number fallback.

Learn: `'static` data, `Option` combinators, and where the kernel defines syscall numbers.

## Chapter 5: arguments and return values

**Goal**: `write(1, 0x7ffd..., 6) = 6` and `access(0x..., 4) = -1 ENOENT`.

Steps:
1. Read the six arguments from `rdi, rsi, rdx, r10, r8, r9`.
2. Define `enum ArgKind { Int, UInt, Fd, Ptr, Str, Buf, Flags(..), ... }` and format by kind.
   Only `Int`, `UInt`, `Fd` and `Ptr` are implemented in this chapter.
3. Interpret a return value in `-4095..=-1` as an error and print the errno name.
4. Print the entry half at entry and the result at exit, so a blocking call such as
   `read` shows up before it returns.

Tests: unit tests for return-value decoding and for formatting a full line from a
hand-built register set.

Learn: the syscall calling convention (`r10` is used where function calls use `rcx`),
signed versus unsigned reinterpretation with `as`, and `impl fmt::Display`.

## Chapter 6: reading the tracee's memory

**Goal**: `openat(AT_FDCWD, "/etc/ld.so.cache", ...)` and `write(1, "hello\n", 6)`.

Steps:
1. `memory::read_bytes(pid, addr, len)` using `process_vm_readv`, with a word-by-word
   `PTRACE_PEEKDATA` fallback.
2. `memory::read_cstring(pid, addr, max)` that reads in chunks until a NUL byte, without
   crossing into an unmapped page.
3. Escape output the way strace does (`\n`, `\t`, `\"`, `\x..` or octal for other
   bytes) and truncate at 32 bytes with a trailing `...`.
4. Decode `Buf` arguments at the right time: `write` buffers at entry, `read` buffers at
   exit, using the return value as the length.
5. Decode `execve`'s `argv` as an array of string pointers.

Tests: unit tests for escaping and truncation. Integration test that
`rtrace echo hello` contains `write(1, "hello\n", 6) = 6`.

Learn: each process has its own address space, so a pointer from the tracee means
nothing in the tracer. Also `unsafe` boundaries, `&[u8]` versus `&str`, and
`String::from_utf8_lossy`.

## Chapter 7: flags, constants and errno

**Goal**: `openat(AT_FDCWD, "/x", O_RDONLY|O_CLOEXEC) = -1 ENOENT (No such file or directory)`.

Steps:
1. A generic `format_flags(value, &[(u64, &str)])` that joins matched names with `|`
   and prints leftover bits in hex.
2. Tables for `open` flags, `mmap` protection and mapping flags, `AT_*` constants and
   `access` modes.
3. Handle `O_RDONLY == 0` and the access-mode mask as a special case.
4. Add the errno description from `strerror`.

Tests: unit tests for each table, covering zero, a single flag, combined flags and
unknown bits.

Learn: bit masks, iterator chains (`filter`, `map`, `collect`, `join`), and passing
tables as slices.

## Chapter 8: signals and exit

**Goal**: `--- SIGINT {si_signo=SIGINT, ...} ---` and `+++ exited with 0 +++`.

Steps:
1. Handle `WaitStatus::Stopped(pid, sig)`: print the signal and pass it on by giving
   `sig` to the next `ptrace::syscall` call. Dropping it here is the classic tracer bug.
2. Read `siginfo` with `ptrace::getsiginfo` for the detail line.
3. Handle `Exited` and `Signaled`: print `+++ exited with N +++` or `+++ killed by SIG +++`.
4. Make `rtrace` exit with the tracee's status.
5. Handle the `execve` special cases properly with `PTRACE_O_TRACEEXEC`.

Tests: integration tests using `sh -c 'kill -TERM $$'` and `sh -c 'exit 7'`.

Learn: signal-delivery stops versus syscall stops, signal injection, and the wait
status encoding.

## Chapter 9: a real command line

**Goal**: the options people use most.

Steps:
1. Replace hand-rolled parsing with `clap` derive.
2. `-o FILE`: send trace output through a `Box<dyn Write>`.
3. `-e trace=open,read`: filter by name with a `HashSet<&str>`.
4. `-c`: count calls, errors and time per syscall, and print a summary table sorted by time.
5. `-p PID`: attach to a running process with `PTRACE_SEIZE` and `PTRACE_INTERRUPT`,
   and detach cleanly on Ctrl-C.
6. `-s N` for the string limit and `-t` for timestamps, as short exercises.

Tests: integration tests for each option. Unit test for the summary table formatting.

Learn: trait objects versus generics, `HashMap` entry API, `Instant` and `Duration`,
and signal handling in the tracer itself.

## Chapter 10: follow forks

**Goal**: `rtrace -f sh -c 'ls | wc -l'` traces every process, with a `[pid N]` prefix.

Steps:
1. Set `PTRACE_O_TRACEFORK | TRACEVFORK | TRACECLONE`.
2. Change `waitpid(child)` to `waitpid(-1, __WALL)` and keep
   `HashMap<Pid, TraceeState>` with each process's phase and pending syscall.
3. Handle `PTRACE_EVENT_*` stops and the new child's initial `SIGSTOP`, which can arrive
   before or after the parent's event.
4. Print `<unfinished ...>` and `<... resumed>` when output from two processes interleaves.
5. Finish when the map is empty.

Tests: integration test that a forking shell command produces lines for at least two pids
and that every traced pid gets a `+++ exited` line.

Learn: per-process state machines, event ordering races, and threads as processes
that share memory.

Optional follow-ups: aarch64 support behind a `trait Arch`, decoding `struct stat` and
`sockaddr`, `PTRACE_GET_SYSCALL_INFO` as a replacement for raw registers, and seccomp-based
tracing for speed.

## How to work through it

1. Read the chapter's goal and "Learn" notes.
2. Write the tests listed for the chapter first and watch them fail.
3. Implement the steps in order.
4. Run `cargo test`, then compare `cargo run -- <cmd>` against the real `strace <cmd>`.
5. Move on only when both look right.
