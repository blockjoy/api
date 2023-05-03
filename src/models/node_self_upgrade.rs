use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct SelfUpgrade {
    pub enabled: bool,
    pub policy: SelfUpgradePolicy,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum SelfUpgradePolicy {
    #[serde(rename = "all")]
    All,
    #[serde(rename = "not_major")]
    NotMajor,
}

#[cfg(test)]
mod tests {
    use crate::models::SelfUpgrade;
    use crate::models::SelfUpgradePolicy;

    #[test]
    fn parsing_correct_self_upgrade() {
        let props = [(
            serde_json::json!({"enabled": "true", "policy": "all"}),
            SelfUpgrade {
                enabled: true,
                policy: SelfUpgradePolicy::All,
            },
        )];
        for (prop, expected) in props {
            let actual: SelfUpgrade = serde_json::from_value(prop).unwrap();
            assert_eq!(actual, expected);
        }
    }
}
