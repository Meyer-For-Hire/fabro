mod df;
mod events;
mod info;
mod prune;
mod repair;

use anyhow::Result;

use crate::args::{SystemCommand, SystemNamespace};
use crate::command_context::CommandContext;

pub(crate) async fn dispatch(ns: SystemNamespace, base_ctx: &CommandContext) -> Result<()> {
    match ns.command {
        SystemCommand::Info(args) => info::info_command(&args, base_ctx).await,
        SystemCommand::Prune(args) => prune::prune_command(&args, base_ctx).await,
        SystemCommand::Df(args) => df::df_command(&args, base_ctx).await,
        SystemCommand::Events(args) => events::events_command(&args, base_ctx).await,
        SystemCommand::Repair(args) => repair::repair_command(&args, base_ctx).await,
    }
}
