use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn help_lists_subcommands() {
    Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("auth"))
        .stdout(predicate::str::contains("foods"))
        .stdout(predicate::str::contains("recipes"))
        .stdout(predicate::str::contains("diary"))
        .stdout(predicate::str::contains("weight"))
        .stdout(predicate::str::contains("exercise"))
        .stdout(predicate::str::contains("water"))
        .stdout(predicate::str::contains("meals"))
        .stdout(predicate::str::contains("settings"))
        .stdout(predicate::str::contains("feed"))
        .stdout(predicate::str::contains("learning"))
        .stdout(predicate::str::contains("account"))
        .stdout(predicate::str::contains("completions"));
}
#[test]
fn search_without_login_fails_closed() {
    // No credentials file and no env triple: must fail, nonzero exit,
    // helpful message — never a panic.
    Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .env("XDG_CONFIG_HOME", "/nonexistent-xdg-fatsecret-cli")
        .env(
            "FATSECRET_CREDENTIALS",
            "/nonexistent-creds-fatsecret-cli.json",
        )
        .env_remove("FATSECRET_SERVER_ID")
        .args(["foods", "search", "oats"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not logged in"));
}

#[test]
fn diary_rm_help_lists_date() {
    Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .args(["diary", "rm", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--date"))
        .stdout(predicate::str::contains("see it in `diary day`"));
}

#[test]
fn diary_rm_rejects_bad_date_offline() {
    Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .env("XDG_CONFIG_HOME", "/nonexistent-xdg-fatsecret-cli")
        .env(
            "FATSECRET_CREDENTIALS",
            "/nonexistent-creds-fatsecret-cli.json",
        )
        .env_remove("FATSECRET_SERVER_ID")
        .args(["diary", "rm", "1", "--date", "not-a-date"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("bad date"))
        .stderr(predicate::str::contains("want YYYY-MM-DD"));
}

#[test]
fn diary_day_help_mentions_history() {
    Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .args(["diary", "day", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("history supported"));
}

#[test]
fn diary_day_past_date_reaches_network_offline() {
    // History is supported: a past date passes validation and fails closed
    // at auth when offline, never on the date itself.
    Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .env("XDG_CONFIG_HOME", "/nonexistent-xdg-fatsecret-cli")
        .env(
            "FATSECRET_CREDENTIALS",
            "/nonexistent-creds-fatsecret-cli.json",
        )
        .env_remove("FATSECRET_SERVER_ID")
        .args(["diary", "day", "--date", "2000-01-01"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not logged in")
                .or(predicate::str::contains("no device model")),
        );
}

#[test]
fn completions_verbose_without_credentials() {
    Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .env_remove("RUST_LOG")
        .args(["-vv", "completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fatsecret-cli"));
}

#[test]
fn color_never_emits_no_ansi_on_closed_failure() {
    let out = Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .env("XDG_CONFIG_HOME", "/nonexistent-xdg-fatsecret-cli")
        .env(
            "FATSECRET_CREDENTIALS",
            "/nonexistent-creds-fatsecret-cli.json",
        )
        .env_remove("FATSECRET_SERVER_ID")
        .env_remove("RUST_LOG")
        .args([
            "--color", "never", "--format", "json", "foods", "search", "oats",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        !out.stdout.windows(2).any(|w| w == [0x1b, b'[']),
        "stdout had ansi: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        !out.stderr.windows(2).any(|w| w == [0x1b, b'[']),
        "stderr had ansi: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn auth_status_json_when_logged_out() {
    let out = Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .env("XDG_CONFIG_HOME", "/nonexistent-xdg-fatsecret-cli")
        .env(
            "FATSECRET_CREDENTIALS",
            "/nonexistent-creds-fatsecret-cli.json",
        )
        .env_remove("FATSECRET_SERVER_ID")
        .env_remove("FATSECRET_SECRET_KEY")
        .env_remove("FATSECRET_DEVICE_KEY")
        .args(["auth", "status", "--format", "json"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains("\"status\": \"logged out\""),
        "stdout: {stdout}"
    );
    assert!(stderr.contains("not logged in"), "stderr: {stderr}");
}

#[test]
fn auth_login_and_logout_help_render() {
    Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .args(["auth", "login", "--help"])
        .assert()
        .success();
    Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .args(["auth", "logout", "--help"])
        .assert()
        .success();
}

#[test]
fn diary_rm_rejects_impossible_date_offline() {
    Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .env("XDG_CONFIG_HOME", "/nonexistent-xdg-fatsecret-cli")
        .env(
            "FATSECRET_CREDENTIALS",
            "/nonexistent-creds-fatsecret-cli.json",
        )
        .env_remove("FATSECRET_SERVER_ID")
        .args(["diary", "rm", "1", "--date", "2026-13-45"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("bad date"))
        .stderr(predicate::str::contains("want YYYY-MM-DD"));
}

#[test]
fn exercise_log_help_renders() {
    Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .args(["exercise", "log", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("type-id"));
}

#[test]
fn water_log_help_renders() {
    Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .args(["water", "log", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("goal-ml"));
}
