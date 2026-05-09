use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use agentforge_platform::grpc::{PlatformRuntimeCreateRequest, PlatformRuntimeGrpcClient, PlatformSessionStatus};
use tonic::{Request, Response, Status};
use uuid::Uuid;

#[derive(Clone, Default)]
struct MockRuntimeState {
    prompts: Arc<Mutex<Vec<(String, String)>>>,
    destroyed: Arc<Mutex<Vec<String>>>,
}

#[derive(Clone, Default)]
struct MockRuntimeService {
    state: MockRuntimeState,
}

#[tonic::async_trait]
impl agentforge_platform::grpc::proto::platform::runtime_service_server::RuntimeService for MockRuntimeService {
    async fn create_agent(
        &self,
        request: Request<agentforge_platform::grpc::proto::platform::CreateAgentRequest>,
    ) -> Result<Response<agentforge_platform::grpc::proto::platform::CreateAgentResponse>, Status> {
        let request = request.into_inner();
        Ok(Response::new(agentforge_platform::grpc::proto::platform::CreateAgentResponse {
            container_id: format!("ctr-{}", request.agent_id),
            agent_id: request.agent_id,
        }))
    }

    async fn start_agent(
        &self,
        _request: Request<agentforge_platform::grpc::proto::platform::StartAgentRequest>,
    ) -> Result<Response<agentforge_platform::grpc::proto::platform::Empty>, Status> {
        Err(Status::unimplemented("not used in rust MCP cutover contract"))
    }

    async fn stop_agent(
        &self,
        _request: Request<agentforge_platform::grpc::proto::platform::StopAgentRequest>,
    ) -> Result<Response<agentforge_platform::grpc::proto::platform::Empty>, Status> {
        Err(Status::unimplemented("not used in rust MCP cutover contract"))
    }

    async fn destroy_agent(
        &self,
        request: Request<agentforge_platform::grpc::proto::platform::DestroyAgentRequest>,
    ) -> Result<Response<agentforge_platform::grpc::proto::platform::Empty>, Status> {
        self.state.destroyed.lock().expect("destroyed").push(request.into_inner().agent_id);
        Ok(Response::new(agentforge_platform::grpc::proto::platform::Empty {}))
    }

    async fn send_prompt(
        &self,
        request: Request<agentforge_platform::grpc::proto::platform::SendPromptRequest>,
    ) -> Result<Response<agentforge_platform::grpc::proto::platform::Empty>, Status> {
        let request = request.into_inner();
        self.state.prompts.lock().expect("prompts").push((request.agent_id, request.prompt));
        Ok(Response::new(agentforge_platform::grpc::proto::platform::Empty {}))
    }

    async fn resume_agent(
        &self,
        _request: Request<agentforge_platform::grpc::proto::platform::ResumeAgentRequest>,
    ) -> Result<Response<agentforge_platform::grpc::proto::platform::Empty>, Status> {
        Err(Status::unimplemented("not used in rust MCP cutover contract"))
    }

    async fn get_agent_output(
        &self,
        _request: Request<agentforge_platform::grpc::proto::platform::GetOutputRequest>,
    ) -> Result<Response<agentforge_platform::grpc::proto::platform::GetOutputResponse>, Status> {
        Err(Status::unimplemented("not used in rust MCP cutover contract"))
    }

    type StreamOutputStream =
        tokio_stream::wrappers::ReceiverStream<Result<agentforge_platform::grpc::proto::platform::OutputChunk, Status>>;

    async fn stream_output(
        &self,
        _request: Request<agentforge_platform::grpc::proto::platform::StreamOutputRequest>,
    ) -> Result<Response<Self::StreamOutputStream>, Status> {
        Err(Status::unimplemented("not used in rust MCP cutover contract"))
    }

    async fn exec_in_agent(
        &self,
        _request: Request<agentforge_platform::grpc::proto::platform::ExecInAgentRequest>,
    ) -> Result<Response<agentforge_platform::grpc::proto::platform::ExecInAgentResponse>, Status> {
        Err(Status::unimplemented("not used in rust MCP cutover contract"))
    }

    async fn is_alive(
        &self,
        _request: Request<agentforge_platform::grpc::proto::platform::IsAliveRequest>,
    ) -> Result<Response<agentforge_platform::grpc::proto::platform::IsAliveResponse>, Status> {
        Err(Status::unimplemented("not used in rust MCP cutover contract"))
    }

    async fn send_raw_input(
        &self,
        _request: Request<agentforge_platform::grpc::proto::platform::SendRawInputRequest>,
    ) -> Result<Response<agentforge_platform::grpc::proto::platform::Empty>, Status> {
        Err(Status::unimplemented("not used in rust MCP cutover contract"))
    }

    type AttachTerminalStream = tokio_stream::wrappers::ReceiverStream<
        Result<agentforge_platform::grpc::proto::platform::TerminalOutput, Status>,
    >;

    async fn attach_terminal(
        &self,
        _request: Request<tonic::Streaming<agentforge_platform::grpc::proto::platform::TerminalInput>>,
    ) -> Result<Response<Self::AttachTerminalStream>, Status> {
        Err(Status::unimplemented("not used in rust MCP cutover contract"))
    }
}

#[derive(Clone, Default)]
struct MockAgentService;

#[tonic::async_trait]
impl agentforge_platform::grpc::proto::platform::agent_service_server::AgentService for MockAgentService {
    async fn get_agent_state(
        &self,
        request: Request<agentforge_platform::grpc::proto::platform::AgentIdRequest>,
    ) -> Result<Response<agentforge_platform::grpc::proto::platform::AgentState>, Status> {
        Ok(Response::new(agentforge_platform::grpc::proto::platform::AgentState {
            agent_id: request.into_inner().agent_id,
            status: "working".to_string(),
            container_id: "ctr-status".to_string(),
            git_status: None,
            tokens: None,
            last_activity: None,
            created_at: None,
        }))
    }

    async fn update_agent_state(
        &self,
        _request: Request<agentforge_platform::grpc::proto::platform::UpdateStateRequest>,
    ) -> Result<Response<agentforge_platform::grpc::proto::platform::AgentState>, Status> {
        Err(Status::unimplemented("not used in rust MCP cutover contract"))
    }

    async fn run_health_check(
        &self,
        _request: Request<agentforge_platform::grpc::proto::platform::Empty>,
    ) -> Result<Response<agentforge_platform::grpc::proto::platform::HealthCheckResult>, Status> {
        Err(Status::unimplemented("not used in rust MCP cutover contract"))
    }

    async fn get_git_status(
        &self,
        _request: Request<agentforge_platform::grpc::proto::platform::AgentIdRequest>,
    ) -> Result<Response<agentforge_platform::grpc::proto::platform::GitStatus>, Status> {
        Err(Status::unimplemented("not used in rust MCP cutover contract"))
    }

    async fn poll_all_git_status(
        &self,
        _request: Request<agentforge_platform::grpc::proto::platform::Empty>,
    ) -> Result<Response<agentforge_platform::grpc::proto::platform::GitPollResponse>, Status> {
        Err(Status::unimplemented("not used in rust MCP cutover contract"))
    }

    async fn list_active_agents(
        &self,
        _request: Request<agentforge_platform::grpc::proto::platform::ListActiveAgentsRequest>,
    ) -> Result<Response<agentforge_platform::grpc::proto::platform::ListActiveAgentsResponse>, Status> {
        Err(Status::unimplemented("not used in rust MCP cutover contract"))
    }
}

#[tokio::test]
async fn grpc_client_supports_create_prompt_status_and_destroy() {
    let runtime = MockRuntimeService::default();
    let state = runtime.state.clone();
    let agent = MockAgentService;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
    let server = tonic::transport::Server::builder()
        .add_service(agentforge_platform::grpc::proto::platform::runtime_service_server::RuntimeServiceServer::new(
            runtime,
        ))
        .add_service(agentforge_platform::grpc::proto::platform::agent_service_server::AgentServiceServer::new(agent))
        .serve_with_incoming(incoming);
    let handle = tokio::spawn(server);

    let endpoint = format!("http://{addr}");
    let client = PlatformRuntimeGrpcClient::connect(endpoint).await.expect("connect grpc client");
    let agent_id = Uuid::now_v7();

    let created = client
        .create_agent(PlatformRuntimeCreateRequest {
            agent_id,
            project_path: "/workspace/project".to_string(),
            image: "agentforge-agent-codex:latest".to_string(),
            env: HashMap::from([("OPENAI_API_KEY".to_string(), "key".to_string())]),
        })
        .await
        .expect("create agent");
    assert_eq!(created.container_id, format!("ctr-{agent_id}"));

    client.send_prompt(agent_id, "ship it").await.expect("send prompt");
    let status = client.session_status(agent_id).await.expect("status");
    assert_eq!(
        status,
        PlatformSessionStatus { agent_id, status: "working".to_string(), container_id: Some("ctr-status".to_string()) }
    );
    client.destroy_agent(agent_id).await.expect("destroy agent");

    handle.abort();
    let _ = handle.await;

    assert_eq!(state.prompts.lock().expect("prompts").as_slice(), &[(agent_id.to_string(), "ship it".to_string())]);
    assert_eq!(state.destroyed.lock().expect("destroyed").as_slice(), &[agent_id.to_string()]);
}
