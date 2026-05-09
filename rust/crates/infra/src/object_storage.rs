//! Attachment object storage backed by either local disk or S3-compatible MinIO.

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use agentforge_core::{AppConfig, AppResult, ErrorKind};
use aws_credential_types::Credentials;
use aws_credential_types::provider::SharedCredentialsProvider;
use aws_sdk_s3::config::{BehaviorVersion, Region};
use aws_sdk_s3::primitives::ByteStream;
use secrecy::ExposeSecret;

#[derive(Clone)]
pub enum ObjectStorageClient {
    Local { root: PathBuf },
    Minio { client: Arc<aws_sdk_s3::Client>, bucket: String },
}

impl ObjectStorageClient {
    pub async fn new(config: &AppConfig) -> AppResult<Self> {
        match config.storage_provider.as_str() {
            "local" => {
                let root = expand_home(&config.storage_local_path);
                tokio::fs::create_dir_all(&root).await.map_err(|err| {
                    ErrorKind::Unavailable(format!("failed to create local attachment storage root: {err}"))
                })?;
                Ok(Self::Local { root })
            }
            "minio" => {
                let endpoint = config.minio_endpoint.as_deref().ok_or_else(|| {
                    ErrorKind::Unavailable("MINIO_ENDPOINT is required when STORAGE_PROVIDER=minio".to_string())
                })?;
                let access_key = config.minio_access_key.as_ref().ok_or_else(|| {
                    ErrorKind::Unavailable("MINIO_ACCESS_KEY is required when STORAGE_PROVIDER=minio".to_string())
                })?;
                let secret_key = config.minio_secret_key.as_ref().ok_or_else(|| {
                    ErrorKind::Unavailable("MINIO_SECRET_KEY is required when STORAGE_PROVIDER=minio".to_string())
                })?;
                let endpoint_url = normalize_endpoint(endpoint, config.minio_use_ssl);
                let region = config.minio_region.as_deref().unwrap_or("us-east-1");
                let credentials = Credentials::new(
                    access_key.expose_secret().to_string(),
                    secret_key.expose_secret().to_string(),
                    None,
                    None,
                    "agentforge-minio",
                );
                let s3_config = aws_sdk_s3::Config::builder()
                    .behavior_version(BehaviorVersion::latest())
                    .endpoint_url(endpoint_url)
                    .force_path_style(true)
                    .region(Region::new(region.to_string()))
                    .credentials_provider(SharedCredentialsProvider::new(credentials))
                    .build();
                let client = aws_sdk_s3::Client::from_conf(s3_config);
                ensure_bucket(&client, &config.minio_bucket).await?;
                Ok(Self::Minio { client: Arc::new(client), bucket: config.minio_bucket.clone() })
            }
            other => Err(ErrorKind::Validation(format!("unsupported storage provider: {other}")).into()),
        }
    }

    pub fn backend(&self) -> &'static str {
        match self {
            Self::Local { .. } => "local",
            Self::Minio { .. } => "minio",
        }
    }

    pub async fn put_bytes(&self, key: &str, content_type: &str, bytes: Vec<u8>) -> AppResult<()> {
        match self {
            Self::Local { root } => {
                let path = safe_local_path(root, key)?;
                if let Some(parent) = path.parent() {
                    tokio::fs::create_dir_all(parent).await.map_err(|err| {
                        ErrorKind::Unavailable(format!("failed to create attachment object directory: {err}"))
                    })?;
                }
                tokio::fs::write(path, bytes)
                    .await
                    .map_err(|err| ErrorKind::Unavailable(format!("failed to write attachment object: {err}")).into())
            }
            Self::Minio { client, bucket } => client
                .put_object()
                .bucket(bucket)
                .key(key)
                .content_type(content_type)
                .body(ByteStream::from(bytes))
                .send()
                .await
                .map(|_| ())
                .map_err(|err| ErrorKind::Unavailable(format!("failed to put attachment object: {err}")).into()),
        }
    }

    pub async fn get_bytes(&self, key: &str) -> AppResult<Vec<u8>> {
        match self {
            Self::Local { root } => {
                let path = safe_local_path(root, key)?;
                tokio::fs::read(path).await.map_err(|err| {
                    let kind = if err.kind() == std::io::ErrorKind::NotFound {
                        ErrorKind::NotFound(format!("attachment object {key}"))
                    } else {
                        ErrorKind::Unavailable(format!("failed to read attachment object: {err}"))
                    };
                    kind.into()
                })
            }
            Self::Minio { client, bucket } => {
                let output = client
                    .get_object()
                    .bucket(bucket)
                    .key(key)
                    .send()
                    .await
                    .map_err(|err| ErrorKind::Unavailable(format!("failed to get attachment object: {err}")))?;
                let bytes =
                    output.body.collect().await.map_err(|err| {
                        ErrorKind::Unavailable(format!("failed to read attachment object body: {err}"))
                    })?;
                Ok(bytes.into_bytes().to_vec())
            }
        }
    }

    pub async fn delete(&self, key: &str) -> AppResult<()> {
        match self {
            Self::Local { root } => {
                let path = safe_local_path(root, key)?;
                match tokio::fs::remove_file(path).await {
                    Ok(()) => Ok(()),
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(err) => {
                        Err(ErrorKind::Unavailable(format!("failed to delete attachment object: {err}")).into())
                    }
                }
            }
            Self::Minio { client, bucket } => client
                .delete_object()
                .bucket(bucket)
                .key(key)
                .send()
                .await
                .map(|_| ())
                .map_err(|err| ErrorKind::Unavailable(format!("failed to delete attachment object: {err}")).into()),
        }
    }
}

async fn ensure_bucket(client: &aws_sdk_s3::Client, bucket: &str) -> AppResult<()> {
    if client.head_bucket().bucket(bucket).send().await.is_ok() {
        return Ok(());
    }
    client
        .create_bucket()
        .bucket(bucket)
        .send()
        .await
        .map(|_| ())
        .map_err(|err| ErrorKind::Unavailable(format!("failed to ensure MinIO bucket '{bucket}': {err}")).into())
}

fn normalize_endpoint(endpoint: &str, use_ssl: bool) -> String {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        endpoint.to_string()
    } else {
        format!("{}://{}", if use_ssl { "https" } else { "http" }, endpoint)
    }
}

fn expand_home(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

fn safe_local_path(root: &Path, key: &str) -> AppResult<PathBuf> {
    let key_path = Path::new(key);
    if key_path.is_absolute()
        || key_path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_)))
    {
        return Err(ErrorKind::Validation("invalid attachment object key".to_string()).into());
    }
    Ok(root.join(key_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_normalization_adds_scheme() {
        assert_eq!(normalize_endpoint("localhost:9000", false), "http://localhost:9000");
        assert_eq!(normalize_endpoint("minio:9000", true), "https://minio:9000");
        assert_eq!(normalize_endpoint("http://minio:9000", true), "http://minio:9000");
    }

    #[test]
    fn local_path_rejects_traversal() {
        let root = PathBuf::from("/tmp/root");
        assert!(safe_local_path(&root, "org/file.txt").is_ok());
        assert!(safe_local_path(&root, "../file.txt").is_err());
        assert!(safe_local_path(&root, "/file.txt").is_err());
    }
}
