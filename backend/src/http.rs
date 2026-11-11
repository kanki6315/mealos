use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, Request, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{PgPool, migrate::MigrationType};
use uuid::Uuid;

use crate::{
    Error, apple::AppleVerifier, assistant, assistant::Failure, auth, db, library_sync, recipes,
};

pub struct AppState {
    pub pool: PgPool,
    pub apple: AppleVerifier,
    pub assistant: Option<assistant::Assistant>,
    auth_requests: Mutex<VecDeque<Instant>>,
}
impl AppState {
    pub fn new(pool: PgPool, apple: AppleVerifier) -> Arc<Self> {
        Self::with_assistant(pool, apple, None)
    }
    pub fn with_assistant(
        pool: PgPool,
        apple: AppleVerifier,
        assistant: Option<assistant::Assistant>,
    ) -> Arc<Self> {
        Arc::new(Self {
            assistant,
            pool,
            apple,
            auth_requests: Mutex::new(VecDeque::new()),
        })
    }
}
pub fn router(state: Arc<AppState>) -> Router {
    let auth_routes = Router::new()
        .route("/apple/challenge", post(challenge))
        .route("/apple", post(sign_in))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth_limit));

    let health_routes = Router::new()
        .route("/health", get(|| async { Json(json!({"ok":true})) }))
        .route("/ready", get(ready))
        .with_state(state.clone());

    let library_routes = Router::new()
        .route("/library", get(library_read).post(library_write))
        .layer(DefaultBodyLimit::max(2_000_000));

    let recipe_chat_route = Router::new()
        .route("/recipe-chat", post(chat))
        .layer(DefaultBodyLimit::max(256_000))
        .layer(middleware::from_fn(assistant_timeout));

    let v1api_routes = Router::new()
        .nest("/auth", auth_routes)
        .route("/me", get(me))
        .route("/recipes/{id}", get(recipe))
        .merge(library_routes)
        .merge(recipe_chat_route)
        .layer(DefaultBodyLimit::max(24_000))
        .layer(middleware::from_fn(headers_and_timeout))
        .with_state(state);

    let api = Router::new().nest("/v1", v1api_routes);

    Router::new()
        .nest("/api", api)
        .merge(health_routes)
        .fallback(|| async { Error(StatusCode::NOT_FOUND, "Not found.") })
}

async fn headers_and_timeout(request: Request, next: Next) -> Response {
    let mut response = tokio::time::timeout(
        Duration::from_secs(if request.uri().path().ends_with("/v1/recipe-chat") {
            115
        } else {
            30
        }),
        next.run(request),
    )
    .await
    .unwrap_or_else(|_| Error(StatusCode::REQUEST_TIMEOUT, "Request timed out.").into_response());

    response.headers_mut().extend([
        (
            HeaderName::from_static("cache-control"),
            HeaderValue::from_static("no-store"),
        ),
        (
            HeaderName::from_static("x-content-type-options"),
            HeaderValue::from_static("nosniff"),
        ),
    ]);

    response
}

async fn auth_limit(State(state): State<Arc<AppState>>, request: Request, next: Next) -> Response {
    let now = Instant::now();

    let is_rate_limited = {
        let mut times = state
            .auth_requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        while times
            .front()
            .is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(60))
        {
            times.pop_front();
        }

        if times.len() >= 60 {
            true
        } else {
            times.push_back(now);
            false
        }
    };

    if is_rate_limited {
        let mut response = Error(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many authentication requests.",
        )
        .into_response();
        response
            .headers_mut()
            .insert("retry-after", HeaderValue::from_static("60"));
        return response;
    }

    next.run(request).await
}

fn object(value: Result<Json<Value>, JsonRejection>) -> Result<(), Error> {
    let Json(value) = value?;
    if value.is_object() {
        Ok(())
    } else {
        Err(Error::bad_request())
    }
}

fn bearer(headers: &HeaderMap) -> Result<&str, Error> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(Error::unauthorized)
}

async fn ready(State(state): State<Arc<AppState>>) -> Result<Json<Value>, Error> {
    let migrations = db::MIGRATOR
        .iter()
        .filter(|m| m.migration_type == MigrationType::Simple)
        .map(|m| m.description.to_string())
        .collect::<Vec<_>>();

    let present = sqlx::query_scalar!(
        r#"
SELECT count(*)=$1 AS "present!"
FROM _sqlx_migrations
WHERE description = ANY($2)
  AND success=TRUE
"#,
        migrations.len() as i64,
        &migrations
    )
        .fetch_one(&state.pool)
        .await?;
    if !present {
        return Err(Error::unavailable());
    }

    Ok(Json(json!({"ok":true})))
}
async fn challenge(
    State(state): State<Arc<AppState>>,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<(StatusCode, Json<auth::Challenge>), Error> {
    object(input)?;
    Ok((
        StatusCode::CREATED,
        Json(auth::challenge(&state.pool).await?),
    ))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignIn {
    challenge_id: Uuid,
    identity_token: String,
}

async fn sign_in(
    State(state): State<Arc<AppState>>,
    input: Result<Json<SignIn>, JsonRejection>,
) -> Result<Json<auth::Tokens>, Error> {
    let Json(input) = input?;
    if input.identity_token.is_empty() || input.identity_token.len() > 16_384 {
        return Err(Error::bad_request());
    }
    let nonce = auth::challenge_nonce(&state.pool, input.challenge_id).await?;
    let subject = state.apple.verify(&input.identity_token, &nonce).await?;
    Ok(Json(
        auth::finish_sign_in(&state.pool, input.challenge_id, &subject).await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Refresh {
    refresh_token: String,
}

async fn refresh(
    State(state): State<Arc<AppState>>,
    input: Result<Json<Refresh>, JsonRejection>,
) -> Result<Json<auth::Tokens>, Error> {
    let Json(input) = input?;
    Ok(Json(
        auth::refresh(&state.pool, &input.refresh_token).await?,
    ))
}

async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    input: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    object(input)?;
    let identity = auth::authenticate(&state.pool, bearer(&headers)?).await?;
    auth::logout(&state.pool, identity).await?;
    Ok(Json(json!({"ok":true})))
}
async fn me(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Result<Json<Value>, Error> {
    let identity = auth::authenticate(&state.pool, bearer(&headers)?).await?;
    Ok(Json(
        json!({"user":{"id":identity.id},"assistantAllowed":state.assistant.as_ref().is_some_and(|a| a.allows(identity.id))}),
    ))
}
async fn recipe(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<recipes::Recipe>, Error> {
    let identity = auth::authenticate(&state.pool, bearer(&headers)?).await?;
    if id.len() > 200 {
        return Err(Error::bad_request());
    }
    Ok(Json(
        recipes::read(&state.pool, &id, identity.id)
            .await?
            .ok_or(Error(StatusCode::NOT_FOUND, "Recipe not found."))?,
    ))
}

async fn assistant_timeout(request: Request, next: Next) -> Response {
    tokio::time::timeout(Duration::from_secs(110), next.run(request))
        .await
        .unwrap_or_else(|_| {
            Failure {
                status: StatusCode::GATEWAY_TIMEOUT,
                code: "provider_timeout",
                message: "Claude timed out. You can try Apple Private Cloud Compute.",
                fallback: true,
            }
            .into_response()
        })
}
async fn chat(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    input: Result<Json<assistant::Chat>, JsonRejection>,
) -> Result<Response, Error> {
    let token = bearer(&headers)?;
    let identity = auth::authenticate(&state.pool, token).await?;

    let Some(assistant) = &state.assistant else {
        return Ok(assistant::disabled().into_response());
    };

    let Json(input) = input?;
    let value = match assistant.chat(identity.id, input).await {
        Ok(value) => value,
        Err(failure) => return Ok(failure.into_response()),
    };

    Ok(Json(value).into_response())
}

async fn library_read(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<library_sync::Snapshot>, Error> {
    let identity = auth::authenticate(&state.pool, bearer(&headers)?).await?;
    Ok(Json(library_sync::read(&state.pool, identity.id).await?))
}
async fn library_write(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    input: Result<Json<library_sync::Write>, JsonRejection>,
) -> Result<Json<Value>, Error> {
    let identity = auth::authenticate(&state.pool, bearer(&headers)?).await?;
    let Json(input) = input.map_err(|_| Error::bad_request())?;
    Ok(Json(
        library_sync::write(&state.pool, identity.id, input).await?,
    ))
}
