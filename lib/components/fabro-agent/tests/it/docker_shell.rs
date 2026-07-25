//! Proves the agent shell tool reports real process outcomes through the
//! Docker provider's streaming path, which uses a `bash -lc` supervisor and
//! separate stdout/stderr channels.

use std::sync::{Arc, Mutex};

use fabro_agent::sandbox::Sandbox;
use fabro_agent::tool_registry::{AgentEventEmitter, ToolContext};
use fabro_agent::tools::make_shell_tool;
use fabro_agent::types::AgentEvent;
use fabro_agent::{DockerSandbox, DockerSandboxOptions};
use fabro_types::CommandTermination;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct RecordingAgentEmitter {
    events: Mutex<Vec<AgentEvent>>,
}

impl AgentEventEmitter for RecordingAgentEmitter {
    fn emit(&self, event: AgentEvent) {
        self.events
            .lock()
            .expect("events lock poisoned")
            .push(event);
    }
}

#[tokio::test]
#[ignore = "requires real Docker container lifecycle; run explicitly when changing shell tool exec integration"]
async fn shell_reports_real_docker_process_outcome() {
    let Ok(sandbox) = DockerSandbox::new(
        DockerSandboxOptions {
            image: "buildpack-deps:noble".to_string(),
            auto_pull: false,
            skip_clone: true,
            ..DockerSandboxOptions::default()
        },
        None,
        None,
        None,
        None,
    ) else {
        return;
    };
    // No Docker daemon or no local image: nothing to prove here.
    if sandbox.initialize().await.is_err() {
        return;
    }

    let sandbox = Arc::new(sandbox);
    let emitter = Arc::new(RecordingAgentEmitter::default());
    let tool = make_shell_tool();
    let result = (tool.executor)(
        serde_json::json!({"command": "printf 'out'; printf 'err' >&2; exit 7"}),
        ToolContext {
            env:                 sandbox.clone() as Arc<dyn Sandbox>,
            cancel:              CancellationToken::new(),
            tool_env_provider:   None,
            session_id:          None,
            root_session_id:     None,
            tool_call_id:        None,
            agent_event_emitter: Some(emitter.clone()),
        },
    )
    .await;
    sandbox
        .cleanup()
        .await
        .expect("docker cleanup should succeed");

    let output = result.expect_err("exit 7 is a failed tool result");
    assert!(output.contains("Termination: exited"), "got: {output}");
    assert!(output.contains("Exit code: 7"), "got: {output}");
    assert!(output.contains("stdout:\nout"), "got: {output}");
    assert!(output.contains("stderr:\nerr"), "got: {output}");

    let events = emitter.events.lock().expect("events lock poisoned").clone();
    assert_eq!(events.len(), 1, "expected one agent event, got {events:?}");
    match &events[0] {
        AgentEvent::ToolProcessCompleted {
            exit_code,
            termination,
            streams_separated,
            exec_output_tail,
            ..
        } => {
            assert_eq!(*exit_code, Some(7));
            assert_eq!(*termination, CommandTermination::Exited);
            assert!(streams_separated);
            let tail = exec_output_tail.as_ref().expect("output tail");
            assert_eq!(tail.stdout.as_deref(), Some("out"));
            assert_eq!(tail.stderr.as_deref(), Some("err"));
        }
        other => panic!("expected a process event, got {other:?}"),
    }
}
