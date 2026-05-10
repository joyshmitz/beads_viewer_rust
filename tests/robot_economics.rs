use std::fs;

use assert_cmd::Command;
use serde_json::Value;

fn run_bvr_json_in_dir(flags: &[&str], dir: &std::path::Path) -> Value {
    let bvr_bin = std::env::var("CARGO_BIN_EXE_bvr").expect("CARGO_BIN_EXE_bvr env var");
    let mut command = Command::new(bvr_bin);
    command.current_dir(dir);
    command.args(flags);

    let output = command.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).expect("valid JSON output")
}

#[test]
fn robot_economics_cost_of_delay_excludes_closed_bottlenecks() {
    let temp = tempfile::tempdir().expect("tempdir");
    let repo_dir = temp.path();
    let beads_path = repo_dir.join(".beads/beads.jsonl");
    let overlay_path = repo_dir.join("economics.json");
    fs::create_dir_all(repo_dir.join(".beads")).expect("mkdir beads");

    let now = chrono::Utc::now();
    let old = (now - chrono::Duration::days(120)).to_rfc3339();
    let closed_recently = (now - chrono::Duration::days(1)).to_rfc3339();
    fs::write(
        &beads_path,
        format!(
            concat!(
                "{{\"id\":\"CLOSED-BLOCKER\",\"title\":\"Closed blocker\",\"status\":\"closed\",\"priority\":1,\"issue_type\":\"task\",\"created_at\":\"{old}\",\"updated_at\":\"{old}\",\"closed_at\":\"{old}\"}}\n",
                "{{\"id\":\"OPEN-BLOCKER\",\"title\":\"Open blocker\",\"status\":\"open\",\"priority\":1,\"issue_type\":\"task\",\"created_at\":\"{old}\",\"updated_at\":\"{old}\"}}\n",
                "{{\"id\":\"ACTIVE-BLOCKER\",\"title\":\"Active blocker\",\"status\":\"in_progress\",\"priority\":1,\"issue_type\":\"task\",\"created_at\":\"{old}\",\"updated_at\":\"{old}\"}}\n",
                "{{\"id\":\"DONE\",\"title\":\"Recent closure\",\"status\":\"closed\",\"priority\":3,\"issue_type\":\"task\",\"created_at\":\"{old}\",\"updated_at\":\"{closed_recently}\",\"closed_at\":\"{closed_recently}\"}}\n",
                "{{\"id\":\"C1\",\"title\":\"Closed dependent 1\",\"status\":\"open\",\"priority\":2,\"issue_type\":\"task\",\"created_at\":\"{old}\",\"updated_at\":\"{old}\",\"dependencies\":[{{\"issue_id\":\"C1\",\"depends_on_id\":\"CLOSED-BLOCKER\",\"type\":\"blocks\"}}]}}\n",
                "{{\"id\":\"C2\",\"title\":\"Closed dependent 2\",\"status\":\"open\",\"priority\":2,\"issue_type\":\"task\",\"created_at\":\"{old}\",\"updated_at\":\"{old}\",\"dependencies\":[{{\"issue_id\":\"C2\",\"depends_on_id\":\"CLOSED-BLOCKER\",\"type\":\"blocks\"}}]}}\n",
                "{{\"id\":\"C3\",\"title\":\"Closed dependent 3\",\"status\":\"open\",\"priority\":2,\"issue_type\":\"task\",\"created_at\":\"{old}\",\"updated_at\":\"{old}\",\"dependencies\":[{{\"issue_id\":\"C3\",\"depends_on_id\":\"CLOSED-BLOCKER\",\"type\":\"blocks\"}}]}}\n",
                "{{\"id\":\"O1\",\"title\":\"Open dependent 1\",\"status\":\"open\",\"priority\":2,\"issue_type\":\"task\",\"created_at\":\"{old}\",\"updated_at\":\"{old}\",\"dependencies\":[{{\"issue_id\":\"O1\",\"depends_on_id\":\"OPEN-BLOCKER\",\"type\":\"blocks\"}}]}}\n",
                "{{\"id\":\"O2\",\"title\":\"Open dependent 2\",\"status\":\"open\",\"priority\":2,\"issue_type\":\"task\",\"created_at\":\"{old}\",\"updated_at\":\"{old}\",\"dependencies\":[{{\"issue_id\":\"O2\",\"depends_on_id\":\"OPEN-BLOCKER\",\"type\":\"blocks\"}}]}}\n",
                "{{\"id\":\"A1\",\"title\":\"Active dependent 1\",\"status\":\"open\",\"priority\":2,\"issue_type\":\"task\",\"created_at\":\"{old}\",\"updated_at\":\"{old}\",\"dependencies\":[{{\"issue_id\":\"A1\",\"depends_on_id\":\"ACTIVE-BLOCKER\",\"type\":\"blocks\"}}]}}\n"
            ),
            old = old,
            closed_recently = closed_recently,
        ),
    )
    .expect("write beads");
    fs::write(
        &overlay_path,
        r#"{"hourly_rate":100,"hours_per_day":6,"currency":"USD","throughput_window_days":30}"#,
    )
    .expect("write overlay");

    let payload = run_bvr_json_in_dir(
        &[
            "--robot-economics",
            "--beads-file",
            beads_path.to_str().expect("utf8 beads path"),
            "--economics-overlay",
            overlay_path.to_str().expect("utf8 overlay path"),
        ],
        repo_dir,
    );

    let ids: Vec<&str> = payload["projections"]["cost_of_delay"]
        .as_array()
        .expect("cost_of_delay array")
        .iter()
        .map(|entry| entry["id"].as_str().expect("entry id"))
        .collect();

    assert_eq!(ids, vec!["OPEN-BLOCKER", "ACTIVE-BLOCKER"]);
    assert!(
        !ids.contains(&"CLOSED-BLOCKER"),
        "closed bottleneck leaked into cost_of_delay: {ids:?}"
    );
    assert_eq!(
        payload["projections"]["cost_of_delay"][0]["dependents_count"],
        2
    );
    assert_eq!(
        payload["projections"]["cost_of_delay"][1]["dependents_count"],
        1
    );
}
