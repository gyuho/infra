use crate::errors::{
    Error::{Other, API},
    Result,
};
use chrono::{DateTime, Utc};
use reqwest::ClientBuilder;
use serde::{Deserialize, Serialize};
use tokio::time::Duration;

/// Fetches the instance ID on the host EC2 machine.
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
pub async fn fetch_instance_id() -> Result<String> {
    fetch_metadata_by_path("instance-id").await
}

/// Fetches the public hostname of the host EC2 machine.
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
pub async fn fetch_public_hostname() -> Result<String> {
    fetch_metadata_by_path("public-hostname").await
}

/// Fetches the public IPv4 address of the host EC2 machine.
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
pub async fn fetch_public_ipv4() -> Result<String> {
    fetch_metadata_by_path("public-ipv4").await
}

/// Fetches the availability of the host EC2 machine.
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
pub async fn fetch_availability_zone() -> Result<String> {
    fetch_metadata_by_path("placement/availability-zone").await
}

/// Fetches the spot instance action.
///
/// If Amazon EC2 is not stopping or terminating the instance, or if you terminated the instance yourself,
/// spot/instance-action is not present in the instance metadata thus returning an HTTP 404 error.
///
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/prepare-for-interruptions.html
pub async fn fetch_spot_instance_action() -> Result<InstanceAction> {
    let resp = fetch_metadata_by_path("spot/instance-action").await?;
    serde_json::from_slice(resp.as_bytes()).map_err(|e| Other {
        message: format!(
            "failed to parse spot/instance-action response '{}' {:?}",
            resp, e
        ),
        is_retryable: false,
    })
}

/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/spot-instance-termination-notices.html#instance-action-metadata
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct InstanceAction {
    pub action: String,
    #[serde(with = "rfc_manager::serde_format::rfc_3339")]
    pub time: DateTime<Utc>,
}

/// Fetches the region of the host EC2 machine.
/// TODO: fix this...
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
pub async fn fetch_region() -> Result<String> {
    let mut az = fetch_availability_zone().await?;
    az.truncate(az.len() - 1);
    Ok(az)
}

/// Fetches instance metadata service v2 with the "path".
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-retrieval.html
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/configuring-instance-metadata-service.html
/// e.g., curl -H "X-aws-ec2-metadata-token: $TOKEN" -v http://169.254.169.254/latest/meta-data/public-ipv4
pub async fn fetch_metadata_by_path(path: &str) -> Result<String> {
    log::info!("fetching meta-data/{}", path);

    let token = fetch_token().await?;

    let uri = format!("http://169.254.169.254/latest/meta-data/{}", path);
    let cli = ClientBuilder::new()
        .user_agent(env!("CARGO_PKG_NAME"))
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(15))
        .connection_verbose(true)
        .build()
        .map_err(|e| API {
            message: format!("failed ClientBuilder build {:?}", e),
            is_retryable: false,
        })?;
    let resp = cli
        .get(&uri)
        .header("X-aws-ec2-metadata-token", token)
        .send()
        .await
        .map_err(|e| API {
            message: format!("failed to build GET meta-data/{} {:?}", path, e),
            is_retryable: false,
        })?;
    let out = resp.bytes().await.map_err(|e| API {
        message: format!("failed to read bytes {:?}", e),
        is_retryable: false,
    })?;
    let out: Vec<u8> = out.into();

    match String::from_utf8(out) {
        Ok(text) => Ok(text),
        Err(e) => Err(API {
            message: format!("GET meta-data/{} failed String::from_utf8 ({})", path, e),
            is_retryable: false,
        }),
    }
}

/// Serves session token for instance metadata service v2.
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-categories.html
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/configuring-instance-metadata-service.html
/// e.g., curl -X PUT "http://169.254.169.254/latest/api/token" -H "X-aws-ec2-metadata-token-ttl-seconds: 21600"
const IMDS_V2_SESSION_TOKEN_URI: &str = "http://169.254.169.254/latest/api/token";

/// Fetches the IMDS v2 token.
async fn fetch_token() -> Result<String> {
    log::info!("fetching IMDS v2 token");

    let cli = ClientBuilder::new()
        .user_agent(env!("CARGO_PKG_NAME"))
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(15))
        .connection_verbose(true)
        .build()
        .map_err(|e| API {
            message: format!("failed ClientBuilder build {:?}", e),
            is_retryable: false,
        })?;
    let resp = cli
        .put(IMDS_V2_SESSION_TOKEN_URI)
        .header("X-aws-ec2-metadata-token-ttl-seconds", "21600")
        .send()
        .await
        .map_err(|e| API {
            message: format!("failed to build PUT api/token {:?}", e),
            is_retryable: false,
        })?;
    let out = resp.bytes().await.map_err(|e| API {
        message: format!("failed to read bytes {:?}", e),
        is_retryable: false,
    })?;
    let out: Vec<u8> = out.into();

    match String::from_utf8(out) {
        Ok(text) => Ok(text),
        Err(e) => Err(API {
            message: format!("GET token failed String::from_utf8 ({})", e),
            is_retryable: false,
        }),
    }
}
