use crate::errors::{Error::API, Result};
use hyper::{Body, Method, Request};
use log::info;
use tokio::time::Duration;

/// Fetches the instance ID on the host EC2 machine.
pub async fn fetch_instance_id() -> Result<String> {
    fetch_metadata("instance-id").await
}

/// Fetches the public hostname of the host EC2 machine.
pub async fn fetch_public_hostname() -> Result<String> {
    fetch_metadata("public-hostname").await
}

/// Fetches the public IPv4 address of the host EC2 machine.
pub async fn fetch_public_ipv4() -> Result<String> {
    fetch_metadata("public-ipv4").await
}

/// Fetches the availability of the host EC2 machine.
pub async fn fetch_availability_zone() -> Result<String> {
    fetch_metadata("placement/availability-zone").await
}

/// Fetches the region of the host EC2 machine.
/// TODO: fix this...
pub async fn fetch_region() -> Result<String> {
    let mut az = fetch_availability_zone().await?;
    az.truncate(az.len() - 1);
    Ok(az)
}

/// Fetches instance metadata service v2 with the "path".
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instancedata-data-retrieval.html
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/configuring-instance-metadata-service.html
/// e.g., curl -H "X-aws-ec2-metadata-token: $TOKEN" -v http://169.254.169.254/latest/meta-data/public-ipv4
async fn fetch_metadata(path: &str) -> Result<String> {
    log::info!("fetching meta-data/{}", path);

    let uri = format!("http://169.254.169.254/latest/meta-data/{}", path);
    let token = fetch_token().await?;
    let req = match Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("X-aws-ec2-metadata-token", token)
        .body(Body::empty())
    {
        Ok(r) => r,
        Err(e) => {
            return Err(API {
                message: format!("failed to build GET meta-data/{} {:?}", path, e),
                is_retryable: false,
            });
        }
    };

    let ret = http_manager::read_bytes(req, Duration::from_secs(5), false, true).await;
    let rs = match ret {
        Ok(bytes) => match String::from_utf8(bytes.to_vec()) {
            Ok(text) => text,
            Err(e) => {
                return Err(API {
                    message: format!(
                        "GET meta-data/{} returned unexpected bytes {:?} ({})",
                        path, bytes, e
                    ),
                    is_retryable: false,
                });
            }
        },
        Err(e) => {
            return Err(API {
                message: format!("failed GET meta-data/{} {:?}", path, e),
                is_retryable: false,
            })
        }
    };
    Ok(rs)
}

/// Serves session token for instance metadata service v2.
/// ref. https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/configuring-instance-metadata-service.html
/// e.g., curl -X PUT "http://169.254.169.254/latest/api/token" -H "X-aws-ec2-metadata-token-ttl-seconds: 21600"
const IMDS_V2_SESSION_TOKEN_URI: &str = "http://169.254.169.254/latest/api/token";

/// Fetches the IMDS v2 token.
async fn fetch_token() -> Result<String> {
    log::info!("fetching IMDS v2 token");

    let req = match Request::builder()
        .method(Method::PUT)
        .uri(IMDS_V2_SESSION_TOKEN_URI)
        .header("X-aws-ec2-metadata-token-ttl-seconds", "21600")
        .body(Body::empty())
    {
        Ok(r) => r,
        Err(e) => {
            return Err(API {
                message: format!("failed to build PUT api/token {:?}", e),
                is_retryable: false,
            });
        }
    };

    let ret = http_manager::read_bytes(req, Duration::from_secs(5), false, true).await;
    let token = match ret {
        Ok(bytes) => match String::from_utf8(bytes.to_vec()) {
            Ok(text) => text,
            Err(e) => {
                return Err(API {
                    message: format!(
                        "PUT api/token returned unexpected bytes {:?} ({})",
                        bytes, e
                    ),
                    is_retryable: false,
                });
            }
        },
        Err(e) => {
            return Err(API {
                message: format!("failed PUT api/token {:?}", e),
                is_retryable: false,
            })
        }
    };
    Ok(token)
}
