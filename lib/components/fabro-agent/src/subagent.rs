use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use fabro_llm::types::ToolDefinition;
use futures::future;
use tokio::sync::{oneshot, watch};
use tokio::task::{AbortHandle, JoinHandle};
use tokio::time::{Instant, timeout_at};
use tokio_util::sync::CancellationToken;

use crate::error::{Error, InterruptReason};
use crate::session::{Session, SessionShutdownReason};
use crate::tool_registry::{RegisteredTool, ToolSource};
use crate::tools::required_str;
use crate::types::{AgentEvent, SessionEvent};

pub type SessionFactory = Arc<dyn Fn() -> Session + Send + Sync>;

#[derive(Debug, Clone)]
pub enum SubAgentCallbackEvent {
    Lifecycle(AgentEvent),
    Forwarded(SessionEvent),
}

pub type SubAgentEventCallback = Arc<dyn Fn(SubAgentCallbackEvent) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct SubAgentResult {
    pub output:     String,
    pub success:    bool,
    pub turns_used: usize,
}

#[derive(Debug, Clone)]
pub enum SubAgentStatus {
    Running,
    Finished(Result<SubAgentResult, Error>),
    Closing,
    Closed,
}

const SUBAGENT_SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

struct SubAgent {
    status:             watch::Sender<SubAgentStatus>,
    cleanup_done:       watch::Sender<bool>,
    cleanup_started:    bool,
    monitor_task:       Option<JoinHandle<()>>,
    event_forwarder:    Option<JoinHandle<()>>,
    cleanup_task:       Option<JoinHandle<()>>,
    child_abort_handle: AbortHandle,
    followup_queue:     Arc<Mutex<VecDeque<String>>>,
    cancel_token:       CancellationToken,
    depth:              usize,
}

impl Drop for SubAgent {
    fn drop(&mut self) {
        self.cancel_token.cancel();
        self.child_abort_handle.abort();
        if let Some(task) = self.monitor_task.take() {
            task.abort();
        }
        if let Some(task) = self.event_forwarder.take() {
            task.abort();
        }
        if let Some(task) = self.cleanup_task.take() {
            task.abort();
        }
    }
}

#[derive(Default)]
struct SupervisorState {
    agents: HashMap<String, SubAgent>,
}

struct ShutdownWork {
    agent_id:            String,
    depth:               usize,
    close_running_agent: bool,
    status:              watch::Sender<SubAgentStatus>,
    cleanup_done:        watch::Sender<bool>,
    monitor_task:        Option<JoinHandle<()>>,
    event_forwarder:     Option<JoinHandle<()>>,
    child_abort_handle:  AbortHandle,
    cancel_token:        CancellationToken,
}

impl Drop for ShutdownWork {
    fn drop(&mut self) {
        self.cancel_token.cancel();
        self.child_abort_handle.abort();
        if let Some(task) = self.monitor_task.take() {
            task.abort();
        }
        if let Some(task) = self.event_forwarder.take() {
            task.abort();
        }
    }
}

enum ShutdownDisposition {
    Lead(ShutdownWork),
    Follow(watch::Receiver<bool>),
    Done,
}

struct CleanupDoneGuard(watch::Sender<bool>);

impl Drop for CleanupDoneGuard {
    fn drop(&mut self) {
        self.0.send_replace(true);
    }
}

fn spawn_result_monitor(
    child_task: JoinHandle<Result<SubAgentResult, Error>>,
    status: watch::Sender<SubAgentStatus>,
    event_callback: Arc<RwLock<Option<SubAgentEventCallback>>>,
    agent_id: String,
    depth: usize,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let task_result = match child_task.await {
            Ok(result) => result,
            Err(err) => Err(Error::InvalidState(format!(
                "Agent task failed to join: {err}"
            ))),
        };
        let committed = status.send_if_modified(|current| {
            if matches!(current, SubAgentStatus::Running) {
                *current = SubAgentStatus::Finished(task_result.clone());
                true
            } else {
                false
            }
        });
        if !committed {
            return;
        }

        let event = match task_result {
            Ok(result) => AgentEvent::SubAgentCompleted {
                agent_id,
                depth,
                success: result.success,
                turns_used: result.turns_used,
            },
            Err(error) => AgentEvent::SubAgentFailed {
                agent_id,
                depth,
                error,
            },
        };
        let callback = event_callback
            .read()
            .expect("subagent callback lock poisoned")
            .clone();
        if let Some(callback) = callback {
            callback(SubAgentCallbackEvent::Lifecycle(event));
        }
    })
}

/// Owns all child-session tasks for one parent agent session.
///
/// The supervisor is the only production-facing subagent handle. Its internal
/// mutex protects short state transitions only; task waits and callbacks always
/// happen after the guard has been released.
#[derive(Clone)]
pub struct SubAgentSupervisor {
    state:          Arc<Mutex<SupervisorState>>,
    max_depth:      usize,
    event_callback: Arc<RwLock<Option<SubAgentEventCallback>>>,
}

impl SubAgentSupervisor {
    #[must_use]
    pub fn new(max_depth: usize) -> Self {
        Self {
            state: Arc::new(Mutex::new(SupervisorState::default())),
            max_depth,
            event_callback: Arc::new(RwLock::new(None)),
        }
    }

    pub fn set_event_callback(&self, cb: SubAgentEventCallback) {
        *self
            .event_callback
            .write()
            .expect("subagent callback lock poisoned") = Some(cb);
    }

    fn emit_event(&self, event: AgentEvent) {
        let callback = self
            .event_callback
            .read()
            .expect("subagent callback lock poisoned")
            .clone();
        if let Some(cb) = callback {
            cb(SubAgentCallbackEvent::Lifecycle(event));
        }
    }

    pub fn spawn(
        &self,
        mut session: Session,
        task_prompt: String,
        depth: usize,
    ) -> Result<String, Error> {
        if depth >= self.max_depth {
            return Err(Error::InvalidState(format!(
                "Maximum subagent depth ({}) reached",
                self.max_depth
            )));
        }

        let agent_id = format!("{:08x}", uuid::Uuid::new_v4().as_fields().0);
        let followup_queue = session.followup_queue_handle();
        let cancel_token = session.cancel_token();

        // Subscribe before moving the session into its task. The forwarding
        // task is owned by the supervisor and joined during shutdown.
        let event_forwarder = if self
            .event_callback
            .read()
            .expect("subagent callback lock poisoned")
            .is_some()
        {
            let mut rx = session.subscribe();
            let callback = Arc::clone(&self.event_callback);
            Some(tokio::spawn(async move {
                while let Ok(event) = rx.recv().await {
                    // Skip streaming / noise events
                    if event.event.is_streaming_noise()
                        || matches!(
                            &event.event,
                            AgentEvent::SessionStarted { .. }
                                | AgentEvent::SessionEnded
                                | AgentEvent::ProcessingEnd
                        )
                    {
                        continue;
                    }
                    let callback = callback
                        .read()
                        .expect("subagent callback lock poisoned")
                        .clone();
                    if let Some(callback) = callback {
                        callback(SubAgentCallbackEvent::Forwarded(event));
                    }
                }
            }))
        } else {
            None
        };

        let task_prompt_for_spawn = task_prompt.clone();
        let (start_tx, start_rx) = oneshot::channel();
        let child_task = tokio::spawn(async move {
            let _ = start_rx.await;
            let result = async {
                session.initialize().await?;
                let output = session
                    .process_input_with_output(&task_prompt_for_spawn)
                    .await?
                    .ok_or_else(|| {
                        Error::InvalidState(
                            "Subagent completed without a non-empty final response".to_string(),
                        )
                    })?;
                let turns = session.history().turns();
                Ok(SubAgentResult {
                    output,
                    success: true,
                    turns_used: turns.len(),
                })
            }
            .await;
            let reason = match &result {
                Ok(_) => SessionShutdownReason::Completed,
                Err(Error::Interrupted(_)) => SessionShutdownReason::Cancelled,
                Err(_) => SessionShutdownReason::Error,
            };
            session.shutdown(reason).await;
            result
        });
        let child_abort_handle = child_task.abort_handle();
        let (status, _) = watch::channel(SubAgentStatus::Running);
        let (cleanup_done, _) = watch::channel(false);
        let child_depth = depth + 1;
        let monitor_task = spawn_result_monitor(
            child_task,
            status.clone(),
            Arc::clone(&self.event_callback),
            agent_id.clone(),
            child_depth,
        );

        {
            let mut state = self.state.lock().expect("subagent state lock poisoned");
            state.agents.insert(agent_id.clone(), SubAgent {
                status,
                cleanup_done,
                cleanup_started: false,
                monitor_task: Some(monitor_task),
                event_forwarder,
                cleanup_task: None,
                child_abort_handle,
                followup_queue,
                cancel_token,
                depth: child_depth,
            });
        }

        self.emit_event(AgentEvent::SubAgentSpawned {
            agent_id: agent_id.clone(),
            depth:    child_depth,
            task:     task_prompt,
        });
        let _ = start_tx.send(());

        Ok(agent_id)
    }

    pub fn send_input(&self, agent_id: &str, message: &str) -> Result<(), Error> {
        let followup_queue = {
            let state = self.state.lock().expect("subagent state lock poisoned");
            let agent = state.agents.get(agent_id).ok_or_else(|| {
                Error::InvalidState(format!(
                    "No agent found with id: {agent_id} (it was never spawned)"
                ))
            })?;
            if !matches!(*agent.status.borrow(), SubAgentStatus::Running) {
                return Err(Error::InvalidState(format!(
                    "Agent {agent_id} is not running"
                )));
            }
            Arc::clone(&agent.followup_queue)
        };

        followup_queue
            .lock()
            .expect("followup queue lock poisoned")
            .push_back(message.to_string());

        Ok(())
    }

    pub async fn wait_with_cancel(
        &self,
        agent_id: &str,
        cancel: &CancellationToken,
    ) -> Result<SubAgentResult, Error> {
        let mut status = {
            let state = self.state.lock().expect("subagent state lock poisoned");
            state
                .agents
                .get(agent_id)
                .ok_or_else(|| {
                    Error::InvalidState(format!(
                        "No agent found with id: {agent_id} (it was never spawned)"
                    ))
                })?
                .status
                .subscribe()
        };

        loop {
            let current = status.borrow().clone();
            match current {
                SubAgentStatus::Running => {
                    tokio::select! {
                        biased;
                        () = cancel.cancelled() => {
                            self.ensure_closed(agent_id).await?;
                            return Err(Error::Interrupted(InterruptReason::Cancelled));
                        }
                        changed = status.changed() => {
                            changed.map_err(|_| {
                                Error::InvalidState(format!(
                                    "Agent {agent_id} result observer closed unexpectedly"
                                ))
                            })?;
                        }
                    }
                }
                SubAgentStatus::Finished(result) => return result,
                SubAgentStatus::Closing | SubAgentStatus::Closed => {
                    return Err(Error::InvalidState(format!(
                        "Agent {agent_id} has been closed"
                    )));
                }
            }
        }
    }

    #[cfg(test)]
    async fn wait(&self, agent_id: &str) -> Result<SubAgentResult, Error> {
        self.wait_with_cancel(agent_id, &CancellationToken::new())
            .await
    }

    fn begin_shutdown(&self, agent_id: &str, strict: bool) -> Result<ShutdownDisposition, Error> {
        let mut state = self.state.lock().expect("subagent state lock poisoned");
        let agent = state.agents.get_mut(agent_id).ok_or_else(|| {
            Error::InvalidState(format!(
                "No agent found with id: {agent_id} (it was never spawned)"
            ))
        })?;

        let close_running_agent = loop {
            let current = agent.status.borrow().clone();
            match current {
                SubAgentStatus::Running => {
                    if agent.status.send_if_modified(|status| {
                        if matches!(status, SubAgentStatus::Running) {
                            *status = SubAgentStatus::Closing;
                            true
                        } else {
                            false
                        }
                    }) {
                        break true;
                    }
                }
                SubAgentStatus::Finished(_) if strict => {
                    return Err(Error::InvalidState(format!(
                        "Agent {agent_id} is not running"
                    )));
                }
                SubAgentStatus::Finished(_) => break false,
                SubAgentStatus::Closing | SubAgentStatus::Closed if strict => {
                    return Err(Error::InvalidState(format!(
                        "Agent {agent_id} is already closed"
                    )));
                }
                SubAgentStatus::Closing => {
                    return Ok(ShutdownDisposition::Follow(agent.cleanup_done.subscribe()));
                }
                SubAgentStatus::Closed => return Ok(ShutdownDisposition::Done),
            }
        };

        if agent.cleanup_started {
            return Ok(ShutdownDisposition::Follow(agent.cleanup_done.subscribe()));
        }
        agent.cleanup_started = true;

        Ok(ShutdownDisposition::Lead(ShutdownWork {
            agent_id: agent_id.to_string(),
            depth: agent.depth,
            close_running_agent,
            status: agent.status.clone(),
            cleanup_done: agent.cleanup_done.clone(),
            monitor_task: agent.monitor_task.take(),
            event_forwarder: agent.event_forwarder.take(),
            child_abort_handle: agent.child_abort_handle.clone(),
            cancel_token: agent.cancel_token.clone(),
        }))
    }

    async fn run_shutdown(
        mut work: ShutdownWork,
        event_callback: Arc<RwLock<Option<SubAgentEventCallback>>>,
    ) {
        let _cleanup_done = CleanupDoneGuard(work.cleanup_done.clone());
        let deadline = Instant::now() + SUBAGENT_SHUTDOWN_GRACE;
        if work.close_running_agent {
            work.cancel_token.cancel();
        }

        if let Some(mut task) = work.monitor_task.take() {
            if timeout_at(deadline, &mut task).await.is_err() {
                work.child_abort_handle.abort();
                let _ = task.await;
            }
        }

        if let Some(mut task) = work.event_forwarder.take() {
            if timeout_at(deadline, &mut task).await.is_err() {
                task.abort();
                let _ = task.await;
            }
        }

        let emit_closed = work.close_running_agent
            && work.status.send_if_modified(|status| {
                if matches!(status, SubAgentStatus::Closing) {
                    *status = SubAgentStatus::Closed;
                    true
                } else {
                    false
                }
            });
        if emit_closed {
            let callback = event_callback
                .read()
                .expect("subagent callback lock poisoned")
                .clone();
            if let Some(callback) = callback {
                callback(SubAgentCallbackEvent::Lifecycle(
                    AgentEvent::SubAgentClosed {
                        agent_id: work.agent_id.clone(),
                        depth:    work.depth,
                    },
                ));
            }
        }
    }

    fn spawn_shutdown(&self, work: ShutdownWork) -> watch::Receiver<bool> {
        let cleanup_done = work.cleanup_done.subscribe();
        let agent_id = work.agent_id.clone();
        let event_callback = Arc::clone(&self.event_callback);
        let cleanup_task = tokio::spawn(Self::run_shutdown(work, event_callback));
        let mut state = self.state.lock().expect("subagent state lock poisoned");
        let agent = state
            .agents
            .get_mut(&agent_id)
            .expect("shutdown agent should remain supervised");
        debug_assert!(agent.cleanup_task.is_none());
        agent.cleanup_task = Some(cleanup_task);
        cleanup_done
    }

    async fn await_shutdown(&self, agent_id: &str, cleanup_done: watch::Receiver<bool>) {
        Self::follow_shutdown(cleanup_done).await;
        let cleanup_task = {
            let mut state = self.state.lock().expect("subagent state lock poisoned");
            state
                .agents
                .get_mut(agent_id)
                .and_then(|agent| agent.cleanup_task.take())
        };
        if let Some(task) = cleanup_task {
            let _ = task.await;
        }
    }

    async fn follow_shutdown(mut cleanup_done: watch::Receiver<bool>) {
        while !*cleanup_done.borrow() {
            if cleanup_done.changed().await.is_err() {
                break;
            }
        }
    }

    async fn ensure_closed(&self, agent_id: &str) -> Result<(), Error> {
        let cleanup_done = match self.begin_shutdown(agent_id, false)? {
            ShutdownDisposition::Lead(work) => self.spawn_shutdown(work),
            ShutdownDisposition::Follow(cleanup_done) => cleanup_done,
            ShutdownDisposition::Done => return Ok(()),
        };
        self.await_shutdown(agent_id, cleanup_done).await;
        Ok(())
    }

    /// Strict user-facing close: only a currently running child may be closed.
    pub async fn close_agent(&self, agent_id: &str) -> Result<(), Error> {
        let cleanup_done = match self.begin_shutdown(agent_id, true)? {
            ShutdownDisposition::Lead(work) => self.spawn_shutdown(work),
            ShutdownDisposition::Follow(_) | ShutdownDisposition::Done => {
                return Err(Error::InvalidState(format!(
                    "Agent {agent_id} is already closed"
                )));
            }
        };
        self.await_shutdown(agent_id, cleanup_done).await;
        Ok(())
    }

    /// Cooperatively shut down active children and join every owned child and
    /// event-forwarding task. Finished children are reaped without rewriting
    /// their terminal result.
    pub async fn shutdown_all(&self) {
        let ids = {
            let state = self.state.lock().expect("subagent state lock poisoned");
            state.agents.keys().cloned().collect::<Vec<_>>()
        };
        future::join_all(ids.iter().map(|id| self.ensure_closed(id))).await;
    }

    #[must_use]
    pub fn status(&self, agent_id: &str) -> Option<SubAgentStatus> {
        let state = self.state.lock().expect("subagent state lock poisoned");
        state
            .agents
            .get(agent_id)
            .map(|agent| agent.status.borrow().clone())
    }

    #[cfg(test)]
    #[must_use]
    fn contains(&self, agent_id: &str) -> bool {
        self.state
            .lock()
            .expect("subagent state lock poisoned")
            .agents
            .contains_key(agent_id)
    }

    #[cfg(test)]
    #[must_use]
    fn is_empty(&self) -> bool {
        self.state
            .lock()
            .expect("subagent state lock poisoned")
            .agents
            .is_empty()
    }

    #[cfg(test)]
    fn supervise_test_task(
        &self,
        agent_id: String,
        child_task: JoinHandle<Result<SubAgentResult, Error>>,
        cancel_token: CancellationToken,
        event_forwarder: Option<JoinHandle<()>>,
    ) {
        let child_abort_handle = child_task.abort_handle();
        let (status, _) = watch::channel(SubAgentStatus::Running);
        let (cleanup_done, _) = watch::channel(false);
        let depth = 1;
        let monitor_task = spawn_result_monitor(
            child_task,
            status.clone(),
            Arc::clone(&self.event_callback),
            agent_id.clone(),
            depth,
        );
        self.state
            .lock()
            .expect("subagent state lock poisoned")
            .agents
            .insert(agent_id, SubAgent {
                status,
                cleanup_done,
                cleanup_started: false,
                monitor_task: Some(monitor_task),
                event_forwarder,
                cleanup_task: None,
                child_abort_handle,
                followup_queue: Arc::new(Mutex::new(VecDeque::new())),
                cancel_token,
                depth,
            });
    }
}

pub fn make_spawn_agent_tool(
    supervisor: SubAgentSupervisor,
    session_factory: SessionFactory,
    current_depth: usize,
) -> RegisteredTool {
    RegisteredTool {
        definition: ToolDefinition {
            name:        "spawn_agent".into(),
            description: "Spawn a subagent for independent work or context isolation. Use it for tasks that can proceed separately, and avoid duplicating the same work in the parent session.".into(),
            parameters:  serde_json::json!({
                "type": "object",
                "properties": {
                    "task": {
                        "type": "string",
                        "description": "The task description for the subagent"
                    }
                },
                "required": ["task"]
            }),
        },
        executor:   Arc::new(move |args, ctx| {
            let supervisor = supervisor.clone();
            let session_factory = session_factory.clone();
            Box::pin(async move {
                let task = required_str(&args, "task")?;

                let mut session = session_factory();
                // Inherit the parent agent's root session ID so todo tools
                // that scope by root (e.g. Anthropic tasks) share one list
                // across the parent and all subagents.
                if let Some(root) = ctx.root_session_id.as_ref().or(ctx.session_id.as_ref()) {
                    session.set_root_session_id(root.clone());
                }
                supervisor
                    .spawn(session, task.to_string(), current_depth)
                    .map_err(|e| e.to_string())
            })
        }),
        source:     ToolSource::Native,
    }
}

pub fn make_send_input_tool(supervisor: SubAgentSupervisor) -> RegisteredTool {
    RegisteredTool {
        definition: ToolDefinition {
            name:        "send_input".into(),
            description: "Send a follow-up message to a running subagent when new information or corrected instructions are needed.".into(),
            parameters:  serde_json::json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "The ID of the agent to send input to"
                    },
                    "message": {
                        "type": "string",
                        "description": "The message to send to the agent"
                    }
                },
                "required": ["agent_id", "message"]
            }),
        },
        executor:   Arc::new(move |args, _ctx| {
            let supervisor = supervisor.clone();
            Box::pin(async move {
                let agent_id = required_str(&args, "agent_id")?;
                let message = required_str(&args, "message")?;

                supervisor
                    .send_input(agent_id, message)
                    .map_err(|e| e.to_string())?;
                Ok(format!("Message sent to agent {agent_id}"))
            })
        }),
        source:     ToolSource::Native,
    }
}

pub fn make_wait_tool(supervisor: SubAgentSupervisor) -> RegisteredTool {
    RegisteredTool {
        definition: ToolDefinition {
            name:        "wait".into(),
            description: "Wait for a subagent to complete, then use the result to synthesize the outcome for the user.".into(),
            parameters:  serde_json::json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "The ID of the agent to wait for"
                    }
                },
                "required": ["agent_id"]
            }),
        },
        executor:   Arc::new(move |args, ctx| {
            let supervisor = supervisor.clone();
            Box::pin(async move {
                let agent_id = required_str(&args, "agent_id")?;
                let result = match supervisor.wait_with_cancel(agent_id, &ctx.cancel).await {
                    Ok(result) => result,
                    Err(Error::Interrupted(InterruptReason::Cancelled)) => {
                        return Err("Cancelled".to_string());
                    }
                    Err(error) => return Err(error.to_string()),
                };
                Ok(format!(
                    "Agent completed (success: {}, turns: {})\n\n{}",
                    result.success, result.turns_used, result.output
                ))
            })
        }),
        source:     ToolSource::Native,
    }
}

pub fn make_close_agent_tool(supervisor: SubAgentSupervisor) -> RegisteredTool {
    RegisteredTool {
        definition: ToolDefinition {
            name:        "close_agent".into(),
            description: "Close a running subagent that is no longer needed.".into(),
            parameters:  serde_json::json!({
                "type": "object",
                "properties": {
                    "agent_id": {
                        "type": "string",
                        "description": "The ID of the agent to close"
                    }
                },
                "required": ["agent_id"]
            }),
        },
        executor:   Arc::new(move |args, _ctx| {
            let supervisor = supervisor.clone();
            Box::pin(async move {
                let agent_id = required_str(&args, "agent_id")?;
                supervisor
                    .close_agent(agent_id)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(format!("Agent {agent_id} closed"))
            })
        }),
        source:     ToolSource::Native,
    }
}

#[cfg(test)]
mod tests {
    use fabro_llm::provider::ProviderAdapter;
    use fabro_llm::types::Role;
    use tokio::task::yield_now;
    use tokio::time;

    use super::*;
    use crate::config::SessionOptions;
    use crate::test_support::*;
    use crate::tool_registry::ToolContext;

    // --- Tests ---

    #[test]
    fn subagent_tool_descriptions_explain_delegation_lifecycle() {
        let manager = SubAgentSupervisor::new(3);
        let factory: SessionFactory = Arc::new(|| {
            panic!("should not construct subagent in description test");
        });

        let spawn = make_spawn_agent_tool(manager.clone(), factory, 0);
        let send = make_send_input_tool(manager.clone());
        let wait = make_wait_tool(manager.clone());
        let close = make_close_agent_tool(manager);

        assert!(spawn.definition.description.contains("independent work"));
        assert!(spawn.definition.description.contains("context isolation"));
        assert!(send.definition.description.contains("follow-up"));
        assert!(wait.definition.description.contains("synthesize"));
        assert!(close.definition.description.contains("no longer needed"));

        for tool in [spawn, send, wait, close] {
            let text = &tool.definition.description;
            assert!(!text.contains("background Bash"));
            assert!(!text.contains("addComment"));
        }
    }

    #[test]
    fn manager_creation() {
        let manager = SubAgentSupervisor::new(3);
        assert_eq!(manager.max_depth, 3);
        assert!(manager.is_empty());
    }

    #[tokio::test]
    async fn spawn_creates_agent_and_returns_id() {
        let manager = SubAgentSupervisor::new(3);
        let session = make_session(vec![text_response("Hello")]).await;
        let result = manager.spawn(session, "Do something".into(), 0);
        assert!(result.is_ok());
        let agent_id = result.unwrap();
        assert!(!agent_id.is_empty());
        assert!(manager.contains(&agent_id));
    }

    #[tokio::test]
    async fn spawn_initializes_session_before_processing_input() {
        let manager = SubAgentSupervisor::new(3);

        let provider = Arc::new(CapturingLlmProvider::new());
        let provider_ref = provider.clone();
        let client = make_client(provider as Arc<dyn ProviderAdapter>).await;
        let profile = Arc::new(TestProfile::new());
        let env = Arc::new(MockSandbox::default());
        let session = Session::new(client, profile, env, SessionOptions::default(), None);

        let agent_id = manager.spawn(session, "Do something".into(), 0).unwrap();
        let _ = manager.wait(&agent_id).await.unwrap();

        let captured = provider_ref.captured_request.lock().unwrap();
        let request = captured
            .as_ref()
            .expect("request should have been captured");
        let system_message = request
            .messages
            .iter()
            .find(|message| message.role == Role::System)
            .expect("subagent request should include system message");

        assert!(
            !system_message.text().trim().is_empty(),
            "subagent system prompt should not be empty"
        );
    }

    #[tokio::test]
    async fn depth_limit_enforced() {
        let manager = SubAgentSupervisor::new(2);
        let session = make_session(vec![text_response("Hello")]).await;
        let result = manager.spawn(session, "Do something".into(), 2);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Maximum subagent depth")
        );
    }

    #[tokio::test]
    async fn close_sets_closed_status() {
        let manager = SubAgentSupervisor::new(3);
        let session = make_session(vec![text_response("Hello")]).await;
        let agent_id = manager.spawn(session, "Do something".into(), 0).unwrap();
        assert!(manager.contains(&agent_id));

        let result = manager.close_agent(&agent_id).await;
        assert!(result.is_ok());
        assert!(matches!(
            manager.status(&agent_id),
            Some(SubAgentStatus::Closed)
        ));
    }

    #[tokio::test]
    async fn send_input_nonexistent_agent_errors() {
        let manager = SubAgentSupervisor::new(3);
        let result = manager.send_input("nonexistent-id", "hello");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No agent found"));
    }

    #[tokio::test]
    async fn wait_nonexistent_agent_errors() {
        let manager = SubAgentSupervisor::new(3);
        let result = manager.wait("nonexistent-id").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No agent found"));
    }

    #[tokio::test]
    async fn wait_returns_result() {
        let manager = SubAgentSupervisor::new(3);
        let session = make_session(vec![text_response("Task completed successfully")]).await;
        let agent_id = manager.spawn(session, "Do something".into(), 0).unwrap();

        let result = manager.wait(&agent_id).await;
        assert!(result.is_ok());
        let agent_result = result.unwrap();
        assert_eq!(agent_result.output, "Task completed successfully");
        assert!(agent_result.success);
        assert!(agent_result.turns_used > 0);
        assert!(matches!(
            manager.status(&agent_id),
            Some(SubAgentStatus::Finished(Ok(_)))
        ));
    }

    #[tokio::test]
    async fn wait_tool_returns_when_context_is_cancelled() {
        let child_cancel = CancellationToken::new();
        let child_cancel_probe = child_cancel.clone();
        let task_cancel = child_cancel.clone();
        let task = tokio::spawn(async move {
            task_cancel.cancelled().await;
            Ok(SubAgentResult {
                output:     String::new(),
                success:    false,
                turns_used: 0,
            })
        });

        let agent_id = "blocked-agent".to_string();
        let manager = SubAgentSupervisor::new(3);
        manager.supervise_test_task(agent_id.clone(), task, child_cancel, None);

        let tool = make_wait_tool(manager.clone());
        let tool_cancel = CancellationToken::new();
        let ctx = ToolContext {
            env:                 Arc::new(MockSandbox::default()),
            cancel:              tool_cancel.clone(),
            tool_env_provider:   None,
            session_id:          None,
            root_session_id:     None,
            tool_call_id:        None,
            agent_event_emitter: None,
        };
        let mut wait = (tool.executor)(serde_json::json!({ "agent_id": agent_id }), ctx);

        assert!(
            futures::poll!(wait.as_mut()).is_pending(),
            "blocked subagent should leave the wait tool pending"
        );
        tool_cancel.cancel();

        let result = time::timeout(std::time::Duration::from_millis(100), wait.as_mut()).await;
        drop(wait);
        manager.shutdown_all().await;

        let result =
            result.expect("wait tool should return promptly when its context is cancelled");
        assert_eq!(result, Err("Cancelled".to_string()));
        assert!(child_cancel_probe.is_cancelled());
        assert!(matches!(
            manager.status(&agent_id),
            Some(SubAgentStatus::Closed)
        ));
    }

    #[test]
    fn tool_definitions_correct() {
        let manager = SubAgentSupervisor::new(3);
        let factory: SessionFactory = Arc::new(|| {
            panic!("should not be called");
        });

        let spawn_tool = make_spawn_agent_tool(manager.clone(), factory, 0);
        assert_eq!(spawn_tool.definition.name, "spawn_agent");
        let spawn_properties = spawn_tool.definition.parameters["properties"]
            .as_object()
            .unwrap();
        assert_eq!(spawn_properties.len(), 1);
        assert!(spawn_properties["task"].is_object());
        let spawn_required = spawn_tool.definition.parameters["required"]
            .as_array()
            .unwrap();
        assert!(spawn_required.contains(&serde_json::json!("task")));

        let send_tool = make_send_input_tool(manager.clone());
        assert_eq!(send_tool.definition.name, "send_input");
        assert!(send_tool.definition.parameters["properties"]["agent_id"].is_object());
        assert!(send_tool.definition.parameters["properties"]["message"].is_object());
        let send_required = send_tool.definition.parameters["required"]
            .as_array()
            .unwrap();
        assert!(send_required.contains(&serde_json::json!("agent_id")));
        assert!(send_required.contains(&serde_json::json!("message")));

        let wait_tool = make_wait_tool(manager.clone());
        assert_eq!(wait_tool.definition.name, "wait");
        assert!(wait_tool.definition.parameters["properties"]["agent_id"].is_object());
        let wait_required = wait_tool.definition.parameters["required"]
            .as_array()
            .unwrap();
        assert!(wait_required.contains(&serde_json::json!("agent_id")));

        let close_tool = make_close_agent_tool(manager);
        assert_eq!(close_tool.definition.name, "close_agent");
        assert!(close_tool.definition.parameters["properties"]["agent_id"].is_object());
        let close_required = close_tool.definition.parameters["required"]
            .as_array()
            .unwrap();
        assert!(close_required.contains(&serde_json::json!("agent_id")));
    }

    fn captured_events() -> (
        SubAgentEventCallback,
        Arc<Mutex<Vec<SubAgentCallbackEvent>>>,
    ) {
        let events: Arc<Mutex<Vec<SubAgentCallbackEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();
        let cb: SubAgentEventCallback = Arc::new(move |event| {
            events_clone.lock().unwrap().push(event);
        });
        (cb, events)
    }

    #[tokio::test]
    async fn callback_captures_spawn_event() {
        let (cb, events) = captured_events();
        let manager = SubAgentSupervisor::new(3);
        manager.set_event_callback(cb);

        let session = make_session(vec![text_response("Hello")]).await;
        let _agent_id = manager.spawn(session, "test task".into(), 0).unwrap();

        let captured = events.lock().unwrap();
        assert_eq!(captured.len(), 1);
        assert!(matches!(
            &captured[0],
            SubAgentCallbackEvent::Lifecycle(AgentEvent::SubAgentSpawned { depth: 1, task, .. })
                if task == "test task"
        ));
    }

    #[tokio::test]
    async fn callback_captures_wait_completed_event() {
        let (cb, events) = captured_events();
        let manager = SubAgentSupervisor::new(3);
        manager.set_event_callback(cb);

        let session = make_session(vec![text_response("done")]).await;
        let agent_id = manager.spawn(session, "task".into(), 0).unwrap();
        let _result = manager.wait(&agent_id).await.unwrap();

        let captured = events.lock().unwrap();
        assert!(captured.iter().any(|e| matches!(
            e,
            SubAgentCallbackEvent::Lifecycle(AgentEvent::SubAgentCompleted {
                success: true,
                depth: 1,
                ..
            })
        )));
    }

    #[tokio::test]
    async fn callback_captures_close_event() {
        let (cb, events) = captured_events();
        let manager = SubAgentSupervisor::new(3);
        manager.set_event_callback(cb);

        let session = make_session(vec![text_response("Hello")]).await;
        let agent_id = manager.spawn(session, "task".into(), 1).unwrap();
        manager.close_agent(&agent_id).await.unwrap();

        let captured = events.lock().unwrap();
        assert!(captured.iter().any(|e| matches!(
            e,
            SubAgentCallbackEvent::Lifecycle(AgentEvent::SubAgentClosed { depth: 2, .. })
        )));
    }

    #[tokio::test]
    async fn callback_forwards_child_events() {
        let (cb, events) = captured_events();
        let manager = SubAgentSupervisor::new(3);
        manager.set_event_callback(cb);

        let session = make_session(vec![text_response("Hello")]).await;
        let agent_id = manager.spawn(session, "task".into(), 0).unwrap();

        // Wait for agent to complete - child events arrive asynchronously
        let _result = manager.wait(&agent_id).await.unwrap();

        // Give the forwarding task a moment to process remaining events
        time::sleep(std::time::Duration::from_millis(50)).await;

        let captured = events.lock().unwrap();
        let forwarded_count = captured
            .iter()
            .filter(|e| matches!(e, SubAgentCallbackEvent::Forwarded(_)))
            .count();
        assert!(
            forwarded_count > 0,
            "expected at least one forwarded child event, got {forwarded_count}"
        );
    }

    #[tokio::test]
    async fn session_callback_stamps_parent_only_once() {
        let parent = make_session(vec![text_response("parent")]).await;
        let callback = parent.sub_agent_event_callback();
        let mut rx = parent.subscribe();

        callback(SubAgentCallbackEvent::Forwarded(SessionEvent {
            event:             AgentEvent::SessionStarted {
                provider: Some("anthropic".into()),
                model:    Some("claude-opus".into()),
            },
            timestamp:         std::time::SystemTime::now(),
            session_id:        "child".into(),
            parent_session_id: None,
            tool_call_id:      None,
        }));
        callback(SubAgentCallbackEvent::Forwarded(SessionEvent {
            event:             AgentEvent::SessionStarted {
                provider: Some("anthropic".into()),
                model:    Some("claude-opus".into()),
            },
            timestamp:         std::time::SystemTime::now(),
            session_id:        "grandchild".into(),
            parent_session_id: Some("child".into()),
            tool_call_id:      None,
        }));

        let child = rx.recv().await.unwrap();
        let grandchild = rx.recv().await.unwrap();
        assert_eq!(child.session_id, "child");
        assert_eq!(child.parent_session_id.as_deref(), Some(parent.id()));
        assert_eq!(grandchild.session_id, "grandchild");
        assert_eq!(grandchild.parent_session_id.as_deref(), Some("child"));
    }

    #[test]
    fn no_callback_does_not_panic() {
        // Manager without callback should not panic on emit
        let manager = SubAgentSupervisor::new(3);
        manager.emit_event(AgentEvent::SubAgentClosed {
            agent_id: "x".into(),
            depth:    0,
        });
    }

    #[tokio::test]
    async fn close_all_closes_all_agents() {
        let manager = SubAgentSupervisor::new(3);
        let session1 = make_session(vec![text_response("Hello")]).await;
        let session2 = make_session(vec![text_response("World")]).await;
        let id1 = manager.spawn(session1, "Task 1".into(), 0).unwrap();
        let id2 = manager.spawn(session2, "Task 2".into(), 0).unwrap();
        assert!(manager.contains(&id1));
        assert!(manager.contains(&id2));

        manager.shutdown_all().await;

        assert!(matches!(manager.status(&id1), Some(SubAgentStatus::Closed)));
        assert!(matches!(manager.status(&id2), Some(SubAgentStatus::Closed)));
    }

    #[tokio::test]
    async fn close_all_on_empty_manager_is_noop() {
        let manager = SubAgentSupervisor::new(3);
        manager.shutdown_all().await;
        assert!(manager.is_empty());
    }

    #[tokio::test]
    async fn wait_twice_returns_cached_result() {
        let manager = SubAgentSupervisor::new(3);
        let session = make_session(vec![text_response("cached output")]).await;
        let agent_id = manager.spawn(session, "Do something".into(), 0).unwrap();

        let result1 = manager.wait(&agent_id).await.unwrap();
        let result2 = manager.wait(&agent_id).await.unwrap();

        assert_eq!(result1.output, "cached output");
        assert_eq!(result2.output, "cached output");
        assert!(matches!(
            manager.status(&agent_id),
            Some(SubAgentStatus::Finished(Ok(_)))
        ));
    }

    #[tokio::test]
    async fn empty_final_response_is_not_reported_as_success() {
        let manager = SubAgentSupervisor::new(3);
        let session = make_session(vec![text_response("")]).await;
        let agent_id = manager.spawn(session, "Do something".into(), 0).unwrap();

        let result = manager.wait(&agent_id).await;

        assert!(
            matches!(result, Err(Error::InvalidState(message)) if message.contains(
                "without a non-empty final response"
            ))
        );
    }

    #[tokio::test]
    async fn send_input_to_completed_agent_errors() {
        let manager = SubAgentSupervisor::new(3);
        let session = make_session(vec![text_response("done")]).await;
        let agent_id = manager.spawn(session, "Do something".into(), 0).unwrap();
        let _ = manager.wait(&agent_id).await.unwrap();

        let result = manager.send_input(&agent_id, "hello");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("is not running"));
    }

    #[tokio::test]
    async fn send_input_to_closed_agent_errors() {
        let manager = SubAgentSupervisor::new(3);
        let session = make_session(vec![text_response("Hello")]).await;
        let agent_id = manager.spawn(session, "Do something".into(), 0).unwrap();
        manager.close_agent(&agent_id).await.unwrap();

        let result = manager.send_input(&agent_id, "hello");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("is not running"));
    }

    #[tokio::test]
    async fn close_already_closed_agent_errors() {
        let manager = SubAgentSupervisor::new(3);
        let session = make_session(vec![text_response("Hello")]).await;
        let agent_id = manager.spawn(session, "Do something".into(), 0).unwrap();
        manager.close_agent(&agent_id).await.unwrap();

        let result = manager.close_agent(&agent_id).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already closed"));
    }

    #[tokio::test]
    async fn close_completed_agent_preserves_finished_result() {
        let manager = SubAgentSupervisor::new(3);
        let session = make_session(vec![text_response("done")]).await;
        let agent_id = manager.spawn(session, "Do something".into(), 0).unwrap();
        let _ = manager.wait(&agent_id).await.unwrap();
        assert!(matches!(
            manager.status(&agent_id),
            Some(SubAgentStatus::Finished(Ok(_)))
        ));

        let result = manager.close_agent(&agent_id).await;
        assert!(result.is_err());
        assert!(matches!(
            manager.status(&agent_id),
            Some(SubAgentStatus::Finished(Ok(_)))
        ));
    }

    #[tokio::test]
    async fn status_is_running_after_spawn() {
        let manager = SubAgentSupervisor::new(3);
        let session = make_session(vec![text_response("Hello")]).await;
        let agent_id = manager.spawn(session, "Do something".into(), 0).unwrap();
        assert!(matches!(
            manager.status(&agent_id),
            Some(SubAgentStatus::Running)
        ));
    }

    #[tokio::test]
    async fn wait_on_closed_agent_errors() {
        let manager = SubAgentSupervisor::new(3);
        let session = make_session(vec![text_response("Hello")]).await;
        let agent_id = manager.spawn(session, "Do something".into(), 0).unwrap();
        manager.close_agent(&agent_id).await.unwrap();

        let result = manager.wait(&agent_id).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("has been closed"));
    }

    #[tokio::test]
    async fn natural_completion_updates_status_without_a_waiter() {
        let (callback, events) = captured_events();
        let supervisor = SubAgentSupervisor::new(3);
        supervisor.set_event_callback(callback);
        let session = make_session(vec![text_response("done")]).await;
        let agent_id = supervisor.spawn(session, "task".into(), 0).unwrap();

        time::timeout(Duration::from_secs(1), async {
            while !matches!(
                supervisor.status(&agent_id),
                Some(SubAgentStatus::Finished(Ok(_)))
            ) {
                yield_now().await;
            }
        })
        .await
        .expect("child completion should update status without a waiter");

        let completion_count = events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    SubAgentCallbackEvent::Lifecycle(AgentEvent::SubAgentCompleted { .. })
                )
            })
            .count();
        assert_eq!(completion_count, 1);
    }

    #[tokio::test]
    async fn concurrent_waiters_share_one_result_and_completion_event() {
        let (callback, events) = captured_events();
        let supervisor = SubAgentSupervisor::new(3);
        supervisor.set_event_callback(callback);
        let session = make_session(vec![text_response("shared")]).await;
        let agent_id = supervisor.spawn(session, "task".into(), 0).unwrap();
        let first_cancel = CancellationToken::new();
        let second_cancel = CancellationToken::new();

        let (first, second) = tokio::join!(
            supervisor.wait_with_cancel(&agent_id, &first_cancel),
            supervisor.wait_with_cancel(&agent_id, &second_cancel),
        );

        assert_eq!(first.unwrap().output, "shared");
        assert_eq!(second.unwrap().output, "shared");
        let completion_count = events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    SubAgentCallbackEvent::Lifecycle(AgentEvent::SubAgentCompleted { .. })
                )
            })
            .count();
        assert_eq!(completion_count, 1);
    }

    #[tokio::test]
    async fn uncooperative_child_and_forwarder_are_aborted_after_grace() {
        struct DropProbe(Arc<std::sync::atomic::AtomicBool>);

        impl Drop for DropProbe {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }

        time::pause();
        let supervisor = SubAgentSupervisor::new(3);
        let child_cancel = CancellationToken::new();
        let child = tokio::spawn(async {
            future::pending::<()>().await;
            Ok(SubAgentResult {
                output:     String::new(),
                success:    false,
                turns_used: 0,
            })
        });
        let forwarder_dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let forwarder_probe = Arc::clone(&forwarder_dropped);
        let forwarder = tokio::spawn(async move {
            let _probe = DropProbe(forwarder_probe);
            future::pending::<()>().await;
        });
        let agent_id = "uncooperative".to_string();
        supervisor.supervise_test_task(
            agent_id.clone(),
            child,
            child_cancel.clone(),
            Some(forwarder),
        );

        let closer = {
            let supervisor = supervisor.clone();
            let agent_id = agent_id.clone();
            tokio::spawn(async move { supervisor.close_agent(&agent_id).await })
        };
        yield_now().await;

        assert!(child_cancel.is_cancelled());
        assert!(matches!(
            supervisor.status(&agent_id),
            Some(SubAgentStatus::Closing)
        ));
        assert!(!closer.is_finished());

        time::advance(SUBAGENT_SHUTDOWN_GRACE).await;
        yield_now().await;
        closer.await.unwrap().unwrap();

        assert!(matches!(
            supervisor.status(&agent_id),
            Some(SubAgentStatus::Closed)
        ));
        assert!(forwarder_dropped.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn concurrent_shutdown_calls_wait_for_the_same_cleanup() {
        let supervisor = SubAgentSupervisor::new(3);
        let child_cancel = CancellationToken::new();
        let task_cancel = child_cancel.clone();
        let child = tokio::spawn(async move {
            task_cancel.cancelled().await;
            Ok(SubAgentResult {
                output:     String::new(),
                success:    false,
                turns_used: 0,
            })
        });
        let agent_id = "concurrent-close".to_string();
        supervisor.supervise_test_task(agent_id.clone(), child, child_cancel, None);

        let (first, second) = tokio::join!(supervisor.shutdown_all(), supervisor.shutdown_all());
        assert_eq!(first, ());
        assert_eq!(second, ());
        assert!(matches!(
            supervisor.status(&agent_id),
            Some(SubAgentStatus::Closed)
        ));
    }

    #[tokio::test]
    async fn lifecycle_callback_can_reenter_supervisor() {
        let supervisor = SubAgentSupervisor::new(3);
        let reentrant_supervisor = supervisor.clone();
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_for_callback = Arc::clone(&observed);
        supervisor.set_event_callback(Arc::new(move |event| {
            let SubAgentCallbackEvent::Lifecycle(
                AgentEvent::SubAgentSpawned { agent_id, .. }
                | AgentEvent::SubAgentCompleted { agent_id, .. },
            ) = event
            else {
                return;
            };
            observed_for_callback
                .lock()
                .unwrap()
                .push(reentrant_supervisor.status(&agent_id).is_some());
        }));

        let session = make_session(vec![text_response("done")]).await;
        let agent_id = supervisor.spawn(session, "task".into(), 0).unwrap();
        supervisor.wait(&agent_id).await.unwrap();

        assert_eq!(*observed.lock().unwrap(), vec![true, true]);
    }
}
