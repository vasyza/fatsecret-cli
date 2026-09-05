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
