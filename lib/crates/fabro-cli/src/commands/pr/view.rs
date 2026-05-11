use anyhow::Result;
use tracing::info;

use crate::args::PrViewArgs;
use crate::command_context::CommandContext;
use crate::shared::print_json_pretty;

pub(super) async fn view_command(args: PrViewArgs, base_ctx: &CommandContext) -> Result<()> {
    let (ctx, client, run_id) =
        super::resolve_run_for_pr(base_ctx, &args.server, &args.run_id).await?;
    let detail = client.get_run_pull_request(&run_id).await?;
    let pull_request = &detail.pull_request;

    info!(
        number = pull_request.number,
        owner = %pull_request.owner,
        repo = %pull_request.repo,
        "Viewing pull request"
    );

    if ctx.json_output() {
        print_json_pretty(&detail)?;
        return Ok(());
    }

    let printer = ctx.printer();
    fabro_util::printout!(printer, "#{} {}", pull_request.number, pull_request.title);
    let state_display = if detail.merged {
        "merged"
    } else if detail.draft {
        "draft"
    } else {
        &detail.state
    };
    fabro_util::printout!(printer, "State:   {state_display}");
    fabro_util::printout!(printer, "URL:     {}", pull_request.html_url);
    fabro_util::printout!(
        printer,
        "Branch:  {} -> {}",
        pull_request.head_branch,
        pull_request.base_branch
    );
    fabro_util::printout!(printer, "Author:  {}", detail.author.login);
    fabro_util::printout!(
        printer,
        "Changes: +{} -{} ({} files)",
        detail.additions,
        detail.deletions,
        detail.changed_files
    );

    Ok(())
}
