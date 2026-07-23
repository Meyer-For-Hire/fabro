use anyhow::Result;

use crate::args::SecretRmArgs;
use crate::command_context::CommandContext;
use crate::shared::print_json_pretty;

pub(super) async fn rm_command(args: &SecretRmArgs, ctx: &CommandContext) -> Result<()> {
    let client = ctx.server().await?;
    client.delete_secret_by_name(&args.key).await?;
    if ctx.json_output() {
        print_json_pretty(&serde_json::json!({ "key": args.key }))?;
    } else {
        fabro_util::printerr!(ctx.printer(), "Removed {}", args.key);
    }
    Ok(())
}
