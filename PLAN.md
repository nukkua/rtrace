# rtrace: build an strace in Rust, test first

This is the implementation plan for `rtrace`, a small `strace` clone. The project is a
tutorial with two goals:

1. Build a working tracer, one chapter at a time.
2. **Learn test-driven development (TDD) well enough to use it on any project**: Rust,
   Node, Vue, anything else.

Every chapter is written as a **test list**, the TDD to-do list. You never write
production code unless a failing test asks for it. Each chapter also teaches one TDD
skill and shows how that skill looks in a Node or Vue project, so the habit carries
over to other languages.

## Part 1: the TDD method used in every chapter

### The cycle

```
        +--------+     +---------+     +------------+
   ---> |  RED   | --> |  GREEN  | --> |  REFACTOR  | ---+
   |    +--------+     +---------+     +------------+    |
   +-----------------------------------------------------+
```

1. **Red**: write *one* small test for the next behaviour on the list. Run it. Watch it
   fail, and check that it fails *for the reason you expect* (an assertion, not a typo).
   A test you have never seen fail is a test you cannot trust.
2. **Green**: write the *least* code that makes it pass. Hard-coding a return value is
   allowed and often right. The next test will force you to generalise.
3. **Refactor**: with every test green, clean up names, duplication and structure, in
   both the code and the tests. Run the tests after each small change. No new behaviour
   here.
4. Tick the item off the test list, add any new cases you thought of, and pick the next one.

Commit after each green or refactor step if you like. Small commits make it easy to go
back when a step goes wrong.

### The three rules (Uncle Bob's "three laws")

1. Do not write production code except to make a failing test pass.
2. Do not write more of a test than you need to see it fail. A compile error counts as a failure.
3. Do not write more production code than you need to pass the one failing test.

### Ways to get to green (Kent Beck)

- **Fake it**: return a constant. Then add a second test that the constant cannot pass.
- **Triangulate**: add examples until the general solution is the simplest one.
- **Obvious implementation**: when the code is obvious, just type it. Drop back to
  fake-it as soon as a test surprises you.

### The test list

Before you code, write the behaviours down as a list (in a comment, a notebook, or the
chapter's checklist below). Order them from simplest to hardest, and start with the
degenerate cases: empty input, zero, one item. When a new idea comes up mid-cycle, *add
it to the list*. Do not chase it now.

### A good test

- One behaviour per test. The test name says the behaviour: `unknown_syscall_falls_back_to_number`.
- Arrange, act, assert (given, when, then), in that order, visibly.
- Fast and deterministic: no sleeps, no network, no dependence on test order.
- Tests the public behaviour, not private details, so a refactor does not break it.

### Test layers in this project

| Layer | Where | What it tests | Speed |
|-------|-------|---------------|-------|
| Unit | `#[cfg(test)] mod tests` next to the code | pure logic: tables, decoding, escaping, formatting, the tracer state machine with a fake | microseconds |
| Integration | `tests/*.rs`, via `env!("CARGO_BIN_EXE_rtrace")` | the real binary against `/bin/true`, `/bin/echo`, `sh -c ...` | milliseconds |

Most tests are unit tests. Each chapter starts with **one** failing integration test
for its goal (the *outer loop*, a.k.a. the acceptance test), then drives the details
with unit tests (the *inner loop*). The outer test goes green last. This "double loop"
is the same shape you will use with Playwright or Cypress around Vitest in a Vue app.

Some sandboxes and containers forbid `ptrace`. Integration tests that trace must detect
`EPERM` on the first attach and skip with a message, not fail. That helper is itself
test-driven in chapter 2.

### The hard part: code that talks to the kernel

`fork`, `waitpid` and `ptrace` cannot run inside a fast unit test. TDD solves this with
a **seam**: put the side effects behind a small trait, give the real tracer one
implementation, and give the tests a scripted fake.

```rust
pub trait Kernel {
    fn wait(&mut self) -> Result<Event, Error>;   // wraps waitpid
    fn regs(&mut self, pid: Pid) -> Result<Regs, Error>;
    fn resume(&mut self, pid: Pid, sig: Option<Signal>) -> Result<(), Error>;
    // more methods are added only when a test needs them
}
```

The decision logic (what to print, when to resume, when to stop) is then plain code you
can unit-test by feeding it a list of events. Only a thin adapter calls `nix`, and the
integration tests cover that adapter. This is the same idea as passing a fake `fetch`
or a mocked API module to a Node service or a Vue composable.

### Layout

TDD decides the final shape, but this is where the project is heading:

```
src/
  main.rs       argument handling, calls into the library
  lib.rs        module declarations and the public `run` function
  kernel.rs     the `Kernel` trait, the real nix adapter, the test fake
  tracer.rs     the wait loop and state machine, written against `Kernel`
  syscalls.rs   syscall number -> name and argument-kind table
  decode.rs     turning register values into printable arguments
  memory.rs     reading strings and buffers out of the tracee
  format.rs     building the output line
  error.rs      the project's error type
tests/
  cli.rs        integration tests that run the built binary against real programs
```

### Ground rules

- **Target**: Linux on x86_64 only.
- **Dependencies**: `nix` and `libc`. `clap` arrives in chapter 9. No mocking library:
  the fakes are hand-written so you see what a mock really is.
- **Checkpoint for every chapter**: `cargo test` passes, every item on the chapter's
  test list is ticked, and `cargo run -- <cmd>` looks like `strace <cmd>` for the new part.

## Part 2: chapter overview

| # | You build | TDD skill | Rust concepts | Process concepts |
|---|-----------|-----------|---------------|------------------|
| 0 | Project skeleton | the red-green-refactor cycle, seeing a test fail | crates, modules, `cargo test` | none |
| 1 | Launch a child program | outside-in: start from an acceptance test | `Result`, `?`, `CString`, `match` | `fork`, `exec`, `waitpid`, exit status |
| 2 | Stop the child under ptrace | seams and fakes for side effects | traits, enums with data | `PTRACE_TRACEME`, stops, `SIGTRAP` |
| 3 | Print raw syscall numbers | testing a state machine with scripted events | structs, loops, state machines | syscall entry/exit stops, registers |
| 4 | Syscall names | fake it, then triangulate | `const` tables, slices, `Option` | the x86_64 syscall ABI |
| 5 | Arguments and return values | test builders and readable fixtures | enums, traits, `impl Display` | argument registers, `-errno` returns |
| 6 | Strings and buffers | edge-case lists and boundary tests | `unsafe`, byte slices, lossy UTF-8 | virtual memory, `process_vm_readv` |
| 7 | Flags and constants | table-driven tests | bit operations, iterators, closures | `open` flags, `mmap` bits, errno |
| 8 | Signals and exit | bug-first: reproduce with a failing test | exhaustive `match`, newtypes | signal stops, injection, exit codes |
| 9 | CLI options | testing I/O by injecting `Write`, fake clocks | `clap`, `HashMap`, `Box<dyn Write>` | `PTRACE_SEIZE`, detaching |
| 10 | Follow children | refactoring under a green bar | `HashMap<Pid, State>`, ownership | `fork`/`clone` events, threads |

## Part 3: the chapters

Each chapter has the same parts:

- **Goal**: what the user sees at the end.
- **Acceptance test**: the outer test you write first. It stays red for most of the chapter.
- **Test list**: the inner cycles, in order. Write one test, make it pass, refactor, repeat.
- **Refactor checkpoints**: clean-ups to do once the related tests are green.
- **TDD skill** and **In Node or Vue**: the transferable lesson.
- **Learn**: the Rust and Linux background.

### Chapter 0: skeleton

**Goal**: a library crate and a binary crate that compile, with tests passing.

Acceptance test (`tests/cli.rs`): `rtrace` with no arguments exits 1 and prints
`usage: rtrace` on stderr.

Test list:
- [ ] `run(&[])` returns `Err(Error::Usage)`. Red first: it fails to compile because
      `run` does not exist. That counts. Then add the stub.
- [ ] `run(&["true"])` returns `Ok(0)` (fake it: hard-code `Ok(0)`).
- [ ] a `nix::Errno` converts into `Error::Nix` through `?` (forces the `From` impl).
- [ ] `Error::Usage` displays as `usage: rtrace <command> [args...]`.
- [ ] Make the acceptance test green by wiring `main.rs` to `run`.

Refactor checkpoint: `main.rs` is only argument collection, `run`, and `process::exit`.

TDD skill: the cycle itself, and *watching the test fail*. Break the code on purpose
once (return `Ok(1)`) to check that each test catches it.

In Node or Vue: the same first test in Vitest is `expect(() => run([])).toThrow(UsageError)`.
Set up `vitest --watch` (or `cargo watch -x test` here) so the cycle takes seconds.

Learn: binary versus library crates, `mod`, `pub`, how `?` uses `From`.
Detailed walkthrough: [docs/chapter-00.md](docs/chapter-00.md).

### Chapter 1: launch a program

**Goal**: `rtrace ls -l` runs `ls -l` and exits with the same status. No tracing yet.

Acceptance test: `rtrace false` exits 1. (The chapter 0 test `a_command_exits_zero_for_now`
still passes by accident. Write `false` first so the outer test is really red.)

Test list:
- [ ] unit: `to_cstrings(&["ls", "-l"])` gives two `CString`s.
- [ ] unit: an argument containing a NUL byte gives an error, not a panic.
- [ ] unit: `exit_code(WaitStatus::Exited(pid, 3))` is `3`.
- [ ] unit: `exit_code(WaitStatus::Signaled(pid, SIGKILL, false))` is `128 + 9`.
- [ ] integration: `rtrace true` exits 0, `rtrace false` exits 1.
- [ ] integration: `rtrace sh -c 'exit 7'` exits 7 (triangulates the exit code).
- [ ] integration: `rtrace no-such-cmd` exits non-zero with `command not found: no-such-cmd`.
- [ ] integration: arguments reach the child: `rtrace echo hi` prints `hi` on stdout.

Then implement `fork` + `execvp` + `waitpid` in `run`, the one part that is only
covered by integration tests.

Refactor checkpoint: pure helpers (`to_cstrings`, `exit_code`) live apart from the
`fork` code, so the untested-by-unit-tests part is as small as possible.

TDD skill: **outside-in**. The acceptance test says *what*, the unit tests drive *how*.
Split pure logic out of side-effecting code so most of it is unit-testable.

In Node: an acceptance test that spawns your CLI with `execa` or `child_process.spawnSync`
and checks `exitCode`, with Vitest unit tests for the argument parsing underneath.

Learn: why `fork` is `unsafe` in `nix`, what survives `exec`, why the child must not
allocate or panic between `fork` and `exec`, and zombie processes.
Detailed walkthrough: [docs/chapter-01.md](docs/chapter-01.md).

### Chapter 2: become a tracer

**Goal**: the child stops before its first instruction, and the parent resumes it.

Acceptance test: `rtrace echo hi` still prints exactly `hi\n` on stdout and exits 0
(tracing must not change the tracee's behaviour).

Test list:
- [ ] Create `kernel.rs` with the `Kernel` trait and an `Event` enum
      (`Stopped(pid, sig)`, `Exited(pid, code)`, `Signaled(pid, sig)`, ...).
- [ ] Write `FakeKernel`: it returns scripted events from a `VecDeque` and records every
      call (`resume`, `set_options`) in a `Vec` so tests can assert on them.
- [ ] unit: given `[Stopped(SIGSTOP), Exited(0)]`, the tracer sets
      `TRACESYSGOOD | EXITKILL` once, then resumes once, and returns 0.
- [ ] unit: the tracer resumes with no signal after the initial `SIGSTOP` (it must not
      re-inject it).
- [ ] unit: an `Err(EPERM)` from the first wait gives `Error::PermissionDenied`.
- [ ] integration helper: `skip_if_no_ptrace()` returns early with a message on `EPERM`.
- [ ] Write `NixKernel`, the real adapter. It is covered by the acceptance test only.

Refactor checkpoint: `run` builds a `NixKernel` and passes it to `trace(&mut impl Kernel)`.
`trace` contains no `nix` calls.

TDD skill: **seams and test doubles**. Learn the names: a *stub* returns canned answers,
a *spy* records calls, a *fake* is a working light version, a *mock* checks expectations.
`FakeKernel` is a stub and a spy. Do not fake what you do not own directly: wrap it
first, then fake your wrapper.

In Node or Vue: pass the HTTP client into a service or composable (`useUsers(api)`) and
give tests `{ get: vi.fn().mockResolvedValue([...]) }`. Prefer injection over
`vi.mock('module')` when you can, for the same reason we use a trait here.

Learn: the tracer/tracee relationship, the kinds of stop `waitpid` reports, and why
`TRACESYSGOOD` makes syscall stops distinguishable from a real `SIGTRAP`.

### Chapter 3: see the syscalls

**Goal**: one line per syscall with its raw number, for example `syscall 59 = 0`, on stderr.

Acceptance test: `rtrace true` stderr contains `syscall 59` and its last trace line is
`syscall 231 = ?` (`exit_group` has no exit stop).

Test list (all with `FakeKernel`, extended with `Syscall(pid)` events and scripted registers):
- [ ] one entry + exit pair prints one line `syscall 0 = 3`.
- [ ] two pairs print two lines in order (forces the `Phase { Entry, Exit }` toggle).
- [ ] the number comes from `orig_rax` at entry, the result from `rax` at exit.
- [ ] an entry followed by `Exited` prints `syscall 231 = ?`.
- [ ] the tracer resumes with `syscall`, not `cont`, after every stop (spy assertion).
- [ ] output goes to the `Write` you pass in, not straight to stderr. Tests pass a
      `Vec<u8>`; `run` passes `io::stderr()`.

Refactor checkpoint: extract a `TracerState` struct. Name the phases. Remove duplication
from the test setup with a helper like `script(&[entry(0), exit(3)])`.

TDD skill: **testing a state machine** by scripting its input events and asserting on
its output and the calls it makes. No real process needed, so you can test orderings
that are hard to produce for real.

In Vue: the same idea tests a component's state machine: mount it, trigger events
(`await wrapper.trigger('click')`), assert on the rendered output, never on internal refs.

Learn: every syscall produces two stops, `orig_rax` versus `rax`, and why `exit_group`
and a successful `execve` are special cases.

### Chapter 4: names

**Goal**: `execve(...) = 0` in place of numbers.

Acceptance test: `rtrace true` stderr contains a line starting `execve(` and one
starting `exit_group(`.

Test list:
- [ ] `lookup(0)` is `read`. Fake it: `if nr == 0 { return Some(&READ) }`.
- [ ] `lookup(1)` is `write`. The fake no longer works: introduce the `const` table.
- [ ] `lookup(59)` is `execve`, `lookup(257)` is `openat` (triangulation).
- [ ] `lookup(99999)` is `None`.
- [ ] `syscall_name(99999)` is `syscall_99999`.
- [ ] the tracer line uses the name (update the chapter 3 tests, which now go red: that
      is expected, change their expectations deliberately).
- [ ] property check: no two entries in the table share a number (a test that loops
      over the whole table).

Refactor checkpoint: the table is sorted by number, and a test enforces that.

TDD skill: **fake it, then triangulate**, and **changing a test on purpose** when the
requirement changes. A red test after a requirement change is information, not failure.

In Node: `getStatusLabel(200)` returns `'OK'` hard-coded, then `404` forces a lookup
object. Same rhythm.

Learn: `'static` data, `Option` combinators, and where the kernel defines syscall numbers.
Optional: generate the full table from `syscall_64.tbl` in `build.rs`, test-first.

### Chapter 5: arguments and return values

**Goal**: `write(1, 0x7ffd..., 6) = 6` and `access(0x..., 4) = -1 ENOENT`.

Acceptance test: `rtrace sh -c 'ls /nonexistent'` stderr contains a line ending `= -1 ENOENT`.

Test list:
- [ ] start a test builder: `RegsBuilder::new().syscall(1).args([1, 0x1000, 6]).ret(6).build()`.
- [ ] `args(&regs)` returns `rdi, rsi, rdx, r10, r8, r9` in that order (use six
      distinct values so a swapped register fails).
- [ ] `format_arg(ArgKind::Int, u64::MAX)` is `-1`; `UInt` is `18446744073709551615`.
- [ ] `format_arg(ArgKind::Fd, 1)` is `1`; `Ptr` of `0` is `NULL`, of `0x1000` is `0x1000`.
- [ ] `decode_ret(6)` is `Ok(6)`; `decode_ret(-2i64 as u64)` is `Err(ENOENT)`.
- [ ] boundaries: `-4095` is an error, `-4096` is not (it is a valid big address).
- [ ] full line from builder regs: `write(1, 0x1000, 6) = 6`.
- [ ] entry half is written at entry, result at exit: after only an entry event the
      output is `write(1, 0x1000, 6` with no newline.

Refactor checkpoint: `impl Display` for the line type. Tests read like the strace output.

TDD skill: **test builders and readable fixtures**. Tests should show only the values
that matter for that test; the builder fills in the rest.

In Node or Vue: factory functions like `makeUser({ admin: true })` (or a library like
`fishery`), so each test shows only the field it cares about.

Learn: the syscall calling convention (`r10` instead of `rcx`), signed versus unsigned
reinterpretation with `as`, and `impl fmt::Display`.

### Chapter 6: reading the tracee's memory

**Goal**: `openat(AT_FDCWD, "/etc/ld.so.cache", ...)` and `write(1, "hello\n", 6)`.

Acceptance test: `rtrace echo hello` stderr contains `write(1, "hello\n", 6) = 6`.

Test list, escaping (pure):
- [ ] `escape(b"")` is `""`.
- [ ] `escape(b"abc")` is `"abc"`.
- [ ] `\n`, `\t`, `\"`, `\\` are escaped.
- [ ] a non-printable byte `0x01` is `\1` (octal, like strace).
- [ ] invalid UTF-8 does not panic.
- [ ] 32 bytes print in full; 33 bytes print 32 and a trailing `...`.

Test list, memory (through the `Kernel` seam: add `read_mem(pid, addr, len)` and let
`FakeKernel` serve bytes from a `HashMap<u64, Vec<u8>>`):
- [ ] `read_cstring` stops at the first NUL.
- [ ] `read_cstring` with no NUL within `max` returns `max` bytes and a truncated flag.
- [ ] a string that runs up to a page boundary is read in chunks without crossing it.
- [ ] a failed read prints the raw pointer instead of a string.
- [ ] `write` buffers are read at entry, `read` buffers at exit with `rax` as the length.
- [ ] `execve`'s `argv` decodes as `["echo", "hello"]`.

Then: `NixKernel::read_mem` with `process_vm_readv` and a `PTRACE_PEEKDATA` fallback,
covered by the acceptance test.

TDD skill: **edge-case lists**. For every input, list empty, one, many, the boundary,
one past the boundary, and invalid. Write them all on the test list before you start.

In Vue: the same list for a text input: empty, whitespace, max length, max length + 1,
emoji. In Node: the same for a pagination function: page 0, last page, past the end.

Learn: each process has its own address space, so a tracee pointer means nothing in the
tracer. Also `unsafe` boundaries, `&[u8]` versus `&str`, `String::from_utf8_lossy`.

### Chapter 7: flags, constants and errno

**Goal**: `openat(AT_FDCWD, "/x", O_RDONLY|O_CLOEXEC) = -1 ENOENT (No such file or directory)`.

Acceptance test: `rtrace cat /x` stderr contains exactly that `openat` line.

Test list (write each table as one table-driven test: a `const CASES: &[(u64, &str)]`
and a loop that names the failing case in its message):
- [ ] `format_flags(0, TABLE)` is `0`.
- [ ] one flag; two flags joined with `|`; unknown bits appended as `0x...`.
- [ ] open flags: `0` is `O_RDONLY`, `O_WRONLY|O_CREAT`, the access-mode mask special case.
- [ ] `mmap` protection: `PROT_NONE`, `PROT_READ|PROT_WRITE`.
- [ ] `AT_FDCWD` is shown by name for `dirfd` arguments.
- [ ] `errno_name(2)` is `ENOENT`; an unknown errno prints as its number.
- [ ] errno description comes from `strerror`.

Refactor checkpoint: one generic `format_flags` serves every table. No per-table loops.

TDD skill: **table-driven (parameterised) tests**. One test body, many rows. Adding a
case is one line.

In Node or Vue: `test.each([[0, 'O_RDONLY'], [1, 'O_WRONLY']])('flags %i', (v, s) => ...)`
in Vitest or Jest.

Learn: bit masks, iterator chains (`filter`, `map`, `collect`, `join`), tables as slices.

### Chapter 8: signals and exit

**Goal**: `--- SIGINT {si_signo=SIGINT, ...} ---` and `+++ exited with 0 +++`.

Acceptance tests: `rtrace sh -c 'exit 7'` exits 7 and ends with `+++ exited with 7 +++`;
`rtrace sh -c 'kill -TERM $$'` ends with `+++ killed by SIGTERM +++`.

Test list:
- [ ] **bug first**: write a test that a `Stopped(pid, SIGINT)` event is passed on to the
      next resume (`resume(pid, Some(SIGINT))`). Your chapter 2 code drops it. See it
      fail, then fix it. This is the classic tracer bug, and now a test guards it forever.
- [ ] a syscall stop (`SIGTRAP | 0x80`) is *not* injected.
- [ ] `Stopped` prints a `--- SIGINT ... ---` line using `siginfo` from the fake.
- [ ] `Exited(0)` prints `+++ exited with 0 +++`; `Signaled(SIGTERM)` prints `+++ killed by SIGTERM +++`.
- [ ] `run` returns the tracee's status for both cases.
- [ ] `PTRACE_EVENT_EXEC` is handled: the line for a successful `execve` is complete.

TDD skill: **every bug starts as a failing test**. Reproduce first, fix second. The test
proves the fix and stops the bug from coming back.

In Node or Vue: on a bug report, first write the Vitest test that reproduces it (or a
Playwright test if it only shows in the browser), commit it red if your team allows,
then fix.

Learn: signal-delivery stops versus syscall stops, signal injection, the wait status encoding.

### Chapter 9: a real command line

**Goal**: `-o FILE`, `-e trace=open,read`, `-c`, `-p PID`, `-s N`, `-t`.

Acceptance tests: one integration test per option, for example `rtrace -o out.txt true`
leaves stderr empty and writes the trace to `out.txt` (use a temp dir).

Test list:
- [ ] parsing: `Options::parse_from(["rtrace", "-e", "trace=open,read", "ls"])` gives a
      filter with two names and the command `["ls"]`. Write these tests *before*
      switching to `clap`, so they protect the switch.
- [ ] parsing: unknown option is a usage error; `--` stops option parsing.
- [ ] filter: a filtered-out syscall prints nothing, but the tracer still resumes it.
- [ ] `-s 5` truncates strings at 5 bytes.
- [ ] `-c`: feed the summary a list of `(name, duration, is_error)` and assert on the
      rendered table, sorted by time.
- [ ] `-t`: timestamps come from an injected clock (`trait Clock`), so the test can say
      the time is `12:00:00` and assert on it.
- [ ] `-p`: with the fake, attaching sends `seize` + `interrupt`, and Ctrl-C leads to `detach`.

Refactor checkpoint: output goes through one `Box<dyn Write>` chosen in `run`.

TDD skill: **inject what you cannot control**: output streams, files, clocks, randomness.
Never assert on real wall-clock time.

In Node or Vue: `vi.useFakeTimers()` and `vi.setSystemTime(...)`, writing to an injected
stream, `msw` for HTTP. In Vue, `provide`/`inject` is the built-in seam.

Learn: trait objects versus generics, `HashMap` entry API, `Instant` and `Duration`,
signal handling in the tracer itself.

### Chapter 10: follow forks

**Goal**: `rtrace -f sh -c 'ls | wc -l'` traces every process, with a `[pid N]` prefix.

Acceptance test: the output has lines for at least two pids, and every traced pid gets
a `+++ exited` line.

Start with a **refactor under green**: change the tracer from one `TracerState` to a
`HashMap<Pid, TracerState>` while all existing tests stay green. Small steps, run the
tests after each. Only then add behaviour.

Test list:
- [ ] existing single-process tests still pass with the map (no new test, the refactor).
- [ ] a `PTRACE_EVENT_FORK` event adds the new pid to the map.
- [ ] the new child's `SIGSTOP` arriving *before* the parent's event is handled.
- [ ] the same `SIGSTOP` arriving *after* the event is handled (the fake makes both orders easy).
- [ ] interleaved syscalls print `<unfinished ...>` and `<... resumed>`.
- [ ] lines get a `[pid N]` prefix only when more than one process is traced.
- [ ] the loop ends when the map is empty.

TDD skill: **refactoring with a safety net**, and **testing races deterministically** by
scripting each event order instead of hoping the real system produces it.

In Node: the same trick tests code with concurrent promises: control resolution order
with manually resolved promises instead of real timing.

Learn: per-process state machines, event ordering races, threads as processes that share memory.

Optional follow-ups, each test-first: aarch64 support behind a `trait Arch`, decoding
`struct stat` and `sockaddr`, `PTRACE_GET_SYSCALL_INFO` instead of raw registers.

## Part 4: taking TDD to other projects

### The same tools in each ecosystem

| Need | Rust (this project) | Node | Vue |
|------|---------------------|------|-----|
| Test runner | `cargo test` | Vitest or `node --test` | Vitest |
| Watch mode | `cargo watch -x test` | `vitest --watch` | `vitest --watch` |
| Unit tests | `#[cfg(test)] mod tests` | `*.test.ts` next to the file | `*.spec.ts` next to the component |
| Table-driven tests | loop over a `const` slice | `test.each` | `test.each` |
| Test doubles | hand-written trait fakes | dependency injection, `vi.fn()` | props, `provide`/`inject`, `vi.fn()` |
| Fake time | inject a `Clock` | `vi.useFakeTimers()` | `vi.useFakeTimers()` |
| Fake HTTP | not needed | `msw` | `msw` |
| Rendering | not needed | not needed | `@vue/test-utils` or Testing Library `render` |
| Outer acceptance test | `tests/cli.rs` runs the binary | `supertest` for an HTTP API, or spawn the CLI | Playwright or Cypress |

### The questions to ask in any project

1. What is the next smallest behaviour? Write it on the test list.
2. Can I see this test fail for the right reason?
3. What is the least code that passes?
4. Is anything duplicated or badly named now that it is green?
5. Is there a side effect (network, disk, time, kernel, DOM) in the way? Put a seam there.

### A first Vue cycle, for practice

A `Counter.vue` that shows `0`, increments on click, and never goes above a `max` prop:

1. Red: `render(Counter)` shows `0`. Fails: the component does not exist.
2. Green: a template that prints `0`.
3. Red: after `click`, it shows `1`. Green: add a `ref` and a handler.
4. Red: with `max: 1`, two clicks show `1`. Green: clamp in the handler.
5. Refactor: move the logic into a `useCounter(max)` composable; the tests do not change,
   because they test behaviour through the UI.

### A first Node cycle, for practice

A `slugify(title)` function: `'' -> ''`, `'Hello' -> 'hello'`, `'Hello World' -> 'hello-world'`,
`'  a  b ' -> 'a-b'`, `'Crème' -> 'creme'`. One `test.each` row per case, added one at a time.

## How to work through each chapter

1. Read the chapter's goal and "Learn" notes.
2. Copy the test list into a scratch file or the top of the test module.
3. Write the acceptance test. Run it. See it fail.
4. Take the first item on the list: red, green, refactor. Tick it. Repeat.
5. When the list is done, the acceptance test should pass. If it does not, the list was
   missing a case: add a unit test for it.
6. Run `cargo test`, then compare `cargo run -- <cmd>` with `strace <cmd>`.
7. Move on only when every item is ticked and both look right.
