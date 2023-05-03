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
        let props = [
            (
                serde_json::json!({"enabled": false, "policy": "all"}),
                SelfUpgrade {
                    enabled: false,
                    policy: SelfUpgradePolicy::All,
                },
            ),
            (
                serde_json::json!({"enabled": true, "policy": "not_major"}),
                SelfUpgrade {
                    enabled: true,
                    policy: SelfUpgradePolicy::NotMajor,
                },
            ),
        ];
        for (prop, expected) in props {
            let actual: SelfUpgrade = serde_json::from_value(prop).unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn parsing_error_self_upgrade() {
        let props = [(
            serde_json::json!({"bled": true, "policy": "all"}),
            SelfUpgrade {
                enabled: true,
                policy: SelfUpgradePolicy::All,
            },
        )];
        for (prop, _) in props {
            let actual: Result<SelfUpgrade, _> = serde_json::from_value(prop);
            assert!(actual.is_err());
        }
    }
}
