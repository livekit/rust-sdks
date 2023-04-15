use super::{ServiceBase, ServiceResult, LIVEKIT_PACKAGE};
use crate::services::twirp_client::TwirpClient;
use crate::{access_token::VideoGrants, get_env_keys};
use livekit_protocol as proto;

const SVC: &'static str = "Egress";

#[derive(Debug)]
pub struct EgressClient {
    base: ServiceBase,
    client: TwirpClient,
}

impl EgressClient {
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

    pub async fn start_room_composite_egress(
        &self,
        req: proto::RoomCompositeEgressRequest,
    ) -> ServiceResult<proto::EgressInfo> {
        self.client
            .request(
                SVC,
                "StartRoomCompositeEgress",
                req,
                self.base.auth_header(VideoGrants { room_record: true }),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn start_web_egress(
        &self,
        req: proto::WebEgressRequest,
    ) -> ServiceResult<proto::EgressInfo> {
        self.client
            .request(
                SVC,
                "StartWebEgress",
                req,
                self.base.auth_header(VideoGrants { room_record: true }),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn start_track_composite_egress(
        &self,
        req: proto::TrackCompositeEgressRequest,
    ) -> ServiceResult<proto::EgrssInfo> {
        self.client
            .request(
                SVC,
                "StartTrackCompositeEgress",
                req,
                self.base.auth_header(VideoGrants { room_record: true }),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn start_track_egress(
        &self,
        req: proto::TrackEgressRequest,
    ) -> ServiceResult<proto::EgressInfo> {
        self.client
            .request(
                SVC,
                "StartTrackEgress",
                req,
                self.base.auth_header(VideoGrants { room_record: true }),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn update_layout(
        &self,
        egress_id: &str,
        layout: &str,
    ) -> ServiceResult<proto::EgressInfo> {
        self.client
            .request(
                SVC,
                "UpdateLayout",
                proto::UpdateLayoutRequest {
                    egress_id: egress_id.to_owned(),
                    layout: layout.to_owned(),
                },
                self.base.auth_header(VideoGrants { room_record: true }),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn update_stream(
        &self,
        req: proto::UpdateStreamRequest,
    ) -> ServiceResult<proto::EgressInfo> {
        self.client
            .request(
                SVC,
                "UpdateStream",
                req,
                self.base.auth_header(VideoGrants { room_record: true }),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn list_egress(&self, room: Option<&str>) -> ServiceResult<Vec<proto::EgressInfo>> {
        self.client
            .request(
                SVC,
                "ListEgress",
                proto::ListEgressRequest {
                    room_name: room.unwrap_or_default().to_owned(),
                },
                self.base.auth_header(VideoGrants { room_record: true }),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn stop_egress(&self, egress_id: &str) -> ServiceResult<proto::EgressInfo> {
        self.client
            .request(
                SVC,
                "StopEgress",
                proto::StopEgressRequest {
                    egress_id: egress_id.to_owned(),
                },
                self.base.auth_header(VideoGrants { room_record: true }),
            )
            .await
            .map_err(Into::into)
    }
}
