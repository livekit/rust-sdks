// @generated
impl serde::Serialize for AgentConfigUpdate {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.id.is_empty() {
            len += 1;
        }
        if self.instructions.is_some() {
            len += 1;
        }
        if !self.tools_added.is_empty() {
            len += 1;
        }
        if !self.tools_removed.is_empty() {
            len += 1;
        }
        if self.created_at.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentConfigUpdate", len)?;
        if !self.id.is_empty() {
            struct_ser.serialize_field("id", &self.id)?;
        }
        if let Some(v) = self.instructions.as_ref() {
            struct_ser.serialize_field("instructions", v)?;
        }
        if !self.tools_added.is_empty() {
            struct_ser.serialize_field("toolsAdded", &self.tools_added)?;
        }
        if !self.tools_removed.is_empty() {
            struct_ser.serialize_field("toolsRemoved", &self.tools_removed)?;
        }
        if let Some(v) = self.created_at.as_ref() {
            struct_ser.serialize_field("createdAt", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AgentConfigUpdate {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "id",
            "instructions",
            "tools_added",
            "toolsAdded",
            "tools_removed",
            "toolsRemoved",
            "created_at",
            "createdAt",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Id,
            Instructions,
            ToolsAdded,
            ToolsRemoved,
            CreatedAt,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "id" => Ok(GeneratedField::Id),
                            "instructions" => Ok(GeneratedField::Instructions),
                            "toolsAdded" | "tools_added" => Ok(GeneratedField::ToolsAdded),
                            "toolsRemoved" | "tools_removed" => Ok(GeneratedField::ToolsRemoved),
                            "createdAt" | "created_at" => Ok(GeneratedField::CreatedAt),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AgentConfigUpdate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentConfigUpdate")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AgentConfigUpdate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut id__ = None;
                let mut instructions__ = None;
                let mut tools_added__ = None;
                let mut tools_removed__ = None;
                let mut created_at__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Instructions => {
                            if instructions__.is_some() {
                                return Err(serde::de::Error::duplicate_field("instructions"));
                            }
                            instructions__ = map_.next_value()?;
                        }
                        GeneratedField::ToolsAdded => {
                            if tools_added__.is_some() {
                                return Err(serde::de::Error::duplicate_field("toolsAdded"));
                            }
                            tools_added__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ToolsRemoved => {
                            if tools_removed__.is_some() {
                                return Err(serde::de::Error::duplicate_field("toolsRemoved"));
                            }
                            tools_removed__ = Some(map_.next_value()?);
                        }
                        GeneratedField::CreatedAt => {
                            if created_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdAt"));
                            }
                            created_at__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AgentConfigUpdate {
                    id: id__.unwrap_or_default(),
                    instructions: instructions__,
                    tools_added: tools_added__.unwrap_or_default(),
                    tools_removed: tools_removed__.unwrap_or_default(),
                    created_at: created_at__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentConfigUpdate", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AgentDevMessage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.message.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentDevMessage", len)?;
        if let Some(v) = self.message.as_ref() {
            match v {
                agent_dev_message::Message::GetRunningJobsRequest(v) => {
                    struct_ser.serialize_field("getRunningJobsRequest", v)?;
                }
                agent_dev_message::Message::GetRunningJobsResponse(v) => {
                    struct_ser.serialize_field("getRunningJobsResponse", v)?;
                }
                agent_dev_message::Message::ServerInfo(v) => {
                    struct_ser.serialize_field("serverInfo", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AgentDevMessage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "get_running_jobs_request",
            "getRunningJobsRequest",
            "get_running_jobs_response",
            "getRunningJobsResponse",
            "server_info",
            "serverInfo",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            GetRunningJobsRequest,
            GetRunningJobsResponse,
            ServerInfo,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "getRunningJobsRequest" | "get_running_jobs_request" => Ok(GeneratedField::GetRunningJobsRequest),
                            "getRunningJobsResponse" | "get_running_jobs_response" => Ok(GeneratedField::GetRunningJobsResponse),
                            "serverInfo" | "server_info" => Ok(GeneratedField::ServerInfo),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AgentDevMessage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentDevMessage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AgentDevMessage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut message__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::GetRunningJobsRequest => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("getRunningJobsRequest"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_dev_message::Message::GetRunningJobsRequest)
;
                        }
                        GeneratedField::GetRunningJobsResponse => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("getRunningJobsResponse"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_dev_message::Message::GetRunningJobsResponse)
;
                        }
                        GeneratedField::ServerInfo => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("serverInfo"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_dev_message::Message::ServerInfo)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AgentDevMessage {
                    message: message__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentDevMessage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AgentHandoff {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.id.is_empty() {
            len += 1;
        }
        if self.old_agent_id.is_some() {
            len += 1;
        }
        if !self.new_agent_id.is_empty() {
            len += 1;
        }
        if self.created_at.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentHandoff", len)?;
        if !self.id.is_empty() {
            struct_ser.serialize_field("id", &self.id)?;
        }
        if let Some(v) = self.old_agent_id.as_ref() {
            struct_ser.serialize_field("oldAgentId", v)?;
        }
        if !self.new_agent_id.is_empty() {
            struct_ser.serialize_field("newAgentId", &self.new_agent_id)?;
        }
        if let Some(v) = self.created_at.as_ref() {
            struct_ser.serialize_field("createdAt", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AgentHandoff {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "id",
            "old_agent_id",
            "oldAgentId",
            "new_agent_id",
            "newAgentId",
            "created_at",
            "createdAt",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Id,
            OldAgentId,
            NewAgentId,
            CreatedAt,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "id" => Ok(GeneratedField::Id),
                            "oldAgentId" | "old_agent_id" => Ok(GeneratedField::OldAgentId),
                            "newAgentId" | "new_agent_id" => Ok(GeneratedField::NewAgentId),
                            "createdAt" | "created_at" => Ok(GeneratedField::CreatedAt),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AgentHandoff;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentHandoff")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AgentHandoff, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut id__ = None;
                let mut old_agent_id__ = None;
                let mut new_agent_id__ = None;
                let mut created_at__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::OldAgentId => {
                            if old_agent_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("oldAgentId"));
                            }
                            old_agent_id__ = map_.next_value()?;
                        }
                        GeneratedField::NewAgentId => {
                            if new_agent_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("newAgentId"));
                            }
                            new_agent_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::CreatedAt => {
                            if created_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdAt"));
                            }
                            created_at__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AgentHandoff {
                    id: id__.unwrap_or_default(),
                    old_agent_id: old_agent_id__,
                    new_agent_id: new_agent_id__.unwrap_or_default(),
                    created_at: created_at__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentHandoff", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AgentSessionEvent {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.created_at.is_some() {
            len += 1;
        }
        if self.event.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent", len)?;
        if let Some(v) = self.created_at.as_ref() {
            struct_ser.serialize_field("createdAt", v)?;
        }
        if let Some(v) = self.event.as_ref() {
            match v {
                agent_session_event::Event::AgentStateChanged(v) => {
                    struct_ser.serialize_field("agentStateChanged", v)?;
                }
                agent_session_event::Event::UserStateChanged(v) => {
                    struct_ser.serialize_field("userStateChanged", v)?;
                }
                agent_session_event::Event::ConversationItemAdded(v) => {
                    struct_ser.serialize_field("conversationItemAdded", v)?;
                }
                agent_session_event::Event::UserInputTranscribed(v) => {
                    struct_ser.serialize_field("userInputTranscribed", v)?;
                }
                agent_session_event::Event::FunctionToolsExecuted(v) => {
                    struct_ser.serialize_field("functionToolsExecuted", v)?;
                }
                agent_session_event::Event::Error(v) => {
                    struct_ser.serialize_field("error", v)?;
                }
                agent_session_event::Event::OverlappingSpeech(v) => {
                    struct_ser.serialize_field("overlappingSpeech", v)?;
                }
                agent_session_event::Event::SessionUsageUpdated(v) => {
                    struct_ser.serialize_field("sessionUsageUpdated", v)?;
                }
                agent_session_event::Event::AmdPrediction(v) => {
                    struct_ser.serialize_field("amdPrediction", v)?;
                }
                agent_session_event::Event::EotPrediction(v) => {
                    struct_ser.serialize_field("eotPrediction", v)?;
                }
                agent_session_event::Event::FunctionToolsStarted(v) => {
                    struct_ser.serialize_field("functionToolsStarted", v)?;
                }
                agent_session_event::Event::DebugMessage(v) => {
                    struct_ser.serialize_field("debugMessage", v)?;
                }
                agent_session_event::Event::ToolExecutionUpdated(v) => {
                    struct_ser.serialize_field("toolExecutionUpdated", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AgentSessionEvent {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "created_at",
            "createdAt",
            "agent_state_changed",
            "agentStateChanged",
            "user_state_changed",
            "userStateChanged",
            "conversation_item_added",
            "conversationItemAdded",
            "user_input_transcribed",
            "userInputTranscribed",
            "function_tools_executed",
            "functionToolsExecuted",
            "error",
            "overlapping_speech",
            "overlappingSpeech",
            "session_usage_updated",
            "sessionUsageUpdated",
            "amd_prediction",
            "amdPrediction",
            "eot_prediction",
            "eotPrediction",
            "function_tools_started",
            "functionToolsStarted",
            "debug_message",
            "debugMessage",
            "tool_execution_updated",
            "toolExecutionUpdated",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            CreatedAt,
            AgentStateChanged,
            UserStateChanged,
            ConversationItemAdded,
            UserInputTranscribed,
            FunctionToolsExecuted,
            Error,
            OverlappingSpeech,
            SessionUsageUpdated,
            AmdPrediction,
            EotPrediction,
            FunctionToolsStarted,
            DebugMessage,
            ToolExecutionUpdated,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "createdAt" | "created_at" => Ok(GeneratedField::CreatedAt),
                            "agentStateChanged" | "agent_state_changed" => Ok(GeneratedField::AgentStateChanged),
                            "userStateChanged" | "user_state_changed" => Ok(GeneratedField::UserStateChanged),
                            "conversationItemAdded" | "conversation_item_added" => Ok(GeneratedField::ConversationItemAdded),
                            "userInputTranscribed" | "user_input_transcribed" => Ok(GeneratedField::UserInputTranscribed),
                            "functionToolsExecuted" | "function_tools_executed" => Ok(GeneratedField::FunctionToolsExecuted),
                            "error" => Ok(GeneratedField::Error),
                            "overlappingSpeech" | "overlapping_speech" => Ok(GeneratedField::OverlappingSpeech),
                            "sessionUsageUpdated" | "session_usage_updated" => Ok(GeneratedField::SessionUsageUpdated),
                            "amdPrediction" | "amd_prediction" => Ok(GeneratedField::AmdPrediction),
                            "eotPrediction" | "eot_prediction" => Ok(GeneratedField::EotPrediction),
                            "functionToolsStarted" | "function_tools_started" => Ok(GeneratedField::FunctionToolsStarted),
                            "debugMessage" | "debug_message" => Ok(GeneratedField::DebugMessage),
                            "toolExecutionUpdated" | "tool_execution_updated" => Ok(GeneratedField::ToolExecutionUpdated),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AgentSessionEvent;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AgentSessionEvent, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut created_at__ = None;
                let mut event__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::CreatedAt => {
                            if created_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdAt"));
                            }
                            created_at__ = map_.next_value()?;
                        }
                        GeneratedField::AgentStateChanged => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("agentStateChanged"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::Event::AgentStateChanged)
;
                        }
                        GeneratedField::UserStateChanged => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("userStateChanged"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::Event::UserStateChanged)
;
                        }
                        GeneratedField::ConversationItemAdded => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("conversationItemAdded"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::Event::ConversationItemAdded)
;
                        }
                        GeneratedField::UserInputTranscribed => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("userInputTranscribed"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::Event::UserInputTranscribed)
;
                        }
                        GeneratedField::FunctionToolsExecuted => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("functionToolsExecuted"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::Event::FunctionToolsExecuted)
;
                        }
                        GeneratedField::Error => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("error"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::Event::Error)
;
                        }
                        GeneratedField::OverlappingSpeech => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("overlappingSpeech"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::Event::OverlappingSpeech)
;
                        }
                        GeneratedField::SessionUsageUpdated => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sessionUsageUpdated"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::Event::SessionUsageUpdated)
;
                        }
                        GeneratedField::AmdPrediction => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("amdPrediction"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::Event::AmdPrediction)
;
                        }
                        GeneratedField::EotPrediction => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("eotPrediction"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::Event::EotPrediction)
;
                        }
                        GeneratedField::FunctionToolsStarted => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("functionToolsStarted"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::Event::FunctionToolsStarted)
;
                        }
                        GeneratedField::DebugMessage => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("debugMessage"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::Event::DebugMessage)
;
                        }
                        GeneratedField::ToolExecutionUpdated => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("toolExecutionUpdated"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::Event::ToolExecutionUpdated)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AgentSessionEvent {
                    created_at: created_at__,
                    event: event__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_event::AgentStateChanged {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.old_state != 0 {
            len += 1;
        }
        if self.new_state != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent.AgentStateChanged", len)?;
        if self.old_state != 0 {
            let v = AgentState::try_from(self.old_state)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.old_state)))?;
            struct_ser.serialize_field("oldState", &v)?;
        }
        if self.new_state != 0 {
            let v = AgentState::try_from(self.new_state)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.new_state)))?;
            struct_ser.serialize_field("newState", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_event::AgentStateChanged {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "old_state",
            "oldState",
            "new_state",
            "newState",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            OldState,
            NewState,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "oldState" | "old_state" => Ok(GeneratedField::OldState),
                            "newState" | "new_state" => Ok(GeneratedField::NewState),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_event::AgentStateChanged;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent.AgentStateChanged")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_event::AgentStateChanged, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut old_state__ = None;
                let mut new_state__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::OldState => {
                            if old_state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("oldState"));
                            }
                            old_state__ = Some(map_.next_value::<AgentState>()? as i32);
                        }
                        GeneratedField::NewState => {
                            if new_state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("newState"));
                            }
                            new_state__ = Some(map_.next_value::<AgentState>()? as i32);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_event::AgentStateChanged {
                    old_state: old_state__.unwrap_or_default(),
                    new_state: new_state__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent.AgentStateChanged", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_event::AmdPrediction {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.speech_duration.is_some() {
            len += 1;
        }
        if self.category != 0 {
            len += 1;
        }
        if !self.reason.is_empty() {
            len += 1;
        }
        if !self.transcript.is_empty() {
            len += 1;
        }
        if self.delay.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent.AmdPrediction", len)?;
        if let Some(v) = self.speech_duration.as_ref() {
            struct_ser.serialize_field("speechDuration", v)?;
        }
        if self.category != 0 {
            let v = AmdCategory::try_from(self.category)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.category)))?;
            struct_ser.serialize_field("category", &v)?;
        }
        if !self.reason.is_empty() {
            struct_ser.serialize_field("reason", &self.reason)?;
        }
        if !self.transcript.is_empty() {
            struct_ser.serialize_field("transcript", &self.transcript)?;
        }
        if let Some(v) = self.delay.as_ref() {
            struct_ser.serialize_field("delay", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_event::AmdPrediction {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "speech_duration",
            "speechDuration",
            "category",
            "reason",
            "transcript",
            "delay",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            SpeechDuration,
            Category,
            Reason,
            Transcript,
            Delay,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "speechDuration" | "speech_duration" => Ok(GeneratedField::SpeechDuration),
                            "category" => Ok(GeneratedField::Category),
                            "reason" => Ok(GeneratedField::Reason),
                            "transcript" => Ok(GeneratedField::Transcript),
                            "delay" => Ok(GeneratedField::Delay),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_event::AmdPrediction;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent.AmdPrediction")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_event::AmdPrediction, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut speech_duration__ = None;
                let mut category__ = None;
                let mut reason__ = None;
                let mut transcript__ = None;
                let mut delay__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::SpeechDuration => {
                            if speech_duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("speechDuration"));
                            }
                            speech_duration__ = map_.next_value()?;
                        }
                        GeneratedField::Category => {
                            if category__.is_some() {
                                return Err(serde::de::Error::duplicate_field("category"));
                            }
                            category__ = Some(map_.next_value::<AmdCategory>()? as i32);
                        }
                        GeneratedField::Reason => {
                            if reason__.is_some() {
                                return Err(serde::de::Error::duplicate_field("reason"));
                            }
                            reason__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Transcript => {
                            if transcript__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transcript"));
                            }
                            transcript__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Delay => {
                            if delay__.is_some() {
                                return Err(serde::de::Error::duplicate_field("delay"));
                            }
                            delay__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_event::AmdPrediction {
                    speech_duration: speech_duration__,
                    category: category__.unwrap_or_default(),
                    reason: reason__.unwrap_or_default(),
                    transcript: transcript__.unwrap_or_default(),
                    delay: delay__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent.AmdPrediction", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_event::ConversationItemAdded {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.item.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent.ConversationItemAdded", len)?;
        if let Some(v) = self.item.as_ref() {
            struct_ser.serialize_field("item", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_event::ConversationItemAdded {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "item",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Item,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "item" => Ok(GeneratedField::Item),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_event::ConversationItemAdded;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent.ConversationItemAdded")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_event::ConversationItemAdded, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut item__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Item => {
                            if item__.is_some() {
                                return Err(serde::de::Error::duplicate_field("item"));
                            }
                            item__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_event::ConversationItemAdded {
                    item: item__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent.ConversationItemAdded", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_event::EotPrediction {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.probability != 0. {
            len += 1;
        }
        if self.threshold != 0. {
            len += 1;
        }
        if self.inference_duration.is_some() {
            len += 1;
        }
        if self.delay.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent.EotPrediction", len)?;
        if self.probability != 0. {
            struct_ser.serialize_field("probability", &self.probability)?;
        }
        if self.threshold != 0. {
            struct_ser.serialize_field("threshold", &self.threshold)?;
        }
        if let Some(v) = self.inference_duration.as_ref() {
            struct_ser.serialize_field("inferenceDuration", v)?;
        }
        if let Some(v) = self.delay.as_ref() {
            struct_ser.serialize_field("delay", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_event::EotPrediction {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "probability",
            "threshold",
            "inference_duration",
            "inferenceDuration",
            "delay",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Probability,
            Threshold,
            InferenceDuration,
            Delay,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "probability" => Ok(GeneratedField::Probability),
                            "threshold" => Ok(GeneratedField::Threshold),
                            "inferenceDuration" | "inference_duration" => Ok(GeneratedField::InferenceDuration),
                            "delay" => Ok(GeneratedField::Delay),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_event::EotPrediction;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent.EotPrediction")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_event::EotPrediction, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut probability__ = None;
                let mut threshold__ = None;
                let mut inference_duration__ = None;
                let mut delay__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Probability => {
                            if probability__.is_some() {
                                return Err(serde::de::Error::duplicate_field("probability"));
                            }
                            probability__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Threshold => {
                            if threshold__.is_some() {
                                return Err(serde::de::Error::duplicate_field("threshold"));
                            }
                            threshold__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::InferenceDuration => {
                            if inference_duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inferenceDuration"));
                            }
                            inference_duration__ = map_.next_value()?;
                        }
                        GeneratedField::Delay => {
                            if delay__.is_some() {
                                return Err(serde::de::Error::duplicate_field("delay"));
                            }
                            delay__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_event::EotPrediction {
                    probability: probability__.unwrap_or_default(),
                    threshold: threshold__.unwrap_or_default(),
                    inference_duration: inference_duration__,
                    delay: delay__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent.EotPrediction", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_event::Error {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.message.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent.Error", len)?;
        if !self.message.is_empty() {
            struct_ser.serialize_field("message", &self.message)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_event::Error {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "message",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Message,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "message" => Ok(GeneratedField::Message),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_event::Error;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent.Error")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_event::Error, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut message__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Message => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("message"));
                            }
                            message__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_event::Error {
                    message: message__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent.Error", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_event::FunctionToolsExecuted {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.function_calls.is_empty() {
            len += 1;
        }
        if !self.function_call_outputs.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent.FunctionToolsExecuted", len)?;
        if !self.function_calls.is_empty() {
            struct_ser.serialize_field("functionCalls", &self.function_calls)?;
        }
        if !self.function_call_outputs.is_empty() {
            struct_ser.serialize_field("functionCallOutputs", &self.function_call_outputs)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_event::FunctionToolsExecuted {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "function_calls",
            "functionCalls",
            "function_call_outputs",
            "functionCallOutputs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            FunctionCalls,
            FunctionCallOutputs,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "functionCalls" | "function_calls" => Ok(GeneratedField::FunctionCalls),
                            "functionCallOutputs" | "function_call_outputs" => Ok(GeneratedField::FunctionCallOutputs),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_event::FunctionToolsExecuted;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent.FunctionToolsExecuted")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_event::FunctionToolsExecuted, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut function_calls__ = None;
                let mut function_call_outputs__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::FunctionCalls => {
                            if function_calls__.is_some() {
                                return Err(serde::de::Error::duplicate_field("functionCalls"));
                            }
                            function_calls__ = Some(map_.next_value()?);
                        }
                        GeneratedField::FunctionCallOutputs => {
                            if function_call_outputs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("functionCallOutputs"));
                            }
                            function_call_outputs__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_event::FunctionToolsExecuted {
                    function_calls: function_calls__.unwrap_or_default(),
                    function_call_outputs: function_call_outputs__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent.FunctionToolsExecuted", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_event::FunctionToolsStarted {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.function_calls.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent.FunctionToolsStarted", len)?;
        if !self.function_calls.is_empty() {
            struct_ser.serialize_field("functionCalls", &self.function_calls)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_event::FunctionToolsStarted {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "function_calls",
            "functionCalls",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            FunctionCalls,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "functionCalls" | "function_calls" => Ok(GeneratedField::FunctionCalls),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_event::FunctionToolsStarted;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent.FunctionToolsStarted")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_event::FunctionToolsStarted, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut function_calls__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::FunctionCalls => {
                            if function_calls__.is_some() {
                                return Err(serde::de::Error::duplicate_field("functionCalls"));
                            }
                            function_calls__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_event::FunctionToolsStarted {
                    function_calls: function_calls__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent.FunctionToolsStarted", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_event::OverlappingSpeech {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.is_interruption {
            len += 1;
        }
        if self.overlap_started_at.is_some() {
            len += 1;
        }
        if self.detection_delay != 0. {
            len += 1;
        }
        if self.detected_at.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent.OverlappingSpeech", len)?;
        if self.is_interruption {
            struct_ser.serialize_field("isInterruption", &self.is_interruption)?;
        }
        if let Some(v) = self.overlap_started_at.as_ref() {
            struct_ser.serialize_field("overlapStartedAt", v)?;
        }
        if self.detection_delay != 0. {
            struct_ser.serialize_field("detectionDelay", &self.detection_delay)?;
        }
        if let Some(v) = self.detected_at.as_ref() {
            struct_ser.serialize_field("detectedAt", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_event::OverlappingSpeech {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "is_interruption",
            "isInterruption",
            "overlap_started_at",
            "overlapStartedAt",
            "detection_delay",
            "detectionDelay",
            "detected_at",
            "detectedAt",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IsInterruption,
            OverlapStartedAt,
            DetectionDelay,
            DetectedAt,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "isInterruption" | "is_interruption" => Ok(GeneratedField::IsInterruption),
                            "overlapStartedAt" | "overlap_started_at" => Ok(GeneratedField::OverlapStartedAt),
                            "detectionDelay" | "detection_delay" => Ok(GeneratedField::DetectionDelay),
                            "detectedAt" | "detected_at" => Ok(GeneratedField::DetectedAt),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_event::OverlappingSpeech;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent.OverlappingSpeech")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_event::OverlappingSpeech, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut is_interruption__ = None;
                let mut overlap_started_at__ = None;
                let mut detection_delay__ = None;
                let mut detected_at__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::IsInterruption => {
                            if is_interruption__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isInterruption"));
                            }
                            is_interruption__ = Some(map_.next_value()?);
                        }
                        GeneratedField::OverlapStartedAt => {
                            if overlap_started_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("overlapStartedAt"));
                            }
                            overlap_started_at__ = map_.next_value()?;
                        }
                        GeneratedField::DetectionDelay => {
                            if detection_delay__.is_some() {
                                return Err(serde::de::Error::duplicate_field("detectionDelay"));
                            }
                            detection_delay__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DetectedAt => {
                            if detected_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("detectedAt"));
                            }
                            detected_at__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_event::OverlappingSpeech {
                    is_interruption: is_interruption__.unwrap_or_default(),
                    overlap_started_at: overlap_started_at__,
                    detection_delay: detection_delay__.unwrap_or_default(),
                    detected_at: detected_at__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent.OverlappingSpeech", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_event::SessionUsageUpdated {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.usage.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent.SessionUsageUpdated", len)?;
        if let Some(v) = self.usage.as_ref() {
            struct_ser.serialize_field("usage", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_event::SessionUsageUpdated {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "usage",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Usage,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "usage" => Ok(GeneratedField::Usage),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_event::SessionUsageUpdated;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent.SessionUsageUpdated")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_event::SessionUsageUpdated, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut usage__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Usage => {
                            if usage__.is_some() {
                                return Err(serde::de::Error::duplicate_field("usage"));
                            }
                            usage__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_event::SessionUsageUpdated {
                    usage: usage__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent.SessionUsageUpdated", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_event::ToolExecutionUpdated {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.update.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent.ToolExecutionUpdated", len)?;
        if let Some(v) = self.update.as_ref() {
            match v {
                agent_session_event::tool_execution_updated::Update::Started(v) => {
                    struct_ser.serialize_field("started", v)?;
                }
                agent_session_event::tool_execution_updated::Update::CallUpdated(v) => {
                    struct_ser.serialize_field("callUpdated", v)?;
                }
                agent_session_event::tool_execution_updated::Update::ReplyUpdated(v) => {
                    struct_ser.serialize_field("replyUpdated", v)?;
                }
                agent_session_event::tool_execution_updated::Update::Ended(v) => {
                    struct_ser.serialize_field("ended", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_event::ToolExecutionUpdated {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "started",
            "call_updated",
            "callUpdated",
            "reply_updated",
            "replyUpdated",
            "ended",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Started,
            CallUpdated,
            ReplyUpdated,
            Ended,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "started" => Ok(GeneratedField::Started),
                            "callUpdated" | "call_updated" => Ok(GeneratedField::CallUpdated),
                            "replyUpdated" | "reply_updated" => Ok(GeneratedField::ReplyUpdated),
                            "ended" => Ok(GeneratedField::Ended),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_event::ToolExecutionUpdated;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent.ToolExecutionUpdated")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_event::ToolExecutionUpdated, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut update__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Started => {
                            if update__.is_some() {
                                return Err(serde::de::Error::duplicate_field("started"));
                            }
                            update__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::tool_execution_updated::Update::Started)
;
                        }
                        GeneratedField::CallUpdated => {
                            if update__.is_some() {
                                return Err(serde::de::Error::duplicate_field("callUpdated"));
                            }
                            update__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::tool_execution_updated::Update::CallUpdated)
;
                        }
                        GeneratedField::ReplyUpdated => {
                            if update__.is_some() {
                                return Err(serde::de::Error::duplicate_field("replyUpdated"));
                            }
                            update__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::tool_execution_updated::Update::ReplyUpdated)
;
                        }
                        GeneratedField::Ended => {
                            if update__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ended"));
                            }
                            update__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_event::tool_execution_updated::Update::Ended)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_event::ToolExecutionUpdated {
                    update: update__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent.ToolExecutionUpdated", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_event::tool_execution_updated::CallUpdated {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.id.is_empty() {
            len += 1;
        }
        if !self.call_id.is_empty() {
            len += 1;
        }
        if !self.message.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent.ToolExecutionUpdated.CallUpdated", len)?;
        if !self.id.is_empty() {
            struct_ser.serialize_field("id", &self.id)?;
        }
        if !self.call_id.is_empty() {
            struct_ser.serialize_field("callId", &self.call_id)?;
        }
        if !self.message.is_empty() {
            struct_ser.serialize_field("message", &self.message)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_event::tool_execution_updated::CallUpdated {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "id",
            "call_id",
            "callId",
            "message",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Id,
            CallId,
            Message,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "id" => Ok(GeneratedField::Id),
                            "callId" | "call_id" => Ok(GeneratedField::CallId),
                            "message" => Ok(GeneratedField::Message),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_event::tool_execution_updated::CallUpdated;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent.ToolExecutionUpdated.CallUpdated")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_event::tool_execution_updated::CallUpdated, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut id__ = None;
                let mut call_id__ = None;
                let mut message__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::CallId => {
                            if call_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("callId"));
                            }
                            call_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Message => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("message"));
                            }
                            message__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_event::tool_execution_updated::CallUpdated {
                    id: id__.unwrap_or_default(),
                    call_id: call_id__.unwrap_or_default(),
                    message: message__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent.ToolExecutionUpdated.CallUpdated", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_event::tool_execution_updated::Ended {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.id.is_empty() {
            len += 1;
        }
        if !self.call_id.is_empty() {
            len += 1;
        }
        if self.message.is_some() {
            len += 1;
        }
        if self.status != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent.ToolExecutionUpdated.Ended", len)?;
        if !self.id.is_empty() {
            struct_ser.serialize_field("id", &self.id)?;
        }
        if !self.call_id.is_empty() {
            struct_ser.serialize_field("callId", &self.call_id)?;
        }
        if let Some(v) = self.message.as_ref() {
            struct_ser.serialize_field("message", v)?;
        }
        if self.status != 0 {
            let v = ToolCallStatus::try_from(self.status)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.status)))?;
            struct_ser.serialize_field("status", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_event::tool_execution_updated::Ended {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "id",
            "call_id",
            "callId",
            "message",
            "status",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Id,
            CallId,
            Message,
            Status,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "id" => Ok(GeneratedField::Id),
                            "callId" | "call_id" => Ok(GeneratedField::CallId),
                            "message" => Ok(GeneratedField::Message),
                            "status" => Ok(GeneratedField::Status),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_event::tool_execution_updated::Ended;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent.ToolExecutionUpdated.Ended")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_event::tool_execution_updated::Ended, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut id__ = None;
                let mut call_id__ = None;
                let mut message__ = None;
                let mut status__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::CallId => {
                            if call_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("callId"));
                            }
                            call_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Message => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("message"));
                            }
                            message__ = map_.next_value()?;
                        }
                        GeneratedField::Status => {
                            if status__.is_some() {
                                return Err(serde::de::Error::duplicate_field("status"));
                            }
                            status__ = Some(map_.next_value::<ToolCallStatus>()? as i32);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_event::tool_execution_updated::Ended {
                    id: id__.unwrap_or_default(),
                    call_id: call_id__.unwrap_or_default(),
                    message: message__,
                    status: status__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent.ToolExecutionUpdated.Ended", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_event::tool_execution_updated::ReplyUpdated {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.update_ids.is_empty() {
            len += 1;
        }
        if self.status != 0 {
            len += 1;
        }
        if !self.speech_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent.ToolExecutionUpdated.ReplyUpdated", len)?;
        if !self.update_ids.is_empty() {
            struct_ser.serialize_field("updateIds", &self.update_ids)?;
        }
        if self.status != 0 {
            let v = ToolReplyStatus::try_from(self.status)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.status)))?;
            struct_ser.serialize_field("status", &v)?;
        }
        if !self.speech_id.is_empty() {
            struct_ser.serialize_field("speechId", &self.speech_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_event::tool_execution_updated::ReplyUpdated {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "update_ids",
            "updateIds",
            "status",
            "speech_id",
            "speechId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            UpdateIds,
            Status,
            SpeechId,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "updateIds" | "update_ids" => Ok(GeneratedField::UpdateIds),
                            "status" => Ok(GeneratedField::Status),
                            "speechId" | "speech_id" => Ok(GeneratedField::SpeechId),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_event::tool_execution_updated::ReplyUpdated;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent.ToolExecutionUpdated.ReplyUpdated")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_event::tool_execution_updated::ReplyUpdated, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut update_ids__ = None;
                let mut status__ = None;
                let mut speech_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::UpdateIds => {
                            if update_ids__.is_some() {
                                return Err(serde::de::Error::duplicate_field("updateIds"));
                            }
                            update_ids__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Status => {
                            if status__.is_some() {
                                return Err(serde::de::Error::duplicate_field("status"));
                            }
                            status__ = Some(map_.next_value::<ToolReplyStatus>()? as i32);
                        }
                        GeneratedField::SpeechId => {
                            if speech_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("speechId"));
                            }
                            speech_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_event::tool_execution_updated::ReplyUpdated {
                    update_ids: update_ids__.unwrap_or_default(),
                    status: status__.unwrap_or_default(),
                    speech_id: speech_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent.ToolExecutionUpdated.ReplyUpdated", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_event::tool_execution_updated::Started {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.function_call.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent.ToolExecutionUpdated.Started", len)?;
        if let Some(v) = self.function_call.as_ref() {
            struct_ser.serialize_field("functionCall", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_event::tool_execution_updated::Started {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "function_call",
            "functionCall",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            FunctionCall,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "functionCall" | "function_call" => Ok(GeneratedField::FunctionCall),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_event::tool_execution_updated::Started;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent.ToolExecutionUpdated.Started")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_event::tool_execution_updated::Started, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut function_call__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::FunctionCall => {
                            if function_call__.is_some() {
                                return Err(serde::de::Error::duplicate_field("functionCall"));
                            }
                            function_call__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_event::tool_execution_updated::Started {
                    function_call: function_call__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent.ToolExecutionUpdated.Started", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_event::UserInputTranscribed {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.transcript.is_empty() {
            len += 1;
        }
        if self.is_final {
            len += 1;
        }
        if self.language.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent.UserInputTranscribed", len)?;
        if !self.transcript.is_empty() {
            struct_ser.serialize_field("transcript", &self.transcript)?;
        }
        if self.is_final {
            struct_ser.serialize_field("isFinal", &self.is_final)?;
        }
        if let Some(v) = self.language.as_ref() {
            struct_ser.serialize_field("language", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_event::UserInputTranscribed {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "transcript",
            "is_final",
            "isFinal",
            "language",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Transcript,
            IsFinal,
            Language,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "transcript" => Ok(GeneratedField::Transcript),
                            "isFinal" | "is_final" => Ok(GeneratedField::IsFinal),
                            "language" => Ok(GeneratedField::Language),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_event::UserInputTranscribed;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent.UserInputTranscribed")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_event::UserInputTranscribed, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut transcript__ = None;
                let mut is_final__ = None;
                let mut language__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Transcript => {
                            if transcript__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transcript"));
                            }
                            transcript__ = Some(map_.next_value()?);
                        }
                        GeneratedField::IsFinal => {
                            if is_final__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isFinal"));
                            }
                            is_final__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Language => {
                            if language__.is_some() {
                                return Err(serde::de::Error::duplicate_field("language"));
                            }
                            language__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_event::UserInputTranscribed {
                    transcript: transcript__.unwrap_or_default(),
                    is_final: is_final__.unwrap_or_default(),
                    language: language__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent.UserInputTranscribed", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_event::UserStateChanged {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.old_state != 0 {
            len += 1;
        }
        if self.new_state != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionEvent.UserStateChanged", len)?;
        if self.old_state != 0 {
            let v = UserState::try_from(self.old_state)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.old_state)))?;
            struct_ser.serialize_field("oldState", &v)?;
        }
        if self.new_state != 0 {
            let v = UserState::try_from(self.new_state)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.new_state)))?;
            struct_ser.serialize_field("newState", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_event::UserStateChanged {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "old_state",
            "oldState",
            "new_state",
            "newState",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            OldState,
            NewState,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "oldState" | "old_state" => Ok(GeneratedField::OldState),
                            "newState" | "new_state" => Ok(GeneratedField::NewState),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_event::UserStateChanged;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionEvent.UserStateChanged")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_event::UserStateChanged, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut old_state__ = None;
                let mut new_state__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::OldState => {
                            if old_state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("oldState"));
                            }
                            old_state__ = Some(map_.next_value::<UserState>()? as i32);
                        }
                        GeneratedField::NewState => {
                            if new_state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("newState"));
                            }
                            new_state__ = Some(map_.next_value::<UserState>()? as i32);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_event::UserStateChanged {
                    old_state: old_state__.unwrap_or_default(),
                    new_state: new_state__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionEvent.UserStateChanged", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AgentSessionMessage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.message.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionMessage", len)?;
        if let Some(v) = self.message.as_ref() {
            match v {
                agent_session_message::Message::AudioInput(v) => {
                    struct_ser.serialize_field("audioInput", v)?;
                }
                agent_session_message::Message::AudioOutput(v) => {
                    struct_ser.serialize_field("audioOutput", v)?;
                }
                agent_session_message::Message::Event(v) => {
                    struct_ser.serialize_field("event", v)?;
                }
                agent_session_message::Message::Request(v) => {
                    struct_ser.serialize_field("request", v)?;
                }
                agent_session_message::Message::Response(v) => {
                    struct_ser.serialize_field("response", v)?;
                }
                agent_session_message::Message::AudioPlaybackFlush(v) => {
                    struct_ser.serialize_field("audioPlaybackFlush", v)?;
                }
                agent_session_message::Message::AudioPlaybackClear(v) => {
                    struct_ser.serialize_field("audioPlaybackClear", v)?;
                }
                agent_session_message::Message::AudioPlaybackFinished(v) => {
                    struct_ser.serialize_field("audioPlaybackFinished", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AgentSessionMessage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "audio_input",
            "audioInput",
            "audio_output",
            "audioOutput",
            "event",
            "request",
            "response",
            "audio_playback_flush",
            "audioPlaybackFlush",
            "audio_playback_clear",
            "audioPlaybackClear",
            "audio_playback_finished",
            "audioPlaybackFinished",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AudioInput,
            AudioOutput,
            Event,
            Request,
            Response,
            AudioPlaybackFlush,
            AudioPlaybackClear,
            AudioPlaybackFinished,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "audioInput" | "audio_input" => Ok(GeneratedField::AudioInput),
                            "audioOutput" | "audio_output" => Ok(GeneratedField::AudioOutput),
                            "event" => Ok(GeneratedField::Event),
                            "request" => Ok(GeneratedField::Request),
                            "response" => Ok(GeneratedField::Response),
                            "audioPlaybackFlush" | "audio_playback_flush" => Ok(GeneratedField::AudioPlaybackFlush),
                            "audioPlaybackClear" | "audio_playback_clear" => Ok(GeneratedField::AudioPlaybackClear),
                            "audioPlaybackFinished" | "audio_playback_finished" => Ok(GeneratedField::AudioPlaybackFinished),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AgentSessionMessage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionMessage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AgentSessionMessage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut message__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AudioInput => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioInput"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_message::Message::AudioInput)
;
                        }
                        GeneratedField::AudioOutput => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioOutput"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_message::Message::AudioOutput)
;
                        }
                        GeneratedField::Event => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("event"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_message::Message::Event)
;
                        }
                        GeneratedField::Request => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("request"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_message::Message::Request)
;
                        }
                        GeneratedField::Response => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("response"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_message::Message::Response)
;
                        }
                        GeneratedField::AudioPlaybackFlush => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioPlaybackFlush"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_message::Message::AudioPlaybackFlush)
;
                        }
                        GeneratedField::AudioPlaybackClear => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioPlaybackClear"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_message::Message::AudioPlaybackClear)
;
                        }
                        GeneratedField::AudioPlaybackFinished => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioPlaybackFinished"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(agent_session_message::Message::AudioPlaybackFinished)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AgentSessionMessage {
                    message: message__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionMessage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_message::ConsoleIo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionMessage.ConsoleIO", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_message::ConsoleIo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_message::ConsoleIo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionMessage.ConsoleIO")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_message::ConsoleIo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(agent_session_message::ConsoleIo {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionMessage.ConsoleIO", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_message::console_io::AudioFrame {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.data.is_empty() {
            len += 1;
        }
        if self.sample_rate != 0 {
            len += 1;
        }
        if self.num_channels != 0 {
            len += 1;
        }
        if self.samples_per_channel != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionMessage.ConsoleIO.AudioFrame", len)?;
        if !self.data.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("data", pbjson::private::base64::encode(&self.data).as_str())?;
        }
        if self.sample_rate != 0 {
            struct_ser.serialize_field("sampleRate", &self.sample_rate)?;
        }
        if self.num_channels != 0 {
            struct_ser.serialize_field("numChannels", &self.num_channels)?;
        }
        if self.samples_per_channel != 0 {
            struct_ser.serialize_field("samplesPerChannel", &self.samples_per_channel)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_message::console_io::AudioFrame {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "data",
            "sample_rate",
            "sampleRate",
            "num_channels",
            "numChannels",
            "samples_per_channel",
            "samplesPerChannel",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Data,
            SampleRate,
            NumChannels,
            SamplesPerChannel,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "data" => Ok(GeneratedField::Data),
                            "sampleRate" | "sample_rate" => Ok(GeneratedField::SampleRate),
                            "numChannels" | "num_channels" => Ok(GeneratedField::NumChannels),
                            "samplesPerChannel" | "samples_per_channel" => Ok(GeneratedField::SamplesPerChannel),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_message::console_io::AudioFrame;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionMessage.ConsoleIO.AudioFrame")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_message::console_io::AudioFrame, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut data__ = None;
                let mut sample_rate__ = None;
                let mut num_channels__ = None;
                let mut samples_per_channel__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Data => {
                            if data__.is_some() {
                                return Err(serde::de::Error::duplicate_field("data"));
                            }
                            data__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::SampleRate => {
                            if sample_rate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sampleRate"));
                            }
                            sample_rate__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::NumChannels => {
                            if num_channels__.is_some() {
                                return Err(serde::de::Error::duplicate_field("numChannels"));
                            }
                            num_channels__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::SamplesPerChannel => {
                            if samples_per_channel__.is_some() {
                                return Err(serde::de::Error::duplicate_field("samplesPerChannel"));
                            }
                            samples_per_channel__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(agent_session_message::console_io::AudioFrame {
                    data: data__.unwrap_or_default(),
                    sample_rate: sample_rate__.unwrap_or_default(),
                    num_channels: num_channels__.unwrap_or_default(),
                    samples_per_channel: samples_per_channel__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionMessage.ConsoleIO.AudioFrame", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_message::console_io::AudioPlaybackClear {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionMessage.ConsoleIO.AudioPlaybackClear", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_message::console_io::AudioPlaybackClear {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_message::console_io::AudioPlaybackClear;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionMessage.ConsoleIO.AudioPlaybackClear")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_message::console_io::AudioPlaybackClear, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(agent_session_message::console_io::AudioPlaybackClear {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionMessage.ConsoleIO.AudioPlaybackClear", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_message::console_io::AudioPlaybackFinished {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionMessage.ConsoleIO.AudioPlaybackFinished", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_message::console_io::AudioPlaybackFinished {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_message::console_io::AudioPlaybackFinished;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionMessage.ConsoleIO.AudioPlaybackFinished")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_message::console_io::AudioPlaybackFinished, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(agent_session_message::console_io::AudioPlaybackFinished {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionMessage.ConsoleIO.AudioPlaybackFinished", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for agent_session_message::console_io::AudioPlaybackFlush {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionMessage.ConsoleIO.AudioPlaybackFlush", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for agent_session_message::console_io::AudioPlaybackFlush {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = agent_session_message::console_io::AudioPlaybackFlush;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionMessage.ConsoleIO.AudioPlaybackFlush")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<agent_session_message::console_io::AudioPlaybackFlush, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(agent_session_message::console_io::AudioPlaybackFlush {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionMessage.ConsoleIO.AudioPlaybackFlush", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AgentSessionState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.version != 0 {
            len += 1;
        }
        if self.data.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionState", len)?;
        if self.version != 0 {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("version", ToString::to_string(&self.version).as_str())?;
        }
        if let Some(v) = self.data.as_ref() {
            match v {
                agent_session_state::Data::Snapshot(v) => {
                    #[allow(clippy::needless_borrow)]
                    #[allow(clippy::needless_borrows_for_generic_args)]
                    struct_ser.serialize_field("snapshot", pbjson::private::base64::encode(&v).as_str())?;
                }
                agent_session_state::Data::Delta(v) => {
                    #[allow(clippy::needless_borrow)]
                    #[allow(clippy::needless_borrows_for_generic_args)]
                    struct_ser.serialize_field("delta", pbjson::private::base64::encode(&v).as_str())?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AgentSessionState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "version",
            "snapshot",
            "delta",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Version,
            Snapshot,
            Delta,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "version" => Ok(GeneratedField::Version),
                            "snapshot" => Ok(GeneratedField::Snapshot),
                            "delta" => Ok(GeneratedField::Delta),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AgentSessionState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionState")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AgentSessionState, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut version__ = None;
                let mut data__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Version => {
                            if version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("version"));
                            }
                            version__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Snapshot => {
                            if data__.is_some() {
                                return Err(serde::de::Error::duplicate_field("snapshot"));
                            }
                            data__ = map_.next_value::<::std::option::Option<::pbjson::private::BytesDeserialize<_>>>()?.map(|x| agent_session_state::Data::Snapshot(x.0));
                        }
                        GeneratedField::Delta => {
                            if data__.is_some() {
                                return Err(serde::de::Error::duplicate_field("delta"));
                            }
                            data__ = map_.next_value::<::std::option::Option<::pbjson::private::BytesDeserialize<_>>>()?.map(|x| agent_session_state::Data::Delta(x.0));
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AgentSessionState {
                    version: version__.unwrap_or_default(),
                    data: data__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionState", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AgentSessionUsage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.model_usage.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.AgentSessionUsage", len)?;
        if !self.model_usage.is_empty() {
            struct_ser.serialize_field("modelUsage", &self.model_usage)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for AgentSessionUsage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "model_usage",
            "modelUsage",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ModelUsage,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "modelUsage" | "model_usage" => Ok(GeneratedField::ModelUsage),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AgentSessionUsage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.AgentSessionUsage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<AgentSessionUsage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut model_usage__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ModelUsage => {
                            if model_usage__.is_some() {
                                return Err(serde::de::Error::duplicate_field("modelUsage"));
                            }
                            model_usage__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(AgentSessionUsage {
                    model_usage: model_usage__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.AgentSessionUsage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for AgentState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::AsInitializing => "AS_INITIALIZING",
            Self::AsIdle => "AS_IDLE",
            Self::AsListening => "AS_LISTENING",
            Self::AsThinking => "AS_THINKING",
            Self::AsSpeaking => "AS_SPEAKING",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for AgentState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "AS_INITIALIZING",
            "AS_IDLE",
            "AS_LISTENING",
            "AS_THINKING",
            "AS_SPEAKING",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AgentState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "AS_INITIALIZING" => Ok(AgentState::AsInitializing),
                    "AS_IDLE" => Ok(AgentState::AsIdle),
                    "AS_LISTENING" => Ok(AgentState::AsListening),
                    "AS_THINKING" => Ok(AgentState::AsThinking),
                    "AS_SPEAKING" => Ok(AgentState::AsSpeaking),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for AmdCategory {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::AmdUnknown => "AMD_UNKNOWN",
            Self::AmdHuman => "AMD_HUMAN",
            Self::AmdMachineIvr => "AMD_MACHINE_IVR",
            Self::AmdMachineVm => "AMD_MACHINE_VM",
            Self::AmdMachineUnavailable => "AMD_MACHINE_UNAVAILABLE",
            Self::AmdUncertain => "AMD_UNCERTAIN",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for AmdCategory {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "AMD_UNKNOWN",
            "AMD_HUMAN",
            "AMD_MACHINE_IVR",
            "AMD_MACHINE_VM",
            "AMD_MACHINE_UNAVAILABLE",
            "AMD_UNCERTAIN",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AmdCategory;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "AMD_UNKNOWN" => Ok(AmdCategory::AmdUnknown),
                    "AMD_HUMAN" => Ok(AmdCategory::AmdHuman),
                    "AMD_MACHINE_IVR" => Ok(AmdCategory::AmdMachineIvr),
                    "AMD_MACHINE_VM" => Ok(AmdCategory::AmdMachineVm),
                    "AMD_MACHINE_UNAVAILABLE" => Ok(AmdCategory::AmdMachineUnavailable),
                    "AMD_UNCERTAIN" => Ok(AmdCategory::AmdUncertain),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for AudioEncoding {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::PcmS16le => "AUDIO_ENCODING_PCM_S16LE",
            Self::Opus => "AUDIO_ENCODING_OPUS",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for AudioEncoding {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "AUDIO_ENCODING_PCM_S16LE",
            "AUDIO_ENCODING_OPUS",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = AudioEncoding;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "AUDIO_ENCODING_PCM_S16LE" => Ok(AudioEncoding::PcmS16le),
                    "AUDIO_ENCODING_OPUS" => Ok(AudioEncoding::Opus),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for BufferStart {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.BufferStart", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for BufferStart {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = BufferStart;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.BufferStart")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<BufferStart, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(BufferStart {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.BufferStart", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for BufferStop {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.BufferStop", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for BufferStop {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = BufferStop;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.BufferStop")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<BufferStop, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(BufferStop {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.BufferStop", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ChatContext {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.items.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.ChatContext", len)?;
        if !self.items.is_empty() {
            struct_ser.serialize_field("items", &self.items)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ChatContext {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "items",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Items,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "items" => Ok(GeneratedField::Items),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ChatContext;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.ChatContext")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ChatContext, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut items__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Items => {
                            if items__.is_some() {
                                return Err(serde::de::Error::duplicate_field("items"));
                            }
                            items__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ChatContext {
                    items: items__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.ChatContext", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for chat_context::ChatItem {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.item.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.ChatContext.ChatItem", len)?;
        if let Some(v) = self.item.as_ref() {
            match v {
                chat_context::chat_item::Item::Message(v) => {
                    struct_ser.serialize_field("message", v)?;
                }
                chat_context::chat_item::Item::FunctionCall(v) => {
                    struct_ser.serialize_field("functionCall", v)?;
                }
                chat_context::chat_item::Item::FunctionCallOutput(v) => {
                    struct_ser.serialize_field("functionCallOutput", v)?;
                }
                chat_context::chat_item::Item::AgentHandoff(v) => {
                    struct_ser.serialize_field("agentHandoff", v)?;
                }
                chat_context::chat_item::Item::AgentConfigUpdate(v) => {
                    struct_ser.serialize_field("agentConfigUpdate", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for chat_context::ChatItem {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "message",
            "function_call",
            "functionCall",
            "function_call_output",
            "functionCallOutput",
            "agent_handoff",
            "agentHandoff",
            "agent_config_update",
            "agentConfigUpdate",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Message,
            FunctionCall,
            FunctionCallOutput,
            AgentHandoff,
            AgentConfigUpdate,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "message" => Ok(GeneratedField::Message),
                            "functionCall" | "function_call" => Ok(GeneratedField::FunctionCall),
                            "functionCallOutput" | "function_call_output" => Ok(GeneratedField::FunctionCallOutput),
                            "agentHandoff" | "agent_handoff" => Ok(GeneratedField::AgentHandoff),
                            "agentConfigUpdate" | "agent_config_update" => Ok(GeneratedField::AgentConfigUpdate),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = chat_context::ChatItem;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.ChatContext.ChatItem")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<chat_context::ChatItem, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut item__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Message => {
                            if item__.is_some() {
                                return Err(serde::de::Error::duplicate_field("message"));
                            }
                            item__ = map_.next_value::<::std::option::Option<_>>()?.map(chat_context::chat_item::Item::Message)
;
                        }
                        GeneratedField::FunctionCall => {
                            if item__.is_some() {
                                return Err(serde::de::Error::duplicate_field("functionCall"));
                            }
                            item__ = map_.next_value::<::std::option::Option<_>>()?.map(chat_context::chat_item::Item::FunctionCall)
;
                        }
                        GeneratedField::FunctionCallOutput => {
                            if item__.is_some() {
                                return Err(serde::de::Error::duplicate_field("functionCallOutput"));
                            }
                            item__ = map_.next_value::<::std::option::Option<_>>()?.map(chat_context::chat_item::Item::FunctionCallOutput)
;
                        }
                        GeneratedField::AgentHandoff => {
                            if item__.is_some() {
                                return Err(serde::de::Error::duplicate_field("agentHandoff"));
                            }
                            item__ = map_.next_value::<::std::option::Option<_>>()?.map(chat_context::chat_item::Item::AgentHandoff)
;
                        }
                        GeneratedField::AgentConfigUpdate => {
                            if item__.is_some() {
                                return Err(serde::de::Error::duplicate_field("agentConfigUpdate"));
                            }
                            item__ = map_.next_value::<::std::option::Option<_>>()?.map(chat_context::chat_item::Item::AgentConfigUpdate)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(chat_context::ChatItem {
                    item: item__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.ChatContext.ChatItem", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ChatMessage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.id.is_empty() {
            len += 1;
        }
        if self.role != 0 {
            len += 1;
        }
        if !self.content.is_empty() {
            len += 1;
        }
        if self.interrupted {
            len += 1;
        }
        if self.transcript_confidence.is_some() {
            len += 1;
        }
        if !self.extra.is_empty() {
            len += 1;
        }
        if self.metrics.is_some() {
            len += 1;
        }
        if self.created_at.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.ChatMessage", len)?;
        if !self.id.is_empty() {
            struct_ser.serialize_field("id", &self.id)?;
        }
        if self.role != 0 {
            let v = ChatRole::try_from(self.role)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.role)))?;
            struct_ser.serialize_field("role", &v)?;
        }
        if !self.content.is_empty() {
            struct_ser.serialize_field("content", &self.content)?;
        }
        if self.interrupted {
            struct_ser.serialize_field("interrupted", &self.interrupted)?;
        }
        if let Some(v) = self.transcript_confidence.as_ref() {
            struct_ser.serialize_field("transcriptConfidence", v)?;
        }
        if !self.extra.is_empty() {
            struct_ser.serialize_field("extra", &self.extra)?;
        }
        if let Some(v) = self.metrics.as_ref() {
            struct_ser.serialize_field("metrics", v)?;
        }
        if let Some(v) = self.created_at.as_ref() {
            struct_ser.serialize_field("createdAt", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ChatMessage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "id",
            "role",
            "content",
            "interrupted",
            "transcript_confidence",
            "transcriptConfidence",
            "extra",
            "metrics",
            "created_at",
            "createdAt",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Id,
            Role,
            Content,
            Interrupted,
            TranscriptConfidence,
            Extra,
            Metrics,
            CreatedAt,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "id" => Ok(GeneratedField::Id),
                            "role" => Ok(GeneratedField::Role),
                            "content" => Ok(GeneratedField::Content),
                            "interrupted" => Ok(GeneratedField::Interrupted),
                            "transcriptConfidence" | "transcript_confidence" => Ok(GeneratedField::TranscriptConfidence),
                            "extra" => Ok(GeneratedField::Extra),
                            "metrics" => Ok(GeneratedField::Metrics),
                            "createdAt" | "created_at" => Ok(GeneratedField::CreatedAt),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ChatMessage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.ChatMessage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ChatMessage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut id__ = None;
                let mut role__ = None;
                let mut content__ = None;
                let mut interrupted__ = None;
                let mut transcript_confidence__ = None;
                let mut extra__ = None;
                let mut metrics__ = None;
                let mut created_at__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Role => {
                            if role__.is_some() {
                                return Err(serde::de::Error::duplicate_field("role"));
                            }
                            role__ = Some(map_.next_value::<ChatRole>()? as i32);
                        }
                        GeneratedField::Content => {
                            if content__.is_some() {
                                return Err(serde::de::Error::duplicate_field("content"));
                            }
                            content__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Interrupted => {
                            if interrupted__.is_some() {
                                return Err(serde::de::Error::duplicate_field("interrupted"));
                            }
                            interrupted__ = Some(map_.next_value()?);
                        }
                        GeneratedField::TranscriptConfidence => {
                            if transcript_confidence__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transcriptConfidence"));
                            }
                            transcript_confidence__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::Extra => {
                            if extra__.is_some() {
                                return Err(serde::de::Error::duplicate_field("extra"));
                            }
                            extra__ = Some(
                                map_.next_value::<std::collections::HashMap<_, _>>()?
                            );
                        }
                        GeneratedField::Metrics => {
                            if metrics__.is_some() {
                                return Err(serde::de::Error::duplicate_field("metrics"));
                            }
                            metrics__ = map_.next_value()?;
                        }
                        GeneratedField::CreatedAt => {
                            if created_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdAt"));
                            }
                            created_at__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ChatMessage {
                    id: id__.unwrap_or_default(),
                    role: role__.unwrap_or_default(),
                    content: content__.unwrap_or_default(),
                    interrupted: interrupted__.unwrap_or_default(),
                    transcript_confidence: transcript_confidence__,
                    extra: extra__.unwrap_or_default(),
                    metrics: metrics__,
                    created_at: created_at__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.ChatMessage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for chat_message::ChatContent {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.payload.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.ChatMessage.ChatContent", len)?;
        if let Some(v) = self.payload.as_ref() {
            match v {
                chat_message::chat_content::Payload::Text(v) => {
                    struct_ser.serialize_field("text", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for chat_message::ChatContent {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "text",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Text,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "text" => Ok(GeneratedField::Text),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = chat_message::ChatContent;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.ChatMessage.ChatContent")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<chat_message::ChatContent, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut payload__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Text => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("text"));
                            }
                            payload__ = map_.next_value::<::std::option::Option<_>>()?.map(chat_message::chat_content::Payload::Text);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(chat_message::ChatContent {
                    payload: payload__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.ChatMessage.ChatContent", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ChatRole {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Developer => "DEVELOPER",
            Self::System => "SYSTEM",
            Self::User => "USER",
            Self::Assistant => "ASSISTANT",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ChatRole {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "DEVELOPER",
            "SYSTEM",
            "USER",
            "ASSISTANT",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ChatRole;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "DEVELOPER" => Ok(ChatRole::Developer),
                    "SYSTEM" => Ok(ChatRole::System),
                    "USER" => Ok(ChatRole::User),
                    "ASSISTANT" => Ok(ChatRole::Assistant),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ClientMessage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.created_at.is_some() {
            len += 1;
        }
        if self.message.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.ClientMessage", len)?;
        if let Some(v) = self.created_at.as_ref() {
            struct_ser.serialize_field("createdAt", v)?;
        }
        if let Some(v) = self.message.as_ref() {
            match v {
                client_message::Message::SessionCreate(v) => {
                    struct_ser.serialize_field("sessionCreate", v)?;
                }
                client_message::Message::InputAudio(v) => {
                    struct_ser.serialize_field("inputAudio", v)?;
                }
                client_message::Message::SessionFlush(v) => {
                    struct_ser.serialize_field("sessionFlush", v)?;
                }
                client_message::Message::SessionClose(v) => {
                    struct_ser.serialize_field("sessionClose", v)?;
                }
                client_message::Message::InferenceStart(v) => {
                    struct_ser.serialize_field("inferenceStart", v)?;
                }
                client_message::Message::InferenceStop(v) => {
                    struct_ser.serialize_field("inferenceStop", v)?;
                }
                client_message::Message::BufferStart(v) => {
                    struct_ser.serialize_field("bufferStart", v)?;
                }
                client_message::Message::BufferStop(v) => {
                    struct_ser.serialize_field("bufferStop", v)?;
                }
                client_message::Message::EotInputChatContext(v) => {
                    struct_ser.serialize_field("eotInputChatContext", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ClientMessage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "created_at",
            "createdAt",
            "session_create",
            "sessionCreate",
            "input_audio",
            "inputAudio",
            "session_flush",
            "sessionFlush",
            "session_close",
            "sessionClose",
            "inference_start",
            "inferenceStart",
            "inference_stop",
            "inferenceStop",
            "buffer_start",
            "bufferStart",
            "buffer_stop",
            "bufferStop",
            "eot_input_chat_context",
            "eotInputChatContext",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            CreatedAt,
            SessionCreate,
            InputAudio,
            SessionFlush,
            SessionClose,
            InferenceStart,
            InferenceStop,
            BufferStart,
            BufferStop,
            EotInputChatContext,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "createdAt" | "created_at" => Ok(GeneratedField::CreatedAt),
                            "sessionCreate" | "session_create" => Ok(GeneratedField::SessionCreate),
                            "inputAudio" | "input_audio" => Ok(GeneratedField::InputAudio),
                            "sessionFlush" | "session_flush" => Ok(GeneratedField::SessionFlush),
                            "sessionClose" | "session_close" => Ok(GeneratedField::SessionClose),
                            "inferenceStart" | "inference_start" => Ok(GeneratedField::InferenceStart),
                            "inferenceStop" | "inference_stop" => Ok(GeneratedField::InferenceStop),
                            "bufferStart" | "buffer_start" => Ok(GeneratedField::BufferStart),
                            "bufferStop" | "buffer_stop" => Ok(GeneratedField::BufferStop),
                            "eotInputChatContext" | "eot_input_chat_context" => Ok(GeneratedField::EotInputChatContext),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ClientMessage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.ClientMessage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ClientMessage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut created_at__ = None;
                let mut message__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::CreatedAt => {
                            if created_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdAt"));
                            }
                            created_at__ = map_.next_value()?;
                        }
                        GeneratedField::SessionCreate => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sessionCreate"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(client_message::Message::SessionCreate)
;
                        }
                        GeneratedField::InputAudio => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inputAudio"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(client_message::Message::InputAudio)
;
                        }
                        GeneratedField::SessionFlush => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sessionFlush"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(client_message::Message::SessionFlush)
;
                        }
                        GeneratedField::SessionClose => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sessionClose"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(client_message::Message::SessionClose)
;
                        }
                        GeneratedField::InferenceStart => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inferenceStart"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(client_message::Message::InferenceStart)
;
                        }
                        GeneratedField::InferenceStop => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inferenceStop"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(client_message::Message::InferenceStop)
;
                        }
                        GeneratedField::BufferStart => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bufferStart"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(client_message::Message::BufferStart)
;
                        }
                        GeneratedField::BufferStop => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("bufferStop"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(client_message::Message::BufferStop)
;
                        }
                        GeneratedField::EotInputChatContext => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("eotInputChatContext"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(client_message::Message::EotInputChatContext)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ClientMessage {
                    created_at: created_at__,
                    message: message__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.ClientMessage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for DebugMessage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.payload.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.DebugMessage", len)?;
        if let Some(v) = self.payload.as_ref() {
            struct_ser.serialize_field("payload", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for DebugMessage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "payload",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Payload,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "payload" => Ok(GeneratedField::Payload),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = DebugMessage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.DebugMessage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<DebugMessage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut payload__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Payload => {
                            if payload__.is_some() {
                                return Err(serde::de::Error::duplicate_field("payload"));
                            }
                            payload__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(DebugMessage {
                    payload: payload__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.DebugMessage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EotInferenceRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.audio.is_empty() {
            len += 1;
        }
        if !self.assistant_text.is_empty() {
            len += 1;
        }
        if self.encoding != 0 {
            len += 1;
        }
        if self.sample_rate != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.EotInferenceRequest", len)?;
        if !self.audio.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("audio", pbjson::private::base64::encode(&self.audio).as_str())?;
        }
        if !self.assistant_text.is_empty() {
            struct_ser.serialize_field("assistantText", &self.assistant_text)?;
        }
        if self.encoding != 0 {
            let v = AudioEncoding::try_from(self.encoding)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.encoding)))?;
            struct_ser.serialize_field("encoding", &v)?;
        }
        if self.sample_rate != 0 {
            struct_ser.serialize_field("sampleRate", &self.sample_rate)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EotInferenceRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "audio",
            "assistant_text",
            "assistantText",
            "encoding",
            "sample_rate",
            "sampleRate",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Audio,
            AssistantText,
            Encoding,
            SampleRate,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "audio" => Ok(GeneratedField::Audio),
                            "assistantText" | "assistant_text" => Ok(GeneratedField::AssistantText),
                            "encoding" => Ok(GeneratedField::Encoding),
                            "sampleRate" | "sample_rate" => Ok(GeneratedField::SampleRate),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EotInferenceRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.EotInferenceRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EotInferenceRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut audio__ = None;
                let mut assistant_text__ = None;
                let mut encoding__ = None;
                let mut sample_rate__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Audio => {
                            if audio__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audio"));
                            }
                            audio__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AssistantText => {
                            if assistant_text__.is_some() {
                                return Err(serde::de::Error::duplicate_field("assistantText"));
                            }
                            assistant_text__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Encoding => {
                            if encoding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("encoding"));
                            }
                            encoding__ = Some(map_.next_value::<AudioEncoding>()? as i32);
                        }
                        GeneratedField::SampleRate => {
                            if sample_rate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sampleRate"));
                            }
                            sample_rate__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EotInferenceRequest {
                    audio: audio__.unwrap_or_default(),
                    assistant_text: assistant_text__.unwrap_or_default(),
                    encoding: encoding__.unwrap_or_default(),
                    sample_rate: sample_rate__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.EotInferenceRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EotInferenceResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.probability != 0. {
            len += 1;
        }
        if self.stats.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.EotInferenceResponse", len)?;
        if self.probability != 0. {
            struct_ser.serialize_field("probability", &self.probability)?;
        }
        if let Some(v) = self.stats.as_ref() {
            struct_ser.serialize_field("stats", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EotInferenceResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "probability",
            "stats",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Probability,
            Stats,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "probability" => Ok(GeneratedField::Probability),
                            "stats" => Ok(GeneratedField::Stats),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EotInferenceResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.EotInferenceResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EotInferenceResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut probability__ = None;
                let mut stats__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Probability => {
                            if probability__.is_some() {
                                return Err(serde::de::Error::duplicate_field("probability"));
                            }
                            probability__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Stats => {
                            if stats__.is_some() {
                                return Err(serde::de::Error::duplicate_field("stats"));
                            }
                            stats__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EotInferenceResponse {
                    probability: probability__.unwrap_or_default(),
                    stats: stats__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.EotInferenceResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EotInputChatContext {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.messages.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.EotInputChatContext", len)?;
        if !self.messages.is_empty() {
            struct_ser.serialize_field("messages", &self.messages)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EotInputChatContext {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "messages",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Messages,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "messages" => Ok(GeneratedField::Messages),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EotInputChatContext;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.EotInputChatContext")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EotInputChatContext, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut messages__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Messages => {
                            if messages__.is_some() {
                                return Err(serde::de::Error::duplicate_field("messages"));
                            }
                            messages__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EotInputChatContext {
                    messages: messages__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.EotInputChatContext", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EotModelUsage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.provider.is_empty() {
            len += 1;
        }
        if !self.model.is_empty() {
            len += 1;
        }
        if self.total_requests != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.EotModelUsage", len)?;
        if !self.provider.is_empty() {
            struct_ser.serialize_field("provider", &self.provider)?;
        }
        if !self.model.is_empty() {
            struct_ser.serialize_field("model", &self.model)?;
        }
        if self.total_requests != 0 {
            struct_ser.serialize_field("totalRequests", &self.total_requests)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EotModelUsage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "provider",
            "model",
            "total_requests",
            "totalRequests",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Provider,
            Model,
            TotalRequests,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "provider" => Ok(GeneratedField::Provider),
                            "model" => Ok(GeneratedField::Model),
                            "totalRequests" | "total_requests" => Ok(GeneratedField::TotalRequests),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EotModelUsage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.EotModelUsage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EotModelUsage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut provider__ = None;
                let mut model__ = None;
                let mut total_requests__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Provider => {
                            if provider__.is_some() {
                                return Err(serde::de::Error::duplicate_field("provider"));
                            }
                            provider__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Model => {
                            if model__.is_some() {
                                return Err(serde::de::Error::duplicate_field("model"));
                            }
                            model__ = Some(map_.next_value()?);
                        }
                        GeneratedField::TotalRequests => {
                            if total_requests__.is_some() {
                                return Err(serde::de::Error::duplicate_field("totalRequests"));
                            }
                            total_requests__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EotModelUsage {
                    provider: provider__.unwrap_or_default(),
                    model: model__.unwrap_or_default(),
                    total_requests: total_requests__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.EotModelUsage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for EotPrediction {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.probability != 0. {
            len += 1;
        }
        if self.inference_stats.is_some() {
            len += 1;
        }
        if self.backend != 0 {
            len += 1;
        }
        if self.backchannel_probability != 0. {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.EotPrediction", len)?;
        if self.probability != 0. {
            struct_ser.serialize_field("probability", &self.probability)?;
        }
        if let Some(v) = self.inference_stats.as_ref() {
            struct_ser.serialize_field("inferenceStats", v)?;
        }
        if self.backend != 0 {
            let v = eot_prediction::EotBackend::try_from(self.backend)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.backend)))?;
            struct_ser.serialize_field("backend", &v)?;
        }
        if self.backchannel_probability != 0. {
            struct_ser.serialize_field("backchannelProbability", &self.backchannel_probability)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EotPrediction {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "probability",
            "inference_stats",
            "inferenceStats",
            "backend",
            "backchannel_probability",
            "backchannelProbability",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Probability,
            InferenceStats,
            Backend,
            BackchannelProbability,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "probability" => Ok(GeneratedField::Probability),
                            "inferenceStats" | "inference_stats" => Ok(GeneratedField::InferenceStats),
                            "backend" => Ok(GeneratedField::Backend),
                            "backchannelProbability" | "backchannel_probability" => Ok(GeneratedField::BackchannelProbability),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EotPrediction;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.EotPrediction")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EotPrediction, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut probability__ = None;
                let mut inference_stats__ = None;
                let mut backend__ = None;
                let mut backchannel_probability__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Probability => {
                            if probability__.is_some() {
                                return Err(serde::de::Error::duplicate_field("probability"));
                            }
                            probability__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::InferenceStats => {
                            if inference_stats__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inferenceStats"));
                            }
                            inference_stats__ = map_.next_value()?;
                        }
                        GeneratedField::Backend => {
                            if backend__.is_some() {
                                return Err(serde::de::Error::duplicate_field("backend"));
                            }
                            backend__ = Some(map_.next_value::<eot_prediction::EotBackend>()? as i32);
                        }
                        GeneratedField::BackchannelProbability => {
                            if backchannel_probability__.is_some() {
                                return Err(serde::de::Error::duplicate_field("backchannelProbability"));
                            }
                            backchannel_probability__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EotPrediction {
                    probability: probability__.unwrap_or_default(),
                    inference_stats: inference_stats__,
                    backend: backend__.unwrap_or_default(),
                    backchannel_probability: backchannel_probability__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.EotPrediction", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for eot_prediction::EotBackend {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::Unknown => "EOT_BACKEND_UNKNOWN",
            Self::Multimodal => "EOT_BACKEND_MULTIMODAL",
            Self::Text => "EOT_BACKEND_TEXT",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for eot_prediction::EotBackend {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "EOT_BACKEND_UNKNOWN",
            "EOT_BACKEND_MULTIMODAL",
            "EOT_BACKEND_TEXT",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = eot_prediction::EotBackend;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "EOT_BACKEND_UNKNOWN" => Ok(eot_prediction::EotBackend::Unknown),
                    "EOT_BACKEND_MULTIMODAL" => Ok(eot_prediction::EotBackend::Multimodal),
                    "EOT_BACKEND_TEXT" => Ok(eot_prediction::EotBackend::Text),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for EotSettings {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.detection_interval.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.EotSettings", len)?;
        if let Some(v) = self.detection_interval.as_ref() {
            struct_ser.serialize_field("detectionInterval", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for EotSettings {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "detection_interval",
            "detectionInterval",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            DetectionInterval,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "detectionInterval" | "detection_interval" => Ok(GeneratedField::DetectionInterval),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = EotSettings;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.EotSettings")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<EotSettings, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut detection_interval__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::DetectionInterval => {
                            if detection_interval__.is_some() {
                                return Err(serde::de::Error::duplicate_field("detectionInterval"));
                            }
                            detection_interval__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(EotSettings {
                    detection_interval: detection_interval__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.EotSettings", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for FunctionCall {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.id.is_empty() {
            len += 1;
        }
        if !self.call_id.is_empty() {
            len += 1;
        }
        if !self.arguments.is_empty() {
            len += 1;
        }
        if !self.name.is_empty() {
            len += 1;
        }
        if self.created_at.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.FunctionCall", len)?;
        if !self.id.is_empty() {
            struct_ser.serialize_field("id", &self.id)?;
        }
        if !self.call_id.is_empty() {
            struct_ser.serialize_field("callId", &self.call_id)?;
        }
        if !self.arguments.is_empty() {
            struct_ser.serialize_field("arguments", &self.arguments)?;
        }
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        if let Some(v) = self.created_at.as_ref() {
            struct_ser.serialize_field("createdAt", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for FunctionCall {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "id",
            "call_id",
            "callId",
            "arguments",
            "name",
            "created_at",
            "createdAt",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Id,
            CallId,
            Arguments,
            Name,
            CreatedAt,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "id" => Ok(GeneratedField::Id),
                            "callId" | "call_id" => Ok(GeneratedField::CallId),
                            "arguments" => Ok(GeneratedField::Arguments),
                            "name" => Ok(GeneratedField::Name),
                            "createdAt" | "created_at" => Ok(GeneratedField::CreatedAt),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = FunctionCall;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.FunctionCall")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<FunctionCall, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut id__ = None;
                let mut call_id__ = None;
                let mut arguments__ = None;
                let mut name__ = None;
                let mut created_at__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::CallId => {
                            if call_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("callId"));
                            }
                            call_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Arguments => {
                            if arguments__.is_some() {
                                return Err(serde::de::Error::duplicate_field("arguments"));
                            }
                            arguments__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map_.next_value()?);
                        }
                        GeneratedField::CreatedAt => {
                            if created_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdAt"));
                            }
                            created_at__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(FunctionCall {
                    id: id__.unwrap_or_default(),
                    call_id: call_id__.unwrap_or_default(),
                    arguments: arguments__.unwrap_or_default(),
                    name: name__.unwrap_or_default(),
                    created_at: created_at__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.FunctionCall", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for FunctionCallOutput {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.id.is_empty() {
            len += 1;
        }
        if !self.name.is_empty() {
            len += 1;
        }
        if !self.call_id.is_empty() {
            len += 1;
        }
        if !self.output.is_empty() {
            len += 1;
        }
        if self.is_error {
            len += 1;
        }
        if self.created_at.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.FunctionCallOutput", len)?;
        if !self.id.is_empty() {
            struct_ser.serialize_field("id", &self.id)?;
        }
        if !self.name.is_empty() {
            struct_ser.serialize_field("name", &self.name)?;
        }
        if !self.call_id.is_empty() {
            struct_ser.serialize_field("callId", &self.call_id)?;
        }
        if !self.output.is_empty() {
            struct_ser.serialize_field("output", &self.output)?;
        }
        if self.is_error {
            struct_ser.serialize_field("isError", &self.is_error)?;
        }
        if let Some(v) = self.created_at.as_ref() {
            struct_ser.serialize_field("createdAt", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for FunctionCallOutput {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "id",
            "name",
            "call_id",
            "callId",
            "output",
            "is_error",
            "isError",
            "created_at",
            "createdAt",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Id,
            Name,
            CallId,
            Output,
            IsError,
            CreatedAt,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "id" => Ok(GeneratedField::Id),
                            "name" => Ok(GeneratedField::Name),
                            "callId" | "call_id" => Ok(GeneratedField::CallId),
                            "output" => Ok(GeneratedField::Output),
                            "isError" | "is_error" => Ok(GeneratedField::IsError),
                            "createdAt" | "created_at" => Ok(GeneratedField::CreatedAt),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = FunctionCallOutput;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.FunctionCallOutput")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<FunctionCallOutput, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut id__ = None;
                let mut name__ = None;
                let mut call_id__ = None;
                let mut output__ = None;
                let mut is_error__ = None;
                let mut created_at__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Name => {
                            if name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("name"));
                            }
                            name__ = Some(map_.next_value()?);
                        }
                        GeneratedField::CallId => {
                            if call_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("callId"));
                            }
                            call_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Output => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("output"));
                            }
                            output__ = Some(map_.next_value()?);
                        }
                        GeneratedField::IsError => {
                            if is_error__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isError"));
                            }
                            is_error__ = Some(map_.next_value()?);
                        }
                        GeneratedField::CreatedAt => {
                            if created_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdAt"));
                            }
                            created_at__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(FunctionCallOutput {
                    id: id__.unwrap_or_default(),
                    name: name__.unwrap_or_default(),
                    call_id: call_id__.unwrap_or_default(),
                    output: output__.unwrap_or_default(),
                    is_error: is_error__.unwrap_or_default(),
                    created_at: created_at__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.FunctionCallOutput", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetRunningAgentJobsRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.GetRunningAgentJobsRequest", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetRunningAgentJobsRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetRunningAgentJobsRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.GetRunningAgentJobsRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetRunningAgentJobsRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(GetRunningAgentJobsRequest {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.GetRunningAgentJobsRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for GetRunningAgentJobsResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.jobs.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.GetRunningAgentJobsResponse", len)?;
        if !self.jobs.is_empty() {
            struct_ser.serialize_field("jobs", &self.jobs)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for GetRunningAgentJobsResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "jobs",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Jobs,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "jobs" => Ok(GeneratedField::Jobs),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = GetRunningAgentJobsResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.GetRunningAgentJobsResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<GetRunningAgentJobsResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut jobs__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Jobs => {
                            if jobs__.is_some() {
                                return Err(serde::de::Error::duplicate_field("jobs"));
                            }
                            jobs__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(GetRunningAgentJobsResponse {
                    jobs: jobs__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.GetRunningAgentJobsResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InferenceError {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.message.is_empty() {
            len += 1;
        }
        if self.code != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.InferenceError", len)?;
        if !self.message.is_empty() {
            struct_ser.serialize_field("message", &self.message)?;
        }
        if self.code != 0 {
            struct_ser.serialize_field("code", &self.code)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InferenceError {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "message",
            "code",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Message,
            Code,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "message" => Ok(GeneratedField::Message),
                            "code" => Ok(GeneratedField::Code),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InferenceError;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.InferenceError")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<InferenceError, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut message__ = None;
                let mut code__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Message => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("message"));
                            }
                            message__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Code => {
                            if code__.is_some() {
                                return Err(serde::de::Error::duplicate_field("code"));
                            }
                            code__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(InferenceError {
                    message: message__.unwrap_or_default(),
                    code: code__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.InferenceError", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InferenceRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.request.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.InferenceRequest", len)?;
        if let Some(v) = self.request.as_ref() {
            match v {
                inference_request::Request::EotInferenceRequest(v) => {
                    struct_ser.serialize_field("eotInferenceRequest", v)?;
                }
                inference_request::Request::InterruptionInferenceRequest(v) => {
                    struct_ser.serialize_field("interruptionInferenceRequest", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InferenceRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "eot_inference_request",
            "eotInferenceRequest",
            "interruption_inference_request",
            "interruptionInferenceRequest",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            EotInferenceRequest,
            InterruptionInferenceRequest,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "eotInferenceRequest" | "eot_inference_request" => Ok(GeneratedField::EotInferenceRequest),
                            "interruptionInferenceRequest" | "interruption_inference_request" => Ok(GeneratedField::InterruptionInferenceRequest),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InferenceRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.InferenceRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<InferenceRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut request__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::EotInferenceRequest => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("eotInferenceRequest"));
                            }
                            request__ = map_.next_value::<::std::option::Option<_>>()?.map(inference_request::Request::EotInferenceRequest)
;
                        }
                        GeneratedField::InterruptionInferenceRequest => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("interruptionInferenceRequest"));
                            }
                            request__ = map_.next_value::<::std::option::Option<_>>()?.map(inference_request::Request::InterruptionInferenceRequest)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(InferenceRequest {
                    request: request__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.InferenceRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InferenceResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.response.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.InferenceResponse", len)?;
        if let Some(v) = self.response.as_ref() {
            match v {
                inference_response::Response::EotInferenceResponse(v) => {
                    struct_ser.serialize_field("eotInferenceResponse", v)?;
                }
                inference_response::Response::InterruptionInferenceResponse(v) => {
                    struct_ser.serialize_field("interruptionInferenceResponse", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InferenceResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "eot_inference_response",
            "eotInferenceResponse",
            "interruption_inference_response",
            "interruptionInferenceResponse",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            EotInferenceResponse,
            InterruptionInferenceResponse,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "eotInferenceResponse" | "eot_inference_response" => Ok(GeneratedField::EotInferenceResponse),
                            "interruptionInferenceResponse" | "interruption_inference_response" => Ok(GeneratedField::InterruptionInferenceResponse),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InferenceResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.InferenceResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<InferenceResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut response__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::EotInferenceResponse => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("eotInferenceResponse"));
                            }
                            response__ = map_.next_value::<::std::option::Option<_>>()?.map(inference_response::Response::EotInferenceResponse)
;
                        }
                        GeneratedField::InterruptionInferenceResponse => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("interruptionInferenceResponse"));
                            }
                            response__ = map_.next_value::<::std::option::Option<_>>()?.map(inference_response::Response::InterruptionInferenceResponse)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(InferenceResponse {
                    response: response__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.InferenceResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InferenceStart {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.request_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.InferenceStart", len)?;
        if !self.request_id.is_empty() {
            struct_ser.serialize_field("requestId", &self.request_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InferenceStart {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "request_id",
            "requestId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RequestId,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "requestId" | "request_id" => Ok(GeneratedField::RequestId),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InferenceStart;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.InferenceStart")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<InferenceStart, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut request_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::RequestId => {
                            if request_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requestId"));
                            }
                            request_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(InferenceStart {
                    request_id: request_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.InferenceStart", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InferenceStarted {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.InferenceStarted", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InferenceStarted {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InferenceStarted;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.InferenceStarted")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<InferenceStarted, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(InferenceStarted {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.InferenceStarted", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InferenceStats {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.earliest_client_created_at.is_some() {
            len += 1;
        }
        if self.latest_client_created_at.is_some() {
            len += 1;
        }
        if self.client_e2e_latency.is_some() {
            len += 1;
        }
        if self.server_e2e_latency.is_some() {
            len += 1;
        }
        if self.preprocessing_duration.is_some() {
            len += 1;
        }
        if self.inference_duration.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.InferenceStats", len)?;
        if let Some(v) = self.earliest_client_created_at.as_ref() {
            struct_ser.serialize_field("earliestClientCreatedAt", v)?;
        }
        if let Some(v) = self.latest_client_created_at.as_ref() {
            struct_ser.serialize_field("latestClientCreatedAt", v)?;
        }
        if let Some(v) = self.client_e2e_latency.as_ref() {
            struct_ser.serialize_field("clientE2eLatency", v)?;
        }
        if let Some(v) = self.server_e2e_latency.as_ref() {
            struct_ser.serialize_field("serverE2eLatency", v)?;
        }
        if let Some(v) = self.preprocessing_duration.as_ref() {
            struct_ser.serialize_field("preprocessingDuration", v)?;
        }
        if let Some(v) = self.inference_duration.as_ref() {
            struct_ser.serialize_field("inferenceDuration", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InferenceStats {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "earliest_client_created_at",
            "earliestClientCreatedAt",
            "latest_client_created_at",
            "latestClientCreatedAt",
            "client_e2e_latency",
            "clientE2eLatency",
            "server_e2e_latency",
            "serverE2eLatency",
            "preprocessing_duration",
            "preprocessingDuration",
            "inference_duration",
            "inferenceDuration",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            EarliestClientCreatedAt,
            LatestClientCreatedAt,
            ClientE2eLatency,
            ServerE2eLatency,
            PreprocessingDuration,
            InferenceDuration,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "earliestClientCreatedAt" | "earliest_client_created_at" => Ok(GeneratedField::EarliestClientCreatedAt),
                            "latestClientCreatedAt" | "latest_client_created_at" => Ok(GeneratedField::LatestClientCreatedAt),
                            "clientE2eLatency" | "client_e2e_latency" => Ok(GeneratedField::ClientE2eLatency),
                            "serverE2eLatency" | "server_e2e_latency" => Ok(GeneratedField::ServerE2eLatency),
                            "preprocessingDuration" | "preprocessing_duration" => Ok(GeneratedField::PreprocessingDuration),
                            "inferenceDuration" | "inference_duration" => Ok(GeneratedField::InferenceDuration),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InferenceStats;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.InferenceStats")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<InferenceStats, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut earliest_client_created_at__ = None;
                let mut latest_client_created_at__ = None;
                let mut client_e2e_latency__ = None;
                let mut server_e2e_latency__ = None;
                let mut preprocessing_duration__ = None;
                let mut inference_duration__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::EarliestClientCreatedAt => {
                            if earliest_client_created_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("earliestClientCreatedAt"));
                            }
                            earliest_client_created_at__ = map_.next_value()?;
                        }
                        GeneratedField::LatestClientCreatedAt => {
                            if latest_client_created_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("latestClientCreatedAt"));
                            }
                            latest_client_created_at__ = map_.next_value()?;
                        }
                        GeneratedField::ClientE2eLatency => {
                            if client_e2e_latency__.is_some() {
                                return Err(serde::de::Error::duplicate_field("clientE2eLatency"));
                            }
                            client_e2e_latency__ = map_.next_value()?;
                        }
                        GeneratedField::ServerE2eLatency => {
                            if server_e2e_latency__.is_some() {
                                return Err(serde::de::Error::duplicate_field("serverE2eLatency"));
                            }
                            server_e2e_latency__ = map_.next_value()?;
                        }
                        GeneratedField::PreprocessingDuration => {
                            if preprocessing_duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("preprocessingDuration"));
                            }
                            preprocessing_duration__ = map_.next_value()?;
                        }
                        GeneratedField::InferenceDuration => {
                            if inference_duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inferenceDuration"));
                            }
                            inference_duration__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(InferenceStats {
                    earliest_client_created_at: earliest_client_created_at__,
                    latest_client_created_at: latest_client_created_at__,
                    client_e2e_latency: client_e2e_latency__,
                    server_e2e_latency: server_e2e_latency__,
                    preprocessing_duration: preprocessing_duration__,
                    inference_duration: inference_duration__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.InferenceStats", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InferenceStop {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.request_id.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.InferenceStop", len)?;
        if !self.request_id.is_empty() {
            struct_ser.serialize_field("requestId", &self.request_id)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InferenceStop {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "request_id",
            "requestId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RequestId,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "requestId" | "request_id" => Ok(GeneratedField::RequestId),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InferenceStop;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.InferenceStop")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<InferenceStop, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut request_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::RequestId => {
                            if request_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requestId"));
                            }
                            request_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(InferenceStop {
                    request_id: request_id__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.InferenceStop", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InferenceStopped {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.InferenceStopped", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InferenceStopped {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InferenceStopped;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.InferenceStopped")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<InferenceStopped, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(InferenceStopped {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.InferenceStopped", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InputAudio {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.audio.is_empty() {
            len += 1;
        }
        if self.created_at.is_some() {
            len += 1;
        }
        if self.num_samples != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.InputAudio", len)?;
        if !self.audio.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("audio", pbjson::private::base64::encode(&self.audio).as_str())?;
        }
        if let Some(v) = self.created_at.as_ref() {
            struct_ser.serialize_field("createdAt", v)?;
        }
        if self.num_samples != 0 {
            struct_ser.serialize_field("numSamples", &self.num_samples)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InputAudio {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "audio",
            "created_at",
            "createdAt",
            "num_samples",
            "numSamples",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Audio,
            CreatedAt,
            NumSamples,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "audio" => Ok(GeneratedField::Audio),
                            "createdAt" | "created_at" => Ok(GeneratedField::CreatedAt),
                            "numSamples" | "num_samples" => Ok(GeneratedField::NumSamples),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InputAudio;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.InputAudio")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<InputAudio, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut audio__ = None;
                let mut created_at__ = None;
                let mut num_samples__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Audio => {
                            if audio__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audio"));
                            }
                            audio__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CreatedAt => {
                            if created_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdAt"));
                            }
                            created_at__ = map_.next_value()?;
                        }
                        GeneratedField::NumSamples => {
                            if num_samples__.is_some() {
                                return Err(serde::de::Error::duplicate_field("numSamples"));
                            }
                            num_samples__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(InputAudio {
                    audio: audio__.unwrap_or_default(),
                    created_at: created_at__,
                    num_samples: num_samples__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.InputAudio", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InterruptionInferenceRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.audio.is_empty() {
            len += 1;
        }
        if self.encoding != 0 {
            len += 1;
        }
        if self.sample_rate != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.InterruptionInferenceRequest", len)?;
        if !self.audio.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("audio", pbjson::private::base64::encode(&self.audio).as_str())?;
        }
        if self.encoding != 0 {
            let v = AudioEncoding::try_from(self.encoding)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.encoding)))?;
            struct_ser.serialize_field("encoding", &v)?;
        }
        if self.sample_rate != 0 {
            struct_ser.serialize_field("sampleRate", &self.sample_rate)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InterruptionInferenceRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "audio",
            "encoding",
            "sample_rate",
            "sampleRate",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Audio,
            Encoding,
            SampleRate,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "audio" => Ok(GeneratedField::Audio),
                            "encoding" => Ok(GeneratedField::Encoding),
                            "sampleRate" | "sample_rate" => Ok(GeneratedField::SampleRate),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InterruptionInferenceRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.InterruptionInferenceRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<InterruptionInferenceRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut audio__ = None;
                let mut encoding__ = None;
                let mut sample_rate__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Audio => {
                            if audio__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audio"));
                            }
                            audio__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Encoding => {
                            if encoding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("encoding"));
                            }
                            encoding__ = Some(map_.next_value::<AudioEncoding>()? as i32);
                        }
                        GeneratedField::SampleRate => {
                            if sample_rate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sampleRate"));
                            }
                            sample_rate__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(InterruptionInferenceRequest {
                    audio: audio__.unwrap_or_default(),
                    encoding: encoding__.unwrap_or_default(),
                    sample_rate: sample_rate__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.InterruptionInferenceRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InterruptionInferenceResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.is_interruption {
            len += 1;
        }
        if !self.probabilities.is_empty() {
            len += 1;
        }
        if self.stats.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.InterruptionInferenceResponse", len)?;
        if self.is_interruption {
            struct_ser.serialize_field("isInterruption", &self.is_interruption)?;
        }
        if !self.probabilities.is_empty() {
            struct_ser.serialize_field("probabilities", &self.probabilities)?;
        }
        if let Some(v) = self.stats.as_ref() {
            struct_ser.serialize_field("stats", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InterruptionInferenceResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "is_interruption",
            "isInterruption",
            "probabilities",
            "stats",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IsInterruption,
            Probabilities,
            Stats,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "isInterruption" | "is_interruption" => Ok(GeneratedField::IsInterruption),
                            "probabilities" => Ok(GeneratedField::Probabilities),
                            "stats" => Ok(GeneratedField::Stats),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InterruptionInferenceResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.InterruptionInferenceResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<InterruptionInferenceResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut is_interruption__ = None;
                let mut probabilities__ = None;
                let mut stats__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::IsInterruption => {
                            if is_interruption__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isInterruption"));
                            }
                            is_interruption__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Probabilities => {
                            if probabilities__.is_some() {
                                return Err(serde::de::Error::duplicate_field("probabilities"));
                            }
                            probabilities__ = 
                                Some(map_.next_value::<Vec<::pbjson::private::NumberDeserialize<_>>>()?
                                    .into_iter().map(|x| x.0).collect())
                            ;
                        }
                        GeneratedField::Stats => {
                            if stats__.is_some() {
                                return Err(serde::de::Error::duplicate_field("stats"));
                            }
                            stats__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(InterruptionInferenceResponse {
                    is_interruption: is_interruption__.unwrap_or_default(),
                    probabilities: probabilities__.unwrap_or_default(),
                    stats: stats__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.InterruptionInferenceResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InterruptionModelUsage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.provider.is_empty() {
            len += 1;
        }
        if !self.model.is_empty() {
            len += 1;
        }
        if self.total_requests != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.InterruptionModelUsage", len)?;
        if !self.provider.is_empty() {
            struct_ser.serialize_field("provider", &self.provider)?;
        }
        if !self.model.is_empty() {
            struct_ser.serialize_field("model", &self.model)?;
        }
        if self.total_requests != 0 {
            struct_ser.serialize_field("totalRequests", &self.total_requests)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InterruptionModelUsage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "provider",
            "model",
            "total_requests",
            "totalRequests",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Provider,
            Model,
            TotalRequests,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "provider" => Ok(GeneratedField::Provider),
                            "model" => Ok(GeneratedField::Model),
                            "totalRequests" | "total_requests" => Ok(GeneratedField::TotalRequests),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InterruptionModelUsage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.InterruptionModelUsage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<InterruptionModelUsage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut provider__ = None;
                let mut model__ = None;
                let mut total_requests__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Provider => {
                            if provider__.is_some() {
                                return Err(serde::de::Error::duplicate_field("provider"));
                            }
                            provider__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Model => {
                            if model__.is_some() {
                                return Err(serde::de::Error::duplicate_field("model"));
                            }
                            model__ = Some(map_.next_value()?);
                        }
                        GeneratedField::TotalRequests => {
                            if total_requests__.is_some() {
                                return Err(serde::de::Error::duplicate_field("totalRequests"));
                            }
                            total_requests__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(InterruptionModelUsage {
                    provider: provider__.unwrap_or_default(),
                    model: model__.unwrap_or_default(),
                    total_requests: total_requests__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.InterruptionModelUsage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InterruptionPrediction {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.is_interruption {
            len += 1;
        }
        if !self.probabilities.is_empty() {
            len += 1;
        }
        if self.inference_stats.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.InterruptionPrediction", len)?;
        if self.is_interruption {
            struct_ser.serialize_field("isInterruption", &self.is_interruption)?;
        }
        if !self.probabilities.is_empty() {
            struct_ser.serialize_field("probabilities", &self.probabilities)?;
        }
        if let Some(v) = self.inference_stats.as_ref() {
            struct_ser.serialize_field("inferenceStats", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InterruptionPrediction {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "is_interruption",
            "isInterruption",
            "probabilities",
            "inference_stats",
            "inferenceStats",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            IsInterruption,
            Probabilities,
            InferenceStats,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "isInterruption" | "is_interruption" => Ok(GeneratedField::IsInterruption),
                            "probabilities" => Ok(GeneratedField::Probabilities),
                            "inferenceStats" | "inference_stats" => Ok(GeneratedField::InferenceStats),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InterruptionPrediction;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.InterruptionPrediction")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<InterruptionPrediction, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut is_interruption__ = None;
                let mut probabilities__ = None;
                let mut inference_stats__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::IsInterruption => {
                            if is_interruption__.is_some() {
                                return Err(serde::de::Error::duplicate_field("isInterruption"));
                            }
                            is_interruption__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Probabilities => {
                            if probabilities__.is_some() {
                                return Err(serde::de::Error::duplicate_field("probabilities"));
                            }
                            probabilities__ = 
                                Some(map_.next_value::<Vec<::pbjson::private::NumberDeserialize<_>>>()?
                                    .into_iter().map(|x| x.0).collect())
                            ;
                        }
                        GeneratedField::InferenceStats => {
                            if inference_stats__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inferenceStats"));
                            }
                            inference_stats__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(InterruptionPrediction {
                    is_interruption: is_interruption__.unwrap_or_default(),
                    probabilities: probabilities__.unwrap_or_default(),
                    inference_stats: inference_stats__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.InterruptionPrediction", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for InterruptionSettings {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.threshold != 0. {
            len += 1;
        }
        if self.min_frames != 0 {
            len += 1;
        }
        if self.max_audio_duration.is_some() {
            len += 1;
        }
        if self.audio_prefix_duration.is_some() {
            len += 1;
        }
        if self.detection_interval.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.InterruptionSettings", len)?;
        if self.threshold != 0. {
            struct_ser.serialize_field("threshold", &self.threshold)?;
        }
        if self.min_frames != 0 {
            struct_ser.serialize_field("minFrames", &self.min_frames)?;
        }
        if let Some(v) = self.max_audio_duration.as_ref() {
            struct_ser.serialize_field("maxAudioDuration", v)?;
        }
        if let Some(v) = self.audio_prefix_duration.as_ref() {
            struct_ser.serialize_field("audioPrefixDuration", v)?;
        }
        if let Some(v) = self.detection_interval.as_ref() {
            struct_ser.serialize_field("detectionInterval", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for InterruptionSettings {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "threshold",
            "min_frames",
            "minFrames",
            "max_audio_duration",
            "maxAudioDuration",
            "audio_prefix_duration",
            "audioPrefixDuration",
            "detection_interval",
            "detectionInterval",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Threshold,
            MinFrames,
            MaxAudioDuration,
            AudioPrefixDuration,
            DetectionInterval,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "threshold" => Ok(GeneratedField::Threshold),
                            "minFrames" | "min_frames" => Ok(GeneratedField::MinFrames),
                            "maxAudioDuration" | "max_audio_duration" => Ok(GeneratedField::MaxAudioDuration),
                            "audioPrefixDuration" | "audio_prefix_duration" => Ok(GeneratedField::AudioPrefixDuration),
                            "detectionInterval" | "detection_interval" => Ok(GeneratedField::DetectionInterval),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = InterruptionSettings;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.InterruptionSettings")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<InterruptionSettings, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut threshold__ = None;
                let mut min_frames__ = None;
                let mut max_audio_duration__ = None;
                let mut audio_prefix_duration__ = None;
                let mut detection_interval__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Threshold => {
                            if threshold__.is_some() {
                                return Err(serde::de::Error::duplicate_field("threshold"));
                            }
                            threshold__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::MinFrames => {
                            if min_frames__.is_some() {
                                return Err(serde::de::Error::duplicate_field("minFrames"));
                            }
                            min_frames__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::MaxAudioDuration => {
                            if max_audio_duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("maxAudioDuration"));
                            }
                            max_audio_duration__ = map_.next_value()?;
                        }
                        GeneratedField::AudioPrefixDuration => {
                            if audio_prefix_duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioPrefixDuration"));
                            }
                            audio_prefix_duration__ = map_.next_value()?;
                        }
                        GeneratedField::DetectionInterval => {
                            if detection_interval__.is_some() {
                                return Err(serde::de::Error::duplicate_field("detectionInterval"));
                            }
                            detection_interval__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(InterruptionSettings {
                    threshold: threshold__.unwrap_or_default(),
                    min_frames: min_frames__.unwrap_or_default(),
                    max_audio_duration: max_audio_duration__,
                    audio_prefix_duration: audio_prefix_duration__,
                    detection_interval: detection_interval__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.InterruptionSettings", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for LlmModelUsage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.provider.is_empty() {
            len += 1;
        }
        if !self.model.is_empty() {
            len += 1;
        }
        if self.input_tokens != 0 {
            len += 1;
        }
        if self.input_cached_tokens != 0 {
            len += 1;
        }
        if self.input_audio_tokens != 0 {
            len += 1;
        }
        if self.input_cached_audio_tokens != 0 {
            len += 1;
        }
        if self.input_text_tokens != 0 {
            len += 1;
        }
        if self.input_cached_text_tokens != 0 {
            len += 1;
        }
        if self.input_image_tokens != 0 {
            len += 1;
        }
        if self.input_cached_image_tokens != 0 {
            len += 1;
        }
        if self.output_tokens != 0 {
            len += 1;
        }
        if self.output_audio_tokens != 0 {
            len += 1;
        }
        if self.output_text_tokens != 0 {
            len += 1;
        }
        if self.session_duration != 0. {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.LLMModelUsage", len)?;
        if !self.provider.is_empty() {
            struct_ser.serialize_field("provider", &self.provider)?;
        }
        if !self.model.is_empty() {
            struct_ser.serialize_field("model", &self.model)?;
        }
        if self.input_tokens != 0 {
            struct_ser.serialize_field("inputTokens", &self.input_tokens)?;
        }
        if self.input_cached_tokens != 0 {
            struct_ser.serialize_field("inputCachedTokens", &self.input_cached_tokens)?;
        }
        if self.input_audio_tokens != 0 {
            struct_ser.serialize_field("inputAudioTokens", &self.input_audio_tokens)?;
        }
        if self.input_cached_audio_tokens != 0 {
            struct_ser.serialize_field("inputCachedAudioTokens", &self.input_cached_audio_tokens)?;
        }
        if self.input_text_tokens != 0 {
            struct_ser.serialize_field("inputTextTokens", &self.input_text_tokens)?;
        }
        if self.input_cached_text_tokens != 0 {
            struct_ser.serialize_field("inputCachedTextTokens", &self.input_cached_text_tokens)?;
        }
        if self.input_image_tokens != 0 {
            struct_ser.serialize_field("inputImageTokens", &self.input_image_tokens)?;
        }
        if self.input_cached_image_tokens != 0 {
            struct_ser.serialize_field("inputCachedImageTokens", &self.input_cached_image_tokens)?;
        }
        if self.output_tokens != 0 {
            struct_ser.serialize_field("outputTokens", &self.output_tokens)?;
        }
        if self.output_audio_tokens != 0 {
            struct_ser.serialize_field("outputAudioTokens", &self.output_audio_tokens)?;
        }
        if self.output_text_tokens != 0 {
            struct_ser.serialize_field("outputTextTokens", &self.output_text_tokens)?;
        }
        if self.session_duration != 0. {
            struct_ser.serialize_field("sessionDuration", &self.session_duration)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for LlmModelUsage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "provider",
            "model",
            "input_tokens",
            "inputTokens",
            "input_cached_tokens",
            "inputCachedTokens",
            "input_audio_tokens",
            "inputAudioTokens",
            "input_cached_audio_tokens",
            "inputCachedAudioTokens",
            "input_text_tokens",
            "inputTextTokens",
            "input_cached_text_tokens",
            "inputCachedTextTokens",
            "input_image_tokens",
            "inputImageTokens",
            "input_cached_image_tokens",
            "inputCachedImageTokens",
            "output_tokens",
            "outputTokens",
            "output_audio_tokens",
            "outputAudioTokens",
            "output_text_tokens",
            "outputTextTokens",
            "session_duration",
            "sessionDuration",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Provider,
            Model,
            InputTokens,
            InputCachedTokens,
            InputAudioTokens,
            InputCachedAudioTokens,
            InputTextTokens,
            InputCachedTextTokens,
            InputImageTokens,
            InputCachedImageTokens,
            OutputTokens,
            OutputAudioTokens,
            OutputTextTokens,
            SessionDuration,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "provider" => Ok(GeneratedField::Provider),
                            "model" => Ok(GeneratedField::Model),
                            "inputTokens" | "input_tokens" => Ok(GeneratedField::InputTokens),
                            "inputCachedTokens" | "input_cached_tokens" => Ok(GeneratedField::InputCachedTokens),
                            "inputAudioTokens" | "input_audio_tokens" => Ok(GeneratedField::InputAudioTokens),
                            "inputCachedAudioTokens" | "input_cached_audio_tokens" => Ok(GeneratedField::InputCachedAudioTokens),
                            "inputTextTokens" | "input_text_tokens" => Ok(GeneratedField::InputTextTokens),
                            "inputCachedTextTokens" | "input_cached_text_tokens" => Ok(GeneratedField::InputCachedTextTokens),
                            "inputImageTokens" | "input_image_tokens" => Ok(GeneratedField::InputImageTokens),
                            "inputCachedImageTokens" | "input_cached_image_tokens" => Ok(GeneratedField::InputCachedImageTokens),
                            "outputTokens" | "output_tokens" => Ok(GeneratedField::OutputTokens),
                            "outputAudioTokens" | "output_audio_tokens" => Ok(GeneratedField::OutputAudioTokens),
                            "outputTextTokens" | "output_text_tokens" => Ok(GeneratedField::OutputTextTokens),
                            "sessionDuration" | "session_duration" => Ok(GeneratedField::SessionDuration),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = LlmModelUsage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.LLMModelUsage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<LlmModelUsage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut provider__ = None;
                let mut model__ = None;
                let mut input_tokens__ = None;
                let mut input_cached_tokens__ = None;
                let mut input_audio_tokens__ = None;
                let mut input_cached_audio_tokens__ = None;
                let mut input_text_tokens__ = None;
                let mut input_cached_text_tokens__ = None;
                let mut input_image_tokens__ = None;
                let mut input_cached_image_tokens__ = None;
                let mut output_tokens__ = None;
                let mut output_audio_tokens__ = None;
                let mut output_text_tokens__ = None;
                let mut session_duration__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Provider => {
                            if provider__.is_some() {
                                return Err(serde::de::Error::duplicate_field("provider"));
                            }
                            provider__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Model => {
                            if model__.is_some() {
                                return Err(serde::de::Error::duplicate_field("model"));
                            }
                            model__ = Some(map_.next_value()?);
                        }
                        GeneratedField::InputTokens => {
                            if input_tokens__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inputTokens"));
                            }
                            input_tokens__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::InputCachedTokens => {
                            if input_cached_tokens__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inputCachedTokens"));
                            }
                            input_cached_tokens__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::InputAudioTokens => {
                            if input_audio_tokens__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inputAudioTokens"));
                            }
                            input_audio_tokens__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::InputCachedAudioTokens => {
                            if input_cached_audio_tokens__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inputCachedAudioTokens"));
                            }
                            input_cached_audio_tokens__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::InputTextTokens => {
                            if input_text_tokens__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inputTextTokens"));
                            }
                            input_text_tokens__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::InputCachedTextTokens => {
                            if input_cached_text_tokens__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inputCachedTextTokens"));
                            }
                            input_cached_text_tokens__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::InputImageTokens => {
                            if input_image_tokens__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inputImageTokens"));
                            }
                            input_image_tokens__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::InputCachedImageTokens => {
                            if input_cached_image_tokens__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inputCachedImageTokens"));
                            }
                            input_cached_image_tokens__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::OutputTokens => {
                            if output_tokens__.is_some() {
                                return Err(serde::de::Error::duplicate_field("outputTokens"));
                            }
                            output_tokens__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::OutputAudioTokens => {
                            if output_audio_tokens__.is_some() {
                                return Err(serde::de::Error::duplicate_field("outputAudioTokens"));
                            }
                            output_audio_tokens__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::OutputTextTokens => {
                            if output_text_tokens__.is_some() {
                                return Err(serde::de::Error::duplicate_field("outputTextTokens"));
                            }
                            output_text_tokens__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::SessionDuration => {
                            if session_duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sessionDuration"));
                            }
                            session_duration__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(LlmModelUsage {
                    provider: provider__.unwrap_or_default(),
                    model: model__.unwrap_or_default(),
                    input_tokens: input_tokens__.unwrap_or_default(),
                    input_cached_tokens: input_cached_tokens__.unwrap_or_default(),
                    input_audio_tokens: input_audio_tokens__.unwrap_or_default(),
                    input_cached_audio_tokens: input_cached_audio_tokens__.unwrap_or_default(),
                    input_text_tokens: input_text_tokens__.unwrap_or_default(),
                    input_cached_text_tokens: input_cached_text_tokens__.unwrap_or_default(),
                    input_image_tokens: input_image_tokens__.unwrap_or_default(),
                    input_cached_image_tokens: input_cached_image_tokens__.unwrap_or_default(),
                    output_tokens: output_tokens__.unwrap_or_default(),
                    output_audio_tokens: output_audio_tokens__.unwrap_or_default(),
                    output_text_tokens: output_text_tokens__.unwrap_or_default(),
                    session_duration: session_duration__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.LLMModelUsage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for MetricsReport {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.started_speaking_at.is_some() {
            len += 1;
        }
        if self.stopped_speaking_at.is_some() {
            len += 1;
        }
        if self.transcription_delay.is_some() {
            len += 1;
        }
        if self.end_of_turn_delay.is_some() {
            len += 1;
        }
        if self.on_user_turn_completed_delay.is_some() {
            len += 1;
        }
        if self.llm_node_ttft.is_some() {
            len += 1;
        }
        if self.tts_node_ttfb.is_some() {
            len += 1;
        }
        if self.e2e_latency.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.MetricsReport", len)?;
        if let Some(v) = self.started_speaking_at.as_ref() {
            struct_ser.serialize_field("startedSpeakingAt", v)?;
        }
        if let Some(v) = self.stopped_speaking_at.as_ref() {
            struct_ser.serialize_field("stoppedSpeakingAt", v)?;
        }
        if let Some(v) = self.transcription_delay.as_ref() {
            struct_ser.serialize_field("transcriptionDelay", v)?;
        }
        if let Some(v) = self.end_of_turn_delay.as_ref() {
            struct_ser.serialize_field("endOfTurnDelay", v)?;
        }
        if let Some(v) = self.on_user_turn_completed_delay.as_ref() {
            struct_ser.serialize_field("onUserTurnCompletedDelay", v)?;
        }
        if let Some(v) = self.llm_node_ttft.as_ref() {
            struct_ser.serialize_field("llmNodeTtft", v)?;
        }
        if let Some(v) = self.tts_node_ttfb.as_ref() {
            struct_ser.serialize_field("ttsNodeTtfb", v)?;
        }
        if let Some(v) = self.e2e_latency.as_ref() {
            struct_ser.serialize_field("e2eLatency", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for MetricsReport {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "started_speaking_at",
            "startedSpeakingAt",
            "stopped_speaking_at",
            "stoppedSpeakingAt",
            "transcription_delay",
            "transcriptionDelay",
            "end_of_turn_delay",
            "endOfTurnDelay",
            "on_user_turn_completed_delay",
            "onUserTurnCompletedDelay",
            "llm_node_ttft",
            "llmNodeTtft",
            "tts_node_ttfb",
            "ttsNodeTtfb",
            "e2e_latency",
            "e2eLatency",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            StartedSpeakingAt,
            StoppedSpeakingAt,
            TranscriptionDelay,
            EndOfTurnDelay,
            OnUserTurnCompletedDelay,
            LlmNodeTtft,
            TtsNodeTtfb,
            E2eLatency,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "startedSpeakingAt" | "started_speaking_at" => Ok(GeneratedField::StartedSpeakingAt),
                            "stoppedSpeakingAt" | "stopped_speaking_at" => Ok(GeneratedField::StoppedSpeakingAt),
                            "transcriptionDelay" | "transcription_delay" => Ok(GeneratedField::TranscriptionDelay),
                            "endOfTurnDelay" | "end_of_turn_delay" => Ok(GeneratedField::EndOfTurnDelay),
                            "onUserTurnCompletedDelay" | "on_user_turn_completed_delay" => Ok(GeneratedField::OnUserTurnCompletedDelay),
                            "llmNodeTtft" | "llm_node_ttft" => Ok(GeneratedField::LlmNodeTtft),
                            "ttsNodeTtfb" | "tts_node_ttfb" => Ok(GeneratedField::TtsNodeTtfb),
                            "e2eLatency" | "e2e_latency" => Ok(GeneratedField::E2eLatency),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = MetricsReport;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.MetricsReport")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<MetricsReport, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut started_speaking_at__ = None;
                let mut stopped_speaking_at__ = None;
                let mut transcription_delay__ = None;
                let mut end_of_turn_delay__ = None;
                let mut on_user_turn_completed_delay__ = None;
                let mut llm_node_ttft__ = None;
                let mut tts_node_ttfb__ = None;
                let mut e2e_latency__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::StartedSpeakingAt => {
                            if started_speaking_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startedSpeakingAt"));
                            }
                            started_speaking_at__ = map_.next_value()?;
                        }
                        GeneratedField::StoppedSpeakingAt => {
                            if stopped_speaking_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("stoppedSpeakingAt"));
                            }
                            stopped_speaking_at__ = map_.next_value()?;
                        }
                        GeneratedField::TranscriptionDelay => {
                            if transcription_delay__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transcriptionDelay"));
                            }
                            transcription_delay__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::EndOfTurnDelay => {
                            if end_of_turn_delay__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endOfTurnDelay"));
                            }
                            end_of_turn_delay__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::OnUserTurnCompletedDelay => {
                            if on_user_turn_completed_delay__.is_some() {
                                return Err(serde::de::Error::duplicate_field("onUserTurnCompletedDelay"));
                            }
                            on_user_turn_completed_delay__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::LlmNodeTtft => {
                            if llm_node_ttft__.is_some() {
                                return Err(serde::de::Error::duplicate_field("llmNodeTtft"));
                            }
                            llm_node_ttft__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::TtsNodeTtfb => {
                            if tts_node_ttfb__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ttsNodeTtfb"));
                            }
                            tts_node_ttfb__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::E2eLatency => {
                            if e2e_latency__.is_some() {
                                return Err(serde::de::Error::duplicate_field("e2eLatency"));
                            }
                            e2e_latency__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(MetricsReport {
                    started_speaking_at: started_speaking_at__,
                    stopped_speaking_at: stopped_speaking_at__,
                    transcription_delay: transcription_delay__,
                    end_of_turn_delay: end_of_turn_delay__,
                    on_user_turn_completed_delay: on_user_turn_completed_delay__,
                    llm_node_ttft: llm_node_ttft__,
                    tts_node_ttfb: tts_node_ttfb__,
                    e2e_latency: e2e_latency__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.MetricsReport", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ModelUsage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.usage.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.ModelUsage", len)?;
        if let Some(v) = self.usage.as_ref() {
            match v {
                model_usage::Usage::Llm(v) => {
                    struct_ser.serialize_field("llm", v)?;
                }
                model_usage::Usage::Tts(v) => {
                    struct_ser.serialize_field("tts", v)?;
                }
                model_usage::Usage::Stt(v) => {
                    struct_ser.serialize_field("stt", v)?;
                }
                model_usage::Usage::Interruption(v) => {
                    struct_ser.serialize_field("interruption", v)?;
                }
                model_usage::Usage::Eot(v) => {
                    struct_ser.serialize_field("eot", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ModelUsage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "llm",
            "tts",
            "stt",
            "interruption",
            "eot",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Llm,
            Tts,
            Stt,
            Interruption,
            Eot,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "llm" => Ok(GeneratedField::Llm),
                            "tts" => Ok(GeneratedField::Tts),
                            "stt" => Ok(GeneratedField::Stt),
                            "interruption" => Ok(GeneratedField::Interruption),
                            "eot" => Ok(GeneratedField::Eot),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ModelUsage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.ModelUsage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ModelUsage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut usage__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Llm => {
                            if usage__.is_some() {
                                return Err(serde::de::Error::duplicate_field("llm"));
                            }
                            usage__ = map_.next_value::<::std::option::Option<_>>()?.map(model_usage::Usage::Llm)
;
                        }
                        GeneratedField::Tts => {
                            if usage__.is_some() {
                                return Err(serde::de::Error::duplicate_field("tts"));
                            }
                            usage__ = map_.next_value::<::std::option::Option<_>>()?.map(model_usage::Usage::Tts)
;
                        }
                        GeneratedField::Stt => {
                            if usage__.is_some() {
                                return Err(serde::de::Error::duplicate_field("stt"));
                            }
                            usage__ = map_.next_value::<::std::option::Option<_>>()?.map(model_usage::Usage::Stt)
;
                        }
                        GeneratedField::Interruption => {
                            if usage__.is_some() {
                                return Err(serde::de::Error::duplicate_field("interruption"));
                            }
                            usage__ = map_.next_value::<::std::option::Option<_>>()?.map(model_usage::Usage::Interruption)
;
                        }
                        GeneratedField::Eot => {
                            if usage__.is_some() {
                                return Err(serde::de::Error::duplicate_field("eot"));
                            }
                            usage__ = map_.next_value::<::std::option::Option<_>>()?.map(model_usage::Usage::Eot)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ModelUsage {
                    usage: usage__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.ModelUsage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for RunningAgentJobInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.job.is_empty() {
            len += 1;
        }
        if !self.accept_name.is_empty() {
            len += 1;
        }
        if !self.accept_identity.is_empty() {
            len += 1;
        }
        if !self.accept_metadata.is_empty() {
            len += 1;
        }
        if !self.url.is_empty() {
            len += 1;
        }
        if !self.token.is_empty() {
            len += 1;
        }
        if !self.worker_id.is_empty() {
            len += 1;
        }
        if self.mock_job {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.RunningAgentJobInfo", len)?;
        if !self.job.is_empty() {
            #[allow(clippy::needless_borrow)]
            #[allow(clippy::needless_borrows_for_generic_args)]
            struct_ser.serialize_field("job", pbjson::private::base64::encode(&self.job).as_str())?;
        }
        if !self.accept_name.is_empty() {
            struct_ser.serialize_field("acceptName", &self.accept_name)?;
        }
        if !self.accept_identity.is_empty() {
            struct_ser.serialize_field("acceptIdentity", &self.accept_identity)?;
        }
        if !self.accept_metadata.is_empty() {
            struct_ser.serialize_field("acceptMetadata", &self.accept_metadata)?;
        }
        if !self.url.is_empty() {
            struct_ser.serialize_field("url", &self.url)?;
        }
        if !self.token.is_empty() {
            struct_ser.serialize_field("token", &self.token)?;
        }
        if !self.worker_id.is_empty() {
            struct_ser.serialize_field("workerId", &self.worker_id)?;
        }
        if self.mock_job {
            struct_ser.serialize_field("mockJob", &self.mock_job)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for RunningAgentJobInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "job",
            "accept_name",
            "acceptName",
            "accept_identity",
            "acceptIdentity",
            "accept_metadata",
            "acceptMetadata",
            "url",
            "token",
            "worker_id",
            "workerId",
            "mock_job",
            "mockJob",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Job,
            AcceptName,
            AcceptIdentity,
            AcceptMetadata,
            Url,
            Token,
            WorkerId,
            MockJob,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "job" => Ok(GeneratedField::Job),
                            "acceptName" | "accept_name" => Ok(GeneratedField::AcceptName),
                            "acceptIdentity" | "accept_identity" => Ok(GeneratedField::AcceptIdentity),
                            "acceptMetadata" | "accept_metadata" => Ok(GeneratedField::AcceptMetadata),
                            "url" => Ok(GeneratedField::Url),
                            "token" => Ok(GeneratedField::Token),
                            "workerId" | "worker_id" => Ok(GeneratedField::WorkerId),
                            "mockJob" | "mock_job" => Ok(GeneratedField::MockJob),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = RunningAgentJobInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.RunningAgentJobInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<RunningAgentJobInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut job__ = None;
                let mut accept_name__ = None;
                let mut accept_identity__ = None;
                let mut accept_metadata__ = None;
                let mut url__ = None;
                let mut token__ = None;
                let mut worker_id__ = None;
                let mut mock_job__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Job => {
                            if job__.is_some() {
                                return Err(serde::de::Error::duplicate_field("job"));
                            }
                            job__ = 
                                Some(map_.next_value::<::pbjson::private::BytesDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AcceptName => {
                            if accept_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("acceptName"));
                            }
                            accept_name__ = Some(map_.next_value()?);
                        }
                        GeneratedField::AcceptIdentity => {
                            if accept_identity__.is_some() {
                                return Err(serde::de::Error::duplicate_field("acceptIdentity"));
                            }
                            accept_identity__ = Some(map_.next_value()?);
                        }
                        GeneratedField::AcceptMetadata => {
                            if accept_metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("acceptMetadata"));
                            }
                            accept_metadata__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Url => {
                            if url__.is_some() {
                                return Err(serde::de::Error::duplicate_field("url"));
                            }
                            url__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Token => {
                            if token__.is_some() {
                                return Err(serde::de::Error::duplicate_field("token"));
                            }
                            token__ = Some(map_.next_value()?);
                        }
                        GeneratedField::WorkerId => {
                            if worker_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("workerId"));
                            }
                            worker_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::MockJob => {
                            if mock_job__.is_some() {
                                return Err(serde::de::Error::duplicate_field("mockJob"));
                            }
                            mock_job__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(RunningAgentJobInfo {
                    job: job__.unwrap_or_default(),
                    accept_name: accept_name__.unwrap_or_default(),
                    accept_identity: accept_identity__.unwrap_or_default(),
                    accept_metadata: accept_metadata__.unwrap_or_default(),
                    url: url__.unwrap_or_default(),
                    token: token__.unwrap_or_default(),
                    worker_id: worker_id__.unwrap_or_default(),
                    mock_job: mock_job__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.RunningAgentJobInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SttModelUsage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.provider.is_empty() {
            len += 1;
        }
        if !self.model.is_empty() {
            len += 1;
        }
        if self.input_tokens != 0 {
            len += 1;
        }
        if self.output_tokens != 0 {
            len += 1;
        }
        if self.audio_duration != 0. {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.STTModelUsage", len)?;
        if !self.provider.is_empty() {
            struct_ser.serialize_field("provider", &self.provider)?;
        }
        if !self.model.is_empty() {
            struct_ser.serialize_field("model", &self.model)?;
        }
        if self.input_tokens != 0 {
            struct_ser.serialize_field("inputTokens", &self.input_tokens)?;
        }
        if self.output_tokens != 0 {
            struct_ser.serialize_field("outputTokens", &self.output_tokens)?;
        }
        if self.audio_duration != 0. {
            struct_ser.serialize_field("audioDuration", &self.audio_duration)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SttModelUsage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "provider",
            "model",
            "input_tokens",
            "inputTokens",
            "output_tokens",
            "outputTokens",
            "audio_duration",
            "audioDuration",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Provider,
            Model,
            InputTokens,
            OutputTokens,
            AudioDuration,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "provider" => Ok(GeneratedField::Provider),
                            "model" => Ok(GeneratedField::Model),
                            "inputTokens" | "input_tokens" => Ok(GeneratedField::InputTokens),
                            "outputTokens" | "output_tokens" => Ok(GeneratedField::OutputTokens),
                            "audioDuration" | "audio_duration" => Ok(GeneratedField::AudioDuration),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SttModelUsage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.STTModelUsage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SttModelUsage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut provider__ = None;
                let mut model__ = None;
                let mut input_tokens__ = None;
                let mut output_tokens__ = None;
                let mut audio_duration__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Provider => {
                            if provider__.is_some() {
                                return Err(serde::de::Error::duplicate_field("provider"));
                            }
                            provider__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Model => {
                            if model__.is_some() {
                                return Err(serde::de::Error::duplicate_field("model"));
                            }
                            model__ = Some(map_.next_value()?);
                        }
                        GeneratedField::InputTokens => {
                            if input_tokens__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inputTokens"));
                            }
                            input_tokens__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::OutputTokens => {
                            if output_tokens__.is_some() {
                                return Err(serde::de::Error::duplicate_field("outputTokens"));
                            }
                            output_tokens__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AudioDuration => {
                            if audio_duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioDuration"));
                            }
                            audio_duration__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SttModelUsage {
                    provider: provider__.unwrap_or_default(),
                    model: model__.unwrap_or_default(),
                    input_tokens: input_tokens__.unwrap_or_default(),
                    output_tokens: output_tokens__.unwrap_or_default(),
                    audio_duration: audio_duration__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.STTModelUsage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ServerInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.agent_name.is_empty() {
            len += 1;
        }
        if !self.url.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.ServerInfo", len)?;
        if !self.agent_name.is_empty() {
            struct_ser.serialize_field("agentName", &self.agent_name)?;
        }
        if !self.url.is_empty() {
            struct_ser.serialize_field("url", &self.url)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ServerInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "agent_name",
            "agentName",
            "url",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AgentName,
            Url,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "agentName" | "agent_name" => Ok(GeneratedField::AgentName),
                            "url" => Ok(GeneratedField::Url),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ServerInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.ServerInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ServerInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut agent_name__ = None;
                let mut url__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AgentName => {
                            if agent_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("agentName"));
                            }
                            agent_name__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Url => {
                            if url__.is_some() {
                                return Err(serde::de::Error::duplicate_field("url"));
                            }
                            url__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ServerInfo {
                    agent_name: agent_name__.unwrap_or_default(),
                    url: url__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.ServerInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ServerMessage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.server_created_at.is_some() {
            len += 1;
        }
        if self.request_id.is_some() {
            len += 1;
        }
        if self.client_created_at.is_some() {
            len += 1;
        }
        if self.message.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.ServerMessage", len)?;
        if let Some(v) = self.server_created_at.as_ref() {
            struct_ser.serialize_field("serverCreatedAt", v)?;
        }
        if let Some(v) = self.request_id.as_ref() {
            struct_ser.serialize_field("requestId", v)?;
        }
        if let Some(v) = self.client_created_at.as_ref() {
            struct_ser.serialize_field("clientCreatedAt", v)?;
        }
        if let Some(v) = self.message.as_ref() {
            match v {
                server_message::Message::SessionCreated(v) => {
                    struct_ser.serialize_field("sessionCreated", v)?;
                }
                server_message::Message::InferenceStarted(v) => {
                    struct_ser.serialize_field("inferenceStarted", v)?;
                }
                server_message::Message::InferenceStopped(v) => {
                    struct_ser.serialize_field("inferenceStopped", v)?;
                }
                server_message::Message::SessionClosed(v) => {
                    struct_ser.serialize_field("sessionClosed", v)?;
                }
                server_message::Message::Error(v) => {
                    struct_ser.serialize_field("error", v)?;
                }
                server_message::Message::EotPrediction(v) => {
                    struct_ser.serialize_field("eotPrediction", v)?;
                }
                server_message::Message::InterruptionPrediction(v) => {
                    struct_ser.serialize_field("interruptionPrediction", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for ServerMessage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "server_created_at",
            "serverCreatedAt",
            "request_id",
            "requestId",
            "client_created_at",
            "clientCreatedAt",
            "session_created",
            "sessionCreated",
            "inference_started",
            "inferenceStarted",
            "inference_stopped",
            "inferenceStopped",
            "session_closed",
            "sessionClosed",
            "error",
            "eot_prediction",
            "eotPrediction",
            "interruption_prediction",
            "interruptionPrediction",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ServerCreatedAt,
            RequestId,
            ClientCreatedAt,
            SessionCreated,
            InferenceStarted,
            InferenceStopped,
            SessionClosed,
            Error,
            EotPrediction,
            InterruptionPrediction,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "serverCreatedAt" | "server_created_at" => Ok(GeneratedField::ServerCreatedAt),
                            "requestId" | "request_id" => Ok(GeneratedField::RequestId),
                            "clientCreatedAt" | "client_created_at" => Ok(GeneratedField::ClientCreatedAt),
                            "sessionCreated" | "session_created" => Ok(GeneratedField::SessionCreated),
                            "inferenceStarted" | "inference_started" => Ok(GeneratedField::InferenceStarted),
                            "inferenceStopped" | "inference_stopped" => Ok(GeneratedField::InferenceStopped),
                            "sessionClosed" | "session_closed" => Ok(GeneratedField::SessionClosed),
                            "error" => Ok(GeneratedField::Error),
                            "eotPrediction" | "eot_prediction" => Ok(GeneratedField::EotPrediction),
                            "interruptionPrediction" | "interruption_prediction" => Ok(GeneratedField::InterruptionPrediction),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ServerMessage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.ServerMessage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<ServerMessage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut server_created_at__ = None;
                let mut request_id__ = None;
                let mut client_created_at__ = None;
                let mut message__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ServerCreatedAt => {
                            if server_created_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("serverCreatedAt"));
                            }
                            server_created_at__ = map_.next_value()?;
                        }
                        GeneratedField::RequestId => {
                            if request_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requestId"));
                            }
                            request_id__ = map_.next_value()?;
                        }
                        GeneratedField::ClientCreatedAt => {
                            if client_created_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("clientCreatedAt"));
                            }
                            client_created_at__ = map_.next_value()?;
                        }
                        GeneratedField::SessionCreated => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sessionCreated"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(server_message::Message::SessionCreated)
;
                        }
                        GeneratedField::InferenceStarted => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inferenceStarted"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(server_message::Message::InferenceStarted)
;
                        }
                        GeneratedField::InferenceStopped => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inferenceStopped"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(server_message::Message::InferenceStopped)
;
                        }
                        GeneratedField::SessionClosed => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sessionClosed"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(server_message::Message::SessionClosed)
;
                        }
                        GeneratedField::Error => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("error"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(server_message::Message::Error)
;
                        }
                        GeneratedField::EotPrediction => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("eotPrediction"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(server_message::Message::EotPrediction)
;
                        }
                        GeneratedField::InterruptionPrediction => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("interruptionPrediction"));
                            }
                            message__ = map_.next_value::<::std::option::Option<_>>()?.map(server_message::Message::InterruptionPrediction)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(ServerMessage {
                    server_created_at: server_created_at__,
                    request_id: request_id__,
                    client_created_at: client_created_at__,
                    message: message__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.ServerMessage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SessionClose {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.SessionClose", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SessionClose {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SessionClose;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionClose")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SessionClose, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(SessionClose {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionClose", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SessionClosed {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.SessionClosed", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SessionClosed {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SessionClosed;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionClosed")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SessionClosed, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(SessionClosed {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionClosed", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SessionCreate {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.settings.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionCreate", len)?;
        if let Some(v) = self.settings.as_ref() {
            struct_ser.serialize_field("settings", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SessionCreate {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "settings",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Settings,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "settings" => Ok(GeneratedField::Settings),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SessionCreate;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionCreate")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SessionCreate, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut settings__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Settings => {
                            if settings__.is_some() {
                                return Err(serde::de::Error::duplicate_field("settings"));
                            }
                            settings__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SessionCreate {
                    settings: settings__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionCreate", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SessionCreated {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.default_thresholds.is_empty() {
            len += 1;
        }
        if self.default_threshold != 0. {
            len += 1;
        }
        if !self.default_backchannel_thresholds.is_empty() {
            len += 1;
        }
        if self.default_backchannel_threshold != 0. {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionCreated", len)?;
        if !self.default_thresholds.is_empty() {
            struct_ser.serialize_field("defaultThresholds", &self.default_thresholds)?;
        }
        if self.default_threshold != 0. {
            struct_ser.serialize_field("defaultThreshold", &self.default_threshold)?;
        }
        if !self.default_backchannel_thresholds.is_empty() {
            struct_ser.serialize_field("defaultBackchannelThresholds", &self.default_backchannel_thresholds)?;
        }
        if self.default_backchannel_threshold != 0. {
            struct_ser.serialize_field("defaultBackchannelThreshold", &self.default_backchannel_threshold)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SessionCreated {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "default_thresholds",
            "defaultThresholds",
            "default_threshold",
            "defaultThreshold",
            "default_backchannel_thresholds",
            "defaultBackchannelThresholds",
            "default_backchannel_threshold",
            "defaultBackchannelThreshold",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            DefaultThresholds,
            DefaultThreshold,
            DefaultBackchannelThresholds,
            DefaultBackchannelThreshold,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "defaultThresholds" | "default_thresholds" => Ok(GeneratedField::DefaultThresholds),
                            "defaultThreshold" | "default_threshold" => Ok(GeneratedField::DefaultThreshold),
                            "defaultBackchannelThresholds" | "default_backchannel_thresholds" => Ok(GeneratedField::DefaultBackchannelThresholds),
                            "defaultBackchannelThreshold" | "default_backchannel_threshold" => Ok(GeneratedField::DefaultBackchannelThreshold),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SessionCreated;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionCreated")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SessionCreated, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut default_thresholds__ = None;
                let mut default_threshold__ = None;
                let mut default_backchannel_thresholds__ = None;
                let mut default_backchannel_threshold__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::DefaultThresholds => {
                            if default_thresholds__.is_some() {
                                return Err(serde::de::Error::duplicate_field("defaultThresholds"));
                            }
                            default_thresholds__ = Some(
                                map_.next_value::<std::collections::HashMap<_, ::pbjson::private::NumberDeserialize<f32>>>()?
                                    .into_iter().map(|(k,v)| (k, v.0)).collect()
                            );
                        }
                        GeneratedField::DefaultThreshold => {
                            if default_threshold__.is_some() {
                                return Err(serde::de::Error::duplicate_field("defaultThreshold"));
                            }
                            default_threshold__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::DefaultBackchannelThresholds => {
                            if default_backchannel_thresholds__.is_some() {
                                return Err(serde::de::Error::duplicate_field("defaultBackchannelThresholds"));
                            }
                            default_backchannel_thresholds__ = Some(
                                map_.next_value::<std::collections::HashMap<_, ::pbjson::private::NumberDeserialize<f32>>>()?
                                    .into_iter().map(|(k,v)| (k, v.0)).collect()
                            );
                        }
                        GeneratedField::DefaultBackchannelThreshold => {
                            if default_backchannel_threshold__.is_some() {
                                return Err(serde::de::Error::duplicate_field("defaultBackchannelThreshold"));
                            }
                            default_backchannel_threshold__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SessionCreated {
                    default_thresholds: default_thresholds__.unwrap_or_default(),
                    default_threshold: default_threshold__.unwrap_or_default(),
                    default_backchannel_thresholds: default_backchannel_thresholds__.unwrap_or_default(),
                    default_backchannel_threshold: default_backchannel_threshold__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionCreated", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SessionFlush {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.SessionFlush", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SessionFlush {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SessionFlush;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionFlush")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SessionFlush, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(SessionFlush {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionFlush", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SessionRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.request_id.is_empty() {
            len += 1;
        }
        if self.request.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionRequest", len)?;
        if !self.request_id.is_empty() {
            struct_ser.serialize_field("requestId", &self.request_id)?;
        }
        if let Some(v) = self.request.as_ref() {
            match v {
                session_request::Request::Ping(v) => {
                    struct_ser.serialize_field("ping", v)?;
                }
                session_request::Request::GetChatHistory(v) => {
                    struct_ser.serialize_field("getChatHistory", v)?;
                }
                session_request::Request::RunInput(v) => {
                    struct_ser.serialize_field("runInput", v)?;
                }
                session_request::Request::GetAgentInfo(v) => {
                    struct_ser.serialize_field("getAgentInfo", v)?;
                }
                session_request::Request::GetSessionState(v) => {
                    struct_ser.serialize_field("getSessionState", v)?;
                }
                session_request::Request::GetRtcStats(v) => {
                    struct_ser.serialize_field("getRtcStats", v)?;
                }
                session_request::Request::GetSessionUsage(v) => {
                    struct_ser.serialize_field("getSessionUsage", v)?;
                }
                session_request::Request::GetFrameworkInfo(v) => {
                    struct_ser.serialize_field("getFrameworkInfo", v)?;
                }
                session_request::Request::UpdateIo(v) => {
                    struct_ser.serialize_field("updateIo", v)?;
                }
                session_request::Request::FinalizeSimulation(v) => {
                    struct_ser.serialize_field("finalizeSimulation", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SessionRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "request_id",
            "requestId",
            "ping",
            "get_chat_history",
            "getChatHistory",
            "run_input",
            "runInput",
            "get_agent_info",
            "getAgentInfo",
            "get_session_state",
            "getSessionState",
            "get_rtc_stats",
            "getRtcStats",
            "get_session_usage",
            "getSessionUsage",
            "get_framework_info",
            "getFrameworkInfo",
            "update_io",
            "updateIo",
            "finalize_simulation",
            "finalizeSimulation",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RequestId,
            Ping,
            GetChatHistory,
            RunInput,
            GetAgentInfo,
            GetSessionState,
            GetRtcStats,
            GetSessionUsage,
            GetFrameworkInfo,
            UpdateIo,
            FinalizeSimulation,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "requestId" | "request_id" => Ok(GeneratedField::RequestId),
                            "ping" => Ok(GeneratedField::Ping),
                            "getChatHistory" | "get_chat_history" => Ok(GeneratedField::GetChatHistory),
                            "runInput" | "run_input" => Ok(GeneratedField::RunInput),
                            "getAgentInfo" | "get_agent_info" => Ok(GeneratedField::GetAgentInfo),
                            "getSessionState" | "get_session_state" => Ok(GeneratedField::GetSessionState),
                            "getRtcStats" | "get_rtc_stats" => Ok(GeneratedField::GetRtcStats),
                            "getSessionUsage" | "get_session_usage" => Ok(GeneratedField::GetSessionUsage),
                            "getFrameworkInfo" | "get_framework_info" => Ok(GeneratedField::GetFrameworkInfo),
                            "updateIo" | "update_io" => Ok(GeneratedField::UpdateIo),
                            "finalizeSimulation" | "finalize_simulation" => Ok(GeneratedField::FinalizeSimulation),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SessionRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SessionRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut request_id__ = None;
                let mut request__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::RequestId => {
                            if request_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requestId"));
                            }
                            request_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Ping => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("ping"));
                            }
                            request__ = map_.next_value::<::std::option::Option<_>>()?.map(session_request::Request::Ping)
;
                        }
                        GeneratedField::GetChatHistory => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("getChatHistory"));
                            }
                            request__ = map_.next_value::<::std::option::Option<_>>()?.map(session_request::Request::GetChatHistory)
;
                        }
                        GeneratedField::RunInput => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("runInput"));
                            }
                            request__ = map_.next_value::<::std::option::Option<_>>()?.map(session_request::Request::RunInput)
;
                        }
                        GeneratedField::GetAgentInfo => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("getAgentInfo"));
                            }
                            request__ = map_.next_value::<::std::option::Option<_>>()?.map(session_request::Request::GetAgentInfo)
;
                        }
                        GeneratedField::GetSessionState => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("getSessionState"));
                            }
                            request__ = map_.next_value::<::std::option::Option<_>>()?.map(session_request::Request::GetSessionState)
;
                        }
                        GeneratedField::GetRtcStats => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("getRtcStats"));
                            }
                            request__ = map_.next_value::<::std::option::Option<_>>()?.map(session_request::Request::GetRtcStats)
;
                        }
                        GeneratedField::GetSessionUsage => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("getSessionUsage"));
                            }
                            request__ = map_.next_value::<::std::option::Option<_>>()?.map(session_request::Request::GetSessionUsage)
;
                        }
                        GeneratedField::GetFrameworkInfo => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("getFrameworkInfo"));
                            }
                            request__ = map_.next_value::<::std::option::Option<_>>()?.map(session_request::Request::GetFrameworkInfo)
;
                        }
                        GeneratedField::UpdateIo => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("updateIo"));
                            }
                            request__ = map_.next_value::<::std::option::Option<_>>()?.map(session_request::Request::UpdateIo)
;
                        }
                        GeneratedField::FinalizeSimulation => {
                            if request__.is_some() {
                                return Err(serde::de::Error::duplicate_field("finalizeSimulation"));
                            }
                            request__ = map_.next_value::<::std::option::Option<_>>()?.map(session_request::Request::FinalizeSimulation)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SessionRequest {
                    request_id: request_id__.unwrap_or_default(),
                    request: request__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_request::FinalizeSimulation {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.provisional_success {
            len += 1;
        }
        if !self.provisional_reason.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionRequest.FinalizeSimulation", len)?;
        if self.provisional_success {
            struct_ser.serialize_field("provisionalSuccess", &self.provisional_success)?;
        }
        if !self.provisional_reason.is_empty() {
            struct_ser.serialize_field("provisionalReason", &self.provisional_reason)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_request::FinalizeSimulation {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "provisional_success",
            "provisionalSuccess",
            "provisional_reason",
            "provisionalReason",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            ProvisionalSuccess,
            ProvisionalReason,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "provisionalSuccess" | "provisional_success" => Ok(GeneratedField::ProvisionalSuccess),
                            "provisionalReason" | "provisional_reason" => Ok(GeneratedField::ProvisionalReason),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_request::FinalizeSimulation;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionRequest.FinalizeSimulation")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_request::FinalizeSimulation, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut provisional_success__ = None;
                let mut provisional_reason__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::ProvisionalSuccess => {
                            if provisional_success__.is_some() {
                                return Err(serde::de::Error::duplicate_field("provisionalSuccess"));
                            }
                            provisional_success__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ProvisionalReason => {
                            if provisional_reason__.is_some() {
                                return Err(serde::de::Error::duplicate_field("provisionalReason"));
                            }
                            provisional_reason__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(session_request::FinalizeSimulation {
                    provisional_success: provisional_success__.unwrap_or_default(),
                    provisional_reason: provisional_reason__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionRequest.FinalizeSimulation", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_request::GetAgentInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.SessionRequest.GetAgentInfo", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_request::GetAgentInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_request::GetAgentInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionRequest.GetAgentInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_request::GetAgentInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(session_request::GetAgentInfo {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionRequest.GetAgentInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_request::GetChatHistory {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.SessionRequest.GetChatHistory", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_request::GetChatHistory {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_request::GetChatHistory;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionRequest.GetChatHistory")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_request::GetChatHistory, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(session_request::GetChatHistory {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionRequest.GetChatHistory", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_request::GetFrameworkInfo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.SessionRequest.GetFrameworkInfo", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_request::GetFrameworkInfo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_request::GetFrameworkInfo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionRequest.GetFrameworkInfo")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_request::GetFrameworkInfo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(session_request::GetFrameworkInfo {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionRequest.GetFrameworkInfo", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_request::GetRtcStats {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.SessionRequest.GetRTCStats", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_request::GetRtcStats {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_request::GetRtcStats;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionRequest.GetRTCStats")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_request::GetRtcStats, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(session_request::GetRtcStats {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionRequest.GetRTCStats", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_request::GetSessionState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.SessionRequest.GetSessionState", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_request::GetSessionState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_request::GetSessionState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionRequest.GetSessionState")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_request::GetSessionState, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(session_request::GetSessionState {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionRequest.GetSessionState", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_request::GetSessionUsage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.SessionRequest.GetSessionUsage", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_request::GetSessionUsage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_request::GetSessionUsage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionRequest.GetSessionUsage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_request::GetSessionUsage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(session_request::GetSessionUsage {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionRequest.GetSessionUsage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_request::Ping {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.SessionRequest.Ping", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_request::Ping {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_request::Ping;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionRequest.Ping")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_request::Ping, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(session_request::Ping {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionRequest.Ping", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_request::RunInput {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.text.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionRequest.RunInput", len)?;
        if !self.text.is_empty() {
            struct_ser.serialize_field("text", &self.text)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_request::RunInput {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "text",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Text,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "text" => Ok(GeneratedField::Text),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_request::RunInput;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionRequest.RunInput")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_request::RunInput, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut text__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Text => {
                            if text__.is_some() {
                                return Err(serde::de::Error::duplicate_field("text"));
                            }
                            text__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(session_request::RunInput {
                    text: text__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionRequest.RunInput", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_request::UpdateIo {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.input.is_some() {
            len += 1;
        }
        if self.output.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionRequest.UpdateIO", len)?;
        if let Some(v) = self.input.as_ref() {
            struct_ser.serialize_field("input", v)?;
        }
        if let Some(v) = self.output.as_ref() {
            struct_ser.serialize_field("output", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_request::UpdateIo {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "input",
            "output",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Input,
            Output,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "input" => Ok(GeneratedField::Input),
                            "output" => Ok(GeneratedField::Output),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_request::UpdateIo;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionRequest.UpdateIO")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_request::UpdateIo, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut input__ = None;
                let mut output__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Input => {
                            if input__.is_some() {
                                return Err(serde::de::Error::duplicate_field("input"));
                            }
                            input__ = map_.next_value()?;
                        }
                        GeneratedField::Output => {
                            if output__.is_some() {
                                return Err(serde::de::Error::duplicate_field("output"));
                            }
                            output__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(session_request::UpdateIo {
                    input: input__,
                    output: output__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionRequest.UpdateIO", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_request::update_io::Input {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.audio_enabled.is_some() {
            len += 1;
        }
        if self.video_enabled.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionRequest.UpdateIO.Input", len)?;
        if let Some(v) = self.audio_enabled.as_ref() {
            struct_ser.serialize_field("audioEnabled", v)?;
        }
        if let Some(v) = self.video_enabled.as_ref() {
            struct_ser.serialize_field("videoEnabled", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_request::update_io::Input {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "audio_enabled",
            "audioEnabled",
            "video_enabled",
            "videoEnabled",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AudioEnabled,
            VideoEnabled,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "audioEnabled" | "audio_enabled" => Ok(GeneratedField::AudioEnabled),
                            "videoEnabled" | "video_enabled" => Ok(GeneratedField::VideoEnabled),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_request::update_io::Input;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionRequest.UpdateIO.Input")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_request::update_io::Input, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut audio_enabled__ = None;
                let mut video_enabled__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AudioEnabled => {
                            if audio_enabled__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioEnabled"));
                            }
                            audio_enabled__ = map_.next_value()?;
                        }
                        GeneratedField::VideoEnabled => {
                            if video_enabled__.is_some() {
                                return Err(serde::de::Error::duplicate_field("videoEnabled"));
                            }
                            video_enabled__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(session_request::update_io::Input {
                    audio_enabled: audio_enabled__,
                    video_enabled: video_enabled__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionRequest.UpdateIO.Input", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_request::update_io::Output {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.audio_enabled.is_some() {
            len += 1;
        }
        if self.video_enabled.is_some() {
            len += 1;
        }
        if self.transcription_enabled.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionRequest.UpdateIO.Output", len)?;
        if let Some(v) = self.audio_enabled.as_ref() {
            struct_ser.serialize_field("audioEnabled", v)?;
        }
        if let Some(v) = self.video_enabled.as_ref() {
            struct_ser.serialize_field("videoEnabled", v)?;
        }
        if let Some(v) = self.transcription_enabled.as_ref() {
            struct_ser.serialize_field("transcriptionEnabled", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_request::update_io::Output {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "audio_enabled",
            "audioEnabled",
            "video_enabled",
            "videoEnabled",
            "transcription_enabled",
            "transcriptionEnabled",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AudioEnabled,
            VideoEnabled,
            TranscriptionEnabled,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "audioEnabled" | "audio_enabled" => Ok(GeneratedField::AudioEnabled),
                            "videoEnabled" | "video_enabled" => Ok(GeneratedField::VideoEnabled),
                            "transcriptionEnabled" | "transcription_enabled" => Ok(GeneratedField::TranscriptionEnabled),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_request::update_io::Output;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionRequest.UpdateIO.Output")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_request::update_io::Output, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut audio_enabled__ = None;
                let mut video_enabled__ = None;
                let mut transcription_enabled__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AudioEnabled => {
                            if audio_enabled__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioEnabled"));
                            }
                            audio_enabled__ = map_.next_value()?;
                        }
                        GeneratedField::VideoEnabled => {
                            if video_enabled__.is_some() {
                                return Err(serde::de::Error::duplicate_field("videoEnabled"));
                            }
                            video_enabled__ = map_.next_value()?;
                        }
                        GeneratedField::TranscriptionEnabled => {
                            if transcription_enabled__.is_some() {
                                return Err(serde::de::Error::duplicate_field("transcriptionEnabled"));
                            }
                            transcription_enabled__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(session_request::update_io::Output {
                    audio_enabled: audio_enabled__,
                    video_enabled: video_enabled__,
                    transcription_enabled: transcription_enabled__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionRequest.UpdateIO.Output", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SessionResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.request_id.is_empty() {
            len += 1;
        }
        if self.error.is_some() {
            len += 1;
        }
        if self.response.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionResponse", len)?;
        if !self.request_id.is_empty() {
            struct_ser.serialize_field("requestId", &self.request_id)?;
        }
        if let Some(v) = self.error.as_ref() {
            struct_ser.serialize_field("error", v)?;
        }
        if let Some(v) = self.response.as_ref() {
            match v {
                session_response::Response::Pong(v) => {
                    struct_ser.serialize_field("pong", v)?;
                }
                session_response::Response::GetChatHistory(v) => {
                    struct_ser.serialize_field("getChatHistory", v)?;
                }
                session_response::Response::RunInput(v) => {
                    struct_ser.serialize_field("runInput", v)?;
                }
                session_response::Response::GetAgentInfo(v) => {
                    struct_ser.serialize_field("getAgentInfo", v)?;
                }
                session_response::Response::GetSessionState(v) => {
                    struct_ser.serialize_field("getSessionState", v)?;
                }
                session_response::Response::GetRtcStats(v) => {
                    struct_ser.serialize_field("getRtcStats", v)?;
                }
                session_response::Response::GetSessionUsage(v) => {
                    struct_ser.serialize_field("getSessionUsage", v)?;
                }
                session_response::Response::GetFrameworkInfo(v) => {
                    struct_ser.serialize_field("getFrameworkInfo", v)?;
                }
                session_response::Response::UpdateIo(v) => {
                    struct_ser.serialize_field("updateIo", v)?;
                }
                session_response::Response::FinalizeSimulation(v) => {
                    struct_ser.serialize_field("finalizeSimulation", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SessionResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "request_id",
            "requestId",
            "error",
            "pong",
            "get_chat_history",
            "getChatHistory",
            "run_input",
            "runInput",
            "get_agent_info",
            "getAgentInfo",
            "get_session_state",
            "getSessionState",
            "get_rtc_stats",
            "getRtcStats",
            "get_session_usage",
            "getSessionUsage",
            "get_framework_info",
            "getFrameworkInfo",
            "update_io",
            "updateIo",
            "finalize_simulation",
            "finalizeSimulation",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            RequestId,
            Error,
            Pong,
            GetChatHistory,
            RunInput,
            GetAgentInfo,
            GetSessionState,
            GetRtcStats,
            GetSessionUsage,
            GetFrameworkInfo,
            UpdateIo,
            FinalizeSimulation,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "requestId" | "request_id" => Ok(GeneratedField::RequestId),
                            "error" => Ok(GeneratedField::Error),
                            "pong" => Ok(GeneratedField::Pong),
                            "getChatHistory" | "get_chat_history" => Ok(GeneratedField::GetChatHistory),
                            "runInput" | "run_input" => Ok(GeneratedField::RunInput),
                            "getAgentInfo" | "get_agent_info" => Ok(GeneratedField::GetAgentInfo),
                            "getSessionState" | "get_session_state" => Ok(GeneratedField::GetSessionState),
                            "getRtcStats" | "get_rtc_stats" => Ok(GeneratedField::GetRtcStats),
                            "getSessionUsage" | "get_session_usage" => Ok(GeneratedField::GetSessionUsage),
                            "getFrameworkInfo" | "get_framework_info" => Ok(GeneratedField::GetFrameworkInfo),
                            "updateIo" | "update_io" => Ok(GeneratedField::UpdateIo),
                            "finalizeSimulation" | "finalize_simulation" => Ok(GeneratedField::FinalizeSimulation),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SessionResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SessionResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut request_id__ = None;
                let mut error__ = None;
                let mut response__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::RequestId => {
                            if request_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("requestId"));
                            }
                            request_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Error => {
                            if error__.is_some() {
                                return Err(serde::de::Error::duplicate_field("error"));
                            }
                            error__ = map_.next_value()?;
                        }
                        GeneratedField::Pong => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("pong"));
                            }
                            response__ = map_.next_value::<::std::option::Option<_>>()?.map(session_response::Response::Pong)
;
                        }
                        GeneratedField::GetChatHistory => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("getChatHistory"));
                            }
                            response__ = map_.next_value::<::std::option::Option<_>>()?.map(session_response::Response::GetChatHistory)
;
                        }
                        GeneratedField::RunInput => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("runInput"));
                            }
                            response__ = map_.next_value::<::std::option::Option<_>>()?.map(session_response::Response::RunInput)
;
                        }
                        GeneratedField::GetAgentInfo => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("getAgentInfo"));
                            }
                            response__ = map_.next_value::<::std::option::Option<_>>()?.map(session_response::Response::GetAgentInfo)
;
                        }
                        GeneratedField::GetSessionState => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("getSessionState"));
                            }
                            response__ = map_.next_value::<::std::option::Option<_>>()?.map(session_response::Response::GetSessionState)
;
                        }
                        GeneratedField::GetRtcStats => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("getRtcStats"));
                            }
                            response__ = map_.next_value::<::std::option::Option<_>>()?.map(session_response::Response::GetRtcStats)
;
                        }
                        GeneratedField::GetSessionUsage => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("getSessionUsage"));
                            }
                            response__ = map_.next_value::<::std::option::Option<_>>()?.map(session_response::Response::GetSessionUsage)
;
                        }
                        GeneratedField::GetFrameworkInfo => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("getFrameworkInfo"));
                            }
                            response__ = map_.next_value::<::std::option::Option<_>>()?.map(session_response::Response::GetFrameworkInfo)
;
                        }
                        GeneratedField::UpdateIo => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("updateIo"));
                            }
                            response__ = map_.next_value::<::std::option::Option<_>>()?.map(session_response::Response::UpdateIo)
;
                        }
                        GeneratedField::FinalizeSimulation => {
                            if response__.is_some() {
                                return Err(serde::de::Error::duplicate_field("finalizeSimulation"));
                            }
                            response__ = map_.next_value::<::std::option::Option<_>>()?.map(session_response::Response::FinalizeSimulation)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SessionResponse {
                    request_id: request_id__.unwrap_or_default(),
                    error: error__,
                    response: response__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_response::FinalizeSimulationResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.user_verdict.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionResponse.FinalizeSimulationResponse", len)?;
        if let Some(v) = self.user_verdict.as_ref() {
            struct_ser.serialize_field("userVerdict", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_response::FinalizeSimulationResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "user_verdict",
            "userVerdict",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            UserVerdict,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "userVerdict" | "user_verdict" => Ok(GeneratedField::UserVerdict),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_response::FinalizeSimulationResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionResponse.FinalizeSimulationResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_response::FinalizeSimulationResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut user_verdict__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::UserVerdict => {
                            if user_verdict__.is_some() {
                                return Err(serde::de::Error::duplicate_field("userVerdict"));
                            }
                            user_verdict__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(session_response::FinalizeSimulationResponse {
                    user_verdict: user_verdict__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionResponse.FinalizeSimulationResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_response::finalize_simulation_response::SimulationVerdict {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.success {
            len += 1;
        }
        if !self.reason.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionResponse.FinalizeSimulationResponse.SimulationVerdict", len)?;
        if self.success {
            struct_ser.serialize_field("success", &self.success)?;
        }
        if !self.reason.is_empty() {
            struct_ser.serialize_field("reason", &self.reason)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_response::finalize_simulation_response::SimulationVerdict {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "success",
            "reason",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Success,
            Reason,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "success" => Ok(GeneratedField::Success),
                            "reason" => Ok(GeneratedField::Reason),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_response::finalize_simulation_response::SimulationVerdict;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionResponse.FinalizeSimulationResponse.SimulationVerdict")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_response::finalize_simulation_response::SimulationVerdict, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut success__ = None;
                let mut reason__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Success => {
                            if success__.is_some() {
                                return Err(serde::de::Error::duplicate_field("success"));
                            }
                            success__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Reason => {
                            if reason__.is_some() {
                                return Err(serde::de::Error::duplicate_field("reason"));
                            }
                            reason__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(session_response::finalize_simulation_response::SimulationVerdict {
                    success: success__.unwrap_or_default(),
                    reason: reason__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionResponse.FinalizeSimulationResponse.SimulationVerdict", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_response::GetAgentInfoResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.id.is_empty() {
            len += 1;
        }
        if self.instructions.is_some() {
            len += 1;
        }
        if !self.tools.is_empty() {
            len += 1;
        }
        if !self.chat_ctx.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionResponse.GetAgentInfoResponse", len)?;
        if !self.id.is_empty() {
            struct_ser.serialize_field("id", &self.id)?;
        }
        if let Some(v) = self.instructions.as_ref() {
            struct_ser.serialize_field("instructions", v)?;
        }
        if !self.tools.is_empty() {
            struct_ser.serialize_field("tools", &self.tools)?;
        }
        if !self.chat_ctx.is_empty() {
            struct_ser.serialize_field("chatCtx", &self.chat_ctx)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_response::GetAgentInfoResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "id",
            "instructions",
            "tools",
            "chat_ctx",
            "chatCtx",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Id,
            Instructions,
            Tools,
            ChatCtx,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "id" => Ok(GeneratedField::Id),
                            "instructions" => Ok(GeneratedField::Instructions),
                            "tools" => Ok(GeneratedField::Tools),
                            "chatCtx" | "chat_ctx" => Ok(GeneratedField::ChatCtx),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_response::GetAgentInfoResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionResponse.GetAgentInfoResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_response::GetAgentInfoResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut id__ = None;
                let mut instructions__ = None;
                let mut tools__ = None;
                let mut chat_ctx__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Id => {
                            if id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("id"));
                            }
                            id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Instructions => {
                            if instructions__.is_some() {
                                return Err(serde::de::Error::duplicate_field("instructions"));
                            }
                            instructions__ = map_.next_value()?;
                        }
                        GeneratedField::Tools => {
                            if tools__.is_some() {
                                return Err(serde::de::Error::duplicate_field("tools"));
                            }
                            tools__ = Some(map_.next_value()?);
                        }
                        GeneratedField::ChatCtx => {
                            if chat_ctx__.is_some() {
                                return Err(serde::de::Error::duplicate_field("chatCtx"));
                            }
                            chat_ctx__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(session_response::GetAgentInfoResponse {
                    id: id__.unwrap_or_default(),
                    instructions: instructions__,
                    tools: tools__.unwrap_or_default(),
                    chat_ctx: chat_ctx__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionResponse.GetAgentInfoResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_response::GetChatHistoryResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.items.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionResponse.GetChatHistoryResponse", len)?;
        if !self.items.is_empty() {
            struct_ser.serialize_field("items", &self.items)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_response::GetChatHistoryResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "items",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Items,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "items" => Ok(GeneratedField::Items),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_response::GetChatHistoryResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionResponse.GetChatHistoryResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_response::GetChatHistoryResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut items__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Items => {
                            if items__.is_some() {
                                return Err(serde::de::Error::duplicate_field("items"));
                            }
                            items__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(session_response::GetChatHistoryResponse {
                    items: items__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionResponse.GetChatHistoryResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_response::GetFrameworkInfoResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.sdk.is_empty() {
            len += 1;
        }
        if !self.sdk_version.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionResponse.GetFrameworkInfoResponse", len)?;
        if !self.sdk.is_empty() {
            struct_ser.serialize_field("sdk", &self.sdk)?;
        }
        if !self.sdk_version.is_empty() {
            struct_ser.serialize_field("sdkVersion", &self.sdk_version)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_response::GetFrameworkInfoResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "sdk",
            "sdk_version",
            "sdkVersion",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Sdk,
            SdkVersion,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "sdk" => Ok(GeneratedField::Sdk),
                            "sdkVersion" | "sdk_version" => Ok(GeneratedField::SdkVersion),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_response::GetFrameworkInfoResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionResponse.GetFrameworkInfoResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_response::GetFrameworkInfoResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut sdk__ = None;
                let mut sdk_version__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Sdk => {
                            if sdk__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sdk"));
                            }
                            sdk__ = Some(map_.next_value()?);
                        }
                        GeneratedField::SdkVersion => {
                            if sdk_version__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sdkVersion"));
                            }
                            sdk_version__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(session_response::GetFrameworkInfoResponse {
                    sdk: sdk__.unwrap_or_default(),
                    sdk_version: sdk_version__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionResponse.GetFrameworkInfoResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_response::GetRtcStatsResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.publisher_stats.is_empty() {
            len += 1;
        }
        if !self.subscriber_stats.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionResponse.GetRTCStatsResponse", len)?;
        if !self.publisher_stats.is_empty() {
            struct_ser.serialize_field("publisherStats", &self.publisher_stats)?;
        }
        if !self.subscriber_stats.is_empty() {
            struct_ser.serialize_field("subscriberStats", &self.subscriber_stats)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_response::GetRtcStatsResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "publisher_stats",
            "publisherStats",
            "subscriber_stats",
            "subscriberStats",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            PublisherStats,
            SubscriberStats,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "publisherStats" | "publisher_stats" => Ok(GeneratedField::PublisherStats),
                            "subscriberStats" | "subscriber_stats" => Ok(GeneratedField::SubscriberStats),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_response::GetRtcStatsResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionResponse.GetRTCStatsResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_response::GetRtcStatsResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut publisher_stats__ = None;
                let mut subscriber_stats__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::PublisherStats => {
                            if publisher_stats__.is_some() {
                                return Err(serde::de::Error::duplicate_field("publisherStats"));
                            }
                            publisher_stats__ = Some(map_.next_value()?);
                        }
                        GeneratedField::SubscriberStats => {
                            if subscriber_stats__.is_some() {
                                return Err(serde::de::Error::duplicate_field("subscriberStats"));
                            }
                            subscriber_stats__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(session_response::GetRtcStatsResponse {
                    publisher_stats: publisher_stats__.unwrap_or_default(),
                    subscriber_stats: subscriber_stats__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionResponse.GetRTCStatsResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_response::GetSessionStateResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.agent_state != 0 {
            len += 1;
        }
        if self.user_state != 0 {
            len += 1;
        }
        if !self.agent_id.is_empty() {
            len += 1;
        }
        if !self.options.is_empty() {
            len += 1;
        }
        if self.created_at.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionResponse.GetSessionStateResponse", len)?;
        if self.agent_state != 0 {
            let v = AgentState::try_from(self.agent_state)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.agent_state)))?;
            struct_ser.serialize_field("agentState", &v)?;
        }
        if self.user_state != 0 {
            let v = UserState::try_from(self.user_state)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.user_state)))?;
            struct_ser.serialize_field("userState", &v)?;
        }
        if !self.agent_id.is_empty() {
            struct_ser.serialize_field("agentId", &self.agent_id)?;
        }
        if !self.options.is_empty() {
            struct_ser.serialize_field("options", &self.options)?;
        }
        if let Some(v) = self.created_at.as_ref() {
            struct_ser.serialize_field("createdAt", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_response::GetSessionStateResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "agent_state",
            "agentState",
            "user_state",
            "userState",
            "agent_id",
            "agentId",
            "options",
            "created_at",
            "createdAt",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            AgentState,
            UserState,
            AgentId,
            Options,
            CreatedAt,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "agentState" | "agent_state" => Ok(GeneratedField::AgentState),
                            "userState" | "user_state" => Ok(GeneratedField::UserState),
                            "agentId" | "agent_id" => Ok(GeneratedField::AgentId),
                            "options" => Ok(GeneratedField::Options),
                            "createdAt" | "created_at" => Ok(GeneratedField::CreatedAt),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_response::GetSessionStateResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionResponse.GetSessionStateResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_response::GetSessionStateResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut agent_state__ = None;
                let mut user_state__ = None;
                let mut agent_id__ = None;
                let mut options__ = None;
                let mut created_at__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::AgentState => {
                            if agent_state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("agentState"));
                            }
                            agent_state__ = Some(map_.next_value::<AgentState>()? as i32);
                        }
                        GeneratedField::UserState => {
                            if user_state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("userState"));
                            }
                            user_state__ = Some(map_.next_value::<UserState>()? as i32);
                        }
                        GeneratedField::AgentId => {
                            if agent_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("agentId"));
                            }
                            agent_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Options => {
                            if options__.is_some() {
                                return Err(serde::de::Error::duplicate_field("options"));
                            }
                            options__ = Some(
                                map_.next_value::<std::collections::HashMap<_, _>>()?
                            );
                        }
                        GeneratedField::CreatedAt => {
                            if created_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdAt"));
                            }
                            created_at__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(session_response::GetSessionStateResponse {
                    agent_state: agent_state__.unwrap_or_default(),
                    user_state: user_state__.unwrap_or_default(),
                    agent_id: agent_id__.unwrap_or_default(),
                    options: options__.unwrap_or_default(),
                    created_at: created_at__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionResponse.GetSessionStateResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_response::GetSessionUsageResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.usage.is_some() {
            len += 1;
        }
        if self.created_at.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionResponse.GetSessionUsageResponse", len)?;
        if let Some(v) = self.usage.as_ref() {
            struct_ser.serialize_field("usage", v)?;
        }
        if let Some(v) = self.created_at.as_ref() {
            struct_ser.serialize_field("createdAt", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_response::GetSessionUsageResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "usage",
            "created_at",
            "createdAt",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Usage,
            CreatedAt,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "usage" => Ok(GeneratedField::Usage),
                            "createdAt" | "created_at" => Ok(GeneratedField::CreatedAt),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_response::GetSessionUsageResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionResponse.GetSessionUsageResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_response::GetSessionUsageResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut usage__ = None;
                let mut created_at__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Usage => {
                            if usage__.is_some() {
                                return Err(serde::de::Error::duplicate_field("usage"));
                            }
                            usage__ = map_.next_value()?;
                        }
                        GeneratedField::CreatedAt => {
                            if created_at__.is_some() {
                                return Err(serde::de::Error::duplicate_field("createdAt"));
                            }
                            created_at__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(session_response::GetSessionUsageResponse {
                    usage: usage__,
                    created_at: created_at__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionResponse.GetSessionUsageResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_response::Pong {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.SessionResponse.Pong", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_response::Pong {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_response::Pong;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionResponse.Pong")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_response::Pong, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(session_response::Pong {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionResponse.Pong", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_response::RunInputResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.items.is_empty() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionResponse.RunInputResponse", len)?;
        if !self.items.is_empty() {
            struct_ser.serialize_field("items", &self.items)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_response::RunInputResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "items",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Items,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "items" => Ok(GeneratedField::Items),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_response::RunInputResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionResponse.RunInputResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_response::RunInputResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut items__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Items => {
                            if items__.is_some() {
                                return Err(serde::de::Error::duplicate_field("items"));
                            }
                            items__ = Some(map_.next_value()?);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(session_response::RunInputResponse {
                    items: items__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionResponse.RunInputResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for session_response::UpdateIoResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let len = 0;
        let struct_ser = serializer.serialize_struct("livekit.agent.SessionResponse.UpdateIOResponse", len)?;
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for session_response::UpdateIoResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                            Ok(GeneratedField::__SkipField__)
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = session_response::UpdateIoResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionResponse.UpdateIOResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<session_response::UpdateIoResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                while map_.next_key::<GeneratedField>()?.is_some() {
                    let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                }
                Ok(session_response::UpdateIoResponse {
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionResponse.UpdateIOResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for SessionSettings {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.sample_rate != 0 {
            len += 1;
        }
        if self.encoding != 0 {
            len += 1;
        }
        if self.type_settings.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.SessionSettings", len)?;
        if self.sample_rate != 0 {
            struct_ser.serialize_field("sampleRate", &self.sample_rate)?;
        }
        if self.encoding != 0 {
            let v = AudioEncoding::try_from(self.encoding)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.encoding)))?;
            struct_ser.serialize_field("encoding", &v)?;
        }
        if let Some(v) = self.type_settings.as_ref() {
            match v {
                session_settings::TypeSettings::EotSettings(v) => {
                    struct_ser.serialize_field("eotSettings", v)?;
                }
                session_settings::TypeSettings::InterruptionSettings(v) => {
                    struct_ser.serialize_field("interruptionSettings", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for SessionSettings {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "sample_rate",
            "sampleRate",
            "encoding",
            "eot_settings",
            "eotSettings",
            "interruption_settings",
            "interruptionSettings",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            SampleRate,
            Encoding,
            EotSettings,
            InterruptionSettings,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "sampleRate" | "sample_rate" => Ok(GeneratedField::SampleRate),
                            "encoding" => Ok(GeneratedField::Encoding),
                            "eotSettings" | "eot_settings" => Ok(GeneratedField::EotSettings),
                            "interruptionSettings" | "interruption_settings" => Ok(GeneratedField::InterruptionSettings),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = SessionSettings;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.SessionSettings")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<SessionSettings, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut sample_rate__ = None;
                let mut encoding__ = None;
                let mut type_settings__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::SampleRate => {
                            if sample_rate__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sampleRate"));
                            }
                            sample_rate__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::Encoding => {
                            if encoding__.is_some() {
                                return Err(serde::de::Error::duplicate_field("encoding"));
                            }
                            encoding__ = Some(map_.next_value::<AudioEncoding>()? as i32);
                        }
                        GeneratedField::EotSettings => {
                            if type_settings__.is_some() {
                                return Err(serde::de::Error::duplicate_field("eotSettings"));
                            }
                            type_settings__ = map_.next_value::<::std::option::Option<_>>()?.map(session_settings::TypeSettings::EotSettings)
;
                        }
                        GeneratedField::InterruptionSettings => {
                            if type_settings__.is_some() {
                                return Err(serde::de::Error::duplicate_field("interruptionSettings"));
                            }
                            type_settings__ = map_.next_value::<::std::option::Option<_>>()?.map(session_settings::TypeSettings::InterruptionSettings)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(SessionSettings {
                    sample_rate: sample_rate__.unwrap_or_default(),
                    encoding: encoding__.unwrap_or_default(),
                    type_settings: type_settings__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.SessionSettings", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TtsModelUsage {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.provider.is_empty() {
            len += 1;
        }
        if !self.model.is_empty() {
            len += 1;
        }
        if self.input_tokens != 0 {
            len += 1;
        }
        if self.output_tokens != 0 {
            len += 1;
        }
        if self.characters_count != 0 {
            len += 1;
        }
        if self.audio_duration != 0. {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.TTSModelUsage", len)?;
        if !self.provider.is_empty() {
            struct_ser.serialize_field("provider", &self.provider)?;
        }
        if !self.model.is_empty() {
            struct_ser.serialize_field("model", &self.model)?;
        }
        if self.input_tokens != 0 {
            struct_ser.serialize_field("inputTokens", &self.input_tokens)?;
        }
        if self.output_tokens != 0 {
            struct_ser.serialize_field("outputTokens", &self.output_tokens)?;
        }
        if self.characters_count != 0 {
            struct_ser.serialize_field("charactersCount", &self.characters_count)?;
        }
        if self.audio_duration != 0. {
            struct_ser.serialize_field("audioDuration", &self.audio_duration)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TtsModelUsage {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "provider",
            "model",
            "input_tokens",
            "inputTokens",
            "output_tokens",
            "outputTokens",
            "characters_count",
            "charactersCount",
            "audio_duration",
            "audioDuration",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Provider,
            Model,
            InputTokens,
            OutputTokens,
            CharactersCount,
            AudioDuration,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "provider" => Ok(GeneratedField::Provider),
                            "model" => Ok(GeneratedField::Model),
                            "inputTokens" | "input_tokens" => Ok(GeneratedField::InputTokens),
                            "outputTokens" | "output_tokens" => Ok(GeneratedField::OutputTokens),
                            "charactersCount" | "characters_count" => Ok(GeneratedField::CharactersCount),
                            "audioDuration" | "audio_duration" => Ok(GeneratedField::AudioDuration),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TtsModelUsage;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.TTSModelUsage")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TtsModelUsage, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut provider__ = None;
                let mut model__ = None;
                let mut input_tokens__ = None;
                let mut output_tokens__ = None;
                let mut characters_count__ = None;
                let mut audio_duration__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Provider => {
                            if provider__.is_some() {
                                return Err(serde::de::Error::duplicate_field("provider"));
                            }
                            provider__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Model => {
                            if model__.is_some() {
                                return Err(serde::de::Error::duplicate_field("model"));
                            }
                            model__ = Some(map_.next_value()?);
                        }
                        GeneratedField::InputTokens => {
                            if input_tokens__.is_some() {
                                return Err(serde::de::Error::duplicate_field("inputTokens"));
                            }
                            input_tokens__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::OutputTokens => {
                            if output_tokens__.is_some() {
                                return Err(serde::de::Error::duplicate_field("outputTokens"));
                            }
                            output_tokens__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::CharactersCount => {
                            if characters_count__.is_some() {
                                return Err(serde::de::Error::duplicate_field("charactersCount"));
                            }
                            characters_count__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::AudioDuration => {
                            if audio_duration__.is_some() {
                                return Err(serde::de::Error::duplicate_field("audioDuration"));
                            }
                            audio_duration__ = 
                                Some(map_.next_value::<::pbjson::private::NumberDeserialize<_>>()?.0)
                            ;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TtsModelUsage {
                    provider: provider__.unwrap_or_default(),
                    model: model__.unwrap_or_default(),
                    input_tokens: input_tokens__.unwrap_or_default(),
                    output_tokens: output_tokens__.unwrap_or_default(),
                    characters_count: characters_count__.unwrap_or_default(),
                    audio_duration: audio_duration__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.TTSModelUsage", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TextMessageComplete {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if self.result.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.TextMessageComplete", len)?;
        if let Some(v) = self.result.as_ref() {
            match v {
                text_message_complete::Result::SessionState(v) => {
                    struct_ser.serialize_field("sessionState", v)?;
                }
                text_message_complete::Result::Error(v) => {
                    struct_ser.serialize_field("error", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TextMessageComplete {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "session_state",
            "sessionState",
            "error",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            SessionState,
            Error,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "sessionState" | "session_state" => Ok(GeneratedField::SessionState),
                            "error" => Ok(GeneratedField::Error),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TextMessageComplete;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.TextMessageComplete")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TextMessageComplete, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut result__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::SessionState => {
                            if result__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sessionState"));
                            }
                            result__ = map_.next_value::<::std::option::Option<_>>()?.map(text_message_complete::Result::SessionState)
;
                        }
                        GeneratedField::Error => {
                            if result__.is_some() {
                                return Err(serde::de::Error::duplicate_field("error"));
                            }
                            result__ = map_.next_value::<::std::option::Option<_>>()?.map(text_message_complete::Result::Error)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TextMessageComplete {
                    result: result__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.TextMessageComplete", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TextMessageError {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.message.is_empty() {
            len += 1;
        }
        if self.code != 0 {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.TextMessageError", len)?;
        if !self.message.is_empty() {
            struct_ser.serialize_field("message", &self.message)?;
        }
        if self.code != 0 {
            let v = TextMessageErrorCode::try_from(self.code)
                .map_err(|_| serde::ser::Error::custom(format!("Invalid variant {}", self.code)))?;
            struct_ser.serialize_field("code", &v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TextMessageError {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "message",
            "code",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Message,
            Code,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "message" => Ok(GeneratedField::Message),
                            "code" => Ok(GeneratedField::Code),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TextMessageError;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.TextMessageError")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TextMessageError, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut message__ = None;
                let mut code__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Message => {
                            if message__.is_some() {
                                return Err(serde::de::Error::duplicate_field("message"));
                            }
                            message__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Code => {
                            if code__.is_some() {
                                return Err(serde::de::Error::duplicate_field("code"));
                            }
                            code__ = Some(map_.next_value::<TextMessageErrorCode>()? as i32);
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TextMessageError {
                    message: message__.unwrap_or_default(),
                    code: code__.unwrap_or_default(),
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.TextMessageError", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TextMessageErrorCode {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::InternalError => "INTERNAL_ERROR",
            Self::SessionStateNotFound => "SESSION_STATE_NOT_FOUND",
            Self::TextHandlerError => "TEXT_HANDLER_ERROR",
            Self::ProcessClosed => "PROCESS_CLOSED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for TextMessageErrorCode {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "INTERNAL_ERROR",
            "SESSION_STATE_NOT_FOUND",
            "TEXT_HANDLER_ERROR",
            "PROCESS_CLOSED",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TextMessageErrorCode;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "INTERNAL_ERROR" => Ok(TextMessageErrorCode::InternalError),
                    "SESSION_STATE_NOT_FOUND" => Ok(TextMessageErrorCode::SessionStateNotFound),
                    "TEXT_HANDLER_ERROR" => Ok(TextMessageErrorCode::TextHandlerError),
                    "PROCESS_CLOSED" => Ok(TextMessageErrorCode::ProcessClosed),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for TextMessageRequest {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.text.is_empty() {
            len += 1;
        }
        if !self.message_id.is_empty() {
            len += 1;
        }
        if !self.session_id.is_empty() {
            len += 1;
        }
        if !self.agent_name.is_empty() {
            len += 1;
        }
        if !self.metadata.is_empty() {
            len += 1;
        }
        if self.session_state.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.TextMessageRequest", len)?;
        if !self.text.is_empty() {
            struct_ser.serialize_field("text", &self.text)?;
        }
        if !self.message_id.is_empty() {
            struct_ser.serialize_field("messageId", &self.message_id)?;
        }
        if !self.session_id.is_empty() {
            struct_ser.serialize_field("sessionId", &self.session_id)?;
        }
        if !self.agent_name.is_empty() {
            struct_ser.serialize_field("agentName", &self.agent_name)?;
        }
        if !self.metadata.is_empty() {
            struct_ser.serialize_field("metadata", &self.metadata)?;
        }
        if let Some(v) = self.session_state.as_ref() {
            struct_ser.serialize_field("sessionState", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TextMessageRequest {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "text",
            "message_id",
            "messageId",
            "session_id",
            "sessionId",
            "agent_name",
            "agentName",
            "metadata",
            "session_state",
            "sessionState",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Text,
            MessageId,
            SessionId,
            AgentName,
            Metadata,
            SessionState,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "text" => Ok(GeneratedField::Text),
                            "messageId" | "message_id" => Ok(GeneratedField::MessageId),
                            "sessionId" | "session_id" => Ok(GeneratedField::SessionId),
                            "agentName" | "agent_name" => Ok(GeneratedField::AgentName),
                            "metadata" => Ok(GeneratedField::Metadata),
                            "sessionState" | "session_state" => Ok(GeneratedField::SessionState),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TextMessageRequest;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.TextMessageRequest")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TextMessageRequest, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut text__ = None;
                let mut message_id__ = None;
                let mut session_id__ = None;
                let mut agent_name__ = None;
                let mut metadata__ = None;
                let mut session_state__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Text => {
                            if text__.is_some() {
                                return Err(serde::de::Error::duplicate_field("text"));
                            }
                            text__ = Some(map_.next_value()?);
                        }
                        GeneratedField::MessageId => {
                            if message_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("messageId"));
                            }
                            message_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::SessionId => {
                            if session_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sessionId"));
                            }
                            session_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::AgentName => {
                            if agent_name__.is_some() {
                                return Err(serde::de::Error::duplicate_field("agentName"));
                            }
                            agent_name__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Metadata => {
                            if metadata__.is_some() {
                                return Err(serde::de::Error::duplicate_field("metadata"));
                            }
                            metadata__ = Some(map_.next_value()?);
                        }
                        GeneratedField::SessionState => {
                            if session_state__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sessionState"));
                            }
                            session_state__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TextMessageRequest {
                    text: text__.unwrap_or_default(),
                    message_id: message_id__.unwrap_or_default(),
                    session_id: session_id__.unwrap_or_default(),
                    agent_name: agent_name__.unwrap_or_default(),
                    metadata: metadata__.unwrap_or_default(),
                    session_state: session_state__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.TextMessageRequest", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TextMessageResponse {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.message_id.is_empty() {
            len += 1;
        }
        if !self.session_id.is_empty() {
            len += 1;
        }
        if self.event.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.TextMessageResponse", len)?;
        if !self.message_id.is_empty() {
            struct_ser.serialize_field("messageId", &self.message_id)?;
        }
        if !self.session_id.is_empty() {
            struct_ser.serialize_field("sessionId", &self.session_id)?;
        }
        if let Some(v) = self.event.as_ref() {
            match v {
                text_message_response::Event::Message(v) => {
                    struct_ser.serialize_field("message", v)?;
                }
                text_message_response::Event::FunctionCall(v) => {
                    struct_ser.serialize_field("functionCall", v)?;
                }
                text_message_response::Event::FunctionCallOutput(v) => {
                    struct_ser.serialize_field("functionCallOutput", v)?;
                }
                text_message_response::Event::AgentHandoff(v) => {
                    struct_ser.serialize_field("agentHandoff", v)?;
                }
                text_message_response::Event::Complete(v) => {
                    struct_ser.serialize_field("complete", v)?;
                }
            }
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TextMessageResponse {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "message_id",
            "messageId",
            "session_id",
            "sessionId",
            "message",
            "function_call",
            "functionCall",
            "function_call_output",
            "functionCallOutput",
            "agent_handoff",
            "agentHandoff",
            "complete",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            MessageId,
            SessionId,
            Message,
            FunctionCall,
            FunctionCallOutput,
            AgentHandoff,
            Complete,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "messageId" | "message_id" => Ok(GeneratedField::MessageId),
                            "sessionId" | "session_id" => Ok(GeneratedField::SessionId),
                            "message" => Ok(GeneratedField::Message),
                            "functionCall" | "function_call" => Ok(GeneratedField::FunctionCall),
                            "functionCallOutput" | "function_call_output" => Ok(GeneratedField::FunctionCallOutput),
                            "agentHandoff" | "agent_handoff" => Ok(GeneratedField::AgentHandoff),
                            "complete" => Ok(GeneratedField::Complete),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TextMessageResponse;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.TextMessageResponse")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TextMessageResponse, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut message_id__ = None;
                let mut session_id__ = None;
                let mut event__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::MessageId => {
                            if message_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("messageId"));
                            }
                            message_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::SessionId => {
                            if session_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("sessionId"));
                            }
                            session_id__ = Some(map_.next_value()?);
                        }
                        GeneratedField::Message => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("message"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(text_message_response::Event::Message)
;
                        }
                        GeneratedField::FunctionCall => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("functionCall"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(text_message_response::Event::FunctionCall)
;
                        }
                        GeneratedField::FunctionCallOutput => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("functionCallOutput"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(text_message_response::Event::FunctionCallOutput)
;
                        }
                        GeneratedField::AgentHandoff => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("agentHandoff"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(text_message_response::Event::AgentHandoff)
;
                        }
                        GeneratedField::Complete => {
                            if event__.is_some() {
                                return Err(serde::de::Error::duplicate_field("complete"));
                            }
                            event__ = map_.next_value::<::std::option::Option<_>>()?.map(text_message_response::Event::Complete)
;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TextMessageResponse {
                    message_id: message_id__.unwrap_or_default(),
                    session_id: session_id__.unwrap_or_default(),
                    event: event__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.TextMessageResponse", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for TimedString {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut len = 0;
        if !self.text.is_empty() {
            len += 1;
        }
        if self.start_time.is_some() {
            len += 1;
        }
        if self.end_time.is_some() {
            len += 1;
        }
        if self.confidence.is_some() {
            len += 1;
        }
        if self.start_time_offset.is_some() {
            len += 1;
        }
        if self.speaker_id.is_some() {
            len += 1;
        }
        let mut struct_ser = serializer.serialize_struct("livekit.agent.TimedString", len)?;
        if !self.text.is_empty() {
            struct_ser.serialize_field("text", &self.text)?;
        }
        if let Some(v) = self.start_time.as_ref() {
            struct_ser.serialize_field("startTime", v)?;
        }
        if let Some(v) = self.end_time.as_ref() {
            struct_ser.serialize_field("endTime", v)?;
        }
        if let Some(v) = self.confidence.as_ref() {
            struct_ser.serialize_field("confidence", v)?;
        }
        if let Some(v) = self.start_time_offset.as_ref() {
            struct_ser.serialize_field("startTimeOffset", v)?;
        }
        if let Some(v) = self.speaker_id.as_ref() {
            struct_ser.serialize_field("speakerId", v)?;
        }
        struct_ser.end()
    }
}
impl<'de> serde::Deserialize<'de> for TimedString {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "text",
            "start_time",
            "startTime",
            "end_time",
            "endTime",
            "confidence",
            "start_time_offset",
            "startTimeOffset",
            "speaker_id",
            "speakerId",
        ];

        #[allow(clippy::enum_variant_names)]
        enum GeneratedField {
            Text,
            StartTime,
            EndTime,
            Confidence,
            StartTimeOffset,
            SpeakerId,
            __SkipField__,
        }
        impl<'de> serde::Deserialize<'de> for GeneratedField {
            fn deserialize<D>(deserializer: D) -> std::result::Result<GeneratedField, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct GeneratedVisitor;

                impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
                    type Value = GeneratedField;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                        write!(formatter, "expected one of: {:?}", &FIELDS)
                    }

                    #[allow(unused_variables)]
                    fn visit_str<E>(self, value: &str) -> std::result::Result<GeneratedField, E>
                    where
                        E: serde::de::Error,
                    {
                        match value {
                            "text" => Ok(GeneratedField::Text),
                            "startTime" | "start_time" => Ok(GeneratedField::StartTime),
                            "endTime" | "end_time" => Ok(GeneratedField::EndTime),
                            "confidence" => Ok(GeneratedField::Confidence),
                            "startTimeOffset" | "start_time_offset" => Ok(GeneratedField::StartTimeOffset),
                            "speakerId" | "speaker_id" => Ok(GeneratedField::SpeakerId),
                            _ => Ok(GeneratedField::__SkipField__),
                        }
                    }
                }
                deserializer.deserialize_identifier(GeneratedVisitor)
            }
        }
        struct GeneratedVisitor;
        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = TimedString;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("struct livekit.agent.TimedString")
            }

            fn visit_map<V>(self, mut map_: V) -> std::result::Result<TimedString, V::Error>
                where
                    V: serde::de::MapAccess<'de>,
            {
                let mut text__ = None;
                let mut start_time__ = None;
                let mut end_time__ = None;
                let mut confidence__ = None;
                let mut start_time_offset__ = None;
                let mut speaker_id__ = None;
                while let Some(k) = map_.next_key()? {
                    match k {
                        GeneratedField::Text => {
                            if text__.is_some() {
                                return Err(serde::de::Error::duplicate_field("text"));
                            }
                            text__ = Some(map_.next_value()?);
                        }
                        GeneratedField::StartTime => {
                            if start_time__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startTime"));
                            }
                            start_time__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::EndTime => {
                            if end_time__.is_some() {
                                return Err(serde::de::Error::duplicate_field("endTime"));
                            }
                            end_time__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::Confidence => {
                            if confidence__.is_some() {
                                return Err(serde::de::Error::duplicate_field("confidence"));
                            }
                            confidence__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::StartTimeOffset => {
                            if start_time_offset__.is_some() {
                                return Err(serde::de::Error::duplicate_field("startTimeOffset"));
                            }
                            start_time_offset__ = 
                                map_.next_value::<::std::option::Option<::pbjson::private::NumberDeserialize<_>>>()?.map(|x| x.0)
                            ;
                        }
                        GeneratedField::SpeakerId => {
                            if speaker_id__.is_some() {
                                return Err(serde::de::Error::duplicate_field("speakerId"));
                            }
                            speaker_id__ = map_.next_value()?;
                        }
                        GeneratedField::__SkipField__ => {
                            let _ = map_.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(TimedString {
                    text: text__.unwrap_or_default(),
                    start_time: start_time__,
                    end_time: end_time__,
                    confidence: confidence__,
                    start_time_offset: start_time_offset__,
                    speaker_id: speaker_id__,
                })
            }
        }
        deserializer.deserialize_struct("livekit.agent.TimedString", FIELDS, GeneratedVisitor)
    }
}
impl serde::Serialize for ToolCallStatus {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::TcRunning => "TC_RUNNING",
            Self::TcDone => "TC_DONE",
            Self::TcError => "TC_ERROR",
            Self::TcCancelled => "TC_CANCELLED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ToolCallStatus {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "TC_RUNNING",
            "TC_DONE",
            "TC_ERROR",
            "TC_CANCELLED",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ToolCallStatus;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "TC_RUNNING" => Ok(ToolCallStatus::TcRunning),
                    "TC_DONE" => Ok(ToolCallStatus::TcDone),
                    "TC_ERROR" => Ok(ToolCallStatus::TcError),
                    "TC_CANCELLED" => Ok(ToolCallStatus::TcCancelled),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for ToolReplyStatus {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::TrScheduled => "TR_SCHEDULED",
            Self::TrCompleted => "TR_COMPLETED",
            Self::TrInterrupted => "TR_INTERRUPTED",
            Self::TrSkipped => "TR_SKIPPED",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for ToolReplyStatus {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "TR_SCHEDULED",
            "TR_COMPLETED",
            "TR_INTERRUPTED",
            "TR_SKIPPED",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = ToolReplyStatus;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "TR_SCHEDULED" => Ok(ToolReplyStatus::TrScheduled),
                    "TR_COMPLETED" => Ok(ToolReplyStatus::TrCompleted),
                    "TR_INTERRUPTED" => Ok(ToolReplyStatus::TrInterrupted),
                    "TR_SKIPPED" => Ok(ToolReplyStatus::TrSkipped),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
impl serde::Serialize for UserState {
    #[allow(deprecated)]
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let variant = match self {
            Self::UsSpeaking => "US_SPEAKING",
            Self::UsListening => "US_LISTENING",
            Self::UsAway => "US_AWAY",
        };
        serializer.serialize_str(variant)
    }
}
impl<'de> serde::Deserialize<'de> for UserState {
    #[allow(deprecated)]
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const FIELDS: &[&str] = &[
            "US_SPEAKING",
            "US_LISTENING",
            "US_AWAY",
        ];

        struct GeneratedVisitor;

        impl<'de> serde::de::Visitor<'de> for GeneratedVisitor {
            type Value = UserState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(formatter, "expected one of: {:?}", &FIELDS)
            }

            fn visit_i64<E>(self, v: i64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Signed(v), &self)
                    })
            }

            fn visit_u64<E>(self, v: u64) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                i32::try_from(v)
                    .ok()
                    .and_then(|x| x.try_into().ok())
                    .ok_or_else(|| {
                        serde::de::Error::invalid_value(serde::de::Unexpected::Unsigned(v), &self)
                    })
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "US_SPEAKING" => Ok(UserState::UsSpeaking),
                    "US_LISTENING" => Ok(UserState::UsListening),
                    "US_AWAY" => Ok(UserState::UsAway),
                    _ => Err(serde::de::Error::unknown_variant(value, FIELDS)),
                }
            }
        }
        deserializer.deserialize_any(GeneratedVisitor)
    }
}
