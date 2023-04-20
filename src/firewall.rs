use crate::grpc::api::{Action, Direction, Rule};
use crate::models::Node;
use crate::Error;
use crate::Result as ApiResult;
use anyhow::anyhow;
use cidr_utils::cidr::IpCidr;

pub fn create_rules_for_node(node: &Node) -> ApiResult<Vec<Rule>> {
    let mut rules: Vec<Rule> = vec![];
    let allow_ips = node
        .allow_ips
        .as_array()
        .ok_or_else(|| Error::UnexpectedError(anyhow!("No allowed IPs defined")))?;
    let deny_ips = node
        .deny_ips
        .as_array()
        .ok_or_else(|| Error::UnexpectedError(anyhow!("No deny IPs defined")))?;
    let mut allow_rules = create_firewall_rules(allow_ips, Action::Allow)?;
    let mut deny_rules = create_firewall_rules(deny_ips, Action::Deny)?;

    rules.append(&mut allow_rules);
    rules.append(&mut deny_rules);

    Ok(rules)
}

fn create_firewall_rules(
    // I'll leave the Vec for now, maybe we need it later
    denied_or_allowed_ips: &Vec<serde_json::Value>,
    action: Action,
) -> ApiResult<Vec<Rule>> {
    let mut rules = vec![];
    for ip in denied_or_allowed_ips {
        let ip = ip
            .as_object()
            .ok_or_else(|| Error::UnexpectedError(anyhow!("Unknown IP object")))?;
        let ip = String::from(
            ip.get("ip")
                .ok_or_else(|| Error::UnexpectedError(anyhow!("Invalid IP format")))?
                .as_str()
                .unwrap_or_default(),
        );

        // Validate IP
        if IpCidr::is_ip_cidr(ip.as_str()) {
            return Err(Error::Cidr);
        }

        rules.push(Rule {
            name: "".to_string(),
            action: action as i32,
            direction: Direction::In as i32,
            protocol: None,
            ips: Some(ip),
            ports: vec![],
        });
    }

    Ok(rules)
}
