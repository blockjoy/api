macro_rules! grpc_clients {
    ( $( $snake_case:ident => $camel_case:ident ),* $(,)? ) => {
        $( paste::paste! {
            impl crate::setup::helper::traits::GrpcClient<Channel> for blockvisor_api::grpc::api::
              [< $snake_case _service_client >]::[< $camel_case ServiceClient >]<Channel> {
                fn create(channel: Channel) -> Self {
                    Self::new(channel)
                }
            }
        } )*
    };
}
