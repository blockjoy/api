//! Executable actions (command pattern)

use std::sync::Arc;
use sqlx::Postgres;

pub type ExecutionResult<R> = Result<R, Box<dyn std::error::Error>>;

/// Create executable action
/// An action is a small piece of business logic, usually combined with other actions
/// to perform more complex tasks
#[tonic::async_trait]
pub trait ExecutableAction<T, R> {
    /// Execute the action using given payload
    async fn execute(self, payload: T) -> ExecutionResult<R>;
}