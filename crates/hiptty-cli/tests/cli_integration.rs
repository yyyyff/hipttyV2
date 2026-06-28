use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;

fn hiptty() -> Command {
    Command::cargo_bin("hiptty").unwrap()
}

fn parse_json(stdout: &[u8]) -> Value {
    serde_json::from_slice(stdout).expect("stdout should be valid JSON")
}

#[test]
fn forums_list_json_envelope() {
    let assert = hiptty().args(["forums", "list"]).assert().success();
    let json = parse_json(&assert.get_output().stdout);

    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["ok"], true);
    assert!(json["data"].is_array());
    assert!(json["data"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["id"] == 7));
}

#[test]
fn auth_status_json_envelope() {
    let assert = hiptty().args(["auth", "status"]).assert().success();
    let json = parse_json(&assert.get_output().stdout);

    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["ok"], true);
    assert!(json["data"]["logged_in"].is_boolean());
}

#[test]
fn thread_show_json_envelope() {
    let assert = hiptty()
        .args(["thread", "show", "448060", "--page", "1"])
        .assert()
        .success();
    let json = parse_json(&assert.get_output().stdout);

    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["tid"], "448060");
    assert!(json["data"]["posts"].is_array());
    assert!(!json["data"]["posts"].as_array().unwrap().is_empty());
}

#[test]
fn human_forums_list_prints_names() {
    hiptty()
        .args(["forums", "list", "--human"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Geek Talks"));
}

#[test]
fn search_without_login_returns_auth_or_business_error() {
    let assert = hiptty()
        .args(["search", "rust", "--fid", "7"])
        .assert()
        .failure();
    let json = parse_json(&assert.get_output().stdout);

    assert_eq!(json["ok"], false);
    assert!(json["error"]["code"].is_string());
}

#[test]
fn invalid_subcommand_exits_usage_error() {
    hiptty().arg("not-a-command").assert().failure().code(2);
}

#[test]
#[ignore = "requires network; run with: cargo test -p hiptty-cli -- --ignored"]
fn dump_fixture_writes_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("forumdisplay_fid7.html");

    let assert = hiptty()
        .args([
            "admin",
            "dump-fixture",
            "forumdisplay.php?fid=7&page=1",
            "-o",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();
    let json = parse_json(&assert.get_output().stdout);

    assert_eq!(json["ok"], true);
    assert!(output.exists());
    let html = std::fs::read_to_string(&output).expect("read fixture");
    assert!(html.contains("charset=utf-8") || html.contains("charset=UTF-8"));
    assert!(html.contains("tbody"));
}
