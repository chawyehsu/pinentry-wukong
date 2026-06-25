# pinentry-wukong

A cross-platform [pinentry](https://gnupg.org/related_software/pinentry/index.html) alternative built with Rust

## What is pinentry?

Pinentry is a passphrase dialog invoked by [gpg-agent](https://gnupg.org/documentation/manuals/gnupg/Agent-Options.html) when GPG operations require a secret key. The standard pinentry implementations (pinentry-gtk2, pinentry-qt, pinentry-curses) are written in C with toolkit-specific dependencies.

**pinentry-wukong** replaces all of them with a single Rust binary that:

- Works on **Windows, macOS, and Linux** from a single codebase
- Uses **ratatui** for a modern, clean terminal UI
- Supports **native OS keychain** integration (macOS Keychain, Linux Secret Service, Windows Credential Manager)
- Implements the **Assuan protocol** for drop-in compatibility with gpg-agent

## Installation

### From source

```sh
cargo install --locked pinentry-wukong
```

### Pre-built binaries

Check the [releases](https://github.com/chawyehsu/pinentry-wukong/releases) page.

## Configuration

Set pinentry-wukong as your pinentry program in `~/.gnupg/gpg-agent.conf`:

```plain
pinentry-program /path/to/pinentry-wukong
```

Then restart gpg-agent:

```sh
gpgconf --kill gpg-agent
```

## Usage

pinentry-wukong is designed to be invoked by gpg-agent automatically. You can also test it manually:

```sh
# Interactive TUI mode
echo -e "SETDESC Enter passphrase to unlock key ABC123\nSETPROMPT Passphrase:\nGETPIN" | pinentry-wukong

# Force TTY fallback mode
echo -e "GETPIN" | pinentry-wukong --ui=tty
```

### CLI options

```
Usage: pinentry-wukong [OPTIONS] [COMMAND]

Commands:
  completions  Generate shell completions
  help         Print this message or the help of the given subcommand(s)

Options:
      --ui <UI>                    Force a specific UI mode (tui, tty)
  -o, --timeout <TIMEOUT>          Timeout in seconds
  -D, --display <DISPLAY>          X display name
  -T, --ttyname <TTYNAME>          TTY terminal node name
  -N, --ttytype <TTYTYPE>          TTY terminal type
  -C, --lc-ctype <LC_CTYPE>        Set LC_CTYPE value
  -M, --lc-messages <LC_MESSAGES>  Set LC_MESSAGES value
  -v, --verbose...                 Increase logging verbosity
  -q, --quiet...                   Decrease logging verbosity
  -h, --help                       Print help
  -V, --version                    Print version
```

## Supported Assuan Commands

| Command | Status | Description |
| --------- | -------- | ------------- |
| `SETDESC` | ✅ | Set description text |
| `SETPROMPT` | ✅ | Set prompt label |
| `SETERROR` | ✅ | Set error message |
| `SETTITLE` | ✅ | Set window title |
| `SETOK` / `SETCANCEL` / `SETNOTOK` | ✅ | Set button labels |
| `GETPIN` | ✅ | Get passphrase from user |
| `CONFIRM` | ✅ | Show confirmation dialog |
| `MESSAGE` | ✅ | Show one-button message |
| `GETINFO` | ✅ | Return version/pid/flavor |
| `OPTION` | ✅ | Set session options |
| `SETKEYINFO` | ✅ | Set key identifier for caching |
| `CLEARPASSPHRASE` | ✅ | Clear cached passphrase |
| `SETQUALITYBAR` | 🔜 | Quality indicator (post-MVP) |
| `SETREPEAT` | 🔜 | Repeat passphrase (post-MVP) |
| `INQUIRE` | 🔜 | Callbacks to gpg-agent (post-MVP) |

## Security

- Passphrase buffers are zeroed on drop using the [`zeroize`](https://docs.rs/zeroize) crate
- Terminal raw mode prevents passphrase echo
- No passphrase data is logged or persisted by pinentry-wukong
- OS keychain integration relies on the platform's native security (macOS Keychain, Linux Secret Service, Windows Credential Manager)

## License

**pinentry-wukong** © [Chawye Hsu](https://github.com/chawyehsu). Released under the [GPL-2.0-only](LICENSE) License.

> [Blog](https://chawyehsu.com) · GitHub [@chawyehsu](https://github.com/chawyehsu) · Twitter [@chawyehsu](https://twitter.com/chawyehsu)
