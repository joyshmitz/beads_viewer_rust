use assert_cmd::Command;
use predicates::prelude::*;

fn bvr_command() -> Command {
    let bvr_bin = std::env::var("CARGO_BIN_EXE_bvr").expect("CARGO_BIN_EXE_bvr env var");
    Command::new(bvr_bin)
}

#[test]
fn check_update_prints_remediation_and_succeeds() {
    let mut cmd = bvr_command();
    cmd.arg("--check-update");

    cmd.assert().success().stdout(
        predicate::str::contains("Automatic self-update checks are not implemented")
            .and(predicate::str::contains("Current version: bvr"))
            .and(predicate::str::contains("cargo install --path .")),
    );
}

#[test]
fn update_exits_with_remediation() {
    let mut cmd = bvr_command();
    cmd.arg("--update");

    cmd.assert().code(2).stderr(
        predicate::str::contains("--update is not supported")
            .and(predicate::str::contains("git pull origin main"))
            .and(predicate::str::contains("cargo install --path .")),
    );
}

#[test]
fn yes_without_update_is_rejected() {
    let mut cmd = bvr_command();
    cmd.arg("--yes");

    cmd.assert().code(2).stderr(
        predicate::str::contains("--yes can only be used with --update")
            .and(predicate::str::contains("--check-update")),
    );
}

#[test]
fn conflicting_operational_actions_are_rejected() {
    let mut cmd = bvr_command();
    cmd.args(["--update", "--rollback"]);

    cmd.assert().code(2).stderr(predicate::str::contains(
        "only one of --check-update/--update/--rollback may be used",
    ));
}

#[test]
fn agents_force_defaults_to_check_mode() {
    let temp = tempfile::tempdir().expect("tempdir");
    let work_dir = temp.path().join("level1/level2/level3");
    std::fs::create_dir_all(&work_dir).expect("create nested work dir");

    let mut cmd = bvr_command();
    cmd.current_dir(&work_dir).arg("--agents-force");

    cmd.assert().success().stdout(
        predicate::str::contains("No agent file found")
            .and(predicate::str::contains("bvr --agents-add")),
    );
}

#[test]
fn conflicting_agents_actions_are_rejected() {
    let temp = tempfile::tempdir().expect("tempdir");

    let mut cmd = bvr_command();
    cmd.current_dir(temp.path())
        .args(["--agents-add", "--agents-remove"]);

    cmd.assert().code(2).stderr(predicate::str::contains(
        "only one of --agents-check/--agents-add/--agents-update/--agents-remove may be used",
    ));
}

#[test]
fn agents_force_uses_workspace_root_discovered_from_repo_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace_root = temp.path().join("workspace");
    let nested_repo = workspace_root.join("services/api");
    std::fs::create_dir_all(workspace_root.join(".bv")).expect("create workspace dir");
    std::fs::create_dir_all(&nested_repo).expect("create nested repo");
    std::fs::write(
        workspace_root.join(".bv/workspace.yaml"),
        "repos:\n  - path: services/api\n    prefix: api-\n",
    )
    .expect("write workspace config");
    std::fs::write(
        workspace_root.join("AGENTS.md"),
        "<!-- bv-agent-instructions-v1 -->\n\n<!-- end-bv-agent-instructions -->\n",
    )
    .expect("write agents file");

    let mut cmd = bvr_command();
    cmd.current_dir(&nested_repo).args([
        "--agents-force",
        "--repo-path",
        nested_repo.to_str().unwrap(),
    ]);

    cmd.assert().success().stdout(
        predicate::str::contains("Found AGENTS.md")
            .and(predicate::str::contains(
                workspace_root.join("AGENTS.md").to_string_lossy(),
            ))
            .and(predicate::str::contains("up to date")),
    );
}
