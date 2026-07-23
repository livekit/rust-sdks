use std::collections::HashMap;

/// Per-call overrides used to parameterize a token request.
///
/// Every field is optional: `None` means "leave it out of the request" and the
/// server picks a default. Because the struct derives `Default`, you only need
/// to set the fields you care about:
///
/// ```
/// TokenSourceFetchOptions {
///     room_name: Some("my-room".to_string()),
///     ..Default::default()
/// };
/// ```
#[derive(Default, Clone, Debug, uniffi::Record)]
pub struct TokenSourceFetchOptions {
    /// The name of the room being requested when generating credentials.
    #[uniffi(default)]
    pub room_name: Option<String>,
    /// The name of the participant being requested when generating credentials.
    #[uniffi(default)]
    pub participant_name: Option<String>,
    /// The identity of the participant being requested when generating credentials.
    #[uniffi(default)]
    pub participant_identity: Option<String>,
    /// The metadata of the participant being requested when generating credentials.
    #[uniffi(default)]
    pub participant_metadata: Option<String>,
    /// The attributes of the participant being requested when generating credentials.
    #[uniffi(default)]
    pub participant_attributes: Option<HashMap<String, String>>,
    /// The name of the agent to dispatch into the room.
    #[uniffi(default)]
    pub agent_name: Option<String>,
    /// The metadata to pass to the dispatched agent.
    #[uniffi(default)]
    pub agent_metadata: Option<String>,
    /// Optional deployment to target. Leave empty to target the production deployment.
    #[uniffi(default)]
    pub agent_deployment: Option<String>,
}

/// The JSON body posted to the token endpoint. Built from [`TokenSourceFetchOptions`];
/// the flat agent fields get nested under `room_config.agents` to match the server's schema.
#[derive(serde::Serialize)]
pub(crate) struct TokenSourceRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    room_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    participant_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    participant_identity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    participant_metadata: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    participant_attributes: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    room_config: Option<RoomConfig>,
}

#[derive(serde::Serialize)]
struct RoomConfig {
    agents: Vec<AgentDispatch>,
}

#[derive(serde::Serialize)]
struct AgentDispatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deployment: Option<String>,
}

impl From<&TokenSourceFetchOptions> for TokenSourceRequest {
    fn from(options: &TokenSourceFetchOptions) -> TokenSourceRequest {
        // Only include a room_config when at least one agent field is set.
        let room_config = if options.agent_name.is_some()
            || options.agent_metadata.is_some()
            || options.agent_deployment.is_some()
        {
            Some(RoomConfig {
                agents: vec![AgentDispatch {
                    agent_name: options.agent_name.clone(),
                    metadata: options.agent_metadata.clone(),
                    deployment: options.agent_deployment.clone(),
                }],
            })
        } else {
            None
        };

        TokenSourceRequest {
            room_name: options.room_name.clone(),
            participant_name: options.participant_name.clone(),
            participant_identity: options.participant_identity.clone(),
            participant_metadata: options.participant_metadata.clone(),
            participant_attributes: options.participant_attributes.clone(),
            room_config,
        }
    }
}
