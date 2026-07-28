use super::*;

/// `TuiClient` must stay object-safe — every future widget (T1's input bar,
/// T2's card editor, …) is written against `&dyn TuiClient`/`Arc<dyn
/// TuiClient>` so it never needs to know whether it's driving a
/// `LocalClient` or a `RemoteClient`. A method added later that breaks
/// object safety (e.g. a generic parameter, or `Self` by value) would fail
/// this at compile time rather than surfacing as a confusing error deep in
/// a widget module.
#[allow(dead_code)]
fn assert_object_safe(_client: &dyn TuiClient) {}

#[test]
fn client_error_messages_name_the_failure_kind() {
    assert_eq!(
        ClientError::NoToken("create_task".to_string()).to_string(),
        "no auth token configured for create_task"
    );
    assert_eq!(
        ClientError::Unauthorized("list_tasks".to_string()).to_string(),
        "unauthorized: list_tasks"
    );
    assert_eq!(
        ClientError::NotFound("abc123".to_string()).to_string(),
        "not found: abc123"
    );
    assert_eq!(
        ClientError::Unsupported("chains".to_string()).to_string(),
        "unsupported: chains"
    );
}
