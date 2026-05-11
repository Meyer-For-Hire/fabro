# Run Terminal Plan

## Summary

Build an embedded run terminal for Daytona and Docker sandboxes.

The run page gets a `Terminal` tab at `/runs/:id/terminal`. The browser uses `xterm.js`, connects to Fabro over WebSocket, and Fabro bridges bytes to a provider-specific terminal session for that run's sandbox. Daytona uses Daytona PTY. Docker uses an attached Docker exec with TTY. If the sandbox exists but is stopped, opening the terminal starts/resumes it before attaching the terminal. Keep the existing SSH access endpoint as a required copyable external-access command for providers that support SSH; do not build an SSH-over-WebSocket fallback.

## Interfaces

- Add WebSocket endpoint: `GET /api/v1/runs/{id}/terminal?cols=120&rows=32`.
- `cols` and `rows` come from the fitted xterm viewport before the WebSocket opens. Validate them server-side and clamp or reject unreasonable values.
- WebSocket auth uses existing Fabro web/session auth. Since all authenticated users can access all runs, the handler does not need per-run ACL checks.
- Reject invalid browser `Origin`/`Host` combinations before upgrade.
- Validate that the requested run exists, has a sandbox record owned by that run, and that the sandbox is not deleted before opening a terminal.
- Browser protocol:
  - Client binary message: raw PTY stdin bytes.
  - Client text message: `{"type":"resize","cols":120,"rows":32}` or `{"type":"close"}`.
  - Server binary message: raw PTY output bytes.
  - Server text message: `{"type":"starting"}`, `{"type":"ready"}`, `{"type":"error","message":"..."}`, `{"type":"closed"}`.
- Bound the protocol:
  - Reject oversized text control messages.
  - Use bounded channels between the WebSocket and provider session.
  - Apply ping/pong or equivalent keepalive timeout so abandoned connections close.
  - Treat terminal output as transient stream data and never persist or log it.
- No OpenAPI/generated API client change for the WebSocket.
- External access:
  - Existing `POST /api/v1/runs/{id}/ssh` remains required for "Copy SSH command".
  - The terminal page shows "Copy SSH command" when the run's sandbox/provider supports SSH access.
  - SSH is only an external-access command; it is not used as the embedded terminal transport.
- Add a small terminal-specific capability in `fabro-sandbox`, not full terminal support on the existing `Sandbox` trait:
  - `TerminalSize { cols: u16, rows: u16 }`
  - `TerminalOptions { size: TerminalSize, cwd: Utf8PathBuf, env: BTreeMap<String, String> }`
  - provider entry point: `open_terminal(options) -> TerminalSession`
  - `TerminalSession` exposes an input sink, an output stream, `resize`, and `close`
  - concrete `DaytonaTerminalSession` and `DockerTerminalSession` implementations

## Key Changes

- Backend:
  - Enable Axum WebSocket support in `fabro-server`.
  - Add a focused terminal handler that:
    - loads the run and sandbox record
    - validates that the sandbox belongs to the run and is not deleted
    - reconnects the concrete provider
    - checks provider sandbox state
    - starts/resumes the sandbox if it is stopped
    - waits for the provider to report ready/running, with a timeout
    - opens a terminal session with the initial xterm size
    - bridges browser input/output
    - handles resize
    - closes/kills only the terminal session when the browser disconnects
  - Add a Daytona PTY helper in `fabro-sandbox` that uses Daytona Toolbox APIs directly for create/connect/resize/kill because the pinned Rust SDK exposes PTY management but not a complete streaming handle.
  - Daytona terminal connection must start/resume a stopped Daytona sandbox before creating the PTY.
  - Add a Docker terminal helper in `fabro-sandbox` that creates an attached Docker exec with `tty=true`, `attach_stdin=true`, `attach_stdout=true`, `attach_stderr=true`, starts it attached, writes browser input into Bollard's exec input writer, forwards exec output to the browser, and calls `resize_exec` on resize.
  - Docker terminal connection must start a stopped run container before creating the exec session. If the container is paused, deleted, or cannot be started, return a clean terminal error.
  - Docker disconnect cleanup should close the exec input/output and run the shell through a lightweight wrapper that records its PID so Fabro can attempt to terminate the shell process if the WebSocket drops. Treat Docker exec cleanup as best-effort and prefer process-group cleanup when available.
  - Use the run sandbox working directory as the PTY `cwd`; set `TERM=xterm-256color` and `LANG=C.UTF-8`.
  - Keep provider credentials server-side only. Never send Daytona API keys, Daytona PTY URLs, Docker socket details, or provider connection handles to the browser.
  - Terminal disconnect must not stop or delete the sandbox. Sandbox deletion stays tied to workflow run deletion.
- Frontend:
  - Add `@xterm/xterm` and `@xterm/addon-fit`.
  - Add `run-terminal.tsx`, mounted as `/runs/:id/terminal`, with full-height terminal layout.
  - Add a `Terminal` tab when the run has a sandbox id.
  - WebSocket URL uses `ws://` for `http://127.0.0.1` and `wss://` for HTTPS.
  - Compute the initial fitted terminal size before opening the WebSocket and pass it as `cols`/`rows`.
  - Send xterm `onData` input as binary WebSocket messages using `TextEncoder`.
  - Debounce xterm resize events and send resize control messages.
  - Add header actions: reconnect terminal, copy existing SSH command when the provider supports SSH, and connection status.
  - Render connection states for starting, connecting, ready, closed, unsupported, and error.
- Behavior:
  - Daytona and Docker are supported in v1. Local sandboxes show an unsupported-provider error.
  - Stopped Daytona and Docker sandboxes are started/resumed when the terminal connects.
  - PTY sessions are not persistent in v1. Closing or refreshing the tab starts a fresh shell.
  - Closing the terminal cleans up the PTY/exec session but does not stop or delete the sandbox.
  - Terminal input/output is not logged by Fabro.

## Test Plan

- Rust unit tests:
  - WebSocket message parser accepts valid resize and rejects malformed/oversized control messages.
  - Origin validation allows same-origin localhost and rejects cross-origin browser origins.
  - Authenticated WebSocket requests for missing runs, runs without sandboxes, deleted sandboxes, and mismatched run/sandbox records fail cleanly.
  - Protocol bridge uses bounded queues and closes cleanly on keepalive timeout.
  - Daytona PTY helper builds the expected Toolbox REST/WebSocket URLs and auth headers.
  - Docker terminal helper creates exec options with TTY, stdin/stdout/stderr attached, workspace cwd, and terminal env.
- Server tests:
  - Unauthenticated terminal WebSocket upgrade is rejected.
  - Local or missing sandbox returns a clean unsupported/unavailable failure.
  - Stopped Daytona sandbox is resumed before PTY creation.
  - Stopped Docker container is started before exec creation.
  - Startup timeout or provider start failure sends a clean terminal error and closes the WebSocket.
  - Daytona runs use the Daytona terminal adapter; Docker runs use the Docker terminal adapter.
  - On browser disconnect, the handler closes/kills the provider terminal session without stopping or deleting the sandbox.
  - Resize messages call the provider resize operation with the latest cols/rows.
- Web tests:
  - Terminal tab appears for sandbox-backed runs.
  - Route opens `ws://127.0.0.1:port/...` on local HTTP and `wss://` on HTTPS, including the fitted `cols`/`rows` query parameters.
  - Binary PTY output is written to xterm; keyboard input sends binary WebSocket messages.
  - Resize events are debounced and sent as resize control messages.
  - "Copy SSH command" appears when the provider supports SSH and is absent/disabled otherwise.
  - Unsupported/error/closed states render without crashing.
- Manual acceptance:
  - Open Daytona-backed and Docker-backed runs, use `ls`, `pwd`, `vim`/`less`, Ctrl-C, resize the browser, refresh the tab, and confirm the old shell session is cleaned up.
  - Stop a Daytona sandbox and a Docker container, open the terminal, and confirm Fabro starts/resumes the sandbox before connecting.
  - Confirm closing or refreshing the terminal does not stop or delete the sandbox.
  - Confirm "Copy SSH command" appears when SSH access is supported and is absent/disabled otherwise.
  - Run `cargo nextest run -p fabro-server`, relevant `fabro-sandbox` tests, and `cd apps/fabro-web && bun test && bun run typecheck`.

## Assumptions

- Daytona PTY and Docker attached exec are the embedded-terminal transports for v1.
- `/api/v1/runs/{id}/ssh` stays as external access for local terminals and IDEs.
- WebSocket over plain `ws://127.0.0.1` is acceptable for local Fabro; hosted HTTPS deployments require `wss://`.
- All authenticated users can access all runs, so terminal access relies on existing session authentication plus run/sandbox existence and ownership validation, not per-run ACLs.
- Run sandboxes are created and deleted with their workflow run, but may be stopped independently. Terminal connect starts/resumes a stopped sandbox; terminal disconnect does not stop or delete it.
- Sources: Daytona PTY docs, Daytona SSH docs, Docker exec/Bollard APIs, and xterm.js docs.
