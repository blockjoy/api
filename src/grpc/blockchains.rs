use super::api::{self, blockchains_server, SupportedNodeProperty};
use crate::cookbook::get_networks;
use crate::models;
use futures_util::future::join_all;
use std::collections::HashMap;

#[tonic::async_trait]
impl blockchains_server::Blockchains for super::GrpcImpl {
    async fn get(
        &self,
        request: tonic::Request<api::GetBlockchainRequest>,
    ) -> super::Result<api::GetBlockchainResponse> {
        let refresh_token = super::get_refresh_token(&request);
        let inner = request.into_inner();
        let id = inner.id.parse().map_err(crate::Error::from)?;
        let mut conn = self.conn().await?;
        let blockchain = models::Blockchain::find_by_id(id, &mut conn)
            .await
            .map_err(|_| tonic::Status::not_found("No such blockchain"))?;
        let response = api::GetBlockchainResponse {
            blockchain: Some(api::Blockchain::from_model(blockchain)?),
        };
        super::response_with_refresh_token(refresh_token, response)
    }

    async fn list(
        &self,
        request: tonic::Request<api::ListBlockchainsRequest>,
    ) -> super::Result<api::ListBlockchainsResponse> {
        // We need to combine info from two seperate sources: the database and cookbook. Since
        // cookbook is slow, the step where we call it is parallelized.
        let refresh_token = super::get_refresh_token(&request);

        // We query the necessary blockchains from the database.
        let mut conn = self.conn().await?;
        let blockchains = models::Blockchain::find_all(&mut conn).await?;

        // This list will contain the dto's that are sent over gRPC after the information from
        // cookbook has been added to them.
        let mut grpc_blockchains = blockchains
            .iter()
            .cloned()
            .map(api::Blockchain::from_model)
            .collect::<crate::Result<Vec<_>>>()?;

        // Now we need to combine this info with the networks that are stored in the cookbook
        // service. Since we want to do this in parallel, this list will contain a number of futures
        // that each resolve to a list of networks for that blockchain.
        let mut network_futs = vec![];
        for blockchain in &blockchains {
            for node_properties in blockchain.supported_node_types()? {
                let name = blockchain.name.clone();
                let node_type = node_properties.id.try_into();
                let node_type = node_type.unwrap_or(models::NodeType::Unknown).to_string();
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
            dto.networks = networks_map.get(&model.id).cloned().unwrap_or_default();
        }

        let response = api::ListBlockchainsResponse {
            blockchains: grpc_blockchains,
        };

        super::response_with_refresh_token(refresh_token, response)
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
) -> (uuid::Uuid, Vec<api::BlockchainNetwork>) {
    // We prepare an error message because we are moving all the arguments used to construct it.
    let err_msg = format!("Could not get networks for {name} {node_type} version {version:?}");

    let networks = match get_networks(name, node_type, version).await {
        Ok(nets) => nets.into_iter().map(Into::into).collect(),
        Err(e) => {
            tracing::error!("{err_msg}: {e}");
            vec![]
        }
    };
    (blockchain_id, networks)
}

impl api::Blockchain {
    fn from_model(model: models::Blockchain) -> crate::Result<Self> {
        let nodes_types = model
            .supported_node_types()?
            .into_iter()
            .map(api::SupportedNodeType::from_model)
            .collect::<crate::Result<_>>()?;

        let mut blockchain = Self {
            id: model.id.to_string(),
            name: model.name,
            // TODO: make this column mandatory
            description: model.description.unwrap_or_default(),
            status: 0, // We use the setter to set this field for type-safety
            project_url: model.project_url,
            repo_url: model.repo_url,
            version: model.version,
            nodes_types,
            created_at: Some(super::try_dt_to_ts(model.created_at)?),
            updated_at: Some(super::try_dt_to_ts(model.updated_at)?),
            networks: vec![],
        };
        blockchain.set_status(api::blockchain::BlockchainStatus::from_model(model.status));
        Ok(blockchain)
    }
}

impl api::SupportedNodeType {
    fn from_model(model: models::BlockchainProperties) -> crate::Result<Self> {
        let mut props = api::SupportedNodeType {
            node_type: 0, // We use the setter to set this field for type-safety
            version: model.version,
            properties: model
                .properties
                .unwrap_or_default()
                .into_iter()
                .map(api::SupportedNodeProperty::from_model)
                .collect(),
        };
        let model = models::NodeType::try_from(model.id)?;
        props.set_node_type(api::node::NodeType::from_model(model));
        Ok(props)
    }
}

impl api::SupportedNodeProperty {
    fn from_model(model: models::BlockchainPropertyValue) -> Self {
        let mut prop = SupportedNodeProperty {
            name: model.name,
            default: model.default,
            ui_type: 0, // We use the setter to set this field for type-safety
            disabled: model.disabled,
            required: model.required,
        };
        prop.set_ui_type(api::UiType::from_model(model.ui_type));
        prop
    }
}

impl api::UiType {
    fn from_model(model: models::BlockchainPropertyUiType) -> Self {
        match model {
            models::BlockchainPropertyUiType::FileUpload => api::UiType::FileUpload,
            models::BlockchainPropertyUiType::Password => api::UiType::Password,
            models::BlockchainPropertyUiType::Text => api::UiType::Text,
            models::BlockchainPropertyUiType::Switch => api::UiType::Switch,
        }
    }
}

impl api::blockchain::BlockchainStatus {
    fn from_model(model: models::BlockchainStatus) -> Self {
        match model {
            models::BlockchainStatus::Development => Self::Development,
            models::BlockchainStatus::Alpha => Self::Alpha,
            models::BlockchainStatus::Beta => Self::Beta,
            models::BlockchainStatus::Production => Self::Production,
            models::BlockchainStatus::Deleted => Self::Deleted,
        }
    }
}
