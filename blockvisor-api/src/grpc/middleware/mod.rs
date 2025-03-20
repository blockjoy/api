use std::task::{Context, Poll};

use futures::future::BoxFuture;
use hyper::body::Body;
use hyper::{Request, Response};
use opentelemetry::trace::{FutureExt, Status, TraceContextExt, Tracer};
use opentelemetry::{KeyValue, global};
use opentelemetry_semantic_conventions::trace::{HTTP_RESPONSE_STATUS_CODE, RPC_GRPC_STATUS_CODE};
use tonic::Code;
use tower::{Layer, Service};

#[derive(Clone, Copy, Debug, Default)]
pub struct MetricsLayer;

impl<S> Layer<S> for MetricsLayer {
    type Service = MetricsService<S>;

    fn layer(&self, service: S) -> Self::Service {
        MetricsService { service }
    }
}

#[derive(Clone, Debug)]
pub struct MetricsService<S> {
    service: S,
}

impl<B, S> Service<Request<B>> for MetricsService<S>
where
    B: Body + Send + 'static,
    S: Service<Request<B>, Response = Response<axum::body::Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: ToString,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        // https://github.com/tower-rs/tower/issues/547#issuecomment-767629149
        let service = self.service.clone();
        let mut service = std::mem::replace(&mut self.service, service);

        let tracer = global::tracer("grpc");
        let path = request.uri().path();
        let path = path.strip_prefix('/').unwrap_or(path).to_string();

        let span = tracer.start(path);
        let ctx = opentelemetry::Context::current_with_span(span);

        Box::pin(async move {
            match service.call(request).with_context(ctx.clone()).await {
                Ok(response) => {
                    let span = ctx.span();
                    let http_status = response.status().as_u16();
                    let grpc_status = match tonic::Status::from_header_map(response.headers()) {
                        Some(status) if status.code() == Code::Ok => {
                            span.set_status(Status::Ok);
                            status.code()
                        }
                        Some(status) => {
                            span.set_status(Status::error(status.to_string()));
                            status.code()
                        }
                        None => Code::Ok,
                    };

                    span.set_attribute(KeyValue::new(RPC_GRPC_STATUS_CODE, grpc_status as i64));
                    span.set_attribute(KeyValue::new(
                        HTTP_RESPONSE_STATUS_CODE,
                        i64::from(http_status),
                    ));
                    span.end();

                    Ok(response)
                }

                Err(error) => {
                    let span = ctx.span();
                    span.set_status(Status::error(error.to_string()));
                    span.end();

                    Err(error)
                }
            }
        })
    }
}
