use crate::grpc::api::{Action, Direction, Rule};
use crate::models::{self, Node};
use crate::Error;
use crate::Result as ApiResult;
use cidr_utils::cidr::IpCidr;

pub fn create_rules_for_node(node: &Node) -> ApiResult<Vec<Rule>> {
    let rules = create_firewall_rules(node.allow_ips()?, Action::Allow)?
        .into_iter()
        .chain(create_firewall_rules(node.deny_ips()?, Action::Deny)?)
        .collect();
    Ok(rules)
}

fn create_firewall_rules(
    // I'll leave the Vec for now, maybe we need it later
    denied_or_allowed_ips: Vec<models::FilteredIpAddr>,
    action: Action,
) -> ApiResult<Vec<Rule>> {
    let mut rules = vec![];
    for ip in denied_or_allowed_ips {
        // Validate IP
        if !IpCidr::is_ip_cidr(&ip.ip) {
            return Err(Error::Cidr);
        }

        rules.push(Rule {
            name: "".to_string(),
            action: action as i32,
            direction: Direction::In as i32,
            protocol: None,
            ips: Some(ip.ip),
            ports: vec![],
        });
    }

    Ok(rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_firewall_rules() {
        let ips = ["1.2.3.3/32", "1.2.3.3/24", "1.2.3.3", "1.2.3.3/16"];
        let ips = ips
            .iter()
            .map(|ip| models::FilteredIpAddr {
                ip: ip.to_string(),
                description: Some("muffin".to_string()),
            })
            .collect();
        create_firewall_rules(ips, Action::Allow).unwrap();
    }
}
