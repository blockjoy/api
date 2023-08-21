use displaydoc::Display;
use serde::Deserialize;
use thiserror::Error;

use super::provider::{self, Provider};

const NODE_MAC_ADDRESS_PREFIX_VAR: &str = "NODE_MAC_ADDRESS_PREFIX";
const NODE_MAC_ADDRESS_PREFIX_ENTRY: &str = "network.node_mac_address_prefix";

#[derive(Debug, Display, Error)]
pub enum Error {
    /// {NODE_MAC_ADDRESS_PREFIX_ENTRY:?} not present: {0}
    MissingNodeMacAddressPrefix(provider::Error),
    /// Failed to parse {NODE_MAC_ADDRESS_PREFIX_ENTRY:?}: {0}
    ParseNodeMacAddressPrefix(String),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub node_mac_address_prefix: [u8; 3],
}

impl TryFrom<&Provider> for Config {
    type Error = Error;

    fn try_from(provider: &Provider) -> Result<Self, Self::Error> {
        Ok(Config {
            node_mac_address_prefix: provider
                .read(NODE_MAC_ADDRESS_PREFIX_VAR, NODE_MAC_ADDRESS_PREFIX_ENTRY)
                .map_err(Error::MissingNodeMacAddressPrefix)
                .map(parse_mac_address_prefix)??,
        })
    }
}

/// Case insensitively parses a mac address prefix of the form `AA:BB:CC`.
fn parse_mac_address_prefix(inp: String) -> Result<[u8; 3], Error> {
    let inp = inp.trim().to_lowercase();
    let err = Error::ParseNodeMacAddressPrefix;
    let (first, rest) = inp
        .split_once(':')
        .ok_or_else(|| err(format!("Cannot parse {inp}: Doesn't contain ':'")))?;
    let (second, third) = rest
        .split_once(':')
        .ok_or_else(|| err(format!("Cannot parse {inp}: Doesn't contain ':' twice")))?;
    let from_hex = |s| {
        u8::from_str_radix(s, 16)
            .map_err(|_| err(format!("Cannot parse {inp}: {s} isn't valid hex")))
    };
    Ok([from_hex(first)?, from_hex(second)?, from_hex(third)?])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mac_address_prefix() {
        let parse = |s: &str| parse_mac_address_prefix(s.to_string());
        assert_eq!(parse("00:11:22").unwrap(), [0, 17, 34]);
        assert_eq!(parse("00:FF:22").unwrap(), [0, 255, 34]);
        parse("00:111:22").unwrap_err();
        parse("00:11::22").unwrap_err();
        parse("0011:22").unwrap_err();
        parse("00:11:22:33").unwrap_err();
    }
}
