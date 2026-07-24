mod list;
mod rm;
mod set;

use anyhow::Result;

use crate::args::{SecretCommand, SecretNamespace};
use crate::command_context::CommandContext;

pub(crate) async fn dispatch(ns: SecretNamespace, base_ctx: &CommandContext) -> Result<()> {
    let ctx = base_ctx.with_target(&ns.target)?;
    match ns.command {
        SecretCommand::List(args) => list::list_command(&args, &ctx).await,
        SecretCommand::Rm(args) => rm::rm_command(&args, &ctx).await,
        SecretCommand::Set(args) => set::set_command(&args, &ctx).await,
    }
}
