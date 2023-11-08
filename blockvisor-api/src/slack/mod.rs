use displaydoc::Display;
use thiserror::Error;

use crate::{config::slack, models};

#[derive(Debug, Display, Error)]
pub enum Error {
    /// Reqwest error: {0}
    Reqwest(#[from] reqwest::Error),
}

pub struct Client {
    inner: reqwest::Client,
    config: slack::Config,
}

impl Client {
    pub async fn node_report(
        &self,
        report: String,
        node: models::Node,
        host: models::Host,
        org: models::Org,
        user: models::User,
        blockchain: models::Blockchain,
    ) -> Result<(), Error> {
        let (node_id, node_name, node_type) = (node.id, &node.name, node.node_type);
        let (host_id, host_name) = (host.id, &host.name);
        let (org_id, org_name) = (org.id, &host.name);
        let (user_name, user_email) = (user.name(), &user.email);
        let blockchain_name = &blockchain.name;

        let msg = format!(
            "Error reported about node {node_name} ({node_id}):
            {report}

            Report was submitted by user {user_name} ({user_email})
            Node Type: {node_type}
            Blockchain: {blockchain_name}
            Host: {host_name} ({host_id})
            Org: {org_name} ({org_id})"
        );
        self.send(msg).await
    }

    async fn send(&self, msg: String) -> Result<(), Error> {
        let msg = Message {
            token: &self.config.token,
            channel: &self.config.channel_id,
            text: msg,
        };
        self.inner.post(&self.config.url).json(&msg).send().await?;
        Ok(())
    }
}

#[derive(serde::Serialize)]
struct Message<'a> {
    token: &'a str,
    channel: &'a str,
    text: String,
}
