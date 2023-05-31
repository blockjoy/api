use super::api::{self, blockchain_service_server, SupportedNodeProperty};
use crate::cookbook::get_networks;
use crate::models;
use futures_util::future::join_all;
use std::collections::HashMap;

#[tonic::async_trait]
impl blockchain_service_server::BlockchainService for super::GrpcImpl {
    async fn get(
        &self,
        req: tonic::Request<api::BlockchainServiceGetRequest>,
    ) -> super::Resp<api::BlockchainServiceGetResponse> {
        let mut conn = self.conn().await?;
        let resp = get(req, &mut conn).await?;
        Ok(resp)
    }

    async fn list(
        &self,
        req: tonic::Request<api::BlockchainServiceListRequest>,
    ) -> super::Resp<api::BlockchainServiceListResponse> {
        let mut conn = self.conn().await?;
        let resp = list(req, &mut conn).await?;
        Ok(resp)
    }
}

async fn get(
    req: tonic::Request<api::BlockchainServiceGetRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::BlockchainServiceGetResponse> {
    let req: api::BlockchainServiceGetRequest = req.into_inner();
    let id = req.id.parse()?;
    let blockchain = models::Blockchain::find_by_id(id, conn).await?;
    let resp = api::BlockchainServiceGetResponse {
        blockchain: Some(api::Blockchain::from_model(blockchain, conn).await?),
    };
    Ok(tonic::Response::new(resp))
}

async fn list(
    _: tonic::Request<api::BlockchainServiceListRequest>,
    conn: &mut diesel_async::AsyncPgConnection,
) -> super::Result<api::BlockchainServiceListResponse> {
    // We need to combine info from two seperate sources: the database and cookbook. Since
    // cookbook is slow, the step where we call it is parallelized.

    // We query the necessary blockchains from the database.
    let blockchains = models::Blockchain::find_all(conn).await?;

    // This list will contain the dto's that are sent over gRPC after the information from
    // cookbook has been added to them.
    let mut grpc_blockchains = api::Blockchain::from_models(blockchains.clone(), conn).await?;

    // Now we need to combine this info with the networks that are stored in the cookbook
    // service. Since we want to do this in parallel, this list will contain a number of futures
    // that each resolve to a list of networks for that blockchain.
    let mut network_futs = vec![];
    for blockchain in &blockchains {
        for node_properties in blockchain.properties(conn).await? {
            let name = blockchain.name.clone();
            let node_type = node_properties.node_type.to_string();
            let version = node_properties.version.clone();
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

    let resp = api::BlockchainServiceListResponse {
        blockchains: grpc_blockchains,
    };
    Ok(tonic::Response::new(resp))
}

/// This is a helper function for BlockchainService::list. It retrieves the networks for a given set
/// of query parameters, and logs an error when something goes wrong. This behaviour is important,
/// because calls to cookbook sometimes fail and we don't want this whole endpoint to crash when
/// cookbook is having a sad day.
async fn try_get_networks(
    blockchain_id: uuid::Uuid,
    name: String,
    node_type: String,
    version: String,
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
    async fn from_models(
        models: Vec<models::Blockchain>,
        conn: &mut diesel_async::AsyncPgConnection,
    ) -> crate::Result<Vec<Self>> {
        let properties = models::BlockchainProperty::by_blockchains(&models, conn).await?;
        let mut properties_map: HashMap<uuid::Uuid, Vec<_>> = HashMap::new();
        for property in properties {
            properties_map
                .entry(property.blockchain_id)
                .or_default()
                .push(property);
        }

        models
            .into_iter()
            .map(|model| {
                let properties = properties_map.get(&model.id).cloned().unwrap_or_default();
                let mut blockchain = Self {
                    id: model.id.to_string(),
                    name: model.name,
                    // TODO: make this column mandatory
                    description: model.description.unwrap_or_default(),
                    status: 0, // We use the setter to set this field for type-safety
                    project_url: model.project_url,
                    repo_url: model.repo_url,
                    version: model.version,
                    nodes_types: api::SupportedNodeType::from_models(properties)?,
                    created_at: Some(super::try_dt_to_ts(model.created_at)?),
                    updated_at: Some(super::try_dt_to_ts(model.updated_at)?),
                    networks: vec![],
                };
                blockchain.set_status(api::BlockchainStatus::from_model(model.status));
                Ok(blockchain)
            })
            .collect()
    }

    async fn from_model(
        model: models::Blockchain,
        conn: &mut diesel_async::AsyncPgConnection,
    ) -> crate::Result<Self> {
        Ok(Self::from_models(vec![model], conn).await?[0].clone())
    }
}

impl api::SupportedNodeType {
    fn from_models(models: Vec<models::BlockchainProperty>) -> crate::Result<Vec<Self>> {
        // First we partition the properties by version
        let mut properties: HashMap<(models::NodeType, &str), Vec<_>> = HashMap::new();
        for model in &models {
            properties
                .entry((model.node_type, &model.version))
                .or_default()
                .push(model);
        }

        models
            .iter()
            .map(|model| {
                let properties = &properties[&(model.node_type, model.version.as_str())];
                let mut props = api::SupportedNodeType {
                    node_type: 0, // We use the setter to set this field for type-safety
                    version: model.version.to_string(),
                    properties: properties
                        .iter()
                        .map(|prop| api::SupportedNodeProperty::from_model(prop))
                        .collect(),
                };
                props.set_node_type(api::NodeType::from_model(model.node_type));
                Ok(props)
            })
            .collect()
    }
}

impl api::SupportedNodeProperty {
    fn from_model(model: &models::BlockchainProperty) -> Self {
        let mut prop = SupportedNodeProperty {
            name: model.name.clone(),
            default: model.default.clone(),
            ui_type: 0, // We use the setter to set this field for type-safety
            disabled: model.disabled,
            required: model.required,
        };
        prop.set_ui_type(api::UiType::from_model(model.ui_type));
        prop
    }
}

impl api::BlockchainStatus {
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
