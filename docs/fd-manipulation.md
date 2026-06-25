# Why the TUI needs fd manipulation

## Root cause

gpg-agent spawns pinentry with stdin/stdout as an **Assuan protocol pipe**. But the TUI needs a **terminal** for user input. These are two different I/O channels sharing the same file descriptors (fd 0 and fd 1).

The pinentry process must:
1. Read/write Assuan protocol on the pipe (fd 0/1)
2. Read/write terminal UI on `/dev/tty`
3. These happen in the same process, on the same fds

This forces a save → redirect → use → restore cycle every time the UI is shown.

## The fd dance

```
gpg-agent
   │
   ├── stdin (pipe)  ──► pinentry fd 0
   └── stdout (pipe) ◄── pinentry fd 1
```

When the TUI runs:
```
1. dup(fd 0) → saved_stdin     // save pipe read end
2. dup(fd 1) → saved_stdout    // save pipe write end
3. open(/dev/tty) → tty_fd     // open terminal directly
4. dup2(tty_fd, 0)             // redirect stdin to terminal
5. dup2(tty_fd, 1)             // redirect stdout to terminal
5. enable_raw_mode()           // configure terminal for TUI
6. [TUI runs, reads from tty_fd, writes to fd 1 (now terminal)]
7. disable_raw_mode()
8. dup2(saved_stdin, 0)        // restore pipe stdin
9. dup2(saved_stdout, 1)       // restore pipe stdout
10. close(saved_stdin)
11. close(saved_stdout)
```

## Why FdWriter / FdReader exist

The Assuan server needs to read/write the pipe while the TUI may have redirected fd 0/1. Solution: dup the pipe fds at startup and use raw fd readers/writers that bypass `std::io::stdin()`/`stdout()`.

Additionally, holding `StdinLock` blocks crossterm's event system (it tries to acquire the same mutex), so we use `FdReader` (raw fd) instead of `stdin.lock()`.

## Why select() instead of poll()

macOS `poll()` returns `POLLNVAL` for `/dev/tty` file descriptors. `select()` works correctly. This is a known macOS kernel quirk.

## Why cleanup_terminal writes escape sequences directly

After the TUI exits, crossterm's `disable_raw_mode()` restores terminal settings, but the escape sequences to leave the alternate screen and show the cursor are written to crossterm's internal stdout handle (which may point to the wrong fd after redirect). Writing them directly to the tty fd ensures they reach the terminal.

## Platform notes

On Windows this would be completely different:
- ConPTY for pseudo-terminal
- Named pipes for Assuan protocol
- `CONIN$`/`CONOUT$` for terminal access
- No fd-level manipulation possible (Windows doesn't share the Unix fd model)

The platform-specific code should be isolated in a single module (`platform.rs`) with `#[cfg(unix)]` / `#[cfg(windows)]` blocks. The rest of the codebase should use platform-agnostic `Read`/`Write` traits.
