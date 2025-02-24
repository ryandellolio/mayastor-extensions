use crate::upgrade::common::error::Error;
use clap::Parser;
use core::time;
use once_cell::sync::OnceCell;
use openapi::tower::client::{ApiClient, Configuration};
use tracing::error;
use url::Url;

use super::{helm::client::HelmClient, k8s::client::K8sClient};

static CONFIG: OnceCell<UpgradeConfig> = OnceCell::new();

/// Cli Args to initialize rest-endpoint, namespace and chart.
#[derive(Parser)]
#[clap(author, version, about)]
pub struct CliArgs {
    /// An URL endpoint to the control plane's rest endpoint.
    #[clap(short = 'e', long, default_value = "http://mayastor-api-rest:8081")]
    rest_endpoint: Url,

    /// The namespace we are supposed to operate in.
    #[clap(short, long, default_value = "mayastor")]
    namespace: String,

    /// The chart_name we are supposed to operate in.
    #[clap(short = 'r', long, default_value = "mayastor")]
    release_name: String,
}

impl CliArgs {
    pub fn args() -> Self {
        CliArgs::parse()
    }
}

/// Upgrade config that can be passed through arguments.
pub struct UpgradeConfig {
    k8s_client: K8sClient,
    helm_client: HelmClient,
    rest_client: ApiClient,
    namespace: String,
    release_name: String,
}

impl UpgradeConfig {
    /// Initialize operator configs.
    pub async fn initialize(args: CliArgs) -> Result<(), Error> {
        let k8s_client = K8sClient::new().await.map_err(|error| {
            error!(?error, "Failed to generate kube API client");
            error
        })?;
        let rest_endpoint = args.rest_endpoint;
        let config_rest = Configuration::builder()
            .with_timeout(time::Duration::from_secs(30))
            .with_tracing(true)
            .build_url(rest_endpoint)
            .map_err(|error| Error::OpenapiClientConfigurationErr {
                source: anyhow::anyhow!(
                    "Failed to create openapi configuration, Error: '{:?}'",
                    error
                ),
            })?;
        let rest_client = ApiClient::new(config_rest);
        let namespace = args.namespace;
        let release_name = args.release_name;
        let helm_client = HelmClient::new()
            .await?
            .with_chart(release_name.to_string(), namespace.to_string())?;

        CONFIG.get_or_init(|| Self {
            k8s_client,
            helm_client,
            rest_client,
            namespace: namespace.to_string(),
            release_name: release_name.to_string(),
        });
        Ok(())
    }

    /// Get Upgrade operator config.
    pub fn get_config() -> &'static UpgradeConfig {
        CONFIG
            .get()
            .expect("Upgrade operator config is not initialized")
    }

    /// Get k8s client.
    pub fn k8s_client(&self) -> K8sClient {
        self.k8s_client.clone()
    }

    /// Get helm client.
    pub(crate) fn helm_client(&self) -> HelmClient {
        self.helm_client.clone()
    }

    /// Get mayastor rest api client.
    pub(crate) fn rest_client(&self) -> ApiClient {
        self.rest_client.clone()
    }

    /// Get namespace.
    pub(crate) fn namespace(&self) -> &String {
        &self.namespace
    }

    /// Get chart name.
    pub(crate) fn chart_name(&self) -> &String {
        &self.release_name
    }
}
