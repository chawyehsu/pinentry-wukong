# pinentry-wukong

> A versatile [pinentry](https://gnupg.org/related_software/pinentry/index.html) implementation written in Rust

[![crates-svg]][crates-url]
[![license][license-badge]](LICENSE-APACHE)
[![codecov][codecov-badge]][codecov]
[![release][release-ci-badge]][release-ci-url]

## What is pinentry?

Pinentry is a passphrase dialog invoked by [gpg-agent](https://gnupg.org/documentation/manuals/gnupg/Agent-Options.html) when GPG operations require a secret key. The standard pinentry implementations (pinentry-gtk2, pinentry-qt, pinentry-curses, pinentry-tty, pinentry-w32) are written in C and platform-specific.

**pinentry-wukong** replaces all of them with a single Rust binary that:

- Works on **Windows, macOS, and Linux** from a single codebase
  - **Yes** you can use the *TTY* mode on Windows as well, natively!
- **Native OS keychain** integration (macOS Keychain, Linux Secret Service, Windows Credential Manager)
- Uses **ratatui** for a modern, clean terminal UI
- More to come! GUI...

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

Make sure to set the `GPG_TTY` environment variable in your shell configuration (e.g., `~/.bashrc`, `~/.zshrc`):

```sh
export GPG_TTY=$(tty)
```

for PowerShell on macOS/Linux:

```powershell
$env:GPG_TTY = $(tty)
```

for PowerShell on Windows:

```powershell
$env:GPG_TTY = "/conhost/$PID"
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

```console
Usage: pinentry-wukong [OPTIONS] [COMMAND]

Commands:
  serve        Run the pinentry server
  completions  Generate shell completions
  config       Manage configuration
  help         Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...            Increase logging verbosity
  -q, --quiet...              Decrease logging verbosity
      --debug                 Enable debug logging (equivalent to -vvv)
      --config <PATH>         Path to a custom config file
      --keyring               Enable system keyring for secret management (default)
      --no-keyring            Disable system keyring for secret management
  -D, --display <DISPLAY>     X display name (ignored on non-X11)
  -T, --ttyname <FILE>        TTY terminal node name
  -N, --ttytype <NAME>        TTY terminal type
  -C, --lc-ctype <STRING>     TTY LC_CTYPE value
  -M, --lc-messages <STRING>  TTY LC_MESSAGES value
  -o, --timeout <SECS>        Input timeout in seconds (default: 60)
  -g, --no-global-grab        Grab keyboard only when the window is focused
      --ui <MODE>             Force a specific UI mode
  -h, --help                  Print help (see more with '--help')
  -V, --version               Print version
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

[crates-svg]: https://img.shields.io/crates/v/pinentry-wukong.svg?style=flat&logo=rust
[crates-url]: https://crates.io/crates/pinentry-wukong
[license-badge]: https://img.shields.io/github/license/chawyehsu/pinentry-wukong?style=flat&logo=spdx
[codecov-badge]: https://img.shields.io/codecov/c/gh/chawyehsu/pinentry-wukong?style=flat&logo=codecov
[codecov]: https://codecov.io/github/chawyehsu/pinentry-wukong
[release-ci-badge]: https://github.com/chawyehsu/pinentry-wukong/actions/workflows/release.yml/badge.svg
[release-ci-url]: https://github.com/chawyehsu/pinentry-wukong/actions/workflows/release.yml
