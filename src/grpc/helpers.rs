use tonic::Status;

pub fn required(name: &'static str) -> impl Fn() -> Status {
    move || Status::invalid_argument(format!("`{name}` is required"))
}
