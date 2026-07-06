// Copyright 2025 LiveKit, Inc.
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
use std::time::Duration;

use crate::access_token::{SIPGrants, VideoGrants};
use crate::get_env_keys;
use crate::services::dial_timeout::{dial_timeout, DEFAULT_RINGING_TIMEOUT};
use crate::services::twirp_client::TwirpClient;
use crate::services::{ServiceBase, ServiceResult, LIVEKIT_PACKAGE};
use pbjson_types::Duration as ProtoDuration;

const SVC: &str = "SIP";

#[derive(Debug)]
pub struct SIPClient {
    base: ServiceBase,
    pub(crate) client: TwirpClient,
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
    pub metadata: Option<String>,
    /// CIDR or IPs that traffic is accepted from
    /// An empty list means all inbound traffic is accepted.
    pub allowed_addresses: Option<Vec<String>>,
    /// Accepted `To` values. This Trunk will only accept a call made to
    /// these numbers. This allows you to have distinct Trunks for different phone
    /// numbers at the same provider.
    pub allowed_numbers: Option<Vec<String>>,
    /// Username and password used to authenticate inbound SIP invites
    /// May be empty to have no Authentication
    pub auth_username: Option<String>,
    pub auth_password: Option<String>,
    pub headers: Option<HashMap<String, String>>,
    pub headers_to_attributes: Option<HashMap<String, String>>,
    pub attributes_to_headers: Option<HashMap<String, String>>,
    pub max_call_duration: Option<Duration>,
    pub ringing_timeout: Option<Duration>,
    pub krisp_enabled: Option<bool>,
    /// Authentication realm advertised on inbound SIP invites.
    pub auth_realm: Option<String>,
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

    pub headers: Option<HashMap<String, String>>,
    pub headers_to_attributes: Option<HashMap<String, String>>,
    pub attributes_to_headers: Option<HashMap<String, String>>,
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
    /// Room configuration for rooms created by this dispatch rule, including
    /// agents to dispatch into the room.
    pub room_config: Option<proto::RoomConfiguration>,
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
    pub participant_name: Option<String>,
    /// Optionally set the free-form metadata of the participant in a LiveKit room
    pub participant_metadata: Option<String>,
    pub participant_attributes: Option<HashMap<String, String>>,
    /// Optional custom caller ID shown to the callee. Requires SIP provider
    /// support. If unset, the phone number is used; set it to an empty string to
    /// trigger a CNAM lookup on providers that support it.
    pub display_name: Option<String>,
    // What number should be dialed via SIP
    pub sip_number: Option<String>,
    /// Optionally send following DTMF digits (extension codes) when making a call.
    /// Character 'w' can be used to add a 0.5 sec delay.
    pub dtmf: Option<String>,
    /// Wait for the call to be answered before returning.
    ///
    /// When `true`, the request blocks until the call is answered or fails,
    /// and returns SIP error codes (e.g., 486 Busy, 603 Decline) on failure.
    /// When `false` (default), returns immediately while the call is still dialing.
    pub wait_until_answered: Option<bool>,
    /// Optionally play dialtone in the room as an audible indicator for existing participants
    pub play_dialtone: Option<bool>,
    pub hide_phone_number: Option<bool>,
    pub ringing_timeout: Option<Duration>,
    pub max_call_duration: Option<Duration>,
    pub enable_krisp: Option<bool>,
    /// SIP headers sent as-is on the INVITE; may help the SIP endpoint identify
    /// the call as coming from LiveKit.
    pub headers: Option<HashMap<String, String>>,
    /// Which SIP response headers to map to `sip.h.*` participant attributes.
    pub include_headers: Option<proto::SipHeaderOptions>,
    /// Media encryption policy for the call.
    pub media_encryption: Option<proto::SipMediaEncryption>,
    /// Per-request timeout override. Defaults to a longer value when
    /// `wait_until_answered` is set (dialing takes time), otherwise the client
    /// default. Raised, if needed, to stay above `ringing_timeout`.
    pub timeout: Option<Duration>,
}

#[derive(Default, Clone, Debug)]
pub struct TransferSIPParticipantOptions {
    /// Optionally play a dialtone to the SIP participant as an audible indicator
    /// of being transferred.
    pub play_dialtone: Option<bool>,
    /// Max time for the transfer destination to answer the call.
    pub ringing_timeout: Option<Duration>,
    /// SIP headers added to the REFER SIP request.
    pub headers: Option<HashMap<String, String>>,
    /// Per-request timeout override. A transfer always dials (REFER) and blocks
    /// until the destination answers, so this is raised, if needed, to stay above
    /// `ringing_timeout`.
    pub timeout: Option<Duration>,
}

impl SIPClient {
    pub fn with_api_key(host: &str, api_key: &str, api_secret: &str) -> Self {
        Self {
            base: ServiceBase::with_api_key(api_key, api_secret),
            client: TwirpClient::new(host, LIVEKIT_PACKAGE, None),
        }
    }

    pub fn with_token(host: &str, token: &str) -> Self {
        Self {
            base: ServiceBase::with_token(token),
            client: TwirpClient::new(host, LIVEKIT_PACKAGE, None),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_default_headers(mut self, headers: http::HeaderMap) -> Self {
        self.client = self.client.with_default_headers(headers);
        self
    }

    pub fn new(host: &str) -> ServiceResult<Self> {
        let (api_key, api_secret) = get_env_keys()?;
        Ok(Self::with_api_key(host, &api_key, &api_secret))
    }

    /// Enables or disables region failover (enabled by default). Failover only
    /// engages for LiveKit Cloud hosts.
    pub fn with_failover(mut self, enabled: bool) -> Self {
        self.client = self.client.with_failover(enabled);
        self
    }

    /// Overrides the default per-request timeout (10s) for calls on this client.
    /// `create_sip_participant` can still override it per call via
    /// [`CreateSIPParticipantOptions::timeout`].
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.client = self.client.with_request_timeout(timeout);
        self
    }

    fn duration_to_proto(d: Option<Duration>) -> Option<ProtoDuration> {
        d.map(|d| ProtoDuration { seconds: d.as_secs() as i64, nanos: d.subsec_nanos() as i32 })
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
                        metadata: options.metadata.unwrap_or_default(),
                        allowed_numbers: options.allowed_numbers.unwrap_or_default(),
                        allowed_addresses: options.allowed_addresses.unwrap_or_default(),
                        auth_username: options.auth_username.unwrap_or_default(),
                        auth_password: options.auth_password.unwrap_or_default(),
                        auth_realm: options.auth_realm.unwrap_or_default(),
                        headers: options.headers.unwrap_or_default(),
                        headers_to_attributes: options.headers_to_attributes.unwrap_or_default(),
                        attributes_to_headers: options.attributes_to_headers.unwrap_or_default(),
                        krisp_enabled: options.krisp_enabled.unwrap_or(false),
                        max_call_duration: Self::duration_to_proto(options.max_call_duration),
                        ringing_timeout: Self::duration_to_proto(options.ringing_timeout),

                        // TODO: support these attributes
                        include_headers: Default::default(),
                        media_encryption: Default::default(),
                        created_at: Default::default(),
                        updated_at: Default::default(),
                        media: Default::default(),
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

                        headers: options.headers.unwrap_or_default(),
                        headers_to_attributes: options.headers_to_attributes.unwrap_or_default(),
                        attributes_to_headers: options.attributes_to_headers.unwrap_or_default(),

                        // TODO: support these attributes
                        include_headers: Default::default(),
                        media_encryption: Default::default(),
                        destination_country: Default::default(),
                        created_at: Default::default(),
                        updated_at: Default::default(),
                        from_host: Default::default(),
                        media: Default::default(),
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

    /// Updates specific fields of an existing SIP inbound trunk. Only the fields
    /// set on `update` are changed; everything else is left as-is. Mirrors the
    /// Python SDK's `update_inbound_trunk_fields` / server-sdk-go's
    /// `SipInboundTrunkUpdate` action.
    pub async fn update_sip_inbound_trunk(
        &self,
        trunk_id: String,
        update: proto::SipInboundTrunkUpdate,
    ) -> ServiceResult<proto::SipInboundTrunkInfo> {
        self.client
            .request(
                SVC,
                "UpdateSIPInboundTrunk",
                proto::UpdateSipInboundTrunkRequest {
                    sip_trunk_id: trunk_id,
                    action: Some(proto::update_sip_inbound_trunk_request::Action::Update(update)),
                },
                self.base.auth_header(
                    Default::default(),
                    Some(SIPGrants { admin: true, ..Default::default() }),
                )?,
            )
            .await
            .map_err(Into::into)
    }

    /// Updates an existing SIP inbound trunk by replacing it entirely with
    /// `trunk`. Mirrors the Python SDK's `update_inbound_trunk` / server-sdk-go's
    /// `SipInboundTrunkInfo` (Replace) action.
    pub async fn update_sip_inbound_trunk_replace(
        &self,
        trunk_id: String,
        trunk: proto::SipInboundTrunkInfo,
    ) -> ServiceResult<proto::SipInboundTrunkInfo> {
        self.client
            .request(
                SVC,
                "UpdateSIPInboundTrunk",
                proto::UpdateSipInboundTrunkRequest {
                    sip_trunk_id: trunk_id,
                    action: Some(proto::update_sip_inbound_trunk_request::Action::Replace(trunk)),
                },
                self.base.auth_header(
                    Default::default(),
                    Some(SIPGrants { admin: true, ..Default::default() }),
                )?,
            )
            .await
            .map_err(Into::into)
    }

    /// Updates specific fields of an existing SIP outbound trunk. Only the fields
    /// set on `update` are changed; everything else is left as-is.
    pub async fn update_sip_outbound_trunk(
        &self,
        trunk_id: String,
        update: proto::SipOutboundTrunkUpdate,
    ) -> ServiceResult<proto::SipOutboundTrunkInfo> {
        self.client
            .request(
                SVC,
                "UpdateSIPOutboundTrunk",
                proto::UpdateSipOutboundTrunkRequest {
                    sip_trunk_id: trunk_id,
                    action: Some(proto::update_sip_outbound_trunk_request::Action::Update(update)),
                },
                self.base.auth_header(
                    Default::default(),
                    Some(SIPGrants { admin: true, ..Default::default() }),
                )?,
            )
            .await
            .map_err(Into::into)
    }

    /// Updates an existing SIP outbound trunk by replacing it entirely with
    /// `trunk`.
    pub async fn update_sip_outbound_trunk_replace(
        &self,
        trunk_id: String,
        trunk: proto::SipOutboundTrunkInfo,
    ) -> ServiceResult<proto::SipOutboundTrunkInfo> {
        self.client
            .request(
                SVC,
                "UpdateSIPOutboundTrunk",
                proto::UpdateSipOutboundTrunkRequest {
                    sip_trunk_id: trunk_id,
                    action: Some(proto::update_sip_outbound_trunk_request::Action::Replace(trunk)),
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
                proto::ListSipTrunkRequest {
                    // TODO support these attributes
                    page: Default::default(),
                },
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
                proto::ListSipInboundTrunkRequest {
                    // TODO: support these attributes
                    page: Default::default(),
                    trunk_ids: Default::default(),
                    numbers: Default::default(),
                },
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
                proto::ListSipOutboundTrunkRequest {
                    // TODO: support these attributes
                    page: Default::default(),
                    trunk_ids: Default::default(),
                    numbers: Default::default(),
                },
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
                    dispatch_rule: Some(proto::SipDispatchRuleInfo {
                        rule: Some(proto::SipDispatchRule { rule: Some(rule) }),
                        name: options.name,
                        metadata: options.metadata,
                        attributes: options.attributes,
                        trunk_ids: options.trunk_ids,
                        inbound_numbers: options.allowed_numbers,
                        hide_phone_number: options.hide_phone_number,
                        room_config: options.room_config,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                self.base.auth_header(
                    Default::default(),
                    Some(SIPGrants { admin: true, ..Default::default() }),
                )?,
            )
            .await
            .map_err(Into::into)
    }

    /// Updates specific fields of an existing SIP dispatch rule. Only the fields
    /// set on `update` are changed; everything else is left as-is.
    pub async fn update_sip_dispatch_rule(
        &self,
        dispatch_rule_id: String,
        update: proto::SipDispatchRuleUpdate,
    ) -> ServiceResult<proto::SipDispatchRuleInfo> {
        self.client
            .request(
                SVC,
                "UpdateSIPDispatchRule",
                proto::UpdateSipDispatchRuleRequest {
                    sip_dispatch_rule_id: dispatch_rule_id,
                    action: Some(proto::update_sip_dispatch_rule_request::Action::Update(update)),
                },
                self.base.auth_header(
                    Default::default(),
                    Some(SIPGrants { admin: true, ..Default::default() }),
                )?,
            )
            .await
            .map_err(Into::into)
    }

    /// Updates an existing SIP dispatch rule by replacing it entirely with
    /// `rule`.
    pub async fn update_sip_dispatch_rule_replace(
        &self,
        dispatch_rule_id: String,
        rule: proto::SipDispatchRuleInfo,
    ) -> ServiceResult<proto::SipDispatchRuleInfo> {
        self.client
            .request(
                SVC,
                "UpdateSIPDispatchRule",
                proto::UpdateSipDispatchRuleRequest {
                    sip_dispatch_rule_id: dispatch_rule_id,
                    action: Some(proto::update_sip_dispatch_rule_request::Action::Replace(rule)),
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
                proto::ListSipDispatchRuleRequest {
                    // TODO: support these attributes
                    page: Default::default(),
                    dispatch_rule_ids: Default::default(),
                    trunk_ids: Default::default(),
                },
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
        outbound_trunk_config: Option<proto::SipOutboundConfig>,
    ) -> ServiceResult<proto::SipParticipantInfo> {
        let wait_until_answered = options.wait_until_answered.unwrap_or(false);
        let user_timeout = options.timeout;
        // When waiting for an answer, pin the ring window explicitly so our request
        // timeout doesn't depend on the server's default (which could change).
        let ringing_timeout =
            options.ringing_timeout.or(wait_until_answered.then_some(DEFAULT_RINGING_TIMEOUT));
        let request = proto::CreateSipParticipantRequest {
            sip_trunk_id: sip_trunk_id.to_owned(),
            trunk: outbound_trunk_config,
            sip_call_to: call_to.to_owned(),
            sip_number: options.sip_number.to_owned().unwrap_or_default(),
            room_name: room_name.to_owned(),
            participant_identity: options.participant_identity.to_owned(),
            participant_name: options.participant_name.to_owned().unwrap_or_default(),
            participant_metadata: options.participant_metadata.to_owned().unwrap_or_default(),
            participant_attributes: options.participant_attributes.to_owned().unwrap_or_default(),
            display_name: options.display_name.to_owned(),
            dtmf: options.dtmf.to_owned().unwrap_or_default(),
            wait_until_answered,
            play_ringtone: options.play_dialtone.unwrap_or(false),
            play_dialtone: options.play_dialtone.unwrap_or(false),
            hide_phone_number: options.hide_phone_number.unwrap_or(false),
            max_call_duration: Self::duration_to_proto(options.max_call_duration),
            ringing_timeout: Self::duration_to_proto(ringing_timeout),
            krisp_enabled: options.enable_krisp.unwrap_or(false),
            headers: options.headers.unwrap_or_default(),
            include_headers: options.include_headers.map(|h| h as i32).unwrap_or_default(),
            media_encryption: options.media_encryption.map(|e| e as i32).unwrap_or_default(),
            ..Default::default()
        };
        let headers = self.base.auth_header(
            Default::default(),
            Some(SIPGrants { call: true, ..Default::default() }),
        )?;

        // A user-specified timeout wins; otherwise waiting for an answer dials a
        // phone, which takes longer and must outlast ringing. Without waiting the
        // request returns immediately, so the client default applies.
        if wait_until_answered {
            self.client
                .request_with_timeout(
                    SVC,
                    "CreateSIPParticipant",
                    request,
                    headers,
                    dial_timeout(user_timeout, ringing_timeout),
                )
                .await
                .map_err(Into::into)
        } else if let Some(timeout) = user_timeout {
            self.client
                .request_with_timeout(SVC, "CreateSIPParticipant", request, headers, timeout)
                .await
                .map_err(Into::into)
        } else {
            self.client
                .request(SVC, "CreateSIPParticipant", request, headers)
                .await
                .map_err(Into::into)
        }
    }

    /// Transfers a SIP participant to another number via a SIP REFER. This always
    /// dials the transfer destination and blocks until it answers or fails, so
    /// the request must outlast the ring window.
    pub async fn transfer_sip_participant(
        &self,
        room_name: String,
        participant_identity: String,
        transfer_to: String,
        options: TransferSIPParticipantOptions,
    ) -> ServiceResult<()> {
        // Pin the ring window explicitly so the request timeout doesn't depend on
        // the server's default (which could change).
        let ringing_timeout = options.ringing_timeout.or(Some(DEFAULT_RINGING_TIMEOUT));
        let request = proto::TransferSipParticipantRequest {
            participant_identity,
            room_name: room_name.to_owned(),
            transfer_to,
            play_dialtone: options.play_dialtone.unwrap_or(false),
            headers: options.headers.unwrap_or_default(),
            ringing_timeout: Self::duration_to_proto(ringing_timeout),
        };
        let headers = self.base.auth_header(
            VideoGrants { room_admin: true, room: room_name, ..Default::default() },
            Some(SIPGrants { call: true, ..Default::default() }),
        )?;

        self.client
            .request_with_timeout(
                SVC,
                "TransferSIPParticipant",
                request,
                headers,
                dial_timeout(options.timeout, ringing_timeout),
            )
            .await
            .map_err(Into::into)
    }
}
