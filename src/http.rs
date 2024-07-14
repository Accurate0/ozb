use anyhow::Context;
use reqwest::{Proxy, Request, Response};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use reqwest_tracing::{
    default_on_request_end, reqwest_otel_span, ReqwestOtelSpanBackend, TracingMiddleware,
};
use std::time::{Duration, Instant};
use tracing::Span;

pub struct TimeTrace;
impl ReqwestOtelSpanBackend for TimeTrace {
    fn on_request_start(req: &Request, extension: &mut http::Extensions) -> Span {
        let url = req.url().as_str();
        extension.insert(Instant::now());

        reqwest_otel_span!(
            name = format!("{} {}", req.method(), url),
            req,
            url = url,
            time_elapsed = tracing::field::Empty,
            time_elapsed_formatted = tracing::field::Empty
        )
    }

    fn on_request_end(
        span: &Span,
        outcome: &reqwest_middleware::Result<Response>,
        extension: &mut http::Extensions,
    ) {
        let time_elapsed = extension.get::<Instant>().unwrap().elapsed().as_millis() as i64;
        default_on_request_end(span, outcome);
        span.record("time_elapsed", time_elapsed);
        span.record("time_elapsed_formatted", format!("{time_elapsed}ms"));
    }
}

fn get_default_middleware_http_client(
    client: reqwest::Client,
) -> reqwest_middleware::ClientWithMiddleware {
    get_default_middleware(client).build()
}

pub fn get_default_middleware(client: reqwest::Client) -> reqwest_middleware::ClientBuilder {
    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(2);
    reqwest_middleware::ClientBuilder::new(client)
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .with(TracingMiddleware::<TimeTrace>::new())
}

pub fn get_default_http_client() -> reqwest_middleware::ClientWithMiddleware {
    get_http_client(get_default_middleware_http_client, None)
}

pub fn get_http_client<T>(
    wrap_in_middleware: T,
    proxy: Option<Proxy>,
) -> reqwest_middleware::ClientWithMiddleware
where
    T: Fn(reqwest::Client) -> reqwest_middleware::ClientWithMiddleware,
{
    let client = reqwest::ClientBuilder::new();

    let client = if let Some(proxy) = proxy {
        client.proxy(proxy)
    } else {
        client
    };

    let client = client
        .timeout(Duration::from_secs(10))
        .build()
        .context("Failed to build http client")
        .unwrap();
    wrap_in_middleware(client)
}
