use axum::{
    extract::{Path, State, Json},
    http::StatusCode,
    routing::{get, post},
    Router,
};
use axum::response::Json as RespJson;
use bson::{doc, oid::ObjectId, DateTime as BsonDateTime};
use chrono::{DateTime, Utc, TimeZone};
use futures_util::TryStreamExt;
use mongodb::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::db::{discussion_collection, user_collection};

type AppState = Arc<Client>;

#[derive(Deserialize)]
struct DiscussionCreate {
    lecture_id: String,
    user_id: String,
    content: String,
}

#[derive(Serialize)]
struct DiscussionOut {
    id: String,
    lecture_id: String,
    user_id: String,
    content: String,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct DiscussionOutWithUser {
    id: String,
    lecture_id: String,
    user_id: String,
    content: String,
    created_at: DateTime<Utc>,
    username: String,
    avatar: String,
}

// POST /discussion/add
async fn add_discussion(
    State(client): State<AppState>,
    Json(payload): Json<DiscussionCreate>,
) -> Result<RespJson<DiscussionOut>, (StatusCode, String)> {
    let coll = discussion_collection(&client);
    let lecture_oid = ObjectId::parse_str(&payload.lecture_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid lecture_id".into()))?;
    let user_oid = ObjectId::parse_str(&payload.user_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid user_id".into()))?;

    let now = Utc::now();
    let doc = doc! {
        "lecture_id": lecture_oid,
        "user_id": user_oid,
        "content": &payload.content,
        "created_at": BsonDateTime::from_millis(now.timestamp_millis()),
    };

    let result = coll
        .insert_one(doc, None)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "插入失败".into()))?;

    let id = result
        .inserted_id
        .as_object_id()
        .unwrap()
        .to_hex();

    Ok(RespJson(DiscussionOut {
        id,
        lecture_id: payload.lecture_id,
        user_id: payload.user_id,
        content: payload.content,
        created_at: now,
    }))
}

// GET /discussion/lecture/{lecture_id}
async fn get_discussions_by_lecture(
    State(client): State<AppState>,
    Path(lecture_id): Path<String>,
) -> Result<RespJson<Vec<DiscussionOutWithUser>>, (StatusCode, String)> {
    let disc_coll = discussion_collection(&client);
    let user_coll = user_collection(&client);
    let lecture_oid = ObjectId::parse_str(&lecture_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid lecture_id".into()))?;

    let mut cursor = disc_coll
        .find(doc! { "lecture_id": lecture_oid }, None)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?;

    let mut list = Vec::new();
    while let Some(doc) = cursor.try_next().await.map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "读取失败".into())
    })? {
        let user_oid = doc.get_object_id("user_id").map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "user_id 缺失".into())
        })?;
        let user_doc = user_coll
            .find_one(doc! { "_id": user_oid }, None)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询用户失败".into()))?
            .unwrap_or(doc! { "username": "未知用户", "avatar": "" });

        list.push(DiscussionOutWithUser {
            id: doc.get_object_id("_id").unwrap().to_hex(),
            lecture_id: lecture_oid.to_hex(),
            user_id: user_oid.to_hex(),
            content: doc.get_str("content").unwrap_or("").to_string(),
            created_at: doc
                .get_datetime("created_at")
                .map(|dt| dt.to_chrono())  // ✅ 已经是 DateTime<Utc>
                .unwrap_or(Utc::now()),
            username: user_doc.get_str("username").unwrap_or("未知用户").to_string(),
            avatar: user_doc.get_str("avatar").unwrap_or("").to_string(),
        });
    }

    Ok(RespJson(list))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/add", post(add_discussion))
        .route("/lecture/:lecture_id", get(get_discussions_by_lecture))
}