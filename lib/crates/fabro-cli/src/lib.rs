#![expect(
    dead_code,
    reason = "the library exports manifest builder helpers while the binary owns most CLI dispatch"
)]

mod args;
mod manifest_builder;

pub use manifest_builder::{BuiltManifest, ManifestBuildInput, build_run_manifest};
