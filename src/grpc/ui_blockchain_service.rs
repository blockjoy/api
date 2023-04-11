use std::collections::HashMap;

use futures_util::future::join_all;

use super::blockjoy_ui::blockchain_service_server::BlockchainService;
use super::blockjoy_ui::{self, ResponseMeta};
use super::convert;
use crate::auth::UserAuthToken;
use crate::cookbook::get_networks;
use crate::grpc::helpers::try_get_token;
use crate::grpc::{get_refresh_token, response_with_refresh_token};
use crate::models;

type Result<T, E = tonic::Status> = std::result::Result<T, E>;

impl blockjoy_ui::Blockchain {
    fn from_model(model: models::Blockchain) -> crate::Result<Self> {
        let supported_nodes_types = serde_json::to_string(&model.supported_node_types()?)?;

        let blockchain = Self {
            id: Some(model.id.to_string()),
            name: Some(model.name.clone()),
            description: model.description.clone(),
            status: model.status as i32,
            project_url: model.project_url.clone(),
            repo_url: model.repo_url.clone(),
            supports_etl: model.supports_etl,
            supports_node: model.supports_node,
            supports_staking: model.supports_staking,
            supports_broadcast: model.supports_broadcast,
            version: model.version.clone(),
            supported_nodes_types,
            created_at: Some(convert::try_dt_to_ts(model.created_at)?),
            updated_at: Some(convert::try_dt_to_ts(model.updated_at)?),
            networks: vec![],
        };
        Ok(blockchain)
    }
}

#[tonic::async_trait]
impl BlockchainService for super::GrpcImpl {
    async fn get(
        &self,
        request: tonic::Request<blockjoy_ui::GetBlockchainRequest>,
    ) -> Result<tonic::Response<blockjoy_ui::GetBlockchainResponse>> {
        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let inner = request.into_inner();
        let id = inner.id.parse().map_err(crate::Error::from)?;
        let mut conn = self.conn().await?;
        let blockchain = models::Blockchain::find_by_id(id, &mut conn)
            .await
            .map_err(|_| tonic::Status::not_found("No such blockchain"))?;
        let response = blockjoy_ui::GetBlockchainResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token))),
            blockchain: Some(blockjoy_ui::Blockchain::from_model(blockchain)?),
        };
        response_with_refresh_token(refresh_token, response)
    }

    async fn list(
        &self,
        request: tonic::Request<blockjoy_ui::ListBlockchainsRequest>,
    ) -> Result<tonic::Response<blockjoy_ui::ListBlockchainsResponse>> {
        // We need to combine info from two seperate sources: the database and cookbook. Since
        // cookbook is slow, the step where we call it is parallelized.

        let refresh_token = get_refresh_token(&request);
        let token = try_get_token::<_, UserAuthToken>(&request)?.try_into()?;
        let inner = request.into_inner();

        // We query the necessary blockchains from the database.
        let mut conn = self.conn().await?;
        let blockchains = models::Blockchain::find_all(&mut conn).await?;

        // This list will contain the dto's that are sent over gRPC after the information from
        // cookbook has been added to them.
        let mut grpc_blockchains = blockchains
            .iter()
            .cloned()
            .map(blockjoy_ui::Blockchain::from_model)
            .collect::<crate::Result<Vec<_>>>()?;

        // Now we need to combine this info with the networks that are stored in the cookbook
        // service. Since we want to do this in parallel, this list will contain a number of futures
        // that each resolve to a list of networks for that blockchain.
        let mut network_futs = vec![];
        for blockchain in &blockchains {
            for node_properties in blockchain.supported_node_types()? {
                let name = blockchain.name.clone();
                let node_type = models::NodeType::str_from_value(node_properties.id);
                let version = Some(node_properties.version.clone());
                network_futs.push(try_get_networks(blockchain.id, name, node_type, version));
            }
        }
        let networks = join_all(network_futs).await;

        // Now that we have fetched our networks, we have to stuff them into the DTO's. To do this
        // we change the list of tuples into a hashmap, mapping the blockchain_id to the networks
        // belonging to it.
        let mut networks_map: HashMap<uuid::Uuid, Vec<_>> = HashMap::new();
        for (blockchain_id, network) in networks {
            networks_map
                .entry(blockchain_id)
                .or_default()
                .extend(network.into_iter());
        }

        // Now that we have our map, we can simply index it with each blockchain id to get the
        // networks belonging to blockchain.
        for (model, dto) in blockchains.into_iter().zip(grpc_blockchains.iter_mut()) {
            dto.networks = networks_map[&model.id].clone();
        }

        let response = blockjoy_ui::ListBlockchainsResponse {
            meta: Some(ResponseMeta::from_meta(inner.meta, Some(token))),
            blockchains: grpc_blockchains,
        };

        response_with_refresh_token(refresh_token, response)
    }
}

/// This is a helper function for BlockchainService::list. It retrieves the networks for a given set
/// of query parameters, and logs an error when something goes wrong. This behaviour is important,
/// because calls to cookbook sometimes fail and we don't want this whole endpoint to crash when
/// cookbook is having a sad day.
async fn try_get_networks(
    blockchain_id: uuid::Uuid,
    name: String,
    node_type: String,
    version: Option<String>,
) -> (uuid::Uuid, Vec<blockjoy_ui::BlockchainNetwork>) {
    // We prepare an error message because we are moving all the arguments used to construct it.
    let err_msg = format!("Could not get networks for {name} {node_type} version {version:?}");

    let networks = match get_networks(name, node_type, version).await {
        Ok(nets) => nets.iter().map(Into::into).collect(),
        Err(e) => {
            tracing::error!("{err_msg}: {e}");
            vec![]
        }
    };
    (blockchain_id, networks)
}
