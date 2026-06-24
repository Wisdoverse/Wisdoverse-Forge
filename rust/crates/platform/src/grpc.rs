use std::collections::HashMap;

use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Status};
use uuid::Uuid;

pub mod proto {
    pub mod platform {
        tonic::include_proto!("platform");
    }
}

use proto::platform::agent_service_client::AgentServiceClient;
use proto::platform::runtime_service_client::RuntimeServiceClient;
use proto::platform::{AgentIdRequest, CreateAgentRequest, DestroyAgentRequest, SendPromptRequest};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformRuntimeCreateRequest {
    pub agent_id: Uuid,
    pub project_path: String,
    pub image: String,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformRuntimeCreateResult {
    pub agent_id: Uuid,
    pub container_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformSessionStatus {
    pub agent_id: Uuid,
    pub status: String,
    pub container_id: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum PlatformGrpcError {
    #[error("invalid grpc endpoint: {0}")]
    InvalidEndpoint(String),
    #[error("platform grpc transport error: {0}")]
    Transport(#[from] tonic::transport::Error),
    #[error("platform grpc status {code}: {message}")]
    Status { code: Code, message: String },
    #[error("invalid uuid from platform: {0}")]
    InvalidUuid(String),
}

impl From<Status> for PlatformGrpcError {
    fn from(status: Status) -> Self {
        Self::Status { code: status.code(), message: status.message().to_string() }
    }
}

#[derive(Clone)]
pub struct PlatformRuntimeGrpcClient {
    channel: Channel,
}

impl PlatformRuntimeGrpcClient {
    pub async fn connect(endpoint: String) -> Result<Self, PlatformGrpcError> {
        let endpoint =
            Endpoint::from_shared(endpoint).map_err(|err| PlatformGrpcError::InvalidEndpoint(err.to_string()))?;
        let channel = endpoint.connect().await?;
        Ok(Self { channel })
    }

    pub async fn create_agent(
        &self,
        request: PlatformRuntimeCreateRequest,
    ) -> Result<PlatformRuntimeCreateResult, PlatformGrpcError> {
        let mut client = RuntimeServiceClient::new(self.channel.clone());
        let response = client
            .create_agent(CreateAgentRequest {
                agent_id: request.agent_id.to_string(),
                project_path: request.project_path,
                image: request.image,
                env: request.env,
                mounts: Vec::new(),
                ssh_private_key: String::new(),
                git_url: String::new(),
                provider: String::new(),
                model: String::new(),
                api_key: String::new(),
                resource_limits: None,
            })
            .await?
            .into_inner();

        Ok(PlatformRuntimeCreateResult {
            agent_id: parse_uuid(&response.agent_id)?,
            container_id: response.container_id,
        })
    }

    pub async fn send_prompt(&self, agent_id: Uuid, prompt: &str) -> Result<(), PlatformGrpcError> {
        let mut client = RuntimeServiceClient::new(self.channel.clone());
        client
            .send_prompt(SendPromptRequest {
                agent_id: agent_id.to_string(),
                prompt: prompt.to_string(),
                image_paths: Vec::new(),
            })
            .await?;
        Ok(())
    }

    pub async fn destroy_agent(&self, agent_id: Uuid) -> Result<(), PlatformGrpcError> {
        let mut client = RuntimeServiceClient::new(self.channel.clone());
        client.destroy_agent(DestroyAgentRequest { agent_id: agent_id.to_string(), force: true }).await?;
        Ok(())
    }

    pub async fn session_status(&self, agent_id: Uuid) -> Result<PlatformSessionStatus, PlatformGrpcError> {
        let mut client = AgentServiceClient::new(self.channel.clone());
        let response = client.get_agent_state(AgentIdRequest { agent_id: agent_id.to_string() }).await?.into_inner();
        Ok(PlatformSessionStatus {
            agent_id: parse_uuid(&response.agent_id)?,
            status: response.status,
            container_id: (!response.container_id.is_empty()).then_some(response.container_id),
        })
    }
}

fn parse_uuid(value: &str) -> Result<Uuid, PlatformGrpcError> {
    Uuid::parse_str(value).map_err(|_| PlatformGrpcError::InvalidUuid(value.to_string()))
}
