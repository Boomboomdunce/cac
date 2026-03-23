use assert_cmd::Command;

#[test]
fn ccp_help_exits_successfully() {
    Command::cargo_bin("ccp").unwrap().arg("--help").assert().success();
}
