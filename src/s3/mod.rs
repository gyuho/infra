use std::{
    collections::HashMap,
    {os::unix::fs::PermissionsExt, path::Path},
};

use crate::errors::{self, Error, Result};
use aws_sdk_s3::{
    operation::{
        create_bucket::CreateBucketError,
        delete_bucket::DeleteBucketError,
        delete_objects::DeleteObjectsError,
        head_object::{HeadObjectError, HeadObjectOutput},
        put_bucket_lifecycle_configuration::PutBucketLifecycleConfigurationError,
    },
    primitives::ByteStream,
    types::{
        BucketCannedAcl, BucketLifecycleConfiguration, BucketLocationConstraint,
        CreateBucketConfiguration, Delete, ExpirationStatus, LifecycleExpiration, LifecycleRule,
        LifecycleRuleFilter, Object, ObjectCannedAcl, ObjectIdentifier,
        PublicAccessBlockConfiguration, ServerSideEncryption, ServerSideEncryptionByDefault,
        ServerSideEncryptionConfiguration, ServerSideEncryptionRule,
    },
    Client,
};
use aws_smithy_client::SdkError;
use aws_types::SdkConfig as AwsSdkConfig;
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
    time::{sleep, Duration, Instant},
};
use tokio_stream::StreamExt;

/// Implements AWS S3 manager.
#[derive(Debug, Clone)]
pub struct Manager {
    pub region: String,
    pub cli: Client,
}

impl Manager {
    pub fn new(shared_config: &AwsSdkConfig) -> Self {
        Self {
            region: shared_config.region().unwrap().to_string(),
            cli: Client::new(shared_config),
        }
    }

    /// Creates a S3 bucket.
    pub async fn create_bucket(&self, s3_bucket: &str) -> Result<()> {
        log::info!("creating bucket '{s3_bucket}' in region {}", self.region);

        let mut req = self
            .cli
            .create_bucket()
            .bucket(s3_bucket)
            .acl(BucketCannedAcl::Private);

        // don't specify if "us-east-1", default is "us-east-1"
        if self.region != "us-east-1" {
            let constraint = BucketLocationConstraint::from(self.region.as_str());
            let bucket_cfg = CreateBucketConfiguration::builder()
                .location_constraint(constraint)
                .build();
            req = req.create_bucket_configuration(bucket_cfg);
        }

        let ret = req.send().await;
        let already_created = match ret {
            Ok(_) => false,
            Err(e) => {
                if !is_err_already_exists_create_bucket(&e) {
                    return Err(Error::API {
                        message: format!("failed create_bucket {:?}", e),
                        retryable: errors::is_sdk_err_retryable(&e),
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
        log::info!("created bucket '{s3_bucket}'");

        log::info!("setting bucket public_access_block configuration to private");
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
            .map_err(|e| Error::API {
                message: format!("failed put_public_access_block {}", e),
                retryable: errors::is_sdk_err_retryable(&e),
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
            .map_err(|e| Error::API {
                message: format!("failed put_bucket_encryption {}", e),
                retryable: errors::is_sdk_err_retryable(&e),
            })?;

        Ok(())
    }

    /// Put object expire configuration on the bucket.
    /// ref. <https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutBucketLifecycleConfiguration.html>
    /// ref. <https://docs.aws.amazon.com/AmazonS3/latest/API/API_LifecycleRule.html>
    /// ref. <https://docs.aws.amazon.com/AmazonS3/latest/API/API_LifecycleExpiration.html>
    pub async fn put_bucket_object_expire_configuration(
        &self,
        s3_bucket: &str,
        days_to_prefixes: HashMap<i32, Vec<String>>,
    ) -> Result<()> {
        if days_to_prefixes.is_empty() {
            return Err(Error::Other {
                message: "empty prefixes".to_string(),
                retryable: false,
            });
        }

        log::info!(
            "put bucket object expire configuration for '{s3_bucket}' with prefixes '{:?}'",
            days_to_prefixes
        );
        let mut rules = Vec::new();
        for (days, pfxs) in days_to_prefixes.iter() {
            for pfx in pfxs {
                rules.push(
                    // ref. <https://docs.aws.amazon.com/AmazonS3/latest/API/API_LifecycleRule.html>
                    LifecycleRule::builder()
                        // ref. <https://docs.aws.amazon.com/AmazonS3/latest/API/API_LifecycleRuleFilter.html>
                        .filter(LifecycleRuleFilter::Prefix(pfx.to_owned()))
                        .expiration(LifecycleExpiration::builder().days(days.to_owned()).build())
                        .status(ExpirationStatus::Enabled) // If 'Enabled', the rule is currently being applied.
                        .build(),
                );
            }
        }
        let lifecycle = BucketLifecycleConfiguration::builder()
            .set_rules(Some(rules))
            .build();

        // ref. <https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutBucketLifecycleConfiguration.html>
        let _ = self
            .cli
            .put_bucket_lifecycle_configuration()
            .bucket(s3_bucket)
            .lifecycle_configuration(lifecycle)
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!(
                    "failed put_bucket_lifecycle_configuration '{}'",
                    explain_err_put_bucket_lifecycle_configuration(&e)
                ),
                retryable: errors::is_sdk_err_retryable(&e),
            })?;

        log::info!("successfullhy updated bucket lifecycle configuration");
        Ok(())
    }

    /// Deletes a S3 bucket.
    pub async fn delete_bucket(&self, s3_bucket: &str) -> Result<()> {
        log::info!("deleting bucket '{s3_bucket}' in region {}", self.region);
        match self.cli.delete_bucket().bucket(s3_bucket).send().await {
            Ok(_) => {
                log::info!("successfully deleted bucket '{s3_bucket}'");
            }
            Err(e) => {
                if !is_err_does_not_exist_delete_bucket(&e) {
                    return Err(Error::API {
                        message: format!(
                            "failed delete_bucket '{}'",
                            explain_err_delete_bucket(&e)
                        ),
                        retryable: errors::is_sdk_err_retryable(&e),
                    });
                }
                log::warn!(
                    "bucket already deleted or does not exist '{}'",
                    explain_err_delete_bucket(&e)
                );
            }
        };

        Ok(())
    }

    /// Deletes objects by "prefix".
    /// If "prefix" is "None", empties a S3 bucket, deleting all files.
    /// ref. https://github.com/awslabs/aws-sdk-rust/blob/main/examples/s3/src/bin/delete-objects.rs
    ///
    /// "If a single piece of data must be accessible from more than one task
    /// concurrently, then it must be shared using synchronization primitives such as Arc."
    /// ref. https://tokio.rs/tokio/tutorial/spawning
    pub async fn delete_objects(&self, s3_bucket: &str, prefix: Option<&str>) -> Result<()> {
        log::info!(
            "deleting objects in bucket '{s3_bucket}' in region '{}' (prefix {:?})",
            self.region,
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
                    return Err(Error::API {
                        message: format!(
                            "failed delete_objects '{}'",
                            explain_err_delete_objects(&e)
                        ),
                        retryable: errors::is_sdk_err_retryable(&e),
                    });
                }
            };
            log::info!("deleted {} objets in bucket '{s3_bucket}'", n);
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
    pub async fn list_objects(&self, s3_bucket: &str, prefix: Option<&str>) -> Result<Vec<Object>> {
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

        log::info!(
            "listing bucket '{}' in the region '{}' with prefix '{:?}'",
            s3_bucket,
            self.region,
            pfx
        );
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
                    return Err(Error::API {
                        message: format!("failed list_objects_v2 {:?}", e),
                        retryable: errors::is_sdk_err_retryable(&e),
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
                    return Err(Error::API {
                        message: String::from("empty key returned"),
                        retryable: false,
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
                "sorting {} objects in bucket {s3_bucket} with prefix {:?}",
                objects.len(),
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
    /// ref. <https://tokio.rs/tokio/tutorial/spawning>
    ///
    /// ref. <https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutObject.html>
    pub async fn put_object(&self, file_path: &str, s3_bucket: &str, s3_key: &str) -> Result<()> {
        self.put_object_with_metadata(file_path, s3_bucket, s3_key, None)
            .await
    }

    /// Writes an object with the metadata.
    /// ref. <https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutObject.html>
    /// ref. <https://docs.aws.amazon.com/AmazonS3/latest/userguide/UsingMetadata.html>
    pub async fn put_object_with_metadata(
        &self,
        file_path: &str,
        s3_bucket: &str,
        s3_key: &str,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<()> {
        let (size, byte_stream) = read_file_to_byte_stream(file_path).await?;
        log::info!(
            "put object '{file_path}' (size {}) to 's3://{}/{}'",
            human_readable::bytes(size),
            s3_bucket,
            s3_key
        );
        self.put_byte_stream_with_metadata(byte_stream, s3_bucket, s3_key, metadata)
            .await
    }

    /// Writes a byte stream with the metadata.
    /// ref. <https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutObject.html>
    /// ref. <https://docs.aws.amazon.com/AmazonS3/latest/userguide/UsingMetadata.html>
    pub async fn put_byte_stream_with_metadata(
        &self,
        byte_stream: ByteStream,
        s3_bucket: &str,
        s3_key: &str,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<()> {
        log::info!(
            "put_byte_stream_with_metadata to 's3://{}/{}' (region '{}')",
            s3_bucket,
            s3_key,
            self.region
        );

        let mut req = self
            .cli
            .put_object()
            .bucket(s3_bucket.to_string())
            .key(s3_key.to_string())
            .body(byte_stream)
            .acl(ObjectCannedAcl::Private);
        if let Some(md) = &metadata {
            for (k, v) in md {
                // "user-defined metadata names must begin with x-amz-meta- to distinguish them from other HTTP headers"
                // ref. <https://docs.aws.amazon.com/AmazonS3/latest/userguide/UsingMetadata.html>
                if !k.starts_with("x-amz-meta-") {
                    return Err(Error::Other {
                        message: format!(
                            "user-defined metadata key '{}' is missing the prefix 'x-amz-meta-'",
                            k
                        ),
                        retryable: false,
                    });
                }

                // "user-defined metadata is limited to 2 KB in size"
                // ref. <https://docs.aws.amazon.com/AmazonS3/latest/userguide/UsingMetadata.html>
                if v.len() > 2048 {
                    return Err(Error::Other {
                        message: format!(
                            "user-defined metadata value is {}-byte, exceeds 2 KiB limit",
                            v.len()
                        ),
                        retryable: false,
                    });
                }

                req = req.metadata(k, v);
            }
        }

        req.send().await.map_err(|e| Error::API {
            message: format!("failed put_object {}", e),
            retryable: errors::is_sdk_err_retryable(&e),
        })?;

        Ok(())
    }

    pub async fn put_bytes_with_metadata_with_retries(
        &self,
        b: Vec<u8>,
        s3_bucket: &str,
        s3_key: &str,
        metadata: Option<HashMap<String, String>>,
        timeout: Duration,
        interval: Duration,
    ) -> Result<()> {
        log::info!(
            "put_byte_stream_with_metadata_with_retries '{s3_bucket}' '{s3_key}' exists with timeout {:?} and interval {:?}",
            timeout,
            interval,
        );

        let start = Instant::now();
        let mut cnt: u128 = 0;
        loop {
            let elapsed = start.elapsed();
            if elapsed.gt(&timeout) {
                return Err(Error::API {
                    message: "put_byte_with_metadata_with_retries not complete in time".to_string(),
                    retryable: true,
                });
            }

            let itv = {
                if cnt == 0 {
                    // first poll with no wait
                    Duration::from_secs(1)
                } else {
                    interval
                }
            };
            sleep(itv).await;

            match self
                .put_byte_stream_with_metadata(
                    ByteStream::from(b.clone()),
                    s3_bucket,
                    s3_key,
                    metadata.clone(),
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err(e) => {
                    if !e.retryable() {
                        return Err(e);
                    }
                }
            }

            cnt += 1;
        }
    }

    /// Returns "None" if the S3 file does not exist.
    pub async fn exists(&self, s3_bucket: &str, s3_key: &str) -> Result<Option<HeadObjectOutput>> {
        let head_output = match self
            .cli
            .head_object()
            .bucket(s3_bucket.to_string())
            .key(s3_key.to_string())
            .send()
            .await
        {
            Ok(out) => out,
            Err(e) => {
                if is_err_head_not_found(&e) {
                    log::info!("{s3_key} not found");
                    return Ok(None);
                }

                log::warn!("failed to head {s3_key}: {}", explain_err_head_object(&e));
                return Err(Error::API {
                    message: format!("failed head_object {}", e),
                    retryable: errors::is_sdk_err_retryable(&e),
                });
            }
        };

        log::info!(
            "head object exists 's3://{}/{}' (content type '{}', size {})",
            s3_bucket,
            s3_key,
            head_output.content_type().unwrap(),
            human_readable::bytes(head_output.content_length() as f64),
        );
        Ok(Some(head_output))
    }

    pub async fn exists_with_retries(
        &self,
        s3_bucket: &str,
        s3_key: &str,
        timeout: Duration,
        interval: Duration,
    ) -> Result<Option<HeadObjectOutput>> {
        log::info!(
            "exists_with_retries '{s3_bucket}' '{s3_key}' exists with timeout {:?} and interval {:?}",
            timeout,
            interval,
        );

        let start = Instant::now();
        let mut cnt: u128 = 0;
        loop {
            let elapsed = start.elapsed();
            if elapsed.gt(&timeout) {
                return Err(Error::API {
                    message: "exists_with_retries not complete in time".to_string(),
                    retryable: true,
                });
            }

            let itv = {
                if cnt == 0 {
                    // first poll with no wait
                    Duration::from_secs(1)
                } else {
                    interval
                }
            };
            sleep(itv).await;

            match self.exists(s3_bucket, s3_key).await {
                Ok(head) => return Ok(head),
                Err(e) => {
                    if !e.retryable() {
                        return Err(e);
                    }
                }
            }

            cnt += 1;
        }
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
    ///
    /// Returns "true" if the file exists and got downloaded successfully.
    pub async fn get_object(&self, s3_bucket: &str, s3_key: &str, file_path: &str) -> Result<bool> {
        if Path::new(file_path).exists() {
            return Err(Error::Other {
                message: format!("file path '{file_path}' already exists"),
                retryable: false,
            });
        }

        log::info!("checking if the s3 object '{s3_key}' exists before downloading");
        let head_object = self.exists(s3_bucket, s3_key).await?;
        if head_object.is_none() {
            log::warn!("s3 file '{s3_key}' does not exist in the bucket {s3_bucket}");
            return Ok(false);
        }

        let mut output = self
            .cli
            .get_object()
            .bucket(s3_bucket.to_string())
            .key(s3_key.to_string())
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!("failed get_object {}", e),
                retryable: errors::is_sdk_err_retryable(&e),
            })?;

        // ref. https://docs.rs/tokio-stream/latest/tokio_stream/
        let mut file = File::create(file_path).await.map_err(|e| Error::Other {
            message: format!("failed File::create {}", e),
            retryable: false,
        })?;

        log::info!("writing byte stream to file {}", file_path);
        while let Some(d) = output.body.try_next().await.map_err(|e| Error::Other {
            message: format!("failed ByteStream::try_next {}", e),
            retryable: false,
        })? {
            file.write_all(&d).await.map_err(|e| Error::API {
                message: format!("failed File.write_all {}", e),
                retryable: false,
            })?;
        }
        file.flush().await.map_err(|e| Error::Other {
            message: format!("failed File.flush {}", e),
            retryable: false,
        })?;

        Ok(true)
    }

    /// Returns "true" if successfully downloaded, or skipped to not overwrite.
    /// Returns "false" if not exists.
    pub async fn download_executable_with_retries(
        &self,
        s3_bucket: &str,
        source_s3_path: &str,
        target_file_path: &str,
        overwrite: bool,
    ) -> Result<bool> {
        log::info!("downloading '{source_s3_path}' in bucket '{s3_bucket}' to executable '{target_file_path}' (overwrite {overwrite})");
        let need_download = if Path::new(target_file_path).exists() {
            if overwrite {
                log::warn!(
                    "'{target_file_path}' already exists but overwrite true thus need download"
                );
                true
            } else {
                log::warn!(
                    "'{target_file_path}' already exists and overwrite false thus no need download"
                );
                false
            }
        } else {
            log::warn!("'{target_file_path}' does not exist thus need download");
            true
        };

        if !need_download {
            log::info!("skipped download");
            return Ok(true);
        }

        let tmp_path = random_manager::tmp_path(15, None).map_err(|e| Error::API {
            message: format!("failed random_manager::tmp_path {}", e),
            retryable: false,
        })?;

        let mut success = false;
        for round in 0..20 {
            log::info!("[ROUND {round}] get_object for '{source_s3_path}'");

            match self.get_object(s3_bucket, source_s3_path, &tmp_path).await {
                Ok(found) => {
                    if found {
                        success = true;
                        break;
                    }
                    log::warn!("'{source_s3_path}' does not exist");
                    return Ok(false);
                }
                Err(e) => {
                    if e.retryable() {
                        log::warn!("retriable s3 error '{}'", e);
                        sleep(Duration::from_secs((round + 1) * 5)).await;
                        continue;
                    };

                    return Err(e);
                }
            };
        }
        if !success {
            return Err(Error::API {
                message: "failed get_object after retries".to_string(),
                retryable: false,
            });
        }

        log::info!("successfully downloaded to a temporary file '{tmp_path}'");
        {
            let f = File::open(&tmp_path).await.map_err(|e| Error::API {
                message: format!("failed File::open {}", e),
                retryable: false,
            })?;
            f.set_permissions(PermissionsExt::from_mode(0o777))
                .await
                .map_err(|e| Error::API {
                    message: format!("failed File::set_permissions {}", e),
                    retryable: false,
                })?;
        }

        log::info!("copying '{tmp_path}' to '{target_file_path}'");
        match fs::copy(&tmp_path, &target_file_path).await {
            Ok(_) => log::info!("successfully copied file"),
            Err(e) => {
                // mask the error
                // Os { code: 26, kind: ExecutableFileBusy, message: "Text file busy" }
                if !e.to_string().to_lowercase().contains("text file busy") {
                    return Err(Error::Other {
                        message: format!("failed fs::copy {}", e),
                        retryable: false,
                    });
                }

                log::warn!("failed copy due to file being used '{}'", e);
                return Err(Error::Other {
                    message: format!("failed fs::copy {}", e),
                    retryable: true,
                });
            }
        }

        fs::remove_file(&tmp_path).await.map_err(|e| Error::API {
            message: format!("failed fs::remove_file {}", e),
            retryable: false,
        })?;

        Ok(true)
    }
}

async fn read_file_to_byte_stream(file_path: &str) -> Result<(f64, ByteStream)> {
    let file = Path::new(file_path);
    if !file.exists() {
        return Err(Error::Other {
            message: format!("file path '{file_path}' does not exist"),
            retryable: false,
        });
    }

    let meta = fs::metadata(file_path).await.map_err(|e| Error::Other {
        message: format!("failed fs::metadata {}", e),
        retryable: false,
    })?;

    let size = meta.len() as f64;
    let byte_stream = ByteStream::from_path(file)
        .await
        .map_err(|e| Error::Other {
            message: format!("failed ByteStream::from_file {}", e),
            retryable: false,
        })?;
    Ok((size, byte_stream))
}

#[inline]
fn is_err_already_exists_create_bucket(e: &SdkError<CreateBucketError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            err.err().is_bucket_already_exists() || err.err().is_bucket_already_owned_by_you()
        }
        _ => false,
    }
}

#[inline]
fn explain_err_delete_bucket(e: &SdkError<DeleteBucketError>) -> String {
    match e {
        SdkError::ServiceError(err) => format!(
            "delete_bucket [code '{:?}', message '{:?}']",
            err.err().meta().code(),
            err.err().meta().message(),
        ),
        _ => e.to_string(),
    }
}

#[inline]
fn is_err_does_not_exist_delete_bucket(e: &SdkError<DeleteBucketError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            let msg = format!("{:?}", err);
            msg.contains("bucket does not exist")
        }
        _ => false,
    }
}

#[inline]
fn is_err_head_not_found(e: &SdkError<HeadObjectError>) -> bool {
    match e {
        SdkError::ServiceError(err) => err.err().is_not_found(),
        _ => false,
    }
}

/// TODO: handle "code" and "message" None if the object does not exist
#[inline]
fn explain_err_head_object(e: &SdkError<HeadObjectError>) -> String {
    match e {
        SdkError::ServiceError(err) => format!(
            "head_object [code '{:?}', message '{:?}']",
            err.err().meta().code(),
            err.err().meta().message(),
        ),
        _ => e.to_string(),
    }
}

#[inline]
fn explain_err_delete_objects(e: &SdkError<DeleteObjectsError>) -> String {
    match e {
        SdkError::ServiceError(err) => format!(
            "delete_objects [code '{:?}', message '{:?}']",
            err.err().meta().code(),
            err.err().meta().message(),
        ),
        _ => e.to_string(),
    }
}

#[inline]
pub fn explain_err_put_bucket_lifecycle_configuration(
    e: &SdkError<PutBucketLifecycleConfigurationError>,
) -> String {
    match e {
        SdkError::ServiceError(err) => format!(
            "put_bucket_lifecycle_configuration [code '{:?}', message '{:?}']",
            err.err().meta().code(),
            err.err().meta().message(),
        ),
        _ => e.to_string(),
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
