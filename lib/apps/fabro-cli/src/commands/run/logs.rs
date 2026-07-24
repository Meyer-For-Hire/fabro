#![expect(
    clippy::disallowed_types,
    reason = "sync CLI `run logs` command: blocking std::io::Write is the intended output mechanism"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "sync CLI `run logs` command: writes raw log bytes to std::io::stdout directly"
)]

use std::io::{self, Write};

use anyhow::{Context as _, Result};
use tracing::info;

use crate::args::LogsArgs;
use crate::command_context::CommandContext;

pub(crate) async fn run(args: &LogsArgs, base_ctx: &CommandContext) -> Result<()> {
    base_ctx.require_no_json_override()?;

    let ctx = base_ctx.with_target(&args.server)?;
    let client = ctx.server().await?;
    let run_id = client.resolve_run(&args.run).await?.id;
    info!(run_id = %run_id, "Showing raw run log");

    let bytes = client
        .get_run_logs(&run_id)
        .await
        .context("Failed to fetch run log from server")?
        .ok_or_else(|| anyhow::anyhow!("Run log not available"))?;

    let stdout = io::stdout();
    let mut out = stdout.lock();

    match args.tail {
        None => out.write_all(&bytes)?,
        Some(0) => {}
        Some(tail) => write_tail(&bytes, tail, &mut out)?,
    }

    Ok(())
}

fn write_tail(bytes: &[u8], tail: usize, out: &mut dyn Write) -> Result<()> {
    let text = std::str::from_utf8(bytes)
        .context("Run log is not valid UTF-8; omit --tail to print raw bytes")?;
    let lines = text.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(tail);
    for line in &lines[start..] {
        writeln!(out, "{line}")?;
    }
    Ok(())
}
