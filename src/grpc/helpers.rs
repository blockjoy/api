use tonic::Status;

pub fn required(name: &'static str) -> impl Fn() -> Status {
    move || Status::invalid_argument(format!("`{name}` is required"))
}

pub fn internal(error: impl std::fmt::Display) -> Status {
    Status::internal(error.to_string())
}
