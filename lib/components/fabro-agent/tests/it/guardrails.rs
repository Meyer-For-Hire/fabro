use std::sync::Arc;

use fabro_agent::{AgentProfile, AgentProfileBuilder};
use fabro_model::Catalog;

#[test]
fn profile_context_window_matches_catalog_for_default_models() {
    let catalog = Arc::new(Catalog::from_builtin().unwrap());
    for provider in catalog.providers() {
        let catalog_info = catalog
            .default_for_provider(&provider.id)
            .cloned()
            .unwrap_or_else(|| panic!("no default model for {:?} in catalog", provider.id));
        let model = &catalog_info.id;
        let context_window = usize::try_from(catalog_info.context_window())
            .expect("catalog context window should be non-negative and fit in usize");

        let profile: Box<dyn AgentProfile> = AgentProfileBuilder::new(
            provider.agent_profile,
            provider.id.clone(),
            model.as_str(),
            Arc::clone(&catalog),
        )
        .build();

        assert_eq!(
            profile.context_window_size(),
            context_window,
            "context_window_size mismatch for {:?} model '{}': profile={} catalog={}",
            provider.id,
            model,
            profile.context_window_size(),
            context_window
        );
    }
}
