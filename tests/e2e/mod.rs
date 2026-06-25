use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};

use insta_cmd::assert_cmd_snapshot;

use crate::utils::TestWorkspace;

#[test]
fn test_cli() {
    let ws = TestWorkspace::new();
    assert_cmd_snapshot!("completions", ws.app().arg("completions").arg("-h"));
    assert_cmd_snapshot!("completions_bash", ws.app().arg("completions").arg("bash"));
}

/// A helper for testing pinentry Assuan protocol interactions.
///
/// Maintains a persistent BufReader over the child's stdout to avoid
/// losing buffered data between reads.
pub struct PinentrySession {
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    child: Child,
}

impl PinentrySession {
    /// Start a new pinentry session with piped stdin/stdout.
    pub fn start(ws: &TestWorkspace) -> Self {
        let mut cmd = ws.app();
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .args(["serve", "--ui=tty"]);

        let mut child = cmd.spawn().expect("failed to spawn pinentry-wukong");
        let stdin = child.stdin.take().expect("stdin not available");
        let stdout = child.stdout.take().expect("stdout not available");

        Self {
            stdin: Some(stdin),
            stdout: BufReader::new(stdout),
            child,
        }
    }

    /// Send a line to pinentry's stdin.
    pub fn send(&mut self, line: &str) {
        let stdin = self.stdin.as_mut().expect("stdin already closed");
        stdin
            .write_all(format!("{line}\n").as_bytes())
            .expect("failed to write to stdin");
        stdin.flush().expect("failed to flush stdin");
    }

    /// Read a line from pinentry's stdout.
    pub fn recv(&mut self) -> Option<String> {
        let mut line = String::new();
        match self.stdout.read_line(&mut line) {
            Ok(0) => None,
            Ok(_) => Some(
                line.trim_end_matches('\n')
                    .trim_end_matches('\r')
                    .to_string(),
            ),
            Err(_) => None,
        }
    }

    /// Send a command and read the response line.
    pub fn command(&mut self, cmd: &str, args: &str) -> Option<String> {
        if args.is_empty() {
            self.send(cmd);
        } else {
            self.send(&format!("{cmd} {args}"));
        }
        self.recv()
    }

    /// Close the session (send EOF and wait).
    pub fn close(mut self) {
        self.stdin.take(); // Drop stdin to send EOF
        let _ = self.child.wait();
    }
}

impl Drop for PinentrySession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn test_assuan_greeting() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);

    let greeting = session.recv().expect("expected greeting");
    assert!(
        greeting.starts_with("OK"),
        "expected OK greeting, got: {greeting}"
    );

    session.close();
}

#[test]
fn test_setdesc() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("SETDESC", "Enter+passphrase+to+unlock");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_setprompt() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("SETPROMPT", "Passphrase:");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_seterror() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("SETERROR", "Bad+passphrase");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_settitle() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("SETTITLE", "Unlock+Key");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_setok_setcancel() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("SETOK", "Confirm");
    assert_eq!(resp.as_deref(), Some("OK"));

    let resp = session.command("SETCANCEL", "Abort");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_option_grab() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("OPTION", "grab");
    assert_eq!(resp.as_deref(), Some("OK"));

    let resp = session.command("OPTION", "no-grab");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_option_display() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("OPTION", "display=:0");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_option_ttyname() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("OPTION", "ttyname=/dev/pts/0");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_getinfo_version() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    session.send("GETINFO version");
    let data_line = session.recv().expect("expected D line");
    assert!(
        data_line.starts_with("D "),
        "expected D line, got: {data_line}"
    );
    let version = data_line.strip_prefix("D ").unwrap();
    assert!(!version.is_empty(), "version should not be empty");

    let ok_line = session.recv().expect("expected OK");
    assert_eq!(ok_line, "OK");

    session.close();
}

#[test]
fn test_getinfo_pid() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    session.send("GETINFO pid");
    let data_line = session.recv().expect("expected D line");
    assert!(
        data_line.starts_with("D "),
        "expected D line, got: {data_line}"
    );
    let pid = data_line.strip_prefix("D ").unwrap();
    assert!(pid.parse::<u32>().is_ok(), "pid should be a number: {pid}");

    let ok_line = session.recv().expect("expected OK");
    assert_eq!(ok_line, "OK");

    session.close();
}

#[test]
fn test_getinfo_flavor() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    session.send("GETINFO flavor");
    let data_line = session.recv().expect("expected D line");
    assert!(
        data_line.starts_with("D "),
        "expected D line, got: {data_line}"
    );
    let flavor = data_line.strip_prefix("D ").unwrap();
    assert!(
        flavor.starts_with("wukong:"),
        "flavor should start with 'wukong:': {flavor}"
    );

    let ok_line = session.recv().expect("expected OK");
    assert_eq!(ok_line, "OK");

    session.close();
}

#[test]
fn test_getinfo_unknown() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("GETINFO", "foobar");
    assert!(resp.is_some());
    let err = resp.unwrap();
    assert!(err.starts_with("ERR"), "expected ERR, got: {err}");

    session.close();
}

#[test]
fn test_unknown_command() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("FOOBAR", "");
    assert!(resp.is_some());
    let err = resp.unwrap();
    assert!(err.starts_with("ERR"), "expected ERR, got: {err}");

    session.close();
}

#[test]
fn test_setkeyinfo() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("SETKEYINFO", "ABC123");
    assert_eq!(resp.as_deref(), Some("OK"));

    let resp = session.command("SETKEYINFO", "--clear");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_settimeout() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("SETTIMEOUT", "30");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_comment_line_ignored() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    session.send("# this is a comment");

    let resp = session.command("SETDESC", "test");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_reset_clears_state() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    session.command("SETDESC", "description1");
    session.command("SETPROMPT", "prompt1");

    let resp = session.command("RESET", "");
    assert_eq!(resp.as_deref(), Some("OK"));

    let resp = session.command("SETDESC", "description2");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_option_allow_external_password_cache() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("OPTION", "allow-external-password-cache");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_multiple_commands_sequence() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    assert_eq!(session.command("SETTITLE", "Test").as_deref(), Some("OK"));
    assert_eq!(
        session.command("SETDESC", "Enter+passphrase").as_deref(),
        Some("OK")
    );
    assert_eq!(session.command("SETPROMPT", "PIN:").as_deref(), Some("OK"));
    assert_eq!(session.command("SETOK", "OK").as_deref(), Some("OK"));
    assert_eq!(
        session.command("SETCANCEL", "Cancel").as_deref(),
        Some("OK")
    );
    assert_eq!(
        session.command("SETKEYINFO", "KEYGRIP123").as_deref(),
        Some("OK")
    );

    session.close();
}
