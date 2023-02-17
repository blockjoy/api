use super::blockjoy_ui::{self, ResponseMeta};
use crate::auth::UserAuthToken;
use crate::cookbook::get_networks;
use crate::errors::ApiError;
use crate::grpc::blockjoy_ui::blockchain_service_server::BlockchainService;
use crate::grpc::blockjoy_ui::Blockchain;
use crate::grpc::helpers::try_get_token;
use crate::grpc::{get_refresh_token, response_with_refresh_token};
use crate::models;
use crate::models::NodeTypeKey;

type Result<T, E = tonic::Status> = std::result::Result<T, E>;

pub struct BlockchainServiceImpl {
    db: models::DbPool,
}

impl BlockchainServiceImpl {
    pub fn new(db: models::DbPool) -> Self {
        Self { db }
    }
}

#[tonic::async_trait]
impl BlockchainService for BlockchainServiceImpl {
    async fn get(
        &self,
        request: tonic::Request<blockjoy_ui::GetBlockchainRequest>,
    ) -> Result<tonic::Response<blockjoy_ui::GetBlockchainResponse>> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let inner = request.into_inner();
        let id = inner.id.parse().map_err(ApiError::from)?;
        let mut tx = self.db.begin().await?;
        let blockchain = models::Blockchain::find_by_id(id, &mut tx)
            .await
            .map_err(|_| tonic::Status::not_found("No such blockchain"))?;
        tx.commit().await?;
        let response = blockjoy_ui::GetBlockchainResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token))),
            blockchain: Some(blockchain.try_into()?),
        };
        Ok(response_with_refresh_token(refresh_token, response)?)
    }

    async fn list(
        &self,
        request: tonic::Request<blockjoy_ui::ListBlockchainsRequest>,
    ) -> Result<tonic::Response<blockjoy_ui::ListBlockchainsResponse>> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let inner = request.into_inner();
        let mut conn = self.db.begin().await?;
        let blockchains = models::Blockchain::find_all(&mut conn).await?;
        let mut grpc_blockchains = vec![];

        for blockchain in &blockchains {
            let node_types = &blockchain.supported_node_types.0;
            let mut g_chain: Blockchain = blockchain.try_into()?;

            for node_type in node_types {
                let nets = get_networks(
                    blockchain.name.clone(),
                    NodeTypeKey::str_from_value(node_type.get_id()),
                    Some(node_type.version().to_string()),
                )
                .await?;

                g_chain.networks.extend(nets.iter().map(|c| c.into()));
            }

            grpc_blockchains.push(g_chain);
        }

        let response = blockjoy_ui::ListBlockchainsResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token))),
            blockchains: grpc_blockchains,
        };

        Ok(response_with_refresh_token(refresh_token, response)?)
    }
}
