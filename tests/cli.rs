use std::process::Command;

#[test]
fn help_command_does_not_require_existing_db_path() {
    let exe = env!("CARGO_BIN_EXE_agent-memory");
    let output = Command::new(exe)
        .args(["--db", "/tmp/agent-memory-does-not-exist.db", "help"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "help command should exit successfully even when db is non-default"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("agent-memory —— 离线优先的 LLM Agent 记忆层"));
}

#[test]
fn version_command_reports_package_version() {
    let exe = env!("CARGO_BIN_EXE_agent-memory");
    let output = Command::new(exe)
        .args(["--db", "/tmp/agent-memory-does-not-exist.db", "version"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "version command should exit successfully even when db path is non-default"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
}
