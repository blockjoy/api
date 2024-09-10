macro_rules! grpc_clients {
    ( $( $snake_case:ident => $pascal_case:ident ),* $(,)? ) => {
        $( paste::paste! {
            #[allow(dead_code)]
            pub type [< $pascal_case Service >] = blockvisor_api::grpc::api::[< $snake_case _service_client >]::[< $pascal_case ServiceClient >]<Channel>;

            impl crate::setup::helper::traits::GrpcClient<Channel> for blockvisor_api::grpc::api::
            [< $snake_case _service_client >]::[< $pascal_case ServiceClient >]<Channel> {
                fn create(channel: Channel) -> Self {
                    Self::new(channel)
                }
            }
        } )*
    };
}

/// Returns the name of the current test.
#[macro_export]
macro_rules! test_name {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }

        let full_name = type_name_of(f);
        full_name
            .strip_suffix("::{{closure}}::f")
            .and_then(|name| name.strip_prefix("suite::"))
            .unwrap_or(full_name)
    }};
}
