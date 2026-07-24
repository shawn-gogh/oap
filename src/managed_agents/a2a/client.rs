use serde_json::{json, Map, Value};

use crate::managed_agents::adapters::source::NegotiatedSourceProfile;

use super::{A2aBinding, A2aProtocolVersion};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum A2aClientError {
    #[error("invalid A2A JSON-RPC response: {0}")]
    InvalidResponse(String),
    #[error("A2A {name} ({code}): {message}")]
    Remote {
        code: i64,
        name: &'static str,
        message: String,
        data: Option<Value>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct A2aRuntimeProfile {
    pub protocol_version: A2aProtocolVersion,
    pub binding: A2aBinding,
    pub interface_url: String,
    pub tenant: Option<String>,
    pub streaming: bool,
    pub push_notifications: bool,
    pub extended_agent_card: bool,
    pub extensions: Vec<String>,
    pub required_extensions: Vec<String>,
}

impl TryFrom<&NegotiatedSourceProfile> for A2aRuntimeProfile {
    type Error = String;

    fn try_from(profile: &NegotiatedSourceProfile) -> Result<Self, Self::Error> {
        if profile.protocol != "a2a" {
            return Err(format!(
                "expected A2A protocol profile, found `{}`",
                profile.protocol
            ));
        }
        let binding = A2aBinding::parse(&profile.protocol_binding)?;
        if binding != A2aBinding::JsonRpc {
            return Err(format!(
                "A2A binding `{}` is not implemented by the JSON-RPC client",
                binding.as_str()
            ));
        }
        Ok(Self {
            protocol_version: profile.protocol_version.parse()?,
            binding,
            interface_url: profile.interface_url.clone(),
            tenant: profile.tenant.clone(),
            streaming: profile.streaming,
            push_notifications: profile.push_notifications,
            extended_agent_card: profile.extended_agent_card,
            extensions: profile.extensions.clone(),
            required_extensions: profile.required_extensions.clone(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A2aJsonRpcOperation {
    SendMessage,
    SendStreamingMessage,
    GetTask,
    ListTasks,
    CancelTask,
    SubscribeToTask,
    CreatePushNotification,
    GetPushNotification,
    ListPushNotifications,
    DeletePushNotification,
    GetExtendedAgentCard,
}

impl A2aJsonRpcOperation {
    pub fn method(self, version: A2aProtocolVersion) -> Result<&'static str, String> {
        let method = match (self, version) {
            (Self::SendMessage, A2aProtocolVersion::V0_3) => "message/send",
            (Self::SendStreamingMessage, A2aProtocolVersion::V0_3) => "message/stream",
            (Self::GetTask, A2aProtocolVersion::V0_3) => "tasks/get",
            (Self::ListTasks, A2aProtocolVersion::V0_3) => {
                return Err("A2A 0.3 does not define ListTasks".to_owned())
            }
            (Self::CancelTask, A2aProtocolVersion::V0_3) => "tasks/cancel",
            (Self::SubscribeToTask, A2aProtocolVersion::V0_3) => "tasks/resubscribe",
            (Self::CreatePushNotification, A2aProtocolVersion::V0_3) => {
                "tasks/pushNotificationConfig/set"
            }
            (Self::GetPushNotification, A2aProtocolVersion::V0_3) => {
                "tasks/pushNotificationConfig/get"
            }
            (Self::ListPushNotifications, A2aProtocolVersion::V0_3) => {
                "tasks/pushNotificationConfig/list"
            }
            (Self::DeletePushNotification, A2aProtocolVersion::V0_3) => {
                "tasks/pushNotificationConfig/delete"
            }
            (Self::GetExtendedAgentCard, A2aProtocolVersion::V0_3) => {
                "agent/getAuthenticatedExtendedCard"
            }
            (Self::SendMessage, A2aProtocolVersion::V1_0) => "SendMessage",
            (Self::SendStreamingMessage, A2aProtocolVersion::V1_0) => "SendStreamingMessage",
            (Self::GetTask, A2aProtocolVersion::V1_0) => "GetTask",
            (Self::ListTasks, A2aProtocolVersion::V1_0) => "ListTasks",
            (Self::CancelTask, A2aProtocolVersion::V1_0) => "CancelTask",
            (Self::SubscribeToTask, A2aProtocolVersion::V1_0) => "SubscribeToTask",
            (Self::CreatePushNotification, A2aProtocolVersion::V1_0) => {
                "CreateTaskPushNotificationConfig"
            }
            (Self::GetPushNotification, A2aProtocolVersion::V1_0) => {
                "GetTaskPushNotificationConfig"
            }
            (Self::ListPushNotifications, A2aProtocolVersion::V1_0) => {
                "ListTaskPushNotificationConfigs"
            }
            (Self::DeletePushNotification, A2aProtocolVersion::V1_0) => {
                "DeleteTaskPushNotificationConfig"
            }
            (Self::GetExtendedAgentCard, A2aProtocolVersion::V1_0) => "GetExtendedAgentCard",
        };
        Ok(method)
    }
}

pub fn json_rpc_request(
    profile: &A2aRuntimeProfile,
    operation: A2aJsonRpcOperation,
    request_id: &str,
    mut params: Value,
) -> Result<Value, String> {
    let params = params
        .as_object_mut()
        .ok_or_else(|| "A2A JSON-RPC params must be an object".to_owned())?;
    if let Some(tenant) = profile.tenant.as_deref() {
        params.insert("tenant".to_owned(), Value::String(tenant.to_owned()));
    }
    Ok(json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": operation.method(profile.protocol_version)?,
        "params": params,
    }))
}

pub fn send_message_params(
    profile: &A2aRuntimeProfile,
    message_id: &str,
    text: &str,
    task_id: Option<&str>,
    context_id: Option<&str>,
) -> Value {
    send_message_params_with_parts(
        profile,
        message_id,
        vec![Value::String(text.to_owned())],
        task_id,
        context_id,
    )
}

pub fn send_message_params_with_parts(
    profile: &A2aRuntimeProfile,
    message_id: &str,
    parts: Vec<Value>,
    task_id: Option<&str>,
    context_id: Option<&str>,
) -> Value {
    let mut message = Map::new();
    message.insert("messageId".to_owned(), Value::String(message_id.to_owned()));
    match profile.protocol_version {
        A2aProtocolVersion::V0_3 => {
            message.insert("kind".to_owned(), Value::String("message".to_owned()));
            message.insert("role".to_owned(), Value::String("user".to_owned()));
            message.insert(
                "parts".to_owned(),
                Value::Array(parts.into_iter().map(v0_3_part).collect()),
            );
        }
        A2aProtocolVersion::V1_0 => {
            message.insert("role".to_owned(), Value::String("ROLE_USER".to_owned()));
            message.insert(
                "parts".to_owned(),
                Value::Array(parts.into_iter().map(v1_part).collect()),
            );
        }
    }
    if let Some(task_id) = task_id {
        message.insert("taskId".to_owned(), Value::String(task_id.to_owned()));
    }
    if let Some(context_id) = context_id {
        message.insert("contextId".to_owned(), Value::String(context_id.to_owned()));
    }
    json!({"message": message})
}

fn v0_3_part(part: Value) -> Value {
    match part {
        Value::String(text) => json!({"kind": "text", "text": text}),
        Value::Object(mut value) => {
            if !value.contains_key("kind") {
                let kind = if value.contains_key("text") {
                    "text"
                } else if value.contains_key("file") {
                    "file"
                } else {
                    "data"
                };
                value.insert("kind".to_owned(), Value::String(kind.to_owned()));
            }
            Value::Object(value)
        }
        value => json!({"kind": "data", "data": value}),
    }
}

fn v1_part(part: Value) -> Value {
    match part {
        Value::String(text) => json!({"text": text}),
        Value::Object(mut value) => {
            value.remove("kind");
            Value::Object(value)
        }
        value => json!({"data": value}),
    }
}

pub fn task_params(task_id: &str) -> Value {
    task_params_with_history(task_id, None)
}

pub fn task_params_with_history(task_id: &str, history_length: Option<u32>) -> Value {
    let mut params = Map::new();
    params.insert("id".to_owned(), Value::String(task_id.to_owned()));
    if let Some(history_length) = history_length {
        params.insert(
            "historyLength".to_owned(),
            Value::Number(history_length.into()),
        );
    }
    Value::Object(params)
}

pub fn push_notification_params(
    profile: &A2aRuntimeProfile,
    task_id: &str,
    url: &str,
    token: &str,
) -> Value {
    match profile.protocol_version {
        A2aProtocolVersion::V0_3 => json!({
            "taskId": task_id,
            "pushNotificationConfig": {
                "url": url,
                "token": token,
                "authentication": {"schemes": ["Bearer"]}
            }
        }),
        A2aProtocolVersion::V1_0 => json!({
            "taskId": task_id,
            "url": url,
            "token": token,
            "authentication": {
                "scheme": "Bearer",
                "credentials": token
            }
        }),
    }
}

pub fn push_notification_identity_params(
    profile: &A2aRuntimeProfile,
    task_id: &str,
    config_id: &str,
) -> Value {
    match profile.protocol_version {
        A2aProtocolVersion::V0_3 => {
            json!({"id": task_id, "pushNotificationConfigId": config_id})
        }
        A2aProtocolVersion::V1_0 => json!({"taskId": task_id, "id": config_id}),
    }
}

pub fn decode_json_rpc_response(
    payload: Value,
    expected_id: &str,
    operation: A2aJsonRpcOperation,
    version: A2aProtocolVersion,
) -> Result<Value, A2aClientError> {
    if payload.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(A2aClientError::InvalidResponse(
            "response is not JSON-RPC 2.0".to_owned(),
        ));
    }
    if payload.get("id").and_then(Value::as_str) != Some(expected_id) {
        return Err(A2aClientError::InvalidResponse(
            "response id does not match the request id".to_owned(),
        ));
    }
    if payload.get("error").is_some() && payload.get("result").is_some() {
        return Err(A2aClientError::InvalidResponse(
            "response contains both result and error".to_owned(),
        ));
    }
    if let Some(error) = payload.get("error") {
        let code = error
            .get("code")
            .and_then(Value::as_i64)
            .unwrap_or_default();
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown A2A error")
            .to_owned();
        return Err(A2aClientError::Remote {
            code,
            name: a2a_error_name(code),
            message,
            data: error.get("data").cloned(),
        });
    }
    let result = payload.get("result").cloned().ok_or_else(|| {
        A2aClientError::InvalidResponse("response contains neither result nor error".to_owned())
    })?;
    if version == A2aProtocolVersion::V1_0
        && matches!(
            operation,
            A2aJsonRpcOperation::SendMessage
                | A2aJsonRpcOperation::SendStreamingMessage
                | A2aJsonRpcOperation::SubscribeToTask
        )
    {
        if matches!(
            operation,
            A2aJsonRpcOperation::SendStreamingMessage | A2aJsonRpcOperation::SubscribeToTask
        ) && (result.get("status").is_some()
            || result.get("artifact").is_some()
            || result.get("taskId").is_some())
        {
            return Ok(result);
        }
        if matches!(
            operation,
            A2aJsonRpcOperation::SendStreamingMessage | A2aJsonRpcOperation::SubscribeToTask
        ) {
            if let Some(update) = result
                .get("statusUpdate")
                .or_else(|| result.get("artifactUpdate"))
            {
                return Ok(update.clone());
            }
        }
        return result
            .get("task")
            .or_else(|| result.get("message"))
            .cloned()
            .ok_or_else(|| {
                A2aClientError::InvalidResponse(
                    "A2A 1.0 streaming result contains no task, message, or update".to_owned(),
                )
            });
    }
    Ok(result)
}

fn a2a_error_name(code: i64) -> &'static str {
    match code {
        -32001 => "TaskNotFoundError",
        -32002 => "TaskNotCancelableError",
        -32003 => "PushNotificationNotSupportedError",
        -32004 => "UnsupportedOperationError",
        -32005 => "ContentTypeNotSupportedError",
        -32006 => "InvalidAgentResponseError",
        -32007 => "ExtendedAgentCardNotConfiguredError",
        -32008 => "ExtensionSupportRequiredError",
        -32009 => "VersionNotSupportedError",
        -32600 => "InvalidRequestError",
        -32601 => "MethodNotFoundError",
        -32602 => "InvalidParamsError",
        -32603 => "InternalError",
        _ => "RemoteError",
    }
}

pub fn normalize_task_state(state: &str) -> Option<&'static str> {
    match state {
        "submitted" | "TASK_STATE_SUBMITTED" => Some("submitted"),
        "working" | "TASK_STATE_WORKING" => Some("working"),
        "completed" | "TASK_STATE_COMPLETED" => Some("completed"),
        "failed" | "TASK_STATE_FAILED" => Some("failed"),
        "canceled" | "cancelled" | "TASK_STATE_CANCELED" => Some("canceled"),
        "rejected" | "TASK_STATE_REJECTED" => Some("rejected"),
        "input-required" | "TASK_STATE_INPUT_REQUIRED" => Some("input-required"),
        "auth-required" | "TASK_STATE_AUTH_REQUIRED" => Some("auth-required"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(version: A2aProtocolVersion) -> A2aRuntimeProfile {
        A2aRuntimeProfile {
            protocol_version: version,
            binding: A2aBinding::JsonRpc,
            interface_url: "https://agent.example/rpc".to_owned(),
            tenant: Some("tenant-a".to_owned()),
            streaming: true,
            push_notifications: false,
            extended_agent_card: false,
            extensions: Vec::new(),
            required_extensions: Vec::new(),
        }
    }

    #[test]
    fn builds_versioned_send_message_requests() {
        let request = json_rpc_request(
            &profile(A2aProtocolVersion::V0_3),
            A2aJsonRpcOperation::SendMessage,
            "request-1",
            send_message_params(
                &profile(A2aProtocolVersion::V0_3),
                "message-1",
                "hello",
                Some("task-1"),
                Some("context-1"),
            ),
        )
        .unwrap();
        assert_eq!(request["method"], "message/send");
        assert_eq!(request["params"]["message"]["role"], "user");
        assert_eq!(request["params"]["message"]["taskId"], "task-1");
        assert_eq!(request["params"]["tenant"], "tenant-a");

        let request = json_rpc_request(
            &profile(A2aProtocolVersion::V1_0),
            A2aJsonRpcOperation::SendMessage,
            "request-2",
            send_message_params(
                &profile(A2aProtocolVersion::V1_0),
                "message-2",
                "hello",
                None,
                None,
            ),
        )
        .unwrap();
        assert_eq!(request["method"], "SendMessage");
        assert_eq!(request["params"]["message"]["role"], "ROLE_USER");
        assert!(request["params"]["message"]["parts"][0]
            .get("kind")
            .is_none());
    }

    #[test]
    fn unwraps_v1_send_message_response_and_validates_id() {
        let task = decode_json_rpc_response(
            json!({
                "jsonrpc": "2.0",
                "id": "request-1",
                "result": {"task": {"id": "task-1"}}
            }),
            "request-1",
            A2aJsonRpcOperation::SendMessage,
            A2aProtocolVersion::V1_0,
        )
        .unwrap();
        assert_eq!(task["id"], "task-1");

        let error = decode_json_rpc_response(
            json!({"jsonrpc": "2.0", "id": "wrong", "result": {}}),
            "request-1",
            A2aJsonRpcOperation::GetTask,
            A2aProtocolVersion::V1_0,
        )
        .unwrap_err();
        assert!(error.to_string().contains("does not match"));
    }

    #[test]
    fn maps_standard_a2a_errors() {
        let error = decode_json_rpc_response(
            json!({
                "jsonrpc": "2.0",
                "id": "request-1",
                "error": {
                    "code": -32009,
                    "message": "unsupported version",
                    "data": [{"supportedVersions": ["1.0"]}]
                }
            }),
            "request-1",
            A2aJsonRpcOperation::SendMessage,
            A2aProtocolVersion::V1_0,
        )
        .unwrap_err();

        assert!(matches!(
            error,
            A2aClientError::Remote {
                code: -32009,
                name: "VersionNotSupportedError",
                ..
            }
        ));
    }

    #[test]
    fn normalizes_versioned_task_states() {
        assert_eq!(
            normalize_task_state("TASK_STATE_INPUT_REQUIRED"),
            Some("input-required")
        );
        assert_eq!(normalize_task_state("completed"), Some("completed"));
        assert_eq!(normalize_task_state("TASK_STATE_UNSPECIFIED"), None);
    }

    #[test]
    fn builds_rich_parts_and_versioned_stream_methods() {
        let parts = vec![
            Value::String("hello".to_owned()),
            json!({"data": {"answer": 42}}),
            json!({"file": {"name": "report.pdf", "uri": "https://files.example/report.pdf"}}),
        ];
        let request = json_rpc_request(
            &profile(A2aProtocolVersion::V0_3),
            A2aJsonRpcOperation::SendStreamingMessage,
            "request-1",
            send_message_params_with_parts(
                &profile(A2aProtocolVersion::V0_3),
                "message-1",
                parts.clone(),
                None,
                None,
            ),
        )
        .unwrap();
        assert_eq!(request["method"], "message/stream");
        assert_eq!(request["params"]["message"]["parts"][1]["kind"], "data");

        let request = json_rpc_request(
            &profile(A2aProtocolVersion::V1_0),
            A2aJsonRpcOperation::SendStreamingMessage,
            "request-2",
            send_message_params_with_parts(
                &profile(A2aProtocolVersion::V1_0),
                "message-2",
                parts,
                None,
                None,
            ),
        )
        .unwrap();
        assert_eq!(request["method"], "SendStreamingMessage");
        assert!(request["params"]["message"]["parts"][1]
            .get("kind")
            .is_none());

        let request = json_rpc_request(
            &profile(A2aProtocolVersion::V1_0),
            A2aJsonRpcOperation::CreatePushNotification,
            "request-3",
            push_notification_params(
                &profile(A2aProtocolVersion::V1_0),
                "task-1",
                "https://gateway.example/api/a2a/push/inv-1",
                "secret",
            ),
        )
        .unwrap();
        assert_eq!(request["method"], "CreateTaskPushNotificationConfig");
        assert_eq!(request["params"]["authentication"]["scheme"], "Bearer");
        assert_eq!(
            push_notification_identity_params(
                &profile(A2aProtocolVersion::V0_3),
                "task-1",
                "config-1",
            ),
            json!({
                "id": "task-1",
                "pushNotificationConfigId": "config-1"
            })
        );
        assert_eq!(
            push_notification_identity_params(
                &profile(A2aProtocolVersion::V1_0),
                "task-1",
                "config-1",
            ),
            json!({"taskId": "task-1", "id": "config-1"})
        );
        assert!(json_rpc_request(
            &profile(A2aProtocolVersion::V0_3),
            A2aJsonRpcOperation::ListTasks,
            "request-4",
            json!({}),
        )
        .is_err());
    }

    #[test]
    fn locks_the_complete_versioned_operation_matrix() {
        let cases = [
            (
                A2aJsonRpcOperation::SubscribeToTask,
                "tasks/resubscribe",
                "SubscribeToTask",
            ),
            (
                A2aJsonRpcOperation::CreatePushNotification,
                "tasks/pushNotificationConfig/set",
                "CreateTaskPushNotificationConfig",
            ),
            (
                A2aJsonRpcOperation::GetPushNotification,
                "tasks/pushNotificationConfig/get",
                "GetTaskPushNotificationConfig",
            ),
            (
                A2aJsonRpcOperation::ListPushNotifications,
                "tasks/pushNotificationConfig/list",
                "ListTaskPushNotificationConfigs",
            ),
            (
                A2aJsonRpcOperation::DeletePushNotification,
                "tasks/pushNotificationConfig/delete",
                "DeleteTaskPushNotificationConfig",
            ),
            (
                A2aJsonRpcOperation::GetExtendedAgentCard,
                "agent/getAuthenticatedExtendedCard",
                "GetExtendedAgentCard",
            ),
        ];
        for (operation, v0_3, v1_0) in cases {
            assert_eq!(operation.method(A2aProtocolVersion::V0_3).unwrap(), v0_3);
            assert_eq!(operation.method(A2aProtocolVersion::V1_0).unwrap(), v1_0);
        }
        assert_eq!(
            A2aJsonRpcOperation::ListTasks
                .method(A2aProtocolVersion::V1_0)
                .unwrap(),
            "ListTasks"
        );
        assert!(A2aJsonRpcOperation::ListTasks
            .method(A2aProtocolVersion::V0_3)
            .is_err());
    }
}
