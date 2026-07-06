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

use super::{ServiceBase, ServiceResult, LIVEKIT_PACKAGE};
use crate::services::dial_timeout::DEFAULT_RINGING_TIMEOUT;
use crate::{access_token::VideoGrants, get_env_keys, services::twirp_client::TwirpClient};

const SVC: &str = "Connector";

/// Options for dialing a WhatsApp call
#[derive(Default, Clone, Debug)]
pub struct DialWhatsAppCallOptions {
    /// Optional - An arbitrary string useful for tracking and logging purposes
    pub biz_opaque_callback_data: Option<String>,
    /// Optional - What LiveKit room should this participant be connected to
    pub room_name: Option<String>,
    /// Optional - Agents to dispatch the call to
    pub agents: Option<Vec<proto::RoomAgentDispatch>>,
    /// Optional - Identity of the participant in LiveKit room
    pub participant_identity: Option<String>,
    /// Optional - Name of the participant in LiveKit room
    pub participant_name: Option<String>,
    /// Optional - User-defined metadata attached to the participant in the room
    pub participant_metadata: Option<String>,
    /// Optional - User-defined attributes attached to the participant in the room
    pub participant_attributes: Option<HashMap<String, String>>,
    /// Optional - Country where the call terminates as ISO 3166-1 alpha-2
    pub destination_country: Option<String>,
}

/// Options for accepting a WhatsApp call
#[derive(Default, Clone, Debug)]
pub struct AcceptWhatsAppCallOptions {
    /// Optional - An arbitrary string useful for tracking and logging purposes
    pub biz_opaque_callback_data: Option<String>,
    /// Optional - What LiveKit room should this participant be connected to
    pub room_name: Option<String>,
    /// Optional - Agents to dispatch the call to
    pub agents: Option<Vec<proto::RoomAgentDispatch>>,
    /// Optional - Identity of the participant in LiveKit room
    pub participant_identity: Option<String>,
    /// Optional - Name of the participant in LiveKit room
    pub participant_name: Option<String>,
    /// Optional - User-defined metadata attached to the participant in the room
    pub participant_metadata: Option<String>,
    /// Optional - User-defined attributes attached to the participant in the room
    pub participant_attributes: Option<HashMap<String, String>>,
    /// Optional - Country where the call terminates as ISO 3166-1 alpha-2
    pub destination_country: Option<String>,
    /// Optional - Wait until the inbound party joins before returning.
    pub wait_until_answered: Option<bool>,
    /// Optional - Per-request timeout override. When `wait_until_answered` is set
    /// it defaults to the standard ring window; otherwise the client default applies.
    pub timeout: Option<Duration>,
}

/// Options for connecting a Twilio call
#[derive(Default, Clone, Debug)]
pub struct ConnectTwilioCallOptions {
    /// Optional - Agents to dispatch the call to
    pub agents: Option<Vec<proto::RoomAgentDispatch>>,
    /// Optional - Identity of the participant in LiveKit room
    pub participant_identity: Option<String>,
    /// Optional - Name of the participant in LiveKit room
    pub participant_name: Option<String>,
    /// Optional - User-defined metadata attached to the participant in the room
    pub participant_metadata: Option<String>,
    /// Optional - User-defined attributes attached to the participant in the room
    pub participant_attributes: Option<HashMap<String, String>>,
    /// Optional - Country where the call terminates as ISO 3166-1 alpha-2
    pub destination_country: Option<String>,
}

#[derive(Debug)]
pub struct ConnectorClient {
    base: ServiceBase,
    pub(crate) client: TwirpClient,
}

impl ConnectorClient {
    /// Authenticates with an API key and secret, signing a short-lived token per request.
    pub fn with_api_key(host: &str, api_key: &str, api_secret: &str) -> Self {
        Self {
            base: ServiceBase::with_api_key(api_key, api_secret),
            client: TwirpClient::new(host, LIVEKIT_PACKAGE, None),
        }
    }

    /// Authenticates with a pre-signed token, sent verbatim on every request.
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

    /// Reads the API key and secret from the `LIVEKIT_API_KEY` and
    /// `LIVEKIT_API_SECRET` environment variables.
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
    pub fn with_request_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.client = self.client.with_request_timeout(timeout);
        self
    }

    /// Dials a WhatsApp call
    ///
    /// # Arguments
    /// * `phone_number_id` - The identifier of the number for business initiating the call
    /// * `to_phone_number` - The number of the user that should receive the call
    /// * `api_key` - The API key of the business initiating the call
    /// * `cloud_api_version` - WhatsApp Cloud API version (e.g., "23.0", "24.0")
    /// * `options` - Additional options for the call
    ///
    /// # Returns
    /// Information about the dialed call including the WhatsApp call ID and room name
    pub async fn dial_whatsapp_call(
        &self,
        phone_number_id: impl Into<String>,
        to_phone_number: impl Into<String>,
        api_key: impl Into<String>,
        cloud_api_version: impl Into<String>,
        options: DialWhatsAppCallOptions,
    ) -> ServiceResult<proto::DialWhatsAppCallResponse> {
        self.client
            .request(
                SVC,
                "DialWhatsAppCall",
                proto::DialWhatsAppCallRequest {
                    whatsapp_phone_number_id: phone_number_id.into(),
                    whatsapp_to_phone_number: to_phone_number.into(),
                    whatsapp_api_key: api_key.into(),
                    whatsapp_cloud_api_version: cloud_api_version.into(),
                    whatsapp_biz_opaque_callback_data: options
                        .biz_opaque_callback_data
                        .unwrap_or_default(),
                    room_name: options.room_name.unwrap_or_default(),
                    agents: options.agents.unwrap_or_default(),
                    participant_identity: options.participant_identity.unwrap_or_default(),
                    participant_name: options.participant_name.unwrap_or_default(),
                    participant_metadata: options.participant_metadata.unwrap_or_default(),
                    participant_attributes: options.participant_attributes.unwrap_or_default(),
                    destination_country: options.destination_country.unwrap_or_default(),
                    ringing_timeout: Default::default(),
                },
                self.base
                    .auth_header(VideoGrants { room_create: true, ..Default::default() }, None)?,
            )
            .await
            .map_err(Into::into)
    }

    /// Disconnects a WhatsApp call initiated by the business.
    ///
    /// This is the `BusinessInitiated` case; use [`disconnect_whatsapp_call_with_reason`]
    /// to disconnect with a different [`DisconnectReason`].
    ///
    /// [`disconnect_whatsapp_call_with_reason`]: Self::disconnect_whatsapp_call_with_reason
    /// [`DisconnectReason`]: proto::disconnect_whats_app_call_request::DisconnectReason
    ///
    /// # Arguments
    /// * `call_id` - Call ID sent by Meta
    /// * `api_key` - The API key of the business disconnecting the call
    ///
    /// # Returns
    /// Empty response on success
    pub async fn disconnect_whatsapp_call(
        &self,
        call_id: impl Into<String>,
        api_key: impl Into<String>,
    ) -> ServiceResult<proto::DisconnectWhatsAppCallResponse> {
        self.disconnect_whatsapp_call_with_reason(
            call_id,
            api_key,
            proto::disconnect_whats_app_call_request::DisconnectReason::BusinessInitiated,
        )
        .await
    }

    /// Disconnects a WhatsApp call, specifying why it is being disconnected.
    ///
    /// # Arguments
    /// * `call_id` - Call ID sent by Meta
    /// * `api_key` - The API key of the business disconnecting the call. Required
    ///   when `reason` is `BusinessInitiated`; optional for `UserInitiated`.
    /// * `reason` - Why the call is being disconnected
    ///
    /// # Returns
    /// Empty response on success
    pub async fn disconnect_whatsapp_call_with_reason(
        &self,
        call_id: impl Into<String>,
        api_key: impl Into<String>,
        reason: proto::disconnect_whats_app_call_request::DisconnectReason,
    ) -> ServiceResult<proto::DisconnectWhatsAppCallResponse> {
        self.client
            .request(
                SVC,
                "DisconnectWhatsAppCall",
                proto::DisconnectWhatsAppCallRequest {
                    whatsapp_call_id: call_id.into(),
                    whatsapp_api_key: api_key.into(),
                    disconnect_reason: reason as i32,
                },
                self.base
                    .auth_header(VideoGrants { room_create: true, ..Default::default() }, None)?,
            )
            .await
            .map_err(Into::into)
    }

    /// Connects a WhatsApp call (handles the SDP exchange)
    ///
    /// # Arguments
    /// * `call_id` - Call ID sent by Meta
    /// * `sdp` - The SDP from Meta (answer SDP for business-initiated call)
    ///
    /// # Returns
    /// Empty response on success
    pub async fn connect_whatsapp_call(
        &self,
        call_id: impl Into<String>,
        sdp: proto::SessionDescription,
    ) -> ServiceResult<proto::ConnectWhatsAppCallResponse> {
        self.client
            .request(
                SVC,
                "ConnectWhatsAppCall",
                proto::ConnectWhatsAppCallRequest {
                    whatsapp_call_id: call_id.into(),
                    sdp: Some(sdp),
                },
                self.base
                    .auth_header(VideoGrants { room_create: true, ..Default::default() }, None)?,
            )
            .await
            .map_err(Into::into)
    }

    /// Accepts an incoming WhatsApp call
    ///
    /// # Arguments
    /// * `phone_number_id` - The identifier of the number for business initiating the call
    /// * `api_key` - The API key of the business connecting the call
    /// * `cloud_api_version` - WhatsApp Cloud API version (e.g., "23.0", "24.0")
    /// * `call_id` - Call ID sent by Meta
    /// * `sdp` - The SDP from Meta (for user-initiated call)
    /// * `options` - Additional options for the call
    ///
    /// # Returns
    /// Information about the accepted call including the room name
    pub async fn accept_whatsapp_call(
        &self,
        phone_number_id: impl Into<String>,
        api_key: impl Into<String>,
        cloud_api_version: impl Into<String>,
        call_id: impl Into<String>,
        sdp: proto::SessionDescription,
        options: AcceptWhatsAppCallOptions,
    ) -> ServiceResult<proto::AcceptWhatsAppCallResponse> {
        let wait_until_answered = options.wait_until_answered.unwrap_or(false);
        let request = proto::AcceptWhatsAppCallRequest {
            whatsapp_phone_number_id: phone_number_id.into(),
            whatsapp_api_key: api_key.into(),
            whatsapp_cloud_api_version: cloud_api_version.into(),
            whatsapp_call_id: call_id.into(),
            whatsapp_biz_opaque_callback_data: options.biz_opaque_callback_data.unwrap_or_default(),
            sdp: Some(sdp),
            room_name: options.room_name.unwrap_or_default(),
            agents: options.agents.unwrap_or_default(),
            participant_identity: options.participant_identity.unwrap_or_default(),
            participant_name: options.participant_name.unwrap_or_default(),
            participant_metadata: options.participant_metadata.unwrap_or_default(),
            participant_attributes: options.participant_attributes.unwrap_or_default(),
            destination_country: options.destination_country.unwrap_or_default(),
            ringing_timeout: None,
            wait_until_answered,
        };
        let headers =
            self.base.auth_header(VideoGrants { room_create: true, ..Default::default() }, None)?;

        // When waiting for the inbound party to join, the request can block, so
        // default its timeout to the standard ring window; otherwise the client
        // default applies.
        let timeout = if wait_until_answered {
            Some(options.timeout.unwrap_or(DEFAULT_RINGING_TIMEOUT))
        } else {
            options.timeout
        };
        match timeout {
            Some(timeout) => self
                .client
                .request_with_timeout(SVC, "AcceptWhatsAppCall", request, headers, timeout)
                .await
                .map_err(Into::into),
            None => self
                .client
                .request(SVC, "AcceptWhatsAppCall", request, headers)
                .await
                .map_err(Into::into),
        }
    }

    /// Connects a Twilio call
    ///
    /// # Arguments
    /// * `direction` - The direction of the call (inbound or outbound)
    /// * `room_name` - What LiveKit room should this call be connected to
    /// * `options` - Additional options for the call
    ///
    /// # Returns
    /// The WebSocket URL which Twilio media stream should connect to
    pub async fn connect_twilio_call(
        &self,
        direction: proto::connect_twilio_call_request::TwilioCallDirection,
        room_name: impl Into<String>,
        options: ConnectTwilioCallOptions,
    ) -> ServiceResult<proto::ConnectTwilioCallResponse> {
        self.client
            .request(
                SVC,
                "ConnectTwilioCall",
                proto::ConnectTwilioCallRequest {
                    twilio_call_direction: direction as i32,
                    room_name: room_name.into(),
                    agents: options.agents.unwrap_or_default(),
                    participant_identity: options.participant_identity.unwrap_or_default(),
                    participant_name: options.participant_name.unwrap_or_default(),
                    participant_metadata: options.participant_metadata.unwrap_or_default(),
                    participant_attributes: options.participant_attributes.unwrap_or_default(),
                    destination_country: options.destination_country.unwrap_or_default(),
                },
                self.base
                    .auth_header(VideoGrants { room_create: true, ..Default::default() }, None)?,
            )
            .await
            .map_err(Into::into)
    }
}
