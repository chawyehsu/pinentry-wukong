use std::process::Command;

use assert_fs::TempDir;

/// A test workspace for tests
#[allow(unused)]
pub struct TestWorkspace {
    /// Temporary file system root for the test workspace
    tempdir: TempDir,
}

impl TestWorkspace {
    /// Create a new test workspace
    pub fn new() -> Self {
        let tempdir = assert_fs::TempDir::new().expect("should create tempdir");

        Self { tempdir }
    }

    /// Get the test workspace tempdir
    #[allow(unused)]
    pub fn tempdir(&self) -> &TempDir {
        &self.tempdir
    }

    /// Get the command of the test target
    pub fn app(&self) -> Command {
        let app_path = insta_cmd::get_cargo_bin(env!("CARGO_PKG_NAME"));
        let mut cmd = Command::new(app_path);
        cmd.current_dir(self.tempdir.path());
        cmd
    }
}
