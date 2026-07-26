use crate::utils::TestWorkspace;

use super::PinentrySession;

#[test]
fn test_option_ttyname_conhost() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("OPTION", "ttyname=/conhost/1234");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_option_ttyname_conhost_zero_pid() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("OPTION", "ttyname=/conhost/0");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_option_ttyname_conhost_large_pid() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("OPTION", "ttyname=/conhost/4294967295");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_option_ttyname_invalid_format() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    let resp = session.command("OPTION", "ttyname=not-a-valid-path");
    assert_eq!(resp.as_deref(), Some("OK"));

    session.close();
}

#[test]
fn test_getinfo_ttyinfo_after_ttyname() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    session.command("OPTION", "ttyname=/conhost/5678");

    session.send("GETINFO ttyinfo");
    let data_line = session.recv().expect("expected D line");
    assert!(
        data_line.starts_with("D "),
        "expected D line, got: {data_line}"
    );
    let ttyinfo = data_line.strip_prefix("D ").unwrap();
    assert!(
        ttyinfo.contains("/conhost/5678"),
        "ttyinfo should contain /conhost/5678: {ttyinfo}"
    );

    let ok_line = session.recv().expect("expected OK");
    assert_eq!(ok_line, "OK");

    session.close();
}

#[test]
fn test_getinfo_ttyinfo_default() {
    let ws = TestWorkspace::new();
    let mut session = PinentrySession::start(&ws);
    session.recv();

    session.send("GETINFO ttyinfo");
    let data_line = session.recv().expect("expected D line");
    assert!(
        data_line.starts_with("D "),
        "expected D line, got: {data_line}"
    );
    let ttyinfo = data_line.strip_prefix("D ").unwrap();
    assert!(
        ttyinfo.contains("-"),
        "ttyinfo should contain '-' for unset fields: {ttyinfo}"
    );

    let ok_line = session.recv().expect("expected OK");
    assert_eq!(ok_line, "OK");

    session.close();
}
