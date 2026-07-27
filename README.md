# pinentry-wukong

> A versatile [pinentry](https://gnupg.org/related_software/pinentry/index.html) implementation written in Rust

[![crates-svg]][crates-url]
[![license][license-badge]](LICENSE-APACHE)
[![codecov][codecov-badge]][codecov]
[![release][release-ci-badge]][release-ci-url]

## What is pinentry?

Pinentry is a passphrase dialog invoked by `gpg-agent` when GPG operations require a secret key. The standard pinentry implementations (pinentry-gtk2, pinentry-qt, pinentry-curses, pinentry-tty, pinentry-w32) are written in C and platform-specific.

These implementations have various issues, such as:

- Windows versions are GUI-only, there is no TUI/TTY/CLI mode available, making it impossible to use GPG in headless environments, such as SSH sessions, on Windows.
- Lack of native OS keychain integration, on macOS there's `pinentry-mac`, a third-party implementation that uses macOS Keychain, but there is no equivalent for Windows or Linux.
- Inconsistent UI across platforms, the Windows version has a focus-capture issue that necessitates the use of a mouse.
- Platform-specific distribution, some implementations are not available on all platforms, for example, `pinentry-mac` is only available on macOS. This makes it difficult for users to reuse and share their gnupg configuration across platforms.

**pinentry-wukong** replaces all of them with a single Rust binary that:

- Works and ships on **Windows, macOS, and Linux** from a single codebase. Same binary works on all platforms, share your gnupg configuration across platforms.
- **TTY** mode is available on all platforms, including Windows. You can use GPG in headless environments, such as SSH sessions, on Windows.
- **Native OS keychain** integration (macOS Keychain, Linux Secret Service, Windows Credential Manager), so you can store your passphrases securely in the OS keychain and avoid typing them repeatedly.
- Uses **ratatui** for a modern, clean terminal UI
- More to come! GUI...

![](https://github.com/user-attachments/assets/36d4e8ea-b878-4e98-a93a-74cda978eb73)
_pinentry-wukong TUI mode on Windows (WIP, non-final deliverable)_

## Getting started

### Install

**pinentry-wukong** is available for installation via different ways.

#### Conda (Cross-platform)

You can install pinentry-wukong with conda/mamba/[pixi](https://pixi.sh) from our conda-forge channel:

```sh
pixi global install pinentry-wukong -c chawyehsu -c conda-forge
```

#### Cargo (Cross-platform)

If you have [cargo-binstall](https://github.com/cargo-bins/cargo-binstall), this downloads pre-built binaries without compiling from source:

```sh
cargo-binstall pinentry-wukong
```

Or you can install by building from source with cargo:

```sh
cargo install pinentry-wukong
```

#### Homebrew (macOS)

If you are on macOS and you have Homebrew installed, you can install pinentry-wukong from our Homebrew Tap:

```zsh
brew install chawyehsu/brew/pinentry-wukong
```

#### Scoop (Windows)

If you are on Windows and you have Scoop installed:

```pwsh
scoop bucket add dorado https://github.com/chawyehsu/dorado
scoop install pinentry-wukong
```

#### GitHub Releases

Or you may download the latest release from [GitHub releases][releases], manually extract the archive and put the executables in a directory that is in your `PATH`.

### Integration with GnuPG

A few steps are required to integrate **pinentry-wukong** with GnuPG.

(1) Set **pinentry-wukong** as your pinentry program in `~/.gnupg/gpg-agent.conf`:

```plain
pinentry-program /path/to/pinentry-wukong
```

(2) Make sure to set the `GPG_TTY` environment variable in your shell configuration (e.g., `~/.bashrc`, `~/.zshrc`):

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

**NOTE:** `GPG_TTY` is required for TTY/TUI mode to work properly.

(3) Then restart `gpg-agent`:

```sh
gpgconf --kill gpg-agent
```

### Usage

**pinentry-wukong** is designed to be invoked by `gpg-agent` automatically. Most of the time, you don't need to run it manually. However, you can play with it interactively for testing purposes:

```sh
# Run the pinentry server in the foreground
pinentry-wukong
```

#### CLI options

**pinentry-wukong** supports most of the command-line options of the original pinentry implementations. It also has a few additional unique options for better experience.

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

For example, you can use `pinentry-wukong config` subcommands to manage the configuration file.

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
[releases]: https://github.com/chawyehsu/pinentry-wukong/releases
