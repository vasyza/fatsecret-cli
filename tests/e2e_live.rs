//! Live end-to-end reads. Gated behind `FATSECRET_LIVE_E2E=1` so the
//! default suite stays hermetic; without the flag every test passes
//! trivially without touching the network.
//!
//! Contract: READS ONLY. No test in this file may mutate the account
//! (no `add`/`rm`/`log`/`save`/`register`). Each test performs a single
//! read with the ambient credentials of the invoking shell.

use assert_cmd::Command;
use predicates::prelude::*;

fn live_enabled() -> bool {
    std::env::var("FATSECRET_LIVE_E2E").as_deref() == Ok("1")
}

#[test]
fn live_diary_day_json_has_dateint() {
    if !live_enabled() {
        return;
    }
    Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .args(["diary", "day", "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dateint"));
}

#[test]
fn live_foods_search_oats_returns_rows() {
    if !live_enabled() {
        return;
    }
    Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .args(["foods", "search", "oats", "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("39715"));
}

#[test]
fn live_auth_status_shows_logged_in() {
    if !live_enabled() {
        return;
    }
    Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .args(["auth", "status", "--format", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("logged in"));
}

#[test]
fn live_exercise_types_renders() {
    if !live_enabled() {
        return;
    }
    Command::cargo_bin("fatsecret-cli")
        .unwrap()
        .args(["exercise", "types"])
        .assert()
        .success();
}
