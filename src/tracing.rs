use std::collections::HashMap;
use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::{BatchConfigBuilder, Config as TraceConfig, Tracer};
use opentelemetry_sdk::Resource;
use opentelemetry_semantic_conventions::resource::DEPLOYMENT_ENVIRONMENT;
use opentelemetry_semantic_conventions::resource::{
    SERVICE_NAME, TELEMETRY_SDK_LANGUAGE, TELEMETRY_SDK_NAME, TELEMETRY_SDK_VERSION,
};
use tracing::Level;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const INGEST_URL: &str = "https://api.axiom.co/v1/traces";

pub fn external_tracer(name: &'static str) -> Tracer {
    let token = std::env::var("AXIOM_TOKEN").expect("must have axiom token configured");
    let dataset_name = std::env::var("AXIOM_DATASET").expect("must have axiom dataset configured");

    let mut headers = HashMap::with_capacity(3);
    headers.insert("Authorization".to_string(), format!("Bearer {token}"));
    headers.insert("X-Axiom-Dataset".to_string(), dataset_name);
    headers.insert(
        "User-Agent".to_string(),
        format!("tracing-axiom/{}", env!("CARGO_PKG_VERSION")),
    );

    let tags = vec![
        KeyValue::new(TELEMETRY_SDK_NAME, "external-tracer".to_string()),
        KeyValue::new(TELEMETRY_SDK_VERSION, env!("CARGO_PKG_VERSION").to_string()),
        KeyValue::new(TELEMETRY_SDK_LANGUAGE, "rust".to_string()),
        KeyValue::new(SERVICE_NAME, name),
        KeyValue::new(
            DEPLOYMENT_ENVIRONMENT,
            if cfg!(debug_assertions) {
                "development"
            } else {
                "production"
            },
        ),
    ];

    let trace_config = TraceConfig::default().with_resource(Resource::new(tags));
    let batch_config = BatchConfigBuilder::default()
        .with_max_export_timeout(Duration::from_secs(3))
        .build();

    let pipeline = opentelemetry_otlp::new_exporter()
        .http()
        .with_http_client(reqwest_old::Client::new())
        .with_endpoint(INGEST_URL)
        .with_headers(headers)
        .with_timeout(Duration::from_secs(3));

    opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(pipeline)
        .with_trace_config(trace_config)
        .with_batch_config(batch_config)
        .install_batch(opentelemetry_sdk::runtime::Tokio)
        .unwrap()
}

pub fn init_logger(name: &'static str) {
    let tracer = external_tracer(name);

    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

    tracing_subscriber::registry()
        .with(
            Targets::default()
                // .with_target("aws_smithy_http_tower::parse_response", Level::WARN)
                // .with_target("aws_config::default_provider::credentials", Level::WARN)
                .with_default(Level::INFO),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .init();
}
