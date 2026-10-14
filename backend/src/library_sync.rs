use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{Error, auth::digest};

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Write {
    pub operation_id: Uuid,
    pub base_revision: i64,
    pub library: Value,
}
#[derive(Serialize, sqlx::FromRow)]
pub struct Snapshot {
    pub revision: i64,
    pub library: Option<Value>,
}

pub async fn read(pool: &PgPool, user: Uuid) -> Result<Snapshot, Error> {
    Ok(sqlx::query_as!(
        Snapshot,
        "SELECT revision,library FROM library_sync WHERE user_id = $1",
        user
    )
    .fetch_optional(pool)
    .await?
    .unwrap_or(Snapshot {
        revision: 0,
        library: None,
    }))
}

pub async fn write(pool: &PgPool, user: Uuid, input: Write) -> Result<Value, Error> {
    validate(&input)?;
    let hash = digest(&serde_json::to_string(&input).map_err(|_| Error::bad_request())?);
    let mut tx = pool.begin().await?;

    sqlx::query!(
        "INSERT INTO library_sync(user_id) VALUES ($1) ON CONFLICT DO NOTHING",
        user
    )
    .execute(&mut *tx)
    .await?;

    let state = sqlx::query!(
        r#"
SELECT s.revision as revision, s.library as library, o.request_hash as "saved_hash?", o.revision as "saved_revision?"
FROM library_sync s
LEFT JOIN library_sync_operations o ON o.user_id = s.user_id AND o.operation_id = $2
WHERE s.user_id = $1
FOR UPDATE OF s
"#,
        user,
        input.operation_id
    )
        .fetch_one(&mut *tx)
        .await?;

    let revision = state.revision;
    let library = state.library;
    let saved_hash = state.saved_hash;
    let saved_revision = state.saved_revision;

    if let (Some(saved_hash), Some(saved_revision)) = (saved_hash, saved_revision) {
        if hash != saved_hash {
            return Err(Error(
                StatusCode::CONFLICT,
                "Operation ID already used for different edits.",
            ));
        }

        return Ok(
            json!({"revision": saved_revision, "snapshot": { "revision": revision, "library": library } }),
        );
    }

    if revision != input.base_revision {
        return Err(Error(
            StatusCode::CONFLICT,
            "Another device changed your library. Your local edits are retained.",
        ));
    }

    let next = revision.checked_add(1).ok_or_else(Error::unavailable)?;

    sqlx::query!(
        r#"
WITH updated AS (
    UPDATE library_sync SET revision = $2, library=$3 where user_id = $1
)
INSERT INTO library_sync_operations(user_id, operation_id, request_hash, revision)
VALUES ($1, $4, $5, $2)
"#,
        user,
        next,
        &input.library,
        input.operation_id,
        hash
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(json!({"revision":next,"snapshot":{"revision":next,"library":input.library}}))
}

// This endpoint stores private snapshots, never publishes or feeds recipe reads.
// Validate the versioned envelope and recipe content before accepting a batch.
fn validate(input: &Write) -> Result<(), Error> {
    let valid = (|| -> Option<()> {
        if input.base_revision < 0 {
            return None;
        }
        let library = input.library.as_object()?;
        if library.len() != 2 {
            return None;
        }
        let recipes = library.get("recipes")?.as_array()?;
        if recipes.len() > 5000 {
            return None;
        }
        let mut ids = std::collections::HashSet::new();
        for recipe in recipes {
            validate_recipe(recipe)?;
            let id = recipe.get("id")?.as_str()?;
            if id.is_empty() || id.len() > 200 || !ids.insert(id) || !recipe.is_object() {
                return None;
            }
        }
        let state = library.get("state")?.as_object()?;
        if state.len() != 7 || state.get("schemaVersion")?.as_i64()? != 2 {
            return None;
        }
        for key in ["favorites", "pantry", "checkedGroceries"] {
            let items = state.get(key)?.as_array()?;
            if items.len() > 20000
                || items
                    .iter()
                    .any(|v| v.as_str().is_none_or(|s| s.len() > 1000))
            {
                return None;
            }
        }
        for (key, value) in state.get("ratings")?.as_object()? {
            if key.len() > 200 || value.as_i64().is_none_or(|n| !(1..=5).contains(&n)) {
                return None;
            }
        }
        for (key, value) in state.get("notes")?.as_object()? {
            if key.len() > 200 || value.as_str().is_none_or(|s| s.len() > 100000) {
                return None;
            }
        }
        let mut days = std::collections::HashSet::new();
        for plan in state.get("plan")?.as_array()? {
            if plan.as_object()?.len() != 3 {
                return None;
            }
            let day = plan.get("day")?.as_i64()?;
            if !(-100000..=1000000).contains(&day)
                || !days.insert(day)
                || plan.get("recipe")?.as_str()?.len() > 200
                || !["ONE", "TWO", "DINNER_PLUS_LUNCH"].contains(&plan.get("portion")?.as_str()?)
            {
                return None;
            }
        }
        Some(())
    })();
    valid.ok_or_else(Error::bad_request)
}

fn text<'a>(value: &'a Value, key: &str, nonempty: bool) -> Option<&'a str> {
    let s = value.get(key)?.as_str()?;
    if s.len() > 100_000 || (nonempty && s.trim().is_empty()) {
        None
    } else {
        Some(s)
    }
}
fn strings(value: &Value, key: &str) -> Option<()> {
    for item in value.get(key)?.as_array()? {
        if item.as_str()?.len() > 1000 {
            return None;
        }
    }
    Some(())
}
fn validate_recipe(recipe: &Value) -> Option<()> {
    let keys = [
        "id",
        "name",
        "shortName",
        "effortMinutes",
        "totalMinutes",
        "baseServings",
        "equipment",
        "tags",
        "leftoverNote",
        "ingredients",
        "steps",
        "description",
        "origin",
    ];
    if recipe
        .as_object()?
        .keys()
        .any(|k| !keys.contains(&k.as_str()))
    {
        return None;
    }
    for key in ["id", "name", "shortName"] {
        text(recipe, key, true)?;
    }
    for key in ["leftoverNote", "description"] {
        text(recipe, key, false)?;
    }
    strings(recipe, "equipment")?;
    strings(recipe, "tags")?;
    if !["CORE", "MANUAL", "CHATGPT_IMPORT"].contains(&text(recipe, "origin", true)?) {
        return None;
    }
    let effort = recipe.get("effortMinutes")?.as_i64()?;
    let total = recipe.get("totalMinutes")?.as_i64()?;
    if effort < 0 || total < 1 || total < effort || recipe.get("baseServings")?.as_i64()? < 1 {
        return None;
    }
    let ingredients = recipe.get("ingredients")?.as_array()?;
    let steps = recipe.get("steps")?.as_array()?;
    if ingredients.is_empty() || steps.is_empty() {
        return None;
    }
    for ingredient in ingredients {
        if ingredient.as_object()?.len() != 6 {
            return None;
        }
        for key in ["id", "name", "unit"] {
            text(ingredient, key, true)?;
        }
        if ingredient.get("amount")?.as_f64()? <= 0.0 {
            return None;
        }
        ingredient.get("pantryStaple")?.as_bool()?;
        if !["PRODUCE", "MEAT", "DAIRY", "BAKERY", "PANTRY", "FROZEN"]
            .contains(&text(ingredient, "category", true)?)
        {
            return None;
        }
    }
    for step in steps {
        if step.as_object()?.keys().any(|k| {
            !["title", "instruction", "equipment", "timerSeconds", "tip"].contains(&k.as_str())
        }) {
            return None;
        }
        for key in ["title", "instruction", "equipment"] {
            text(step, key, true)?;
        }
        if let Some(timer) = step.get("timerSeconds").filter(|v| !v.is_null())
            && timer.as_i64()? <= 0
        {
            return None;
        }
        if let Some(tip) = step.get("tip").filter(|v| !v.is_null()) {
            tip.as_str()?;
        }
    }
    Some(())
}
