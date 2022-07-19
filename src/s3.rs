use std::{fs, path::Path, sync::Arc};

use crate::{
    errors::{
        Error::{Other, API},
        Result,
    },
    kms::envelope,
};
use aws_sdk_s3::{
    error::{CreateBucketError, CreateBucketErrorKind, DeleteBucketError},
    model::{
        BucketCannedAcl, BucketLocationConstraint, CreateBucketConfiguration, Delete, Object,
        ObjectCannedAcl, ObjectIdentifier, PublicAccessBlockConfiguration, ServerSideEncryption,
        ServerSideEncryptionByDefault, ServerSideEncryptionConfiguration, ServerSideEncryptionRule,
    },
    types::{ByteStream, SdkError},
    Client,
};
use aws_types::SdkConfig as AwsSdkConfig;
use tokio::{fs::File, io::AsyncWriteExt};
use tokio_stream::StreamExt;

/// Implements AWS S3 manager.
#[derive(Debug, Clone)]
pub struct Manager {
    #[allow(dead_code)]
    shared_config: AwsSdkConfig,
    cli: Client,
}

impl Manager {
    pub fn new(shared_config: &AwsSdkConfig) -> Self {
        let cloned = shared_config.clone();
        let cli = Client::new(shared_config);
        Self {
            shared_config: cloned,
            cli,
        }
    }

    pub fn client(&self) -> Client {
        self.cli.clone()
    }

    /// Creates a S3 bucket.
    pub async fn create_bucket(&self, s3_bucket: &str) -> Result<()> {
        let reg = self.shared_config.region().unwrap();
        let constraint = BucketLocationConstraint::from(reg.to_string().as_str());
        let bucket_cfg = CreateBucketConfiguration::builder()
            .location_constraint(constraint)
            .build();

        log::info!(
            "creating S3 bucket '{}' in region {}",
            s3_bucket,
            reg.to_string()
        );
        let ret = self
            .cli
            .create_bucket()
            .create_bucket_configuration(bucket_cfg)
            .bucket(s3_bucket)
            .acl(BucketCannedAcl::Private)
            .send()
            .await;
        let already_created = match ret {
            Ok(_) => false,
            Err(e) => {
                if !is_error_bucket_already_exist(&e) {
                    return Err(API {
                        message: format!("failed create_bucket {:?}", e),
                        is_retryable: is_error_retryable(&e),
                    });
                }
                log::warn!(
                    "bucket already exists so returning early (original error '{}')",
                    e
                );
                true
            }
        };
        if already_created {
            return Ok(());
        }
        log::info!("created S3 bucket '{}'", s3_bucket);

        log::info!("setting S3 bucket public_access_block configuration to private");
        let public_access_block_cfg = PublicAccessBlockConfiguration::builder()
            .block_public_acls(true)
            .block_public_policy(true)
            .ignore_public_acls(true)
            .restrict_public_buckets(true)
            .build();
        self.cli
            .put_public_access_block()
            .bucket(s3_bucket)
            .public_access_block_configuration(public_access_block_cfg)
            .send()
            .await
            .map_err(|e| API {
                message: format!("failed put_public_access_block {}", e),
                is_retryable: is_error_retryable(&e),
            })?;

        let algo = ServerSideEncryption::Aes256;
        let sse = ServerSideEncryptionByDefault::builder()
            .set_sse_algorithm(Some(algo))
            .build();
        let server_side_encryption_rule = ServerSideEncryptionRule::builder()
            .apply_server_side_encryption_by_default(sse)
            .build();
        let server_side_encryption_cfg = ServerSideEncryptionConfiguration::builder()
            .rules(server_side_encryption_rule)
            .build();
        self.cli
            .put_bucket_encryption()
            .bucket(s3_bucket)
            .server_side_encryption_configuration(server_side_encryption_cfg)
            .send()
            .await
            .map_err(|e| API {
                message: format!("failed put_bucket_encryption {}", e),
                is_retryable: is_error_retryable(&e),
            })?;

        Ok(())
    }

    /// Deletes a S3 bucket.
    pub async fn delete_bucket(&self, s3_bucket: &str) -> Result<()> {
        let reg = self.shared_config.region().unwrap();
        log::info!(
            "deleting S3 bucket '{}' in region {}",
            s3_bucket,
            reg.to_string()
        );
        let ret = self.cli.delete_bucket().bucket(s3_bucket).send().await;
        match ret {
            Ok(_) => {}
            Err(e) => {
                if !is_error_bucket_does_not_exist(&e) {
                    return Err(API {
                        message: format!("failed delete_bucket {:?}", e),
                        is_retryable: is_error_retryable(&e),
                    });
                }
                log::warn!("bucket already deleted or does not exist ({})", e);
            }
        };
        log::info!("deleted S3 bucket '{}'", s3_bucket);

        Ok(())
    }

    /// Deletes objects by "prefix".
    /// If "prefix" is "None", empties a S3 bucket, deleting all files.
    /// ref. https://github.com/awslabs/aws-sdk-rust/blob/main/examples/s3/src/bin/delete-objects.rs
    ///
    /// "If a single piece of data must be accessible from more than one task
    /// concurrently, then it must be shared using synchronization primitives such as Arc."
    /// ref. https://tokio.rs/tokio/tutorial/spawning
    pub async fn delete_objects(
        &self,
        s3_bucket: Arc<String>,
        prefix: Option<Arc<String>>,
    ) -> Result<()> {
        let reg = self.shared_config.region().unwrap();
        log::info!(
            "deleting objects S3 bucket '{}' in region {} (prefix {:?})",
            s3_bucket,
            reg.to_string(),
            prefix,
        );

        let objects = self.list_objects(s3_bucket.clone(), prefix).await?;
        let mut object_ids: Vec<ObjectIdentifier> = vec![];
        for obj in objects {
            let k = String::from(obj.key().unwrap_or(""));
            let obj_id = ObjectIdentifier::builder().set_key(Some(k)).build();
            object_ids.push(obj_id);
        }

        let n = object_ids.len();
        if n > 0 {
            let deletes = Delete::builder().set_objects(Some(object_ids)).build();
            let ret = self
                .cli
                .delete_objects()
                .bucket(s3_bucket.to_string())
                .delete(deletes)
                .send()
                .await;
            match ret {
                Ok(_) => {}
                Err(e) => {
                    return Err(API {
                        message: format!("failed delete_bucket {:?}", e),
                        is_retryable: is_error_retryable(&e),
                    });
                }
            };
            log::info!("deleted {} objets in S3 bucket '{}'", n, s3_bucket);
        } else {
            log::info!("nothing to delete; skipping...");
        }

        Ok(())
    }

    /// List objects in the bucket with an optional prefix,
    /// in the descending order of "last_modified" timestamps.
    /// "bucket_name" implies the suffix "/", so no need to prefix
    /// sub-directory with "/".
    /// Passing "bucket_name" + "directory" is enough!
    ///
    /// e.g.
    /// "foo-mydatabucket" for bucket_name
    /// "mydata/myprefix/" for prefix
    pub async fn list_objects(
        &self,
        s3_bucket: Arc<String>,
        prefix: Option<Arc<String>>,
    ) -> Result<Vec<Object>> {
        let pfx = {
            if let Some(s) = prefix {
                let s = s.to_string();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            } else {
                None
            }
        };

        log::info!("listing bucket {} with prefix '{:?}'", s3_bucket, pfx);
        let mut objects: Vec<Object> = Vec::new();
        let mut token = String::new();
        loop {
            let mut builder = self.cli.list_objects_v2().bucket(s3_bucket.to_string());
            if pfx.is_some() {
                builder = builder.set_prefix(pfx.clone());
            }
            if !token.is_empty() {
                builder = builder.set_continuation_token(Some(token.to_owned()));
            }
            let ret = match builder.send().await {
                Ok(r) => r,
                Err(e) => {
                    return Err(API {
                        message: format!("failed list_objects_v2 {:?}", e),
                        is_retryable: is_error_retryable(&e),
                    });
                }
            };
            if ret.key_count == 0 {
                break;
            }
            if ret.contents.is_none() {
                break;
            }
            let contents = ret.contents.unwrap();
            for obj in contents.iter() {
                let k = obj.key().unwrap_or("");
                if k.is_empty() {
                    return Err(API {
                        message: String::from("empty key returned"),
                        is_retryable: false,
                    });
                }
                log::debug!("listing [{}]", k);
                objects.push(obj.to_owned());
            }

            token = match ret.next_continuation_token {
                Some(v) => v,
                None => String::new(),
            };
            if token.is_empty() {
                break;
            }
        }

        if objects.len() > 1 {
            log::info!(
                "sorting {} objects in bucket {} with prefix {:?}",
                objects.len(),
                s3_bucket,
                pfx
            );
            objects.sort_by(|a, b| {
                let a_modified = a.last_modified.unwrap();
                let a_modified = a_modified.as_nanos();

                let b_modified = b.last_modified.unwrap();
                let b_modified = b_modified.as_nanos();

                // reverse comparison!
                // older file placed in later in the array
                // latest file first!
                b_modified.cmp(&a_modified)
            });
        }
        Ok(objects)
    }

    /// Writes an object to a S3 bucket using stream.
    ///
    /// WARN: use stream! otherwise it can cause OOM -- don't do the following!
    ///       "fs::read" reads all data onto memory
    ///       ".body(ByteStream::from(contents))" passes the whole data to an API call
    ///
    /// "If a single piece of data must be accessible from more than one task
    /// concurrently, then it must be shared using synchronization primitives such as Arc."
    /// ref. https://tokio.rs/tokio/tutorial/spawning
    pub async fn put_object(
        &self,
        file_path: Arc<String>,
        s3_bucket: Arc<String>,
        s3_key: Arc<String>,
    ) -> Result<()> {
        if !Path::new(file_path.as_str()).exists() {
            return Err(Other {
                message: format!("file path {} does not exist", file_path),
                is_retryable: false,
            });
        }

        let meta = fs::metadata(file_path.as_str()).map_err(|e| Other {
            message: format!("failed metadata {}", e),
            is_retryable: false,
        })?;
        let size = meta.len() as f64;
        log::info!(
            "starting put_object '{}' (size {}) to 's3://{}/{}'",
            file_path,
            human_readable::bytes(size),
            s3_bucket,
            s3_key
        );

        let byte_stream = ByteStream::from_path(Path::new(file_path.as_str()))
            .await
            .map_err(|e| Other {
                message: format!("failed ByteStream::from_file {}", e),
                is_retryable: false,
            })?;
        self.cli
            .put_object()
            .bucket(s3_bucket.to_string())
            .key(s3_key.to_string())
            .body(byte_stream)
            .acl(ObjectCannedAcl::Private)
            .send()
            .await
            .map_err(|e| API {
                message: format!("failed put_object {}", e),
                is_retryable: is_error_retryable(&e),
            })?;

        Ok(())
    }

    /// Downloads an object from a S3 bucket using stream.
    ///
    /// WARN: use stream! otherwise it can cause OOM -- don't do the following!
    ///       "aws_smithy_http::byte_stream:ByteStream.collect" reads all the data into memory
    ///       "File.write_all_buf(&mut bytes)" to write bytes
    ///
    /// "If a single piece of data must be accessible from more than one task
    /// concurrently, then it must be shared using synchronization primitives such as Arc."
    /// ref. https://tokio.rs/tokio/tutorial/spawning
    pub async fn get_object(
        &self,
        s3_bucket: Arc<String>,
        s3_key: Arc<String>,
        file_path: Arc<String>,
    ) -> Result<()> {
        if Path::new(file_path.as_str()).exists() {
            return Err(Other {
                message: format!("file path {} already exists", file_path),
                is_retryable: false,
            });
        }

        let head_output = self
            .cli
            .head_object()
            .bucket(s3_bucket.to_string())
            .key(s3_key.to_string())
            .send()
            .await
            .map_err(|e| API {
                message: format!("failed head_object {}", e),
                is_retryable: is_error_retryable(&e),
            })?;

        log::info!(
            "starting get_object 's3://{}/{}' (content type '{}', size {})",
            s3_bucket,
            s3_key,
            head_output.content_type().unwrap(),
            human_readable::bytes(head_output.content_length() as f64),
        );
        let mut output = self
            .cli
            .get_object()
            .bucket(s3_bucket.to_string())
            .key(s3_key.to_string())
            .send()
            .await
            .map_err(|e| API {
                message: format!("failed get_object {}", e),
                is_retryable: is_error_retryable(&e),
            })?;

        // ref. https://docs.rs/tokio-stream/latest/tokio_stream/
        let mut file = File::create(file_path.as_str()).await.map_err(|e| Other {
            message: format!("failed File::create {}", e),
            is_retryable: false,
        })?;

        log::info!("writing byte stream to file {}", file_path);
        while let Some(d) = output.body.try_next().await.map_err(|e| Other {
            message: format!("failed ByteStream::try_next {}", e),
            is_retryable: false,
        })? {
            file.write_all(&d).await.map_err(|e| API {
                message: format!("failed File.write_all {}", e),
                is_retryable: false,
            })?;
        }
        file.flush().await.map_err(|e| Other {
            message: format!("failed File.flush {}", e),
            is_retryable: false,
        })?;

        Ok(())
    }

    /// Compresses the file, encrypts, and uploads to S3.
    pub async fn compress_seal_put_object(
        &self,
        envelope_manager: Arc<envelope::Manager>,
        source_file_path: Arc<String>,
        s3_bucket: Arc<String>,
        s3_key: Arc<String>,
    ) -> Result<()> {
        log::info!(
            "compress-seal-put-object: compress and seal '{}'",
            source_file_path.as_str()
        );

        let tmp_compressed_sealed_path = random_manager::tmp_path(10, None).unwrap();
        envelope_manager
            .compress_seal(
                source_file_path.clone(),
                Arc::new(tmp_compressed_sealed_path.clone()),
            )
            .await?;

        log::info!(
            "compress-seal-put-object: upload object '{}'",
            tmp_compressed_sealed_path
        );
        self.put_object(
            Arc::new(tmp_compressed_sealed_path.clone()),
            s3_bucket.clone(),
            s3_key.clone(),
        )
        .await?;

        fs::remove_file(tmp_compressed_sealed_path).map_err(|e| API {
            message: format!("failed remove_file tmp_compressed_sealed_path: {}", e),
            is_retryable: false,
        })
    }

    /// Reverse of "compress_seal_put_object".
    pub async fn get_object_unseal_decompress(
        &self,
        envelope_manager: Arc<envelope::Manager>,
        s3_bucket: Arc<String>,
        s3_key: Arc<String>,
        download_file_path: Arc<String>,
    ) -> Result<()> {
        log::info!(
            "get-object-unseal-decompress: downloading object {}/{}",
            s3_bucket.as_str(),
            s3_key.as_str()
        );

        let tmp_downloaded_path = random_manager::tmp_path(10, None).unwrap();
        self.get_object(
            s3_bucket.clone(),
            s3_key.clone(),
            Arc::new(tmp_downloaded_path.clone()),
        )
        .await?;

        log::info!(
            "get-object-unseal-decompress: unseal and decompress '{}'",
            tmp_downloaded_path
        );
        envelope_manager
            .unseal_decompress(
                Arc::new(tmp_downloaded_path.clone()),
                download_file_path.clone(),
            )
            .await?;

        fs::remove_file(tmp_downloaded_path).map_err(|e| API {
            message: format!("failed remove_file tmp_downloaded_path: {}", e),
            is_retryable: false,
        })
    }
}

#[inline]
pub fn is_error_retryable<E>(e: &SdkError<E>) -> bool {
    match e {
        SdkError::TimeoutError(_) | SdkError::ResponseError { .. } => true,
        SdkError::DispatchFailure(e) => e.is_timeout() || e.is_io(),
        _ => false,
    }
}

#[inline]
fn is_error_bucket_already_exist(e: &SdkError<CreateBucketError>) -> bool {
    match e {
        SdkError::ServiceError { err, .. } => {
            matches!(
                err.kind,
                CreateBucketErrorKind::BucketAlreadyExists(_)
                    | CreateBucketErrorKind::BucketAlreadyOwnedByYou(_)
            )
        }
        _ => false,
    }
}

#[inline]
fn is_error_bucket_does_not_exist(e: &SdkError<DeleteBucketError>) -> bool {
    match e {
        SdkError::ServiceError { err, .. } => {
            let msg = format!("{:?}", err);
            msg.contains("bucket does not exist")
        }
        _ => false,
    }
}

#[test]
fn test_append_slash() {
    let s = "hello";
    assert_eq!(append_slash(s), "hello/");

    let s = "hello/";
    assert_eq!(append_slash(s), "hello/");
}

pub fn append_slash(k: &str) -> String {
    let n = k.len();
    if &k[n - 1..] == "/" {
        String::from(k)
    } else {
        format!("{}/", k)
    }
}

pub async fn spawn_list_objects<S>(
    s3_manager: Manager,
    s3_bucket: S,
    prefix: Option<String>,
) -> Result<Vec<Object>>
where
    S: AsRef<str>,
{
    let s3_manager_arc = Arc::new(s3_manager);
    let s3_bucket_arc = Arc::new(s3_bucket.as_ref().to_string());
    let pfx = {
        if let Some(s) = prefix {
            if s.is_empty() {
                None
            } else {
                Some(Arc::new(s))
            }
        } else {
            None
        }
    };
    tokio::spawn(async move { s3_manager_arc.list_objects(s3_bucket_arc, pfx).await })
        .await
        .expect("failed spawn await")
}

pub async fn spawn_delete_objects<S>(
    s3_manager: Manager,
    s3_bucket: S,
    prefix: Option<String>,
) -> Result<()>
where
    S: AsRef<str>,
{
    let s3_manager_arc = Arc::new(s3_manager);
    let s3_bucket_arc = Arc::new(s3_bucket.as_ref().to_string());
    let pfx = {
        if let Some(s) = prefix {
            if s.is_empty() {
                None
            } else {
                Some(Arc::new(s))
            }
        } else {
            None
        }
    };
    tokio::spawn(async move { s3_manager_arc.delete_objects(s3_bucket_arc, pfx).await })
        .await
        .expect("failed spawn await")
}

pub async fn spawn_put_object<S>(
    s3_manager: Manager,
    file_path: S,
    s3_bucket: S,
    s3_key: S,
) -> Result<()>
where
    S: AsRef<str>,
{
    let s3_manager_arc = Arc::new(s3_manager);
    let file_path_arc = Arc::new(file_path.as_ref().to_string());
    let s3_bucket_arc = Arc::new(s3_bucket.as_ref().to_string());
    let s3_key_arc = Arc::new(s3_key.as_ref().to_string());
    tokio::spawn(async move {
        s3_manager_arc
            .put_object(file_path_arc, s3_bucket_arc, s3_key_arc)
            .await
    })
    .await
    .expect("failed spawn await")
}

pub async fn spawn_get_object<S>(
    s3_manager: Manager,
    s3_bucket: S,
    s3_key: S,
    file_path: S,
) -> Result<()>
where
    S: AsRef<str>,
{
    let s3_manager_arc = Arc::new(s3_manager);
    let s3_bucket_arc = Arc::new(s3_bucket.as_ref().to_string());
    let s3_key_arc = Arc::new(s3_key.as_ref().to_string());
    let file_path_arc = Arc::new(file_path.as_ref().to_string());
    tokio::spawn(async move {
        s3_manager_arc
            .get_object(s3_bucket_arc, s3_key_arc, file_path_arc)
            .await
    })
    .await
    .expect("failed spawn await")
}

pub async fn spawn_compress_seal_put_object<S>(
    s3_manager: Manager,
    envelope_manager: envelope::Manager,
    file_path: S,
    s3_bucket: S,
    s3_key: S,
) -> Result<()>
where
    S: AsRef<str>,
{
    let s3_manager_arc = Arc::new(s3_manager);
    let envelope_manager_arc = Arc::new(envelope_manager);
    let file_path_arc = Arc::new(file_path.as_ref().to_string());
    let s3_bucket_arc = Arc::new(s3_bucket.as_ref().to_string());
    let s3_key_arc = Arc::new(s3_key.as_ref().to_string());
    tokio::spawn(async move {
        s3_manager_arc
            .compress_seal_put_object(
                envelope_manager_arc,
                file_path_arc,
                s3_bucket_arc,
                s3_key_arc,
            )
            .await
    })
    .await
    .expect("failed spawn await")
}

pub async fn spawn_get_object_unseal_decompress<S>(
    s3_manager: Manager,
    envelope_manager: envelope::Manager,
    s3_bucket: S,
    s3_key: S,
    file_path: S,
) -> Result<()>
where
    S: AsRef<str>,
{
    let s3_manager_arc = Arc::new(s3_manager);
    let envelope_manager_arc = Arc::new(envelope_manager);
    let file_path_arc = Arc::new(file_path.as_ref().to_string());
    let s3_bucket_arc = Arc::new(s3_bucket.as_ref().to_string());
    let s3_key_arc = Arc::new(s3_key.as_ref().to_string());
    tokio::spawn(async move {
        s3_manager_arc
            .get_object_unseal_decompress(
                envelope_manager_arc,
                s3_bucket_arc,
                s3_key_arc,
                file_path_arc,
            )
            .await
    })
    .await
    .expect("failed spawn await")
}
