use std::path::Path;

fn main() {
    let metadata = fabro_build_support::collect_from(Path::new("."));

    for path in metadata.rerun_paths {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    println!("cargo:rustc-env=FABRO_GIT_SHA={}", metadata.short_sha);

    let build_date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    println!("cargo:rustc-env=FABRO_BUILD_DATE={build_date}");

    let profile = fabro_build_support::cargo_profile();
    let profile_suffix = if profile == "release" {
        String::new()
    } else {
        format!(" {profile}")
    };
    println!("cargo:rustc-env=FABRO_BUILD_PROFILE={profile}");
    println!("cargo:rustc-env=FABRO_BUILD_PROFILE_SUFFIX={profile_suffix}");
}
