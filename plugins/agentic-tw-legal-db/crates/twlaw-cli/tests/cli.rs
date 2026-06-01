use std::process::Command;

fn twlaw() -> Command {
    Command::new(env!("CARGO_BIN_EXE_twlaw"))
}

#[test]
fn pcode_outputs_json() {
    let output = twlaw()
        .args(["regulation", "pcode", "--law", "民法", "--json"])
        .output()
        .expect("run twlaw");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json stdout");
    assert_eq!(json["success"], true);
    assert_eq!(json["pcode"], "B0000001");
}

#[test]
fn interpretation_get_outputs_json() {
    let output = twlaw()
        .args(["interpretation", "get", "釋字748", "--json"])
        .output()
        .expect("run twlaw");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json stdout");
    assert_eq!(json["success"], true);
    assert_eq!(json["type"], "釋字");
}

#[test]
fn invalid_regulation_uses_not_found_exit_code() {
    let output = twlaw()
        .args(["regulation", "pcode", "--law", "不存在的測試法規", "--json"])
        .output()
        .expect("run twlaw");
    assert_eq!(output.status.code(), Some(3));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json stdout");
    assert_eq!(json["success"], false);
    assert_eq!(json["error"]["code"], "not_found");
}

#[test]
fn sources_status_outputs_agent_policy() {
    let output = twlaw()
        .args(["sources", "status", "--json"])
        .output()
        .expect("run twlaw");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json stdout");
    assert_eq!(json["success"], true);
    assert_eq!(
        json["goal"]["requires_external_api_application_by_default"],
        false
    );
    assert!(
        json["coverage"]["official_sources_tracked"]
            .as_u64()
            .unwrap()
            >= 10
    );
}

#[test]
fn agent_guide_outputs_workflows() {
    let output = twlaw()
        .args(["agent", "guide", "--json"])
        .output()
        .expect("run twlaw");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json stdout");
    assert_eq!(json["success"], true);
    assert_eq!(json["contract"]["external_api_application_required"], false);
    assert!(json["workflows"].as_array().unwrap().len() >= 3);
}

#[test]
fn interpretation_search_accepts_boolean_values_and_no_flags() {
    let explicit_false = twlaw()
        .args([
            "interpretation",
            "search",
            "--include-old",
            "false",
            "--limit",
            "1",
            "--json",
        ])
        .output()
        .expect("run twlaw");
    assert!(explicit_false.status.success());

    let no_old = twlaw()
        .args([
            "interpretation",
            "search",
            "--no-old",
            "--limit",
            "1",
            "--json",
        ])
        .output()
        .expect("run twlaw");
    assert!(no_old.status.success());
}

#[test]
fn moj_datasets_outputs_no_credential_sources() {
    let output = twlaw()
        .args(["moj", "datasets", "--json"])
        .output()
        .expect("run twlaw");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json stdout");
    assert_eq!(json["success"], true);
    assert_eq!(json["dataset_count"], 4);
    assert_eq!(json["datasets"][0]["credentials_required"], false);
}

#[test]
fn moj_status_does_not_require_remote_check() {
    let output = twlaw()
        .args(["moj", "status", "--dataset", "ch-law", "--json"])
        .output()
        .expect("run twlaw");
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json stdout");
    assert_eq!(json["success"], true);
    assert_eq!(json["remote_checked"], false);
    assert_eq!(json["datasets"][0]["dataset"]["id"], "ch-law");
}

#[test]
fn moj_agreements_invalid_kind_fails_before_network() {
    let output = twlaw()
        .args(["moj", "agreements", "--kind", "not-a-kind", "--json"])
        .output()
        .expect("run twlaw");
    assert_eq!(output.status.code(), Some(2));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json stdout");
    assert_eq!(json["success"], false);
    assert_eq!(json["error"]["code"], "invalid_input");
}

#[test]
fn terminal_cases_invalid_kind_fails_before_network() {
    let output = twlaw()
        .args([
            "interpretation",
            "terminal",
            "--kind",
            "not-a-kind",
            "--json",
        ])
        .output()
        .expect("run twlaw");
    assert_eq!(output.status.code(), Some(2));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json stdout");
    assert_eq!(json["success"], false);
    assert_eq!(json["error"]["code"], "invalid_input");
}

#[test]
fn special_judgment_invalid_kind_fails_before_network() {
    let output = twlaw()
        .args([
            "judgment",
            "special",
            "--kind",
            "not-a-kind",
            "--keyword",
            "x",
            "--json",
        ])
        .output()
        .expect("run twlaw");
    assert_eq!(output.status.code(), Some(2));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json stdout");
    assert_eq!(json["success"], false);
    assert_eq!(json["error"]["code"], "invalid_input");
}

#[test]
fn mojlaw_invalid_kind_fails_before_network() {
    let output = twlaw()
        .args([
            "mojlaw",
            "search",
            "--kind",
            "not-a-kind",
            "--keyword",
            "個資",
            "--json",
        ])
        .output()
        .expect("run twlaw");
    assert_eq!(output.status.code(), Some(2));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("json stdout");
    assert_eq!(json["success"], false);
    assert_eq!(json["error"]["code"], "invalid_input");
}
