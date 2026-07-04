use insta_cmd::assert_cmd_snapshot;

use crate::utils::TestWorkspace;

#[test]
fn test_cli() {
    let ws = TestWorkspace::new();
    assert_cmd_snapshot!("completions", ws.app().arg("completions").arg("-h"));
    assert_cmd_snapshot!("completions_bash", ws.app().arg("completions").arg("bash"));
}
