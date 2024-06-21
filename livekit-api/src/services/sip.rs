// Copyright 2024 LiveKit, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use livekit_protocol as proto;
use std::collections::HashMap;
use std::ptr::null;

use crate::access_token::{SIPGrants, VideoGrants};
use crate::get_env_keys;
use crate::services::twirp_client::TwirpClient;
use crate::services::{ServiceBase, ServiceResult, LIVEKIT_PACKAGE};

const SVC: &str = "SIP";

#[derive(Debug)]
pub struct SIPClient {
    base: ServiceBase,
    client: TwirpClient,
}

#[deprecated]
#[derive(Default, Clone, Debug)]
pub struct CreateSIPTrunkOptions {
    /// Human-readable name for the Trunk.
    pub name: String,
    /// Optional free-form metadata.
    pub metadata: String,
    /// CIDR or IPs that traffic is accepted from
    /// An empty list means all inbound traffic is accepted.
    pub inbound_addresses: Vec<String>,
    /// Accepted `To` values. This Trunk will only accept a call made to
    /// these numbers. This allows you to have distinct Trunks for different phone
    /// numbers at the same provider.
    pub inbound_numbers: Vec<String>,
    /// Username and password used to authenticate inbound SIP invites
    /// May be empty to have no Authentication
    pub inbound_username: String,
    pub inbound_password: String,

    /// IP that SIP INVITE is sent too
    pub outbound_address: String,
    /// Username and password used to authenticate outbound SIP invites
    /// May be empty to have no Authentication
    pub outbound_username: String,
    pub outbound_password: String,
}

#[derive(Default, Clone, Debug)]
pub struct CreateSIPInboundTrunkOptions {
    /// Optional free-form metadata.
    pub metadata: String,
    /// CIDR or IPs that traffic is accepted from
    /// An empty list means all inbound traffic is accepted.
    pub allowed_addresses: Vec<String>,
    /// Accepted `To` values. This Trunk will only accept a call made to
    /// these numbers. This allows you to have distinct Trunks for different phone
    /// numbers at the same provider.
    pub allowed_numbers: Vec<String>,
    /// Username and password used to authenticate inbound SIP invites
    /// May be empty to have no Authentication
    pub auth_username: String,
    pub auth_password: String,
}

#[derive(Default, Clone, Debug)]
pub struct CreateSIPOutboundTrunkOptions {
    pub transport: proto::SipTransport,
    /// Optional free-form metadata.
    pub metadata: String,
    /// Username and password used to authenticate outbound SIP invites
    /// May be empty to have no Authentication
    pub auth_username: String,
    pub auth_password: String,
}

#[deprecated]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListSIPTrunkFilter {
    All,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListSIPInboundTrunkFilter {
    All,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListSIPOutboundTrunkFilter {
    All,
}

#[derive(Default, Clone, Debug)]
pub struct CreateSIPDispatchRuleOptions {
    pub name: String,
    pub metadata: String,
    pub attributes: HashMap<String, String>,
    /// What trunks are accepted for this dispatch rule
    /// If empty all trunks will match this dispatch rule
    pub trunk_ids: Vec<String>,
    pub allowed_numbers: Vec<String>,
    pub hide_phone_number: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListSIPDispatchRuleFilter {
    All,
}

#[derive(Default, Clone, Debug)]
pub struct CreateSIPParticipantOptions {
    /// Optional identity of the participant in LiveKit room
    pub participant_identity: String,
    /// Optionally set the name of the participant in a LiveKit room
    pub participant_name: String,
    /// Optionally set the free-form metadata of the participant in a LiveKit room
    pub participant_metadata: String,
    pub participant_attributes: HashMap<String, String>,
    /// Optionally send following DTMF digits (extension codes) when making a call.
    /// Character 'w' can be used to add a 0.5 sec delay.
    pub dtmf: String,
    /// Optionally play ringtone in the room as an audible indicator for existing participants
    pub play_ringtone: bool,
    pub hide_phone_number: bool,
}

impl SIPClient {
    pub fn with_api_key(host: &str, api_key: &str, api_secret: &str) -> Self {
        Self {
            base: ServiceBase::with_api_key(api_key, api_secret),
            client: TwirpClient::new(host, LIVEKIT_PACKAGE, None),
        }
    }

    pub fn new(host: &str) -> ServiceResult<Self> {
        let (api_key, api_secret) = get_env_keys()?;
        Ok(Self::with_api_key(host, &api_key, &api_secret))
    }

    #[deprecated]
    pub async fn create_sip_trunk(
        &self,
        number: String,
        options: CreateSIPTrunkOptions,
    ) -> ServiceResult<proto::SipTrunkInfo> {
        self.client
            .request(
                SVC,
                "CreateSIPTrunk",
                proto::CreateSipTrunkRequest {
                    name: options.name,
                    metadata: options.metadata,

                    outbound_number: number.to_owned(),
                    outbound_address: options.outbound_address.to_owned(),
                    outbound_username: options.outbound_username.to_owned(),
                    outbound_password: options.outbound_password.to_owned(),

                    inbound_numbers: options.inbound_numbers.to_owned(),
                    inbound_numbers_regex: Vec::new(),
                    inbound_addresses: options.inbound_addresses.to_owned(),
                    inbound_username: options.inbound_username.to_owned(),
                    inbound_password: options.inbound_password.to_owned(),
                },
                self.base.auth_header(
                    Default::default(),
                    Some(SIPGrants { admin: true, ..Default::default() }),
                )?,
            )
            .await
            .map_err(Into::into)
    }

    pub async fn create_sip_inbound_trunk(
        &self,
        name: String,
        numbers: Vec<String>,
        options: CreateSIPInboundTrunkOptions,
    ) -> ServiceResult<proto::SipInboundTrunkInfo> {
        self.client
            .request(
                SVC,
                "CreateSIPInboundTrunk",
                proto::CreateSipInboundTrunkRequest {
                    trunk: Some(proto::SipInboundTrunkInfo {
                        sip_trunk_id: Default::default(),
                        name,
                        numbers,
                        metadata: options.metadata,

                        allowed_numbers: options.allowed_numbers.to_owned(),
                        allowed_addresses: options.allowed_addresses.to_owned(),
                        auth_username: options.auth_username.to_owned(),
                        auth_password: options.auth_password.to_owned(),
                    }),
                },
                self.base.auth_header(
                    Default::default(),
                    Some(SIPGrants { admin: true, ..Default::default() }),
                )?,
            )
            .await
            .map_err(Into::into)
    }

    pub async fn create_sip_outbound_trunk(
        &self,
        name: String,
        address: String,
        numbers: Vec<String>,
        options: CreateSIPOutboundTrunkOptions,
    ) -> ServiceResult<proto::SipOutboundTrunkInfo> {
        self.client
            .request(
                SVC,
                "CreateSIPOutboundTrunk",
                proto::CreateSipOutboundTrunkRequest {
                    trunk: Some(proto::SipOutboundTrunkInfo {
                        sip_trunk_id: Default::default(),
                        name,
                        address,
                        numbers,
                        transport: options.transport as i32,
                        metadata: options.metadata,

                        auth_username: options.auth_username.to_owned(),
                        auth_password: options.auth_password.to_owned(),
                    }),
                },
                self.base.auth_header(
                    Default::default(),
                    Some(SIPGrants { admin: true, ..Default::default() }),
                )?,
            )
            .await
            .map_err(Into::into)
    }

    #[deprecated]
    pub async fn list_sip_trunk(
        &self,
        filter: ListSIPTrunkFilter,
    ) -> ServiceResult<Vec<proto::SipTrunkInfo>> {
        let resp: proto::ListSipTrunkResponse = self
            .client
            .request(
                SVC,
                "ListSIPTrunk",
                proto::ListSipTrunkRequest {},
                self.base.auth_header(
                    Default::default(),
                    Some(SIPGrants { admin: true, ..Default::default() }),
                )?,
            )
            .await?;

        Ok(resp.items)
    }

    pub async fn list_sip_inbound_trunk(
        &self,
        filter: ListSIPInboundTrunkFilter,
    ) -> ServiceResult<Vec<proto::SipInboundTrunkInfo>> {
        let resp: proto::ListSipInboundTrunkResponse = self
            .client
            .request(
                SVC,
                "ListSIPInboundTrunk",
                proto::ListSipInboundTrunkRequest {},
                self.base.auth_header(
                    Default::default(),
                    Some(SIPGrants { admin: true, ..Default::default() }),
                )?,
            )
            .await?;

        Ok(resp.items)
    }

    pub async fn list_sip_outbound_trunk(
        &self,
        filter: ListSIPOutboundTrunkFilter,
    ) -> ServiceResult<Vec<proto::SipOutboundTrunkInfo>> {
        let resp: proto::ListSipOutboundTrunkResponse = self
            .client
            .request(
                SVC,
                "ListSIPOutboundTrunk",
                proto::ListSipOutboundTrunkRequest {},
                self.base.auth_header(
                    Default::default(),
                    Some(SIPGrants { admin: true, ..Default::default() }),
                )?,
            )
            .await?;

        Ok(resp.items)
    }

    pub async fn delete_sip_trunk(&self, sip_trunk_id: &str) -> ServiceResult<proto::SipTrunkInfo> {
        self.client
            .request(
                SVC,
                "DeleteSIPTrunk",
                proto::DeleteSipTrunkRequest { sip_trunk_id: sip_trunk_id.to_owned() },
                self.base.auth_header(
                    Default::default(),
                    Some(SIPGrants { admin: true, ..Default::default() }),
                )?,
            )
            .await
            .map_err(Into::into)
    }

    pub async fn create_sip_dispatch_rule(
        &self,
        rule: proto::sip_dispatch_rule::Rule,
        options: CreateSIPDispatchRuleOptions,
    ) -> ServiceResult<proto::SipDispatchRuleInfo> {
        self.client
            .request(
                SVC,
                "CreateSIPDispatchRule",
                proto::CreateSipDispatchRuleRequest {
                    name: options.name,
                    metadata: options.metadata,
                    attributes: options.attributes,
                    trunk_ids: options.trunk_ids.to_owned(),
                    inbound_numbers: options.allowed_numbers.to_owned(),
                    hide_phone_number: options.hide_phone_number,
                    rule: Some(proto::SipDispatchRule { rule: Some(rule.to_owned()) }),
                },
                self.base.auth_header(
                    Default::default(),
                    Some(SIPGrants { admin: true, ..Default::default() }),
                )?,
            )
            .await
            .map_err(Into::into)
    }

    pub async fn list_sip_dispatch_rule(
        &self,
        filter: ListSIPDispatchRuleFilter,
    ) -> ServiceResult<Vec<proto::SipDispatchRuleInfo>> {
        let resp: proto::ListSipDispatchRuleResponse = self
            .client
            .request(
                SVC,
                "ListSIPDispatchRule",
                proto::ListSipDispatchRuleRequest {},
                self.base.auth_header(
                    Default::default(),
                    Some(SIPGrants { admin: true, ..Default::default() }),
                )?,
            )
            .await?;

        Ok(resp.items)
    }

    pub async fn delete_sip_dispatch_rule(
        &self,
        sip_dispatch_rule_id: &str,
    ) -> ServiceResult<proto::SipDispatchRuleInfo> {
        self.client
            .request(
                SVC,
                "DeleteSIPDispatchRule",
                proto::DeleteSipDispatchRuleRequest {
                    sip_dispatch_rule_id: sip_dispatch_rule_id.to_owned(),
                },
                self.base.auth_header(
                    Default::default(),
                    Some(SIPGrants { admin: true, ..Default::default() }),
                )?,
            )
            .await
            .map_err(Into::into)
    }

    pub async fn create_sip_participant(
        &self,
        sip_trunk_id: String,
        call_to: String,
        room_name: String,
        options: CreateSIPParticipantOptions,
    ) -> ServiceResult<proto::SipParticipantInfo> {
        self.client
            .request(
                SVC,
                "CreateSIPParticipant",
                proto::CreateSipParticipantRequest {
                    sip_trunk_id: sip_trunk_id.to_owned(),
                    sip_call_to: call_to.to_owned(),
                    room_name: room_name.to_owned(),
                    participant_identity: options.participant_identity.to_owned(),
                    participant_name: options.participant_name.to_owned(),
                    participant_metadata: options.participant_metadata.to_owned(),
                    participant_attributes: options.participant_attributes.to_owned(),
                    dtmf: options.dtmf.to_owned(),
                    play_ringtone: options.play_ringtone,
                    hide_phone_number: options.hide_phone_number,
                },
                self.base.auth_header(
                    Default::default(),
                    Some(SIPGrants { call: true, ..Default::default() }),
                )?,
            )
            .await
            .map_err(Into::into)
    }
}
