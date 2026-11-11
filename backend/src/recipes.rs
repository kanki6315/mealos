use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::Error;

#[derive(Serialize, sqlx::FromRow)]
pub struct Recipe {
    pub id: String,
    pub version_id: Uuid,
    pub version: i32,
    pub content: Value,
}
pub async fn read(pool: &PgPool, recipe_id: &str, user_id: Uuid) -> Result<Option<Recipe>, Error> {
    Ok(sqlx::query_as!(
        Recipe,
        r#"SELECT r.id, rv.id AS version_id, rv.version, rv.content
FROM recipes r
         JOIN recipe_versions rv ON rv.recipe_id = r.id
WHERE r.id = $1
  AND r.deleted_at IS NULL
  AND (r.owner_id = $2 OR EXISTS(SELECT 1
                                FROM publications p
                                WHERE p.recipe_id = r.id
                                  AND p.version_id = rv.id
                                  AND p.visibility = 'public'
                                  AND p.review_state = 'approved'
                                  AND p.withdrawn_at IS NULL))
ORDER BY rv.version DESC
LIMIT 1"#,
        recipe_id,
        user_id,
    )
    .fetch_optional(pool)
    .await?)
}

pub async fn seed(pool: &PgPool, recipes: &[Value]) -> Result<usize, Error> {
    let mut ids = std::collections::HashSet::new();
    if recipes.is_empty() {
        return Err(Error::bad_request());
    }
    for recipe in recipes {
        let id = recipe["id"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 200)
            .ok_or_else(Error::bad_request)?;
        if !ids.insert(id)
            || recipe["name"].as_str().is_none_or(|s| s.trim().is_empty())
            || recipe["ingredients"]
                .as_array()
                .is_none_or(|v| v.is_empty())
            || recipe["steps"].as_array().is_none_or(|v| v.is_empty())
        {
            return Err(Error::bad_request());
        }
    }
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(72984102)")
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO collections(id,title) VALUES ('forkware-core','Forkware essentials') ON CONFLICT DO NOTHING").execute(&mut *tx).await?;
    for (position, recipe) in recipes.iter().enumerate() {
        let id = recipe["id"].as_str().unwrap();
        sqlx::query("INSERT INTO recipes(id) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let usable: bool = sqlx::query_scalar(
            "SELECT owner_id IS NULL AND deleted_at IS NULL FROM recipes WHERE id=$1 FOR UPDATE",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if !usable {
            return Err(Error(
                axum::http::StatusCode::CONFLICT,
                "Seed ID already belongs to another recipe.",
            ));
        }
        let existing: Option<(Uuid, bool)> = sqlx::query_as(
            "SELECT id,content=$2 FROM recipe_versions WHERE recipe_id=$1 AND version=1",
        )
        .bind(id)
        .bind(recipe)
        .fetch_optional(&mut *tx)
        .await?;
        let version_id=match existing {
            Some((_,false))=>return Err(Error(axum::http::StatusCode::CONFLICT,"Seed version changed; publish a new version explicitly.")),
            Some((id,true))=>id,
            None=>sqlx::query_scalar::<_,Uuid>("INSERT INTO recipe_versions(recipe_id,version,title,content) VALUES ($1,1,$2,$3) RETURNING id").bind(id).bind(recipe["name"].as_str().unwrap()).bind(recipe).fetch_one(&mut *tx).await?
        };
        let existing: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM publications WHERE recipe_id=$1 ORDER BY published_at LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        let publication = if let Some(id) = existing {
            id
        } else {
            sqlx::query_scalar("INSERT INTO publications(recipe_id,version_id,visibility,review_state) VALUES ($1,$2,'public','approved') RETURNING id").bind(id).bind(version_id).fetch_one(&mut *tx).await?
        };
        sqlx::query("INSERT INTO collection_entries(collection_id,publication_id,position) VALUES ('forkware-core',$1,$2) ON CONFLICT DO NOTHING").bind(publication).bind(position as i32).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(recipes.len())
}
