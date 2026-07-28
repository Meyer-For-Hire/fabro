pub mod app;
pub mod branches;
pub mod git;
pub mod graphql;
pub mod installations;
pub mod manifests;
pub mod oauth;
pub mod pulls;
pub mod releases;
pub mod users;

use axum::Router;
use axum::routing::{get, patch, post, put};

use crate::server::SharedState;

pub fn build_router(state: SharedState) -> Router {
    Router::new()
        // App endpoints
        .route("/app", get(app::get_app))
        .route("/apps/{slug}", get(app::get_app_by_slug))
        .route("/app/hook/config", patch(app::patch_webhook_config))
        // Installation endpoints
        .route(
            "/repos/{owner}/{repo}/installation",
            get(installations::get_installation),
        )
        .route(
            "/app/installations/{id}/access_tokens",
            post(installations::create_access_token),
        )
        // Branch endpoints
        // Wildcard: branch names contain slashes (`fabro/run/<id>`), and
        // GitHub routes the branch as the remainder of the path.
        .route(
            "/repos/{owner}/{repo}/branches/{*branch}",
            get(branches::get_branch),
        )
        // Pull request endpoints
        .route(
            "/repos/{owner}/{repo}/pulls",
            post(pulls::create_pull_request),
        )
        .route(
            "/repos/{owner}/{repo}/pulls/{number}",
            get(pulls::get_pull_request),
        )
        .route(
            "/repos/{owner}/{repo}/pulls/{number}",
            patch(pulls::update_pull_request),
        )
        .route(
            "/repos/{owner}/{repo}/pulls/{number}/merge",
            put(pulls::merge_pull_request),
        )
        // Manifest conversion
        .route(
            "/app-manifests/{code}/conversions",
            post(manifests::convert_manifest),
        )
        // OAuth endpoints
        .route("/login/oauth/authorize", get(oauth::authorize))
        .route("/login/oauth/access_token", post(oauth::access_token))
        .route("/user", get(users::get_user))
        .route("/user/emails", get(users::get_emails))
        // Releases
        .route(
            "/repos/{owner}/{repo}/releases/latest",
            get(releases::get_latest_release),
        )
        // GraphQL
        .route("/graphql", post(graphql::handle_graphql))
        // Git smart HTTP transport routes
        .route("/{owner}/{repo}/info/refs", get(git::git_info_refs))
        .route(
            "/{owner}/{repo}/git-upload-pack",
            post(git::git_upload_pack),
        )
        .route(
            "/{owner}/{repo}/git-receive-pack",
            post(git::git_receive_pack),
        )
        .with_state(state)
}
