use std::task::{Context, Poll};

use futures::future::BoxFuture;
use hyper::{Body, Request, Response};
use opentelemetry::global;
use opentelemetry::trace::{FutureExt, SpanKind, Status, TraceContextExt, Tracer};
use opentelemetry_http::HeaderExtractor;
use opentelemetry_semantic_conventions::trace::{HTTP_RESPONSE_STATUS_CODE, RPC_GRPC_STATUS_CODE};
use tonic::body::BoxBody;
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

impl<S> Service<Request<Body>> for MetricsService<S>
where
    S: Service<Request<Body>, Response = Response<BoxBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: ToString,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        let path = request.uri().path();
        let path = path.strip_prefix('/').unwrap_or(path).to_string();

        let extractor = HeaderExtractor(request.headers());
        let parent = global::get_text_map_propagator(|propagator| propagator.extract(&extractor));

        let tracer = global::tracer("grpc");
        let span = tracer
            .span_builder(path)
            .with_kind(SpanKind::Server)
            .start_with_context(&tracer, &parent);
        let context = parent.with_span(span);

        // https://github.com/tower-rs/tower/issues/547#issuecomment-767629149
        let service = self.service.clone();
        let mut service = std::mem::replace(&mut self.service, service);

        Box::pin(async move {
            match service.call(request).with_context(context.clone()).await {
                Ok(response) => {
                    let span = context.span();

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

                    span.set_attribute(RPC_GRPC_STATUS_CODE.i64(grpc_status as i64));
                    span.set_attribute(HTTP_RESPONSE_STATUS_CODE.i64(http_status as i64));
                    span.end();

                    Ok(response)
                }

                Err(error) => {
                    let span = context.span();
                    span.set_status(Status::error(error.to_string()));
                    span.end();

                    Err(error)
                }
            }
        })
    }
}
