use std::{fs, path::Path};

use crate::errors::{self, Error, Result};
use aws_sdk_s3::{
    operation::{
        create_bucket::CreateBucketError, delete_bucket::DeleteBucketError,
        head_object::HeadObjectError,
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
use tokio::{fs::File, io::AsyncWriteExt};
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
        log::info!(
            "creating S3 bucket '{}' in region {}",
            s3_bucket,
            self.region
        );

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
                if !is_err_already_exists_bucket(&e) {
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
        days: i32,
        prefixes: Vec<String>,
    ) -> Result<()> {
        log::info!("put bucket object expire configuration for '{s3_bucket}' in {days} days with prefixes '{:?}'", prefixes);

        let lifecycle = if !prefixes.is_empty() {
            let mut rules = Vec::new();
            for pfx in prefixes {
                rules.push(
                    // ref. <https://docs.aws.amazon.com/AmazonS3/latest/API/API_LifecycleRule.html>
                    LifecycleRule::builder()
                        // ref. <https://docs.aws.amazon.com/AmazonS3/latest/API/API_LifecycleRuleFilter.html>
                        .filter(LifecycleRuleFilter::Prefix(pfx.to_owned()))
                        .expiration(LifecycleExpiration::builder().days(days).build())
                        .status(ExpirationStatus::Enabled) // If 'Enabled', the rule is currently being applied.
                        .build(),
                );
            }
            BucketLifecycleConfiguration::builder()
                .set_rules(Some(rules))
                .build()
        } else {
            BucketLifecycleConfiguration::builder()
                .rules(
                    LifecycleRule::builder()
                        .expiration(LifecycleExpiration::builder().days(days).build())
                        .status(ExpirationStatus::Enabled) // If 'Enabled', the rule is currently being applied.
                        .build(),
                )
                .build()
        };

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
        log::info!(
            "deleting S3 bucket '{}' in region {}",
            s3_bucket,
            self.region
        );
        let ret = self.cli.delete_bucket().bucket(s3_bucket).send().await;
        match ret {
            Ok(_) => {}
            Err(e) => {
                if !is_err_does_not_exist_bucket(&e) {
                    return Err(Error::API {
                        message: format!("failed delete_bucket {:?}", e),
                        retryable: errors::is_sdk_err_retryable(&e),
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
    pub async fn delete_objects(&self, s3_bucket: &str, prefix: Option<&str>) -> Result<()> {
        log::info!(
            "deleting objects S3 bucket '{}' in region {} (prefix {:?})",
            s3_bucket,
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
                        message: format!("failed delete_bucket {:?}", e),
                        retryable: errors::is_sdk_err_retryable(&e),
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
    /// ref. <https://tokio.rs/tokio/tutorial/spawning>
    ///
    /// ref. <https://docs.aws.amazon.com/AmazonS3/latest/API/API_PutObject.html>
    pub async fn put_object(&self, file_path: &str, s3_bucket: &str, s3_key: &str) -> Result<()> {
        let file = Path::new(file_path);
        if !file.exists() {
            return Err(Error::Other {
                message: format!("file path {} does not exist", file_path),
                retryable: false,
            });
        }

        let meta = fs::metadata(file_path).map_err(|e| Error::Other {
            message: format!("failed metadata {}", e),
            retryable: false,
        })?;
        let size = meta.len() as f64;
        log::info!(
            "put '{}' (size {}) to 's3://{}/{}'",
            file_path,
            human_readable::bytes(size),
            s3_bucket,
            s3_key
        );

        let byte_stream = ByteStream::from_path(file)
            .await
            .map_err(|e| Error::Other {
                message: format!("failed ByteStream::from_file {}", e),
                retryable: false,
            })?;
        self.cli
            .put_object()
            .bucket(s3_bucket.to_string())
            .key(s3_key.to_string())
            .body(byte_stream)
            .acl(ObjectCannedAcl::Private)
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!("failed put_object {}", e),
                retryable: errors::is_sdk_err_retryable(&e),
            })?;

        Ok(())
    }

    /// Returns "true" if the file exists.
    pub async fn exists(&self, s3_bucket: &str, s3_key: &str) -> Result<bool> {
        let res = self
            .cli
            .head_object()
            .bucket(s3_bucket.to_string())
            .key(s3_key.to_string())
            .send()
            .await;

        if let Some(e) = res.as_ref().err() {
            if is_err_head_not_found(e) {
                log::info!("{s3_key} not found");
                return Ok(false);
            }

            log::warn!("failed to head {s3_key} {}", e);
            return Err(Error::API {
                message: format!("failed head_object {}", e),
                retryable: errors::is_sdk_err_retryable(&e),
            });
        }

        let head_output = res.unwrap();
        log::info!(
            "head object exists 's3://{}/{}' (content type '{}', size {})",
            s3_bucket,
            s3_key,
            head_output.content_type().unwrap(),
            human_readable::bytes(head_output.content_length() as f64),
        );

        Ok(true)
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
    pub async fn get_object(&self, s3_bucket: &str, s3_key: &str, file_path: &str) -> Result<()> {
        if Path::new(file_path).exists() {
            return Err(Error::Other {
                message: format!("file path {} already exists", file_path),
                retryable: false,
            });
        }

        let head_output = self
            .cli
            .head_object()
            .bucket(s3_bucket.to_string())
            .key(s3_key.to_string())
            .send()
            .await
            .map_err(|e| Error::API {
                message: format!("failed head_object {}", e),
                retryable: errors::is_sdk_err_retryable(&e),
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

        Ok(())
    }
}

#[inline]
fn is_err_already_exists_bucket(e: &SdkError<CreateBucketError>) -> bool {
    match e {
        SdkError::ServiceError(err) => {
            err.err().is_bucket_already_exists() || err.err().is_bucket_already_owned_by_you()
        }
        _ => false,
    }
}

#[inline]
fn is_err_does_not_exist_bucket(e: &SdkError<DeleteBucketError>) -> bool {
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
