//- [ ] unit: `to_cstrings(["ls", "-l"])` gives two `CString`s
//- [ ] unit: an argument containing a NUL byte gives an error, not a panic
//- [ ] unit: `exit_code(WaitStatus::Exited(pid, 3))` is `3`
//- [ ] unit: `exit_code(WaitStatus::Signaled(pid, SIGKILL, false))` is `128 + 9`
//- [ ] integration: `rtrace true` exits 0, `rtrace false` exits 1
//- [ ] integration: `rtrace sh -c 'exit 7'` exits 7
//- [ ] integration: `rtrace no-such-cmd` exits non-zero with `command not found: no-such-cmd`
//- [ ] integration: `rtrace echo hi` prints `hi` on stdout
