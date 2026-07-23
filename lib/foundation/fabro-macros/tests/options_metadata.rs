#![expect(dead_code, reason = "derive test fixtures are inspected via metadata")]

use clap::ValueEnum;
use fabro_options_metadata::{OptionEntry, OptionsMetadata as _};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ExecutionMode {
    Fast,
    Careful,
}

/// Root command options.
#[derive(fabro_macros::OptionsMetadata)]
struct RootArgs {
    /// Enable verbose output.
    #[arg(long)]
    #[option(added_in = "0.213.0")]
    verbose_output: bool,

    /// Select execution mode.
    #[arg(long, value_enum)]
    #[option]
    mode: Option<ExecutionMode>,

    #[option_group]
    nested: Option<NestedArgs>,

    #[option]
    undocumented: bool,
}

#[derive(fabro_macros::OptionsMetadata)]
struct NestedArgs {
    /// Preview the work.
    #[arg(long)]
    #[option]
    dry_run: bool,
}

#[test]
fn derive_records_fields_docs_and_value_enum_variants() {
    let metadata = RootArgs::metadata();

    assert_eq!(RootArgs::documentation(), Some("Root command options."));
    assert!(metadata.has("verbose-output"));
    assert!(metadata.has("nested.dry-run"));

    let Some(OptionEntry::Field(verbose)) = metadata.find("verbose-output") else {
        panic!("verbose-output should be a field");
    };
    assert_eq!(verbose.doc, Some("Enable verbose output."));
    assert_eq!(verbose.added_in, Some("0.213.0"));

    let Some(OptionEntry::Field(mode)) = metadata.find("mode") else {
        panic!("mode should be a field");
    };
    let values = mode
        .possible_values
        .expect("value_enum field should record possible values");
    assert_eq!(
        values
            .iter()
            .map(|value| value.name.as_str())
            .collect::<Vec<_>>(),
        ["fast", "careful"]
    );
}

#[test]
fn derive_allows_missing_field_doc() {
    let Some(OptionEntry::Field(field)) = RootArgs::metadata().find("undocumented") else {
        panic!("undocumented should be a field");
    };

    assert_eq!(field.doc, None);
}
