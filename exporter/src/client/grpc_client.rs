use crate::{client::ApiVersion, error::ExporterError};

use actix_web::http;
use rpc::io_engine::IoEngineClientV0;
use std::{str::FromStr, time::Duration};
use tokio::time::sleep;
use tonic::transport::Channel;
use tracing::error;

/// Timeout for gRPC
#[derive(Debug, Clone)]
pub struct Timeouts {
    connect: std::time::Duration,
    request: std::time::Duration,
}

impl Timeouts {
    /// return a new `Self` with the connect and request timeouts
    pub fn new(connect: std::time::Duration, request: std::time::Duration) -> Self {
        Self { connect, request }
    }
    /// timeout to establish connection to the node
    pub fn connect(&self) -> std::time::Duration {
        self.connect
    }
    /// timeout for the request itself
    pub fn request(&self) -> std::time::Duration {
        self.request
    }
}

/// Context for Grpc client
#[derive(Debug, Clone)]
pub struct GrpcContext {
    endpoint: tonic::transport::Endpoint,
    timeouts: Timeouts,
    api_version: ApiVersion,
}

impl GrpcContext {
    /// initialize context
    pub fn new(
        endpoint: &str,
        timeouts: Timeouts,
        api_version: ApiVersion,
    ) -> Result<Self, ExporterError> {
        let uri = format!("http://{}", endpoint);
        let uri = match http::uri::Uri::from_str(&uri) {
            Ok(uri) => uri,
            Err(err) => {
                error!(error=%err, "Encountered error while parsing uri");
                return Err(ExporterError::InvalidURI("Invalid uri:{}".to_string()));
            }
        };
        let endpoint = tonic::transport::Endpoint::from(uri)
            .connect_timeout(timeouts.connect() + Duration::from_millis(500))
            .timeout(timeouts.request);
        Ok(Self {
            endpoint,
            timeouts,
            api_version,
        })
    }
}
/// The V0 Mayastor client;
pub type MayaClientV0 = IoEngineClientV0<Channel>;

/// The V1 PoolClient.
pub type PoolClient = rpc::v1::pool::pool_rpc_client::PoolRpcClient<Channel>;

/// A wrapper for client for the V1 dataplane interface.
#[derive(Clone, Debug)]
pub(crate) struct MayaClientV1 {
    pub(crate) pool: PoolClient,
}

/// Grpc client
#[derive(Debug, Clone)]
pub struct GrpcClient {
    ctx: GrpcContext,
    v0_client: Option<MayaClientV0>,
    v1_client: Option<MayaClientV1>,
}

impl GrpcClient {
    /// initialize gRPC client
    pub async fn new(context: GrpcContext) -> Result<Self, ExporterError> {
        loop {
            match context.api_version {
                ApiVersion::V0 => {
                    match tokio::time::timeout(
                        context.timeouts.connect(),
                        MayaClientV0::connect(context.endpoint.clone()),
                    )
                    .await
                    {
                        Err(error) => {
                            error!(error=%error, "Grpc connection timeout, retrying after 10s");
                        }
                        Ok(result) => match result {
                            Ok(v0_client) => {
                                return Ok(Self {
                                    ctx: context.clone(),
                                    v0_client: Some(v0_client),
                                    v1_client: None,
                                })
                            }
                            Err(error) => {
                                error!(error=%error, "Grpc client connection error, retrying after 10s");
                            }
                        },
                    }
                }
                ApiVersion::V1 => {
                    match tokio::time::timeout(
                        context.timeouts.connect(),
                        PoolClient::connect(context.endpoint.clone()),
                    )
                    .await
                    {
                        Err(error) => {
                            error!(error=%error, "Grpc connection timeout, retrying after 10s");
                        }
                        Ok(result) => match result {
                            Ok(pool) => {
                                return Ok(Self {
                                    ctx: context.clone(),
                                    v0_client: None,
                                    v1_client: Some(MayaClientV1 { pool }),
                                })
                            }
                            Err(error) => {
                                error!(error=%error, "Grpc client connection error, retrying after 10s");
                            }
                        },
                    }
                }
            }
            sleep(Duration::from_secs(10)).await;
        }
    }

    /// Get the v0 api client.
    pub(crate) fn client_v0(&self) -> Result<MayaClientV0, ExporterError> {
        match self.v0_client.clone() {
            Some(client) => Ok(client),
            None => Err(ExporterError::GrpcClientError(
                "Could not get v0 client".to_string(),
            )),
        }
    }

    /// Get the v1 api client.
    pub(crate) fn client_v1(&self) -> Result<MayaClientV1, ExporterError> {
        match self.v1_client.clone() {
            Some(client) => Ok(client),
            None => Err(ExporterError::GrpcClientError(
                "Could not get v1 client".to_string(),
            )),
        }
    }

    /// Get the api version.
    pub(crate) fn api_version(&self) -> ApiVersion {
        self.ctx.api_version.clone()
    }
}
