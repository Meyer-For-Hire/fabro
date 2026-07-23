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
    settings.run.model.provider = resolved_provider;

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
) -> Result<(String, Option<String>), ModelSelectionError> {
    if let Some(provider) = provider.filter(|provider| !provider.is_empty()) {
        let requested = ProviderId::new(provider);
        let provider =
            catalog
                .provider(&requested)
                .ok_or_else(|| ModelSelectionError::UnknownProvider {
                    provider: requested.clone(),
                })?;
        let canonical_provider = provider.id.clone();
        let canonical_eligible = eligible.iter().any(|eligible| {
            catalog
                .provider(eligible)
                .is_some_and(|provider| provider.id == canonical_provider)
        });
        if !canonical_eligible {
            return Err(ModelSelectionError::ProviderUnavailable {
                provider: canonical_provider,
            });
        }
        if let Some(model) = model {
            return match catalog.resolve_on_provider(&provider.id, model) {
                Ok(offering) => Ok((offering.id.to_string(), Some(offering.provider.to_string()))),
                Err(ModelSelectionError::UnknownSelectorOnProvider { .. }) => {
                    Ok((model.to_string(), Some(provider.id.to_string())))
                }
                Err(error) => Err(error),
            };
        }
        let offering = catalog.default_for_provider(&provider.id).ok_or_else(|| {
            ModelSelectionError::UnknownSelectorOnProvider {
                selector: "<default model>".to_string(),
                provider: provider.id.clone(),
            }
        })?;
        return Ok((offering.id.to_string(), Some(offering.provider.to_string())));
    }

    if let Some(model) = model {
        return match catalog.select(model, None, eligible) {
            Ok(offering) => Ok((offering.id.to_string(), Some(offering.provider.to_string()))),
            Err(ModelSelectionError::UnknownSelector { .. }) => {
                let default = catalog.select_default(eligible)?;
                Ok((model.to_string(), Some(default.provider.to_string())))
            }
            Err(error) => Err(error),
        };
    }

    let default = catalog.select_default(eligible)?;
    Ok((default.id.to_string(), Some(default.provider.to_string())))
}
