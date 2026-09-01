# Input key handling

## Overview

Each platform reads terminal input through a different mechanism:

- **Unix**: raw bytes from the TTY file descriptor via `libc::read`. Multi-byte ANSI escape sequences must be parsed manually.
- **Windows**: structured `INPUT_RECORD` from `ReadConsoleInputW`. The console has already parsed key events, so no escape sequence handling is needed.

Both platforms produce a shared `Key` enum consumed by the dialog logic (`handle_getpin_key`, confirm, message handlers).

## Key enum

```rust
enum Key {
    Char(char),
    Enter,
    Esc,
    Backspace,
    Tab,
    BackTab,
    CtrlC,
    Up,
    Down,
    Left,
    Right,
}
```

## Byte-to-Key mapping

### Unix (`read_key`)

Reads one byte at a time from the TTY fd. When `0x1b` (ESC) is received, waits 50ms (via `select`) for a second byte to determine whether it's a bare ESC or the start of an escape sequence.

| Byte(s) | Key | Notes |
|---------|-----|-------|
| `0x0d` (`\r`), `0x0a` (`\n`) | Enter | |
| `0x7f`, `0x08` | Backspace | DEL and BS |
| `0x03` | CtrlC | ETX |
| `0x09` | Tab | |
| `0x1b` (alone, 50ms timeout) | Esc | No following bytes within 50ms |
| `0x1b 0x5b 0x41` (`\x1b[A`) | Up | CSI sequence |
| `0x1b 0x5b 0x42` (`\x1b[B`) | Down | CSI sequence |
| `0x1b 0x5b 0x44` (`\x1b[D`) | Left | CSI sequence |
| `0x1b 0x5b 0x43` (`\x1b[C`) | Right | CSI sequence |
| `0x1b 0x5b 0x5a` (`\x1b[Z`) | BackTab | CSI sequence (xterm) |
| `0x1b 0x5b` + unknown | *ignored* | Drains remaining bytes through terminator (`m`/`M`), loops to read next input |
| `0x1b` + non-`[` byte | *ignored* | Consumes the byte, loops to read next input |
| `0x20..=0x7e` | Char(c) | Printable ASCII |
| Everything else | *ignored* | Control chars outside 0x20-0x7e |

### Windows (`input_record_to_key`)

Reads `INPUT_RECORD` via `ReadConsoleInputW`. Non-key events (mouse, focus, resize) are filtered by checking `EventType == KEY_EVENT` and `bKeyDown == TRUE`. BackTab is detected via `dwControlKeyState & SHIFT_PRESSED`.

| UnicodeChar | Key | Notes |
|-------------|-----|-------|
| `0x0003` | CtrlC | ETX |
| `0x000d`, `0x000a` | Enter | CR and LF |
| `0x0008`, `0x007f` | Backspace | BS and DEL |
| `0x0009` | Tab | Without SHIFT |
| `0x0009` + SHIFT | BackTab | `dwControlKeyState & 0x0010` |
| `0x001b` | Esc | |
| `0x0000` + arrow virtual key | Up/Down/Left/Right | Uses `wVirtualKeyCode` |
| `0x0000` + other virtual key | *ignored* | Unhandled control/function key |
| `0xD800..=0xDBFF` | *ignored* | UTF-16 high surrogate, waits for low |
| Other | Char(c) | `char::from_u32(c as u32)` |

## Unknown input handling

The two platforms handle unrecognized input differently:

- **Unix**: unknown CSI escape sequences (mouse events, page up/down, etc.) are explicitly drained — remaining bytes are consumed through the sequence terminator (`m` or `M`) so they don't pollute the next `read_key` call. Bare ESC followed by non-`[` is also consumed and ignored.
- **Windows**: the console delivers one parsed key event per `INPUT_RECORD`. Non-key events are filtered at the event type level. Arrow keys with `UnicodeChar == 0x0000` are mapped using `wVirtualKeyCode`; other such keys are silently ignored.

Both approaches result in the same behavior: unrecognized inputs are discarded, and only the `Key` variants above reach the dialog logic.

## Why the implementations are not shared

Unix needs ~90 lines of byte-level escape sequence parsing (`select` timeouts, multi-byte reads, drain loops). Windows needs ~30 lines of structured event matching. The `Key` enum is the shared contract; the input mechanisms are fundamentally different and are kept as platform-specific code.
