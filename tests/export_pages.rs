use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command as ProcessCommand, Stdio};
use std::thread;
use std::time::Duration;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

fn write_test_beads(repo_dir: &Path) {
    fs::create_dir_all(repo_dir.join(".beads")).expect("create .beads");
    fs::write(
        repo_dir.join(".beads/beads.jsonl"),
        concat!(
            "{\"id\":\"BD-OPEN\",\"title\":\"Open Issue\",\"status\":\"open\",\"priority\":1,\"issue_type\":\"task\"}\n",
            "{\"id\":\"BD-CLOSED\",\"title\":\"Closed Issue\",\"status\":\"closed\",\"priority\":2,\"issue_type\":\"task\"}\n"
        ),
    )
    .expect("write beads file");
}

fn bvr_cmd(repo_dir: &Path) -> Command {
    let bvr_bin = std::env::var("CARGO_BIN_EXE_bvr").expect("CARGO_BIN_EXE_bvr env var");
    let mut command = Command::new(bvr_bin);
    command.current_dir(repo_dir);
    command
}

fn write_repo_scoped_beads(repo_dir: &Path) {
    fs::create_dir_all(repo_dir.join(".beads")).expect("create .beads");
    fs::write(
        repo_dir.join(".beads/beads.jsonl"),
        concat!(
            "{\"id\":\"BD-ALPHA-1\",\"title\":\"Alpha One\",\"status\":\"open\",\"priority\":1,\"issue_type\":\"task\",\"source_repo\":\"alpha\"}\n",
            "{\"id\":\"BD-BETA-1\",\"title\":\"Beta One\",\"status\":\"open\",\"priority\":2,\"issue_type\":\"task\",\"source_repo\":\"beta\"}\n"
        ),
    )
    .expect("write beads file");
}

fn write_hooks(repo_dir: &Path, yaml: &str) {
    fs::create_dir_all(repo_dir.join(".bv")).expect("create .bv");
    fs::write(repo_dir.join(".bv/hooks.yaml"), yaml).expect("write hooks");
}

fn preview_request(port: u16, path: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect preview server");
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .expect("write preview request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read preview response");
    response
}

fn preview_response_body(response: &str) -> &str {
    response
        .split_once("\r\n\r\n")
        .map_or("", |(_, body)| body)
        .trim()
}

#[test]
fn export_pages_writes_bundle_files() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_dir = temp.path();
    write_test_beads(repo_dir);

    bvr_cmd(repo_dir)
        .args([
            "--export-pages",
            "pages-out",
            "--pages-title",
            "Sprint Dashboard",
        ])
        .assert()
        .success();

    let out = repo_dir.join("pages-out");
    assert!(out.join("index.html").is_file());
    assert!(out.join("assets/style.css").is_file());
    assert!(out.join("assets/viewer.js").is_file());
    assert!(out.join("data/issues.json").is_file());
    assert!(out.join("data/meta.json").is_file());
    assert!(out.join("data/triage.json").is_file());
    assert!(out.join("data/insights.json").is_file());
    assert!(out.join("data/history.json").is_file());
    assert!(out.join("data/export_summary.json").is_file());

    let meta = fs::read_to_string(out.join("data/meta.json")).expect("read meta.json");
    assert!(meta.contains("\"Sprint Dashboard\""));
}

#[test]
fn export_pages_can_exclude_closed_and_history() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_dir = temp.path();
    write_test_beads(repo_dir);

    bvr_cmd(repo_dir)
        .args([
            "--export-pages",
            "pages-out",
            "--pages-include-closed=false",
            "--pages-include-history=false",
        ])
        .assert()
        .success();

    let out = repo_dir.join("pages-out");
    let issues = fs::read_to_string(out.join("data/issues.json")).expect("read issues.json");
    assert!(issues.contains("BD-OPEN"));
    assert!(!issues.contains("BD-CLOSED"));
    assert!(
        !out.join("data/history.json").exists(),
        "history should be omitted when --pages-include-history=false"
    );
}

#[test]
fn export_pages_runs_hooks_and_passes_export_context() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_dir = temp.path();
    write_test_beads(repo_dir);
    write_hooks(
        repo_dir,
        concat!(
            "hooks:\n",
            "  pre-export:\n",
            "    - name: mark-pre\n",
            "      command: 'mkdir -p \"$BV_EXPORT_PATH\" && echo pre > \"$BV_EXPORT_PATH/pre-hook.txt\"'\n",
            "    - name: capture-env\n",
            "      command: 'printf \"%s\\n\" \"$BV_EXPORT_FORMAT|$BV_ISSUE_COUNT\" > \"$BV_EXPORT_PATH/hook-env.txt\"'\n",
            "  post-export:\n",
            "    - name: mark-post\n",
            "      command: 'echo post > \"$BV_EXPORT_PATH/post-hook.txt\"'\n",
        ),
    );

    bvr_cmd(repo_dir)
        .args([
            "--export-pages",
            "pages-out",
            "--pages-include-closed=false",
        ])
        .assert()
        .success();

    let out = repo_dir.join("pages-out");
    assert!(out.join("pre-hook.txt").is_file());
    assert!(out.join("post-hook.txt").is_file());
    let env_line = fs::read_to_string(out.join("hook-env.txt")).expect("read hook env");
    assert_eq!(env_line.trim(), "html|1");
}

#[test]
fn export_pages_pre_hook_failure_blocks_export() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_dir = temp.path();
    write_test_beads(repo_dir);
    write_hooks(
        repo_dir,
        "hooks:\n  pre-export:\n    - name: fail-fast\n      command: 'exit 7'\n",
    );

    bvr_cmd(repo_dir)
        .args(["--export-pages", "pages-out"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("pre-export hook"));

    assert!(
        !repo_dir.join("pages-out/index.html").exists(),
        "bundle should not be written when pre-export hook fails"
    );
}

#[test]
fn watch_export_requires_export_pages() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_dir = temp.path();
    write_test_beads(repo_dir);

    bvr_cmd(repo_dir)
        .arg("--watch-export")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "--watch-export requires --export-pages <dir>",
        ));
}

#[test]
fn preview_pages_is_handled_before_issue_loading() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_dir = temp.path();

    bvr_cmd(repo_dir)
        .args(["--preview-pages", "missing-bundle"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "preview bundle directory not found",
        ))
        .stderr(predicate::str::contains("beads directory not found").not());
}

#[test]
fn watch_export_regenerates_after_change_and_keeps_repo_filter() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_dir = temp.path();
    write_repo_scoped_beads(repo_dir);

    let bvr_bin = std::env::var("CARGO_BIN_EXE_bvr").expect("CARGO_BIN_EXE_bvr env var");
    let child = ProcessCommand::new(bvr_bin)
        .current_dir(repo_dir)
        .args([
            "--export-pages",
            "pages-out",
            "--watch-export",
            "--repo",
            "alpha",
        ])
        .env("BVR_WATCH_MAX_LOOPS", "8")
        .env("BVR_WATCH_INTERVAL_MS", "200")
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("spawn bvr watch export");

    thread::sleep(Duration::from_millis(350));
    let mut beads = fs::OpenOptions::new()
        .append(true)
        .open(repo_dir.join(".beads/beads.jsonl"))
        .expect("open beads for append");
    beads
        .write_all(
            b"{\"id\":\"BD-ALPHA-2\",\"title\":\"Alpha Two\",\"status\":\"open\",\"priority\":1,\"issue_type\":\"task\",\"source_repo\":\"alpha\"}\n",
        )
        .expect("append issue");
    beads.flush().expect("flush append");

    let output = child.wait_with_output().expect("wait for watch process");
    assert!(
        output.status.success(),
        "watch export failed with stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Regenerated pages bundle at"),
        "expected regeneration log in stderr, got: {stderr}"
    );

    let issues = fs::read_to_string(repo_dir.join("pages-out/data/issues.json")).expect("issues");
    assert!(issues.contains("BD-ALPHA-1"));
    assert!(issues.contains("BD-ALPHA-2"));
    assert!(
        !issues.contains("BD-BETA-1"),
        "repo filter should exclude non-target repos after watch refresh"
    );
}

#[test]
fn watch_export_regenerates_after_workspace_change() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_dir = temp.path();
    fs::create_dir_all(repo_dir.join(".bv")).expect("create .bv");
    fs::create_dir_all(repo_dir.join("services/api/.beads")).expect("create api beads");
    fs::create_dir_all(repo_dir.join("apps/web/.beads")).expect("create web beads");
    fs::write(
        repo_dir.join(".bv/workspace.yaml"),
        concat!(
            "repos:\n",
            "  - name: api\n",
            "    path: services/api\n",
            "    prefix: api-\n",
            "  - name: web\n",
            "    path: apps/web\n",
            "    prefix: web-\n",
        ),
    )
    .expect("write workspace");
    fs::write(
        repo_dir.join("services/api/.beads/issues.jsonl"),
        "{\"id\":\"AUTH-1\",\"title\":\"API Auth\",\"status\":\"open\",\"priority\":1,\"issue_type\":\"task\"}\n",
    )
    .expect("write api issues");
    fs::write(
        repo_dir.join("apps/web/.beads/issues.jsonl"),
        "{\"id\":\"UI-1\",\"title\":\"Web UI\",\"status\":\"open\",\"priority\":2,\"issue_type\":\"task\"}\n",
    )
    .expect("write web issues");

    let bvr_bin = std::env::var("CARGO_BIN_EXE_bvr").expect("CARGO_BIN_EXE_bvr env var");
    let child = ProcessCommand::new(bvr_bin)
        .current_dir(repo_dir)
        .args([
            "--export-pages",
            "pages-out",
            "--watch-export",
            "--workspace",
            ".",
        ])
        .env("BVR_WATCH_MAX_LOOPS", "8")
        .env("BVR_WATCH_INTERVAL_MS", "200")
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("spawn workspace watch export");

    thread::sleep(Duration::from_millis(350));
    let mut beads = fs::OpenOptions::new()
        .append(true)
        .open(repo_dir.join("apps/web/.beads/issues.jsonl"))
        .expect("open web beads for append");
    beads
        .write_all(
            b"{\"id\":\"UI-2\",\"title\":\"Web UI Two\",\"status\":\"open\",\"priority\":1,\"issue_type\":\"task\"}\n",
        )
        .expect("append web issue");
    beads.flush().expect("flush append");

    let output = child.wait_with_output().expect("wait for workspace watch");
    assert!(
        output.status.success(),
        "workspace watch export failed with stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Regenerated pages bundle at"),
        "expected regeneration log in stderr, got: {stderr}"
    );

    let issues = fs::read_to_string(repo_dir.join("pages-out/data/issues.json")).expect("issues");
    assert!(issues.contains("api-AUTH-1"));
    assert!(issues.contains("web-UI-1"));
    assert!(issues.contains("web-UI-2"));
}

#[test]
fn preview_pages_reports_session_diagnostics() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_dir = temp.path();
    let bundle_dir = repo_dir.join("bundle");
    fs::create_dir_all(&bundle_dir).expect("create bundle dir");
    fs::write(
        bundle_dir.join("index.html"),
        "<!doctype html><html><body>ok</body></html>",
    )
    .expect("write index");

    let probe = TcpListener::bind(("127.0.0.1", 0)).expect("probe port");
    let port = probe.local_addr().expect("probe addr").port();
    drop(probe);

    let bvr_bin = std::env::var("CARGO_BIN_EXE_bvr").expect("CARGO_BIN_EXE_bvr env var");
    let child = ProcessCommand::new(bvr_bin)
        .current_dir(repo_dir)
        .args(["--preview-pages", "bundle"])
        .env("BVR_PREVIEW_PORT", port.to_string())
        .env("BVR_PREVIEW_MAX_REQUESTS", "1")
        .env("BV_NO_BROWSER", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn preview");

    let mut connected = false;
    for _ in 0..40 {
        if let Ok(mut stream) = TcpStream::connect(("127.0.0.1", port)) {
            stream
                .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .expect("write request");
            connected = true;
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(connected, "failed to connect to preview server");

    let output = child.wait_with_output().expect("wait for preview");
    assert!(
        output.status.success(),
        "preview failed with stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!(
        "Preview server running at http://127.0.0.1:{port}"
    )));
    assert!(stdout.contains("Serving bundle:"));
    assert!(stdout.contains("Live reload: enabled"));
}

#[test]
fn preview_pages_status_endpoint_reports_bundle_metadata() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_dir = temp.path();
    let bundle_dir = repo_dir.join("bundle");
    fs::create_dir_all(bundle_dir.join("assets")).expect("create assets");
    fs::write(
        bundle_dir.join("index.html"),
        "<!doctype html><html><body>ok</body></html>",
    )
    .expect("write index");
    fs::write(bundle_dir.join("assets/style.css"), "body{}").expect("write style");

    let probe = TcpListener::bind(("127.0.0.1", 0)).expect("probe port");
    let port = probe.local_addr().expect("probe addr").port();
    drop(probe);

    let bvr_bin = std::env::var("CARGO_BIN_EXE_bvr").expect("CARGO_BIN_EXE_bvr env var");
    let child = ProcessCommand::new(bvr_bin)
        .current_dir(repo_dir)
        .args(["--preview-pages", "bundle"])
        .env("BVR_PREVIEW_PORT", port.to_string())
        .env("BVR_PREVIEW_MAX_REQUESTS", "2")
        .env("BV_NO_BROWSER", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn preview");

    let mut response = None::<String>;
    for _ in 0..40 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            response = Some(preview_request(port, "/__preview__/status"));
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let response = response.expect("status response");
    let body = preview_response_body(&response);
    let payload = serde_json::from_str::<Value>(body).expect("decode status json");
    assert_eq!(payload["status"], "running");
    assert_eq!(payload["port"], port);
    assert_eq!(payload["has_index"], true);
    assert_eq!(payload["live_reload"], true);
    assert_eq!(payload["file_count"], 2);
    assert!(
        payload["bundle_path"]
            .as_str()
            .is_some_and(|value| value.ends_with("bundle"))
    );

    let output = child.wait_with_output().expect("wait for preview");
    assert!(
        output.status.success(),
        "preview status request failed with stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn preview_pages_no_live_reload_omits_reload_script() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_dir = temp.path();
    let bundle_dir = repo_dir.join("bundle");
    fs::create_dir_all(&bundle_dir).expect("create bundle dir");
    fs::write(
        bundle_dir.join("index.html"),
        "<!doctype html><html><body>ok</body></html>",
    )
    .expect("write index");

    let probe = TcpListener::bind(("127.0.0.1", 0)).expect("probe port");
    let port = probe.local_addr().expect("probe addr").port();
    drop(probe);

    let bvr_bin = std::env::var("CARGO_BIN_EXE_bvr").expect("CARGO_BIN_EXE_bvr env var");
    let child = ProcessCommand::new(bvr_bin)
        .current_dir(repo_dir)
        .args(["--preview-pages", "bundle", "--no-live-reload"])
        .env("BVR_PREVIEW_PORT", port.to_string())
        .env("BVR_PREVIEW_MAX_REQUESTS", "2")
        .env("BV_NO_BROWSER", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn preview");

    let mut response = None::<String>;
    for _ in 0..40 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            response = Some(preview_request(port, "/"));
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let response = response.expect("html response");
    let body = preview_response_body(&response);
    assert!(body.contains("<body>ok</body>"));
    assert!(!body.contains("window.location.reload"));

    let output = child.wait_with_output().expect("wait for preview");
    assert!(
        output.status.success(),
        "preview html request failed with stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
