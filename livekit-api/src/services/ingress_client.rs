use super::{ServiceBase, ServiceResult, LIVEKIT_PACKAGE};
use crate::services::twirp_client::TwirpClient;
use crate::{access_token::VideoGrants, get_env_keys};
use livekit_protocol as proto;

const SVC: &'static str = "Ingress";

#[derive(Debug)]
pub struct IngressClient {
    base: ServiceBase,
    client: TwirpClient,
}

impl IngressClient {
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

    pub async fn create_ingress(
        &self,
        req: proto::CreateIngressRequest,
    ) -> ServiceResult<proto::IngressInfo> {
        self.client
            .request(
                SVC,
                "CreateIngress",
                req,
                self.base.auth_header(VideoGrants {
                    ingress_admin: true,
                }),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn update_ingress(
        &self,
        req: proto::UpdateIngressRequest,
    ) -> ServiceResult<proto::IngressInfo> {
        self.client
            .request(
                SVC,
                "UpdateIngress",
                req,
                self.base.auth_header(VideoGrants {
                    ingress_admin: true,
                }),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn list_ingress(&self, room: Option<&str>) -> ServiceResult<Vec<proto::IngressInfo>> {
        self.client
            .request(
                SVC,
                "ListIngress",
                proto::ListIngressRequest {
                    room_name: room.unwrap_or_default().to_owned(),
                },
                self.base.auth_header(VideoGrants {
                    ingress_admin: true,
                }),
            )
            .await
            .map_err(Into::into)
    }

    pub async fn delete_ingress(&self, ingress_id: &str) -> ServiceResult<proto::IngressInfo> {
        self.client
            .request(
                SVC,
                "DeleteIngress",
                proto::DeleteIngressRequest {
                    ingress_id: ingress_id.to_owned(),
                },
                self.base.auth_header(VideoGrants {
                    ingress_admin: true,
                }),
            )
            .await
            .map_err(Into::into)
    }
}
