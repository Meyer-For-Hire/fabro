use std::collections::HashSet;

use fabro_graphviz::graph::Graph;
use fabro_model::{Catalog, ModelSelectionError, ProviderId};
use fabro_types::WorkflowSettings;
use fabro_types::settings::InterpString;
use fabro_types::settings::run::RunGoal;

use crate::error::Error;

pub fn materialize_run(
    mut settings: WorkflowSettings,
    graph: &Graph,
    catalog: &Catalog,
    configured_providers: &[ProviderId],
) -> Result<WorkflowSettings, Error> {
    let configured_model = settings.run.model.name.take();
    let configured_provider = settings.run.model.provider.take();
    let graph_provider = graph
        .attrs
        .get("default_provider")
        .and_then(|value| value.as_str())
        .map(str::to_string);
    let graph_model = graph
        .attrs
        .get("default_model")
        .and_then(|value| value.as_str())
        .map(str::to_string);

    let provider = configured_provider.or(graph_provider);
    let model = configured_model.or(graph_model);
    let eligible = configured_providers.iter().cloned().collect::<HashSet<_>>();
    let (resolved_model, resolved_provider) =
        resolve_run_model(catalog, &eligible, model.as_deref(), provider.as_deref())?;

    settings.run.model.name = Some(resolved_model);
    settings.run.model.provider = Some(resolved_provider.into_inner());

    let goal = graph.goal().to_string();
    settings.run.goal = if goal.is_empty() {
        None
    } else {
        Some(RunGoal::Inline(InterpString::parse(&goal)))
    };

    if settings
        .run
        .pull_request
        .as_ref()
        .is_some_and(|pull_request| !pull_request.enabled)
    {
        settings.run.pull_request = None;
    }

    Ok(settings)
}

pub(crate) fn resolve_run_model(
    catalog: &Catalog,
    eligible: &HashSet<ProviderId>,
    model: Option<&str>,
    provider: Option<&str>,
) -> Result<(String, ProviderId), ModelSelectionError> {
    let provider = provider
        .filter(|provider| !provider.is_empty())
        .map(ProviderId::new);
    let selected = catalog.resolve_selection(model, provider.as_ref(), eligible)?;
    Ok((selected.model, selected.provider))
}
