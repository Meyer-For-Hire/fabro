use std::collections::HashSet;

use fabro_graphviz::graph::Graph;
use fabro_model::{Catalog, ModelSelectionError, ProviderId};
use fabro_types::WorkflowSettings;
use fabro_types::settings::InterpString;
use fabro_types::settings::run::RunGoal;

use crate::error::Error;

pub fn materialize_run(
    settings: WorkflowSettings,
    graph: &Graph,
    catalog: &Catalog,
    configured_providers: &[ProviderId],
) -> Result<WorkflowSettings, Error> {
    materialize_run_with_provider_sets(settings, graph, catalog, configured_providers, None)
}

pub fn materialize_run_with_provider_fallback(
    settings: WorkflowSettings,
    graph: &Graph,
    catalog: &Catalog,
    preferred_providers: &[ProviderId],
    fallback_providers: &[ProviderId],
) -> Result<WorkflowSettings, Error> {
    materialize_run_with_provider_sets(
        settings,
        graph,
        catalog,
        preferred_providers,
        Some(fallback_providers),
    )
}

fn materialize_run_with_provider_sets(
    mut settings: WorkflowSettings,
    graph: &Graph,
    catalog: &Catalog,
    configured_providers: &[ProviderId],
    fallback_providers: Option<&[ProviderId]>,
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
    let fallback =
        fallback_providers.map(|providers| providers.iter().cloned().collect::<HashSet<_>>());
    let provider = provider
        .as_deref()
        .filter(|provider| !provider.is_empty())
        .map(ProviderId::new);
    let selected = match fallback {
        Some(fallback) => catalog.resolve_selection_with_fallback(
            model.as_deref(),
            provider.as_ref(),
            &eligible,
            &fallback,
        ),
        None => catalog.resolve_selection(model.as_deref(), provider.as_ref(), &eligible),
    }?;

    settings.run.model.name = Some(selected.model);
    settings.run.model.provider = Some(selected.provider.into_inner());

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
