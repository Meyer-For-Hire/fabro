use std::sync::Arc;

use fabro_auth::OAuthCredential;
use fabro_static::EnvVars;

use super::super::{
    ApiError, AppState, CreateSecretRequest, DeleteSecretRequest, IntoResponse, Json, RequiredUser,
    Response, Router, SecretStoreError, SecretType, State, StatusCode, get,
};

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new().route(
        "/secrets",
        get(list_secrets)
            .post(create_secret)
            .delete(delete_secret_by_name),
    )
}

async fn list_secrets(_auth: RequiredUser, State(state): State<Arc<AppState>>) -> Response {
    match state.stores.vault.list().await {
        Ok(data) => (StatusCode::OK, Json(serde_json::json!({ "data": data }))).into_response(),
        Err(err) => secret_store_error(&err),
    }
}

async fn create_secret(
    _auth: RequiredUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateSecretRequest>,
) -> Response {
    let secret_type = body.type_;
    let name = body.name;
    let value = body.value;
    let description = body.description;
    if fabro_static::is_bootstrap_secret(&name) {
        return ApiError::bad_request(format!(
            "{name} is a bootstrap secret; configure it with process env or server.env"
        ))
        .into_response();
    }
    if secret_type == SecretType::Oauth {
        if let Err(err) = serde_json::from_str::<OAuthCredential>(&value) {
            return ApiError::bad_request(format!("invalid oauth credential JSON: {err}"))
                .into_response();
        }
    }
    if secret_type == SecretType::Token && name == EnvVars::DAYTONA_API_KEY {
        match state.check_daytona_api_key(value.clone()).await {
            Ok(check) if check.ok() => {}
            Ok(check) => {
                return ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, check.missing_message())
                    .into_response();
            }
            Err(err) => {
                return ApiError::new(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    format!("daytona credential validation failed: {err}"),
                )
                .into_response();
            }
        }
    }
    match state
        .stores
        .vault
        .set(&name, &value, secret_type, description.as_deref())
        .await
    {
        Ok(meta) => (StatusCode::OK, Json(meta)).into_response(),
        Err(SecretStoreError::InvalidName(_)) => {
            ApiError::bad_request("invalid secret name").into_response()
        }
        Err(SecretStoreError::InvalidOauth { .. }) => {
            ApiError::bad_request("invalid oauth credential JSON").into_response()
        }
        Err(err) => secret_store_error(&err),
    }
}

async fn delete_secret_by_name(
    _auth: RequiredUser,
    State(state): State<Arc<AppState>>,
    Json(body): Json<DeleteSecretRequest>,
) -> Response {
    let name = body.name;
    match state.stores.vault.remove(&name).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(SecretStoreError::NotFound(name)) => {
            ApiError::new(StatusCode::NOT_FOUND, format!("secret not found: {name}"))
                .into_response()
        }
        Err(err) => secret_store_error(&err),
    }
}

fn secret_store_error(err: &SecretStoreError) -> Response {
    tracing::error!(error = ?err, "Secret store operation failed");
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "secret store operation failed",
    )
    .into_response()
}
