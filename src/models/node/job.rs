#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeJob {
    pub name: String,
    pub status: Option<NodeJobStatus>,
    pub exit_code: Option<i32>,
    pub message: Option<String>,
    pub logs: Vec<String>,
    pub restarts: u64,
    pub progress: Option<NodeJobProgress>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeJobStatus {
    Pending,
    Running,
    Finished,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NodeJobProgress {
    pub total: Option<u32>,
    pub current: Option<u32>,
    pub message: Option<String>,
}
