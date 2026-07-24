use serde::Deserialize;
use serde_json::Value;

use super::{
    A2aBinding, A2aInterface, A2aNegotiatedProfile, A2aProtocolVersion, A2aSelectionPolicy,
};

#[derive(Debug, Clone)]
pub struct ParsedAgentCard {
    pub name: String,
    pub description: Option<String>,
    pub stable_id: String,
    pub profile: A2aNegotiatedProfile,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct V1AgentCard {
    name: String,
    description: String,
    supported_interfaces: Vec<V1Interface>,
    version: String,
    capabilities: Value,
    default_input_modes: Vec<String>,
    default_output_modes: Vec<String>,
    skills: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct V1Interface {
    url: String,
    protocol_binding: String,
    protocol_version: String,
    #[serde(default)]
    tenant: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct V03AgentCard {
    protocol_version: String,
    name: String,
    description: String,
    url: String,
    preferred_transport: String,
    #[serde(default)]
    additional_interfaces: Vec<V03Interface>,
    version: String,
    capabilities: Value,
    default_input_modes: Vec<String>,
    default_output_modes: Vec<String>,
    skills: Vec<Value>,
    #[serde(default)]
    supports_authenticated_extended_card: bool,
}

#[derive(Debug, Deserialize)]
struct V03Interface {
    url: String,
    transport: String,
}

pub fn parse_agent_card(
    endpoint: &str,
    raw: &Value,
    policy: &A2aSelectionPolicy,
) -> Result<ParsedAgentCard, String> {
    let raw_bytes = serde_json::to_vec(raw).map_err(|error| error.to_string())?;
    let mut card = if raw.get("supportedInterfaces").is_some() {
        parse_v1(endpoint, raw, &raw_bytes, policy)
    } else {
        parse_v0_3(raw, &raw_bytes, policy)
    }?;
    if let Some(id) = raw
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        card.stable_id = id.to_owned();
    }
    Ok(card)
}

fn parse_v1(
    endpoint: &str,
    raw: &Value,
    raw_bytes: &[u8],
    policy: &A2aSelectionPolicy,
) -> Result<ParsedAgentCard, String> {
    let card: V1AgentCard = serde_json::from_value(raw.clone())
        .map_err(|error| format!("无效的 A2A 1.0 卡片：{error}"))?;
    validate_common(
        &card.name,
        &card.description,
        &card.version,
        &card.capabilities,
        &card.default_input_modes,
        &card.default_output_modes,
        &card.skills,
    )?;
    if card.supported_interfaces.is_empty() {
        return Err("A2A 1.0 智能体卡片必须声明 supportedInterfaces".to_owned());
    }
    let interfaces = card
        .supported_interfaces
        .into_iter()
        .map(|interface| {
            parse_interface(
                interface.url,
                &interface.protocol_binding,
                &interface.protocol_version,
                interface.tenant,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let extensions = extensions(&card.capabilities)?;
    reject_required_extensions(&extensions)?;
    let profile = A2aNegotiatedProfile::select(
        raw_bytes,
        Some(card.version),
        interfaces,
        capability(&card.capabilities, "streaming"),
        capability(&card.capabilities, "pushNotifications"),
        capability(&card.capabilities, "extendedAgentCard"),
        extensions
            .iter()
            .map(|extension| extension.0.clone())
            .collect(),
        extensions
            .iter()
            .filter(|extension| extension.1)
            .map(|extension| extension.0.clone())
            .collect(),
        policy,
    )?;
    Ok(ParsedAgentCard {
        stable_id: if endpoint.trim().is_empty() {
            profile.interface_url.clone()
        } else {
            endpoint.trim_end_matches('/').to_owned()
        },
        name: card.name.trim().to_owned(),
        description: Some(card.description.trim().to_owned()),
        profile,
    })
}

fn parse_v0_3(
    raw: &Value,
    raw_bytes: &[u8],
    policy: &A2aSelectionPolicy,
) -> Result<ParsedAgentCard, String> {
    let card: V03AgentCard = serde_json::from_value(raw.clone())
        .map_err(|error| format!("无效的 A2A 0.3 卡片：{error}"))?;
    validate_common(
        &card.name,
        &card.description,
        &card.version,
        &card.capabilities,
        &card.default_input_modes,
        &card.default_output_modes,
        &card.skills,
    )?;
    let version = card.protocol_version.parse::<A2aProtocolVersion>()?;
    if version != A2aProtocolVersion::V0_3 {
        return Err(format!(
            "legacy Agent Card structure cannot declare A2A version {version}"
        ));
    }
    let mut interfaces = vec![parse_interface(
        card.url,
        &card.preferred_transport,
        version.as_str(),
        None,
    )?];
    for interface in card.additional_interfaces {
        let parsed = parse_interface(interface.url, &interface.transport, version.as_str(), None)?;
        if !interfaces.contains(&parsed) {
            interfaces.push(parsed);
        }
    }
    let extensions = extensions(&card.capabilities)?;
    reject_required_extensions(&extensions)?;
    let profile = A2aNegotiatedProfile::select(
        raw_bytes,
        Some(card.version),
        interfaces,
        capability(&card.capabilities, "streaming"),
        capability(&card.capabilities, "pushNotifications"),
        card.supports_authenticated_extended_card,
        extensions
            .iter()
            .map(|extension| extension.0.clone())
            .collect(),
        extensions
            .iter()
            .filter(|extension| extension.1)
            .map(|extension| extension.0.clone())
            .collect(),
        policy,
    )?;
    Ok(ParsedAgentCard {
        stable_id: profile.interface_url.clone(),
        name: card.name.trim().to_owned(),
        description: Some(card.description.trim().to_owned()),
        profile,
    })
}

fn capability(capabilities: &Value, name: &str) -> bool {
    capabilities
        .get(name)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn extensions(capabilities: &Value) -> Result<Vec<(String, bool)>, String> {
    let Some(extensions) = capabilities.get("extensions") else {
        return Ok(Vec::new());
    };
    let extensions = extensions
        .as_array()
        .ok_or_else(|| "A2A capabilities.extensions must be an array".to_owned())?;
    extensions
        .iter()
        .map(|extension| {
            let uri = extension
                .get("uri")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|uri| !uri.is_empty())
                .ok_or_else(|| "A2A extension must declare a URI".to_owned())?;
            let parsed = reqwest::Url::parse(uri)
                .map_err(|error| format!("无效的 A2A 扩展 URI `{uri}`：{error}"))?;
            if parsed.scheme().is_empty() {
                return Err(format!("A2A 扩展 URI `{uri}` 必须是绝对地址"));
            }
            Ok((
                uri.to_owned(),
                extension
                    .get("required")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ))
        })
        .collect()
}

fn reject_required_extensions(extensions: &[(String, bool)]) -> Result<(), String> {
    let required = extensions
        .iter()
        .filter(|extension| extension.1)
        .map(|extension| extension.0.as_str())
        .collect::<Vec<_>>();
    if required.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "A2A 智能体卡片要求了不受支持的扩展：{}",
            required.join(", ")
        ))
    }
}

fn parse_interface(
    url: String,
    binding: &str,
    version: &str,
    tenant: Option<String>,
) -> Result<A2aInterface, String> {
    let url = validate_url(&url)?;
    Ok(A2aInterface {
        url,
        binding: A2aBinding::parse(binding)?,
        protocol_version: version.parse()?,
        tenant: tenant
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
    })
}

fn validate_common(
    name: &str,
    description: &str,
    version: &str,
    capabilities: &Value,
    input_modes: &[String],
    output_modes: &[String],
    skills: &[Value],
) -> Result<(), String> {
    for (field, value) in [
        ("name", name),
        ("description", description),
        ("version", version),
    ] {
        if value.trim().is_empty() {
            return Err(format!("A2A 智能体卡片字段 `{field}` 不能为空"));
        }
    }
    if !capabilities.is_object() {
        return Err("A2A 智能体卡片的 capabilities 必须是一个对象".to_owned());
    }
    if input_modes.is_empty() || output_modes.is_empty() {
        return Err("A2A 智能体卡片必须声明默认的输入与输出模式".to_owned());
    }
    if skills.is_empty() {
        return Err("A2A 智能体卡片必须至少声明一个技能".to_owned());
    }
    Ok(())
}

fn validate_url(value: &str) -> Result<String, String> {
    let value = value.trim();
    let parsed = reqwest::Url::parse(value)
        .map_err(|error| format!("无效的 A2A 接口 URL `{value}`：{error}"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(format!(
            "A2A interface URL `{value}` must be an absolute HTTP(S) URL"
        ));
    }
    Ok(value.to_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn common() -> Value {
        json!({
            "name": "Research agent",
            "description": "Performs research",
            "version": "2.1.0",
            "capabilities": {"streaming": true},
            "defaultInputModes": ["text/plain"],
            "defaultOutputModes": ["text/plain"],
            "skills": [{
                "id": "research",
                "name": "Research",
                "description": "Researches a subject",
                "tags": ["research"]
            }]
        })
    }

    #[test]
    fn parses_v1_supported_interfaces() {
        let mut card = common();
        card.as_object_mut().unwrap().insert(
            "supportedInterfaces".to_owned(),
            json!([
                {
                    "url": "https://agent.example/rpc-v03",
                    "protocolBinding": "JSONRPC",
                    "protocolVersion": "0.3"
                },
                {
                    "url": "https://agent.example/rpc",
                    "protocolBinding": "JSONRPC",
                    "protocolVersion": "1.0",
                    "tenant": "tenant-a"
                }
            ]),
        );

        let parsed = parse_agent_card("https://agent.example", &card, &Default::default()).unwrap();

        assert_eq!(parsed.profile.protocol_version, A2aProtocolVersion::V1_0);
        assert_eq!(parsed.stable_id, "https://agent.example");
        assert_eq!(parsed.profile.interface_url, "https://agent.example/rpc");
        assert_eq!(parsed.profile.tenant.as_deref(), Some("tenant-a"));
        assert!(parsed.profile.streaming);
        assert!(!parsed.profile.push_notifications);
    }

    #[test]
    fn rejects_required_extensions_the_client_does_not_implement() {
        let mut card = common();
        card.as_object_mut().unwrap().insert(
            "supportedInterfaces".to_owned(),
            json!([{
                "url": "https://agent.example/rpc",
                "protocolBinding": "JSONRPC",
                "protocolVersion": "1.0"
            }]),
        );
        card["capabilities"]["extensions"] = json!([{
            "uri": "https://extensions.example/required/v1",
            "required": true
        }]);

        let error =
            parse_agent_card("https://agent.example", &card, &Default::default()).unwrap_err();
        assert!(error.contains("不受支持的扩展"));
    }

    #[test]
    fn parses_v0_3_legacy_card() {
        let mut card = common();
        card.as_object_mut()
            .unwrap()
            .insert("protocolVersion".to_owned(), json!("0.3.0"));
        card.as_object_mut()
            .unwrap()
            .insert("url".to_owned(), json!("https://agent.example/rpc"));
        card.as_object_mut()
            .unwrap()
            .insert("preferredTransport".to_owned(), json!("JSONRPC"));
        card.as_object_mut()
            .unwrap()
            .insert("id".to_owned(), json!("legacy-agent-id"));

        let parsed = parse_agent_card("https://agent.example", &card, &Default::default()).unwrap();

        assert_eq!(parsed.profile.protocol_version, A2aProtocolVersion::V0_3);
        assert_eq!(parsed.stable_id, "legacy-agent-id");
        assert_eq!(
            parsed.profile.selection_reason,
            "legacy_only_compatible_version"
        );
    }

    #[test]
    fn rejects_unknown_protocol_version() {
        let mut card = common();
        card.as_object_mut().unwrap().insert(
            "supportedInterfaces".to_owned(),
            json!([{
                "url": "https://agent.example/rpc",
                "protocolBinding": "JSONRPC",
                "protocolVersion": "2.0"
            }]),
        );

        let error =
            parse_agent_card("https://agent.example", &card, &Default::default()).unwrap_err();

        assert!(error.contains("不受支持的 A2A 协议版本"));
    }
}
