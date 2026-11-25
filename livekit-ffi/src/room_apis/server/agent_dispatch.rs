// Copyright 2026 LiveKit, Inc.
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

//! UniFFI surface for the AgentDispatchService server API.
//!
//! Thin wrapper over [`livekit_api::services::agent_dispatch::AgentDispatchClient`]: token
//! signing, the Twirp transport, region failover and timeouts all stay in `livekit-api`. This
//! module only converts between the protobuf types and UniFFI records.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use livekit_api::services::{agent_dispatch, ServerError, ServiceError};
use livekit_protocol as proto;

/// Error returned by [`AgentDispatchClient`] calls.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum AgentDispatchError {
    /// `LIVEKIT_API_KEY` or `LIVEKIT_API_SECRET` is missing or invalid.
    #[error("invalid environment: {message}")]
    InvalidEnvironment { message: String },
    /// The access token for the request could not be created.
    #[error("invalid access token: {message}")]
    InvalidAccessToken { message: String },
    /// The server answered with a Twirp error, for example `not_found` or `unauthenticated`.
    #[error("server error {code}: {message}")]
    Server { code: String, message: String },
    /// The request did not complete: connection, TLS, timeout or decode failure.
    #[error("request failed: {message}")]
    Request { message: String },
}

impl From<ServiceError> for AgentDispatchError {
    fn from(err: ServiceError) -> Self {
        match err {
            ServiceError::Env(e) => Self::InvalidEnvironment { message: e.to_string() },
            ServiceError::AccessToken(e) => Self::InvalidAccessToken { message: e.to_string() },
            ServiceError::Twirp(ServerError::Twirp(code)) => {
                Self::Server { code: code.code, message: code.msg }
            }
            ServiceError::Twirp(e) => Self::Request { message: e.to_string() },
        }
    }
}

/// Whether a failed job restarts. Cloud only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Enum)]
pub enum JobRestartPolicy {
    #[default]
    OnFailure,
    Never,
}

/// How a dispatch assigns jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Enum)]
pub enum JobType {
    #[default]
    Room,
    Publisher,
    Participant,
}

/// Run state of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Enum)]
pub enum JobStatus {
    #[default]
    Pending,
    Running,
    Success,
    Failed,
    Cancelled,
}

/// Runtime status of one job.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct JobState {
    pub status: JobStatus,
    pub error: String,
    pub started_at: i64,
    pub ended_at: i64,
    pub updated_at: i64,
    pub participant_identity: String,
    pub worker_id: String,
    pub agent_id: String,
}

/// One agent job created by a dispatch.
///
/// `room` and `participant` are reduced to their identifiers; the full models are not part of
/// the dispatch use case.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Job {
    pub id: String,
    pub dispatch_id: String,
    pub job_type: JobType,
    pub room_name: String,
    pub room_sid: String,
    pub participant_identity: Option<String>,
    pub metadata: String,
    pub agent_name: String,
    pub state: Option<JobState>,
    pub enable_recording: bool,
    pub deployment: String,
    pub attributes: HashMap<String, String>,
}

/// Jobs and timestamps of a dispatch.
#[derive(Debug, Clone, PartialEq, Default, uniffi::Record)]
pub struct AgentDispatchState {
    pub jobs: Vec<Job>,
    pub created_at: i64,
    pub deleted_at: i64,
}

/// An explicit agent dispatch.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AgentDispatch {
    pub id: String,
    pub agent_name: String,
    pub room: String,
    pub metadata: String,
    pub state: Option<AgentDispatchState>,
    pub restart_policy: JobRestartPolicy,
    pub deployment: String,
    pub attributes: HashMap<String, String>,
}

/// Parameters of [`AgentDispatchClient::create_dispatch`].
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CreateAgentDispatchRequest {
    pub agent_name: String,
    pub room: String,
    #[uniffi(default)]
    pub metadata: String,
    #[uniffi(default)]
    pub restart_policy: Option<JobRestartPolicy>,
    #[uniffi(default)]
    pub deployment: String,
    #[uniffi(default)]
    pub attributes: HashMap<String, String>,
}

/// Transport configuration of [`AgentDispatchClient`]. `None` keeps the `livekit-api` default.
#[derive(Debug, Clone, PartialEq, Default, uniffi::Record)]
pub struct AgentDispatchClientOptions {
    #[uniffi(default)]
    pub request_timeout_ms: Option<u64>,
    #[uniffi(default)]
    pub failover: Option<bool>,
}

/// Client for the AgentDispatchService API. Every call needs the `roomAdmin` grant for the room.
#[derive(uniffi::Object)]
pub struct AgentDispatchClient {
    inner: agent_dispatch::AgentDispatchClient,
}

impl AgentDispatchClient {
    fn configure(
        mut inner: agent_dispatch::AgentDispatchClient,
        options: Option<AgentDispatchClientOptions>,
    ) -> Arc<Self> {
        let options = options.unwrap_or_default();
        if let Some(ms) = options.request_timeout_ms {
            inner = inner.with_request_timeout(Duration::from_millis(ms));
        }
        if let Some(failover) = options.failover {
            inner = inner.with_failover(failover);
        }
        Arc::new(Self { inner })
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl AgentDispatchClient {
    /// Signs a short-lived token per request from an API key and secret.
    #[uniffi::constructor(default(options = None))]
    pub fn with_api_key(
        host: String,
        api_key: String,
        api_secret: String,
        options: Option<AgentDispatchClientOptions>,
    ) -> Arc<Self> {
        Self::configure(
            agent_dispatch::AgentDispatchClient::with_api_key(&host, &api_key, &api_secret),
            options,
        )
    }

    /// Sends a pre-signed token with the `roomAdmin` grant on every request.
    #[uniffi::constructor(default(options = None))]
    pub fn with_token(
        host: String,
        token: String,
        options: Option<AgentDispatchClientOptions>,
    ) -> Arc<Self> {
        Self::configure(agent_dispatch::AgentDispatchClient::with_token(&host, &token), options)
    }

    /// Reads the API key and secret from `LIVEKIT_API_KEY` and `LIVEKIT_API_SECRET`.
    #[uniffi::constructor(default(options = None))]
    pub fn from_env(
        host: String,
        options: Option<AgentDispatchClientOptions>,
    ) -> Result<Arc<Self>, AgentDispatchError> {
        Ok(Self::configure(agent_dispatch::AgentDispatchClient::new(&host)?, options))
    }

    /// Creates an explicit dispatch of a named agent into a room.
    pub async fn create_dispatch(
        &self,
        req: CreateAgentDispatchRequest,
    ) -> Result<AgentDispatch, AgentDispatchError> {
        Ok(self.inner.create_dispatch(req.into()).await?.into())
    }

    /// Deletes a dispatch and returns it.
    pub async fn delete_dispatch(
        &self,
        dispatch_id: String,
        room_name: String,
    ) -> Result<AgentDispatch, AgentDispatchError> {
        Ok(self.inner.delete_dispatch(dispatch_id, room_name).await?.into())
    }

    /// Lists every dispatch in a room.
    pub async fn list_dispatch(
        &self,
        room_name: String,
    ) -> Result<Vec<AgentDispatch>, AgentDispatchError> {
        Ok(self.inner.list_dispatch(room_name).await?.into_iter().map(Into::into).collect())
    }

    /// Returns one dispatch, or `None` when the room has no dispatch with that ID.
    pub async fn get_dispatch(
        &self,
        dispatch_id: String,
        room_name: String,
    ) -> Result<Option<AgentDispatch>, AgentDispatchError> {
        Ok(self.inner.get_dispatch(dispatch_id, room_name).await?.map(Into::into))
    }
}

// Unknown values from a newer server map to the proto default.
impl From<i32> for JobRestartPolicy {
    fn from(v: i32) -> Self {
        match proto::JobRestartPolicy::try_from(v).unwrap_or_default() {
            proto::JobRestartPolicy::JrpOnFailure => Self::OnFailure,
            proto::JobRestartPolicy::JrpNever => Self::Never,
        }
    }
}

impl From<JobRestartPolicy> for proto::JobRestartPolicy {
    fn from(v: JobRestartPolicy) -> Self {
        match v {
            JobRestartPolicy::OnFailure => Self::JrpOnFailure,
            JobRestartPolicy::Never => Self::JrpNever,
        }
    }
}

impl From<i32> for JobType {
    fn from(v: i32) -> Self {
        match proto::JobType::try_from(v).unwrap_or_default() {
            proto::JobType::JtRoom => Self::Room,
            proto::JobType::JtPublisher => Self::Publisher,
            proto::JobType::JtParticipant => Self::Participant,
        }
    }
}

impl From<i32> for JobStatus {
    fn from(v: i32) -> Self {
        match proto::JobStatus::try_from(v).unwrap_or_default() {
            proto::JobStatus::JsPending => Self::Pending,
            proto::JobStatus::JsRunning => Self::Running,
            proto::JobStatus::JsSuccess => Self::Success,
            proto::JobStatus::JsFailed => Self::Failed,
            proto::JobStatus::JsCancelled => Self::Cancelled,
        }
    }
}

impl From<proto::JobState> for JobState {
    fn from(s: proto::JobState) -> Self {
        Self {
            status: s.status.into(),
            error: s.error,
            started_at: s.started_at,
            ended_at: s.ended_at,
            updated_at: s.updated_at,
            participant_identity: s.participant_identity,
            worker_id: s.worker_id,
            agent_id: s.agent_id,
        }
    }
}

impl From<proto::Job> for Job {
    fn from(j: proto::Job) -> Self {
        let (room_name, room_sid) = j.room.map(|r| (r.name, r.sid)).unwrap_or_default();
        Self {
            id: j.id,
            dispatch_id: j.dispatch_id,
            job_type: j.r#type.into(),
            room_name,
            room_sid,
            participant_identity: j.participant.map(|p| p.identity),
            metadata: j.metadata,
            agent_name: j.agent_name,
            state: j.state.map(Into::into),
            enable_recording: j.enable_recording,
            deployment: j.deployment,
            attributes: j.attributes,
        }
    }
}

impl From<proto::AgentDispatchState> for AgentDispatchState {
    fn from(s: proto::AgentDispatchState) -> Self {
        Self {
            jobs: s.jobs.into_iter().map(Into::into).collect(),
            created_at: s.created_at,
            deleted_at: s.deleted_at,
        }
    }
}

impl From<proto::AgentDispatch> for AgentDispatch {
    fn from(d: proto::AgentDispatch) -> Self {
        Self {
            id: d.id,
            agent_name: d.agent_name,
            room: d.room,
            metadata: d.metadata,
            state: d.state.map(Into::into),
            restart_policy: d.restart_policy.into(),
            deployment: d.deployment,
            attributes: d.attributes,
        }
    }
}

impl From<CreateAgentDispatchRequest> for proto::CreateAgentDispatchRequest {
    fn from(r: CreateAgentDispatchRequest) -> Self {
        Self {
            agent_name: r.agent_name,
            room: r.room,
            metadata: r.metadata,
            restart_policy: proto::JobRestartPolicy::from(r.restart_policy.unwrap_or_default())
                as i32,
            deployment: r.deployment,
            attributes: r.attributes,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use livekit_api::services::ServerErrorCode;

    #[test]
    fn create_request_maps_every_field() {
        let req = CreateAgentDispatchRequest {
            agent_name: "agent".into(),
            room: "room".into(),
            metadata: "{}".into(),
            restart_policy: Some(JobRestartPolicy::Never),
            deployment: "staging".into(),
            attributes: HashMap::from([("k".into(), "v".into())]),
        };
        let p: proto::CreateAgentDispatchRequest = req.into();
        assert_eq!(p.agent_name, "agent");
        assert_eq!(p.room, "room");
        assert_eq!(p.metadata, "{}");
        assert_eq!(p.restart_policy, proto::JobRestartPolicy::JrpNever as i32);
        assert_eq!(p.deployment, "staging");
        assert_eq!(p.attributes.get("k").map(String::as_str), Some("v"));
    }

    #[test]
    fn dispatch_maps_jobs_and_state() {
        let d = proto::AgentDispatch {
            id: "AD_1".into(),
            agent_name: "agent".into(),
            room: "room".into(),
            restart_policy: proto::JobRestartPolicy::JrpNever as i32,
            state: Some(proto::AgentDispatchState {
                created_at: 10,
                jobs: vec![proto::Job {
                    id: "AJ_1".into(),
                    r#type: proto::JobType::JtPublisher as i32,
                    room: Some(proto::Room {
                        name: "room".into(),
                        sid: "RM_1".into(),
                        ..Default::default()
                    }),
                    participant: Some(proto::ParticipantInfo {
                        identity: "alice".into(),
                        ..Default::default()
                    }),
                    state: Some(proto::JobState {
                        status: proto::JobStatus::JsCancelled as i32,
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        let d: AgentDispatch = d.into();
        assert_eq!(d.restart_policy, JobRestartPolicy::Never);
        let state = d.state.unwrap();
        assert_eq!(state.created_at, 10);
        let job = &state.jobs[0];
        assert_eq!(job.job_type, JobType::Publisher);
        assert_eq!(job.room_sid, "RM_1");
        assert_eq!(job.participant_identity.as_deref(), Some("alice"));
        assert_eq!(job.state.as_ref().unwrap().status, JobStatus::Cancelled);
    }

    #[test]
    fn unknown_enum_values_fall_back_to_default() {
        assert_eq!(JobStatus::from(99), JobStatus::Pending);
        assert_eq!(JobType::from(-1), JobType::Room);
        assert_eq!(JobRestartPolicy::from(7), JobRestartPolicy::OnFailure);
    }

    #[test]
    fn twirp_error_keeps_code_and_message() {
        let err: AgentDispatchError = ServiceError::Twirp(ServerError::Twirp(ServerErrorCode {
            code: "not_found".into(),
            msg: "no dispatch".into(),
            meta: HashMap::new(),
        }))
        .into();
        match err {
            AgentDispatchError::Server { code, message } => {
                assert_eq!(code, "not_found");
                assert_eq!(message, "no dispatch");
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
