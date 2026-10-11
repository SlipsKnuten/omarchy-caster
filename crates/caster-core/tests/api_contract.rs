use omarchy_caster_core::{Status, api::Response};

fn fixture(value: &str) -> serde_json::Value {
    serde_json::from_str(value).unwrap()
}

#[test]
fn default_status_matches_public_fixture() {
    let actual = serde_json::to_value(Response::success(Status::default())).unwrap();
    let expected = fixture(include_str!("fixtures/status-success.json"));
    assert_eq!(actual, expected);
}

#[test]
fn error_matches_public_fixture() {
    let actual =
        serde_json::to_value(Response::<()>::error("fixture_error", "fixture message")).unwrap();
    let expected = fixture(include_str!("fixtures/error.json"));
    assert_eq!(actual, expected);
}
