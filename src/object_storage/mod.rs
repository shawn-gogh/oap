//! MinIO/S3-compatible object storage for per-session workspaces.
//!
//! Two `aws_sdk_s3::Client`s are kept: `internal` (real network calls —
//! create/list/delete against the docker-network-only MinIO endpoint) and
//! `presign` (never sends a request; only used to compute presigned URL
//! signatures against the browser-reachable public endpoint). SigV4 signs
//! the request's Host header, so a presigned URL generated for one host and
//! then rewritten to another would fail signature verification — using two
//! separately-configured clients avoids that instead of string-rewriting URLs.

use std::time::Duration;

use aws_sdk_s3::{
    config::{BehaviorVersion, Builder as S3ConfigBuilder, Credentials, Region},
    presigning::PresigningConfig,
    primitives::ByteStream,
    Client,
};

use crate::{errors::GatewayError, proxy::config::GeneralSettings};

#[derive(Debug, Clone)]
pub struct ObjectStorageClient {
    internal: Client,
    presign: Client,
}

pub struct ObjectMeta {
    pub key: String,
    pub size: i64,
    pub last_modified_ms: Option<i64>,
    pub etag: Option<String>,
}

fn env_or(configured: Option<&str>, var: &str) -> Option<String> {
    configured
        .map(str::to_owned)
        .or_else(|| std::env::var(var).ok())
}

fn build_client(endpoint: &str, access_key: &str, secret_key: &str) -> Client {
    let credentials = Credentials::new(access_key, secret_key, None, None, "minio-static");
    let config = S3ConfigBuilder::new()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new("us-east-1"))
        .endpoint_url(endpoint)
        .credentials_provider(credentials)
        // MinIO (and most self-hosted S3-compatible stores) expect path-style
        // addressing (http://host/bucket/key) rather than virtual-hosted
        // (http://bucket.host/key), which requires per-bucket DNS.
        .force_path_style(true)
        .build();
    Client::from_conf(config)
}

impl ObjectStorageClient {
    /// Builds a client from config-file settings, falling back to the
    /// MINIO_* env vars directly (so a plain docker-compose env-var setup
    /// works without touching general_settings.yaml). Returns None if MinIO
    /// isn't configured at all — workspace features are simply unavailable
    /// in that case, everything else keeps working.
    pub fn from_settings(settings: &GeneralSettings) -> Option<Self> {
        let internal_endpoint = env_or(settings.minio_endpoint.as_deref(), "MINIO_ENDPOINT")?;
        let public_endpoint = env_or(
            settings.minio_public_endpoint.as_deref(),
            "MINIO_PUBLIC_ENDPOINT",
        )
        .unwrap_or_else(|| internal_endpoint.clone());
        let access_key = env_or(settings.minio_access_key.as_deref(), "MINIO_ACCESS_KEY")?;
        let secret_key = env_or(settings.minio_secret_key.as_deref(), "MINIO_SECRET_KEY")?;

        Some(Self {
            internal: build_client(&internal_endpoint, &access_key, &secret_key),
            presign: build_client(&public_endpoint, &access_key, &secret_key),
        })
    }

    /// S3/MinIO bucket names must be lowercase and can't contain underscores;
    /// session ids look like `ses_<hex>`.
    pub fn bucket_name(session_id: &str) -> String {
        format!("workspace-{}", session_id.replace('_', "-").to_lowercase())
    }

    /// Bucket holding an agent's knowledge/template files, seeded into each
    /// new session workspace. Derived by convention like session buckets —
    /// no column needed.
    pub fn agent_bucket_name(agent_id: &str) -> String {
        format!(
            "agent-workspace-{}",
            agent_id.replace('_', "-").to_lowercase()
        )
    }

    pub async fn bucket_exists(&self, bucket: &str) -> bool {
        self.internal
            .head_bucket()
            .bucket(bucket)
            .send()
            .await
            .is_ok()
    }

    pub async fn ensure_bucket(&self, bucket: &str) -> Result<(), GatewayError> {
        if self
            .internal
            .head_bucket()
            .bucket(bucket)
            .send()
            .await
            .is_ok()
        {
            return Ok(());
        }
        self.internal
            .create_bucket()
            .bucket(bucket)
            .send()
            .await
            .map(|_| ())
            .map_err(|e| {
                GatewayError::SandboxError(format!("failed to create bucket {bucket}: {e:?}"))
            })
    }

    pub async fn presign_put(
        &self,
        bucket: &str,
        key: &str,
        ttl: Duration,
    ) -> Result<String, GatewayError> {
        let presigned = self
            .presign
            .put_object()
            .bucket(bucket)
            .key(key)
            .presigned(presigning_config(ttl)?)
            .await
            .map_err(|e| GatewayError::SandboxError(format!("presign put failed: {e}")))?;
        Ok(presigned.uri().to_string())
    }

    pub async fn presign_get(
        &self,
        bucket: &str,
        key: &str,
        ttl: Duration,
    ) -> Result<String, GatewayError> {
        let presigned = self
            .presign
            .get_object()
            .bucket(bucket)
            .key(key)
            .presigned(presigning_config(ttl)?)
            .await
            .map_err(|e| GatewayError::SandboxError(format!("presign get failed: {e}")))?;
        Ok(presigned.uri().to_string())
    }

    pub async fn list_objects(&self, bucket: &str) -> Result<Vec<ObjectMeta>, GatewayError> {
        // ListObjectsV2 caps a single response at 1000 keys; a session's
        // workspace routinely exceeds that once opencode's own .git/.opencode
        // bookkeeping is in the bucket, so this must page through
        // continuation tokens rather than silently truncating.
        let mut items = Vec::new();
        let mut continuation_token = None;
        loop {
            let mut req = self.internal.list_objects_v2().bucket(bucket);
            if let Some(token) = continuation_token.take() {
                req = req.continuation_token(token);
            }
            let resp = req
                .send()
                .await
                .map_err(|e| GatewayError::SandboxError(format!("list objects failed: {e}")))?;
            items.extend(resp.contents().iter().filter_map(|obj| {
                Some(ObjectMeta {
                    key: obj.key()?.to_owned(),
                    size: obj.size().unwrap_or(0),
                    last_modified_ms: obj.last_modified().map(|t| t.secs() * 1000),
                    etag: obj.e_tag().map(|value| value.trim_matches('"').to_owned()),
                })
            }));
            if resp.is_truncated().unwrap_or(false) {
                continuation_token = resp.next_continuation_token().map(str::to_owned);
            } else {
                break;
            }
        }
        Ok(items)
    }

    pub async fn copy_object(
        &self,
        src_bucket: &str,
        key: &str,
        dst_bucket: &str,
    ) -> Result<(), GatewayError> {
        self.copy_object_as(src_bucket, key, dst_bucket, key).await
    }

    pub async fn copy_object_as(
        &self,
        src_bucket: &str,
        src_key: &str,
        dst_bucket: &str,
        dst_key: &str,
    ) -> Result<(), GatewayError> {
        let encoded_key: String = src_key
            .split('/')
            .map(urlencoding_encode)
            .collect::<Vec<_>>()
            .join("/");
        self.internal
            .copy_object()
            .copy_source(format!("{src_bucket}/{encoded_key}"))
            .bucket(dst_bucket)
            .key(dst_key)
            .send()
            .await
            .map(|_| ())
            .map_err(|e| {
                GatewayError::SandboxError(format!(
                    "copy object {src_key} to {dst_key} failed: {e}"
                ))
            })
    }

    /// Copies every object from `src_bucket` into `dst_bucket`. A missing
    /// source bucket counts as empty (agent workspaces are created lazily on
    /// first upload, so most agents won't have one).
    pub async fn copy_all(&self, src_bucket: &str, dst_bucket: &str) -> Result<(), GatewayError> {
        if self
            .internal
            .head_bucket()
            .bucket(src_bucket)
            .send()
            .await
            .is_err()
        {
            return Ok(());
        }
        for obj in self.list_objects(src_bucket).await? {
            self.copy_object(src_bucket, &obj.key, dst_bucket).await?;
        }
        Ok(())
    }

    pub async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), GatewayError> {
        self.internal
            .delete_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map(|_| ())
            .map_err(|e| GatewayError::SandboxError(format!("delete object failed: {e}")))
    }

    pub async fn put_bytes(
        &self,
        bucket: &str,
        key: &str,
        bytes: Vec<u8>,
    ) -> Result<(), GatewayError> {
        self.internal
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(ByteStream::from(bytes))
            .send()
            .await
            .map(|_| ())
            .map_err(|e| GatewayError::SandboxError(format!("put object {key} failed: {e}")))
    }

    pub async fn get_bytes(&self, bucket: &str, key: &str) -> Result<Vec<u8>, GatewayError> {
        let response = self
            .internal
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| GatewayError::SandboxError(format!("get object {key} failed: {e}")))?;
        response
            .body
            .collect()
            .await
            .map(|bytes| bytes.into_bytes().to_vec())
            .map_err(|e| GatewayError::SandboxError(format!("read object {key} failed: {e}")))
    }

    pub async fn object_meta(&self, bucket: &str, key: &str) -> Result<ObjectMeta, GatewayError> {
        let response = self
            .internal
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| GatewayError::SandboxError(format!("head object {key} failed: {e}")))?;
        Ok(ObjectMeta {
            key: key.to_owned(),
            size: response.content_length().unwrap_or_default(),
            last_modified_ms: response.last_modified().map(|time| time.secs() * 1000),
            etag: response
                .e_tag()
                .map(|value| value.trim_matches('"').to_owned()),
        })
    }

    /// Deletes every object in the bucket, then the bucket itself. Used when
    /// a session is deleted; best-effort per-object (a single stuck object
    /// shouldn't block session deletion), but bucket deletion is reported.
    pub async fn delete_bucket_recursive(&self, bucket: &str) -> Result<(), GatewayError> {
        loop {
            let resp = self
                .internal
                .list_objects_v2()
                .bucket(bucket)
                .send()
                .await
                .map_err(|e| GatewayError::SandboxError(format!("list objects failed: {e}")))?;
            let keys: Vec<String> = resp
                .contents()
                .iter()
                .filter_map(|o| o.key().map(str::to_owned))
                .collect();
            if keys.is_empty() {
                break;
            }
            for key in &keys {
                let _ = self
                    .internal
                    .delete_object()
                    .bucket(bucket)
                    .key(key)
                    .send()
                    .await;
            }
            if !resp.is_truncated().unwrap_or(false) {
                break;
            }
        }
        self.internal
            .delete_bucket()
            .bucket(bucket)
            .send()
            .await
            .map(|_| ())
            .map_err(|e| {
                GatewayError::SandboxError(format!("failed to delete bucket {bucket}: {e}"))
            })
    }
}

/// Minimal percent-encoding for one path segment of an S3 CopySource header
/// (RFC 3986 unreserved characters pass through). Avoids pulling in a crate
/// for the one place we need it.
fn urlencoding_encode(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn presigning_config(ttl: Duration) -> Result<PresigningConfig, GatewayError> {
    PresigningConfig::expires_in(ttl)
        .map_err(|e| GatewayError::SandboxError(format!("invalid presign ttl: {e}")))
}
