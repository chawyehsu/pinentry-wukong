# pinentry-wukong

A cross-platform pinentry replacement for GnuPG's gpg-agent. Implements the Assuan IPC protocol to prompt users for passphrases via terminal UI or line-based fallback.

## Language

**Terminal Access**:
Platform-specific mechanism for obtaining a handle to the user's interactive terminal. On Unix, this is always `/dev/tty` or `OPTION ttyname`. On Windows, this is the console (`CONIN$`/`CONOUT$`) obtained via console resolution.
_Avoid_: TTY access, console handle (too vague)

**Console Resolution**:
The process of determining which Windows console to use. Three-tier: Direct → Ttyname → Allocated.
_Avoid_: console detection, TTY detection

**Console Source**:
How the Windows console was obtained.
- **Direct**: stdin is already a real console (`GetConsoleMode` succeeds)
- **Ttyname**: `OPTION ttyname` provided `/conhost/<pid>`, attached via `AttachConsole`
- **Allocated**: `AllocConsole()` created a new console window (last resort)

_Avoid_: console type, console mode (overloaded with `GetConsoleMode`)

**TTY Resolver**:
Windows-only helper executable that finds the conhost PID hosting the user's shell and sets `$env:GPG_TTY=/conhost/<pid>`. Equivalent to Unix's `tty` command, which returns the terminal device path (e.g., `/dev/ttys002`). Part of the cooperation chain: TTY Resolver → `$env:GPG_TTY` → gpg → gpg-agent → `OPTION ttyname` → pinentry-wukong.
_Avoid_: TTY helper, console finder

**TTY Device**:
On Unix: `/dev/tty` or the path from `OPTION ttyname`. On Windows: the console devices `CONIN$` and `CONOUT$` opened from the resolved console source.
_Avoid_: terminal device, TTY path (platform-specific)

**Assuan Protocol**:
GPG's IPC protocol. Text commands on stdin, responses on stdout. pinentry-wukong implements the server side.

**Pinentry State**:
Accumulates all configuration from Assuan commands (`SETDESC`, `SETPROMPT`, `SETERROR`, etc.) before a UI action (`GETPIN`, `CONFIRM`, `MESSAGE`).

**UI Mode**:
Which UI backend to use: `Tui` (ratatui widgets), `Tty` (line-based fallback), or `Auto` (detect from `$TERM`).

**Secret Bytes**:
A `Vec<u8>` wrapper that is zeroed on drop. Holds passphrase data securely.

**Caller Feedback**:
Text supplied by the Assuan caller to explain the current UI action, such as an incorrect passphrase. It is displayed verbatim for the next UI action and then consumed.
_Avoid_: locally generated retry message, `ERROR:` prefix
