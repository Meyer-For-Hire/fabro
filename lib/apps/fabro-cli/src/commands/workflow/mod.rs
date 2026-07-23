mod create;
mod list;

use anyhow::Result;

use crate::args::{WorkflowCommand, WorkflowNamespace};
use crate::command_context::CommandContext;

pub(crate) fn dispatch(ns: WorkflowNamespace, base_ctx: &CommandContext) -> Result<()> {
    match ns.command {
        WorkflowCommand::List(args) => list::list_command(&args, base_ctx),
        WorkflowCommand::Create(args) => create::create_command(&args, base_ctx),
    }
}
