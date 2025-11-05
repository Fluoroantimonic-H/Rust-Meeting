use axum::{
    extract::{Path, State, Json},
    http::StatusCode,
    routing::{get, post},
    Router,
};
use axum::response::Json as RespJson;
use bson::{doc, oid::ObjectId, DateTime as BsonDateTime};
use chrono::Utc;
use futures_util::TryStreamExt;
use mongodb::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::db::{feedback_collection, user_collection};

type AppState = Arc<Client>;

#[derive(Deserialize)]
struct FeedbackRequest {
    lecture_id: String,
    user_id: String,
    too_fast: Option<bool>,
    too_slow: Option<bool>,
    boring: Option<bool>,
    bad_question_quality: Option<bool>,
    other: Option<String>,
}

#[derive(Serialize)]
struct FeedbackSubmitResp {
    message: String,
    upserted_id: String,
}

// POST /feedback/submit
async fn submit_feedback(
    State(client): State<AppState>,
    Json(payload): Json<FeedbackRequest>,
) -> Result<RespJson<FeedbackSubmitResp>, (StatusCode, String)> {
    let coll = feedback_collection(&client);

    let lecture_oid = ObjectId::parse_str(&payload.lecture_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid lecture_id".into()))?;
    let user_oid = ObjectId::parse_str(&payload.user_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid user_id".into()))?;

    let filter = doc! {
        "lecture_id": lecture_oid,
        "user_id": user_oid,
    };

    let update = doc! {
        "$set": {
            "too_fast": payload.too_fast.unwrap_or(false),
            "too_slow": payload.too_slow.unwrap_or(false),
            "boring": payload.boring.unwrap_or(false),
            "bad_question_quality": payload.bad_question_quality.unwrap_or(false),
            "other": payload.other.unwrap_or_default(),
            "created_at": BsonDateTime::from_millis(Utc::now().timestamp_millis()),
        }
    };

    let result = coll
        .update_one(
            filter,
            update,
            Some(mongodb::options::UpdateOptions::builder().upsert(true).build()),
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "提交反馈失败".into()))?;

    let upserted = if let Some(id) = result.upserted_id {
        id.as_object_id().unwrap().to_hex()
    } else {
        "existing".into()
    };

    Ok(RespJson(FeedbackSubmitResp {
        message: "反馈提交成功（已覆盖旧记录）".into(),
        upserted_id: upserted,
    }))
}

// GET /feedback/lecture/{lecture_id}/feedback_summary
async fn feedback_summary(
    State(client): State<AppState>,
    Path(lecture_id): Path<String>,
) -> Result<RespJson<serde_json::Value>, (StatusCode, String)> {
    let coll = feedback_collection(&client);
    let lecture_oid = ObjectId::parse_str(&lecture_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid lecture_id".into()))?;

    let pipeline = vec![
        doc! { "$match": { "lecture_id": lecture_oid } },
        doc! {
            "$group": {
                "_id": null,
                "too_fast": { "$sum": { "$cond": [{ "$eq": ["$too_fast", true] }, 1, 0] } },
                "too_slow": { "$sum": { "$cond": [{ "$eq": ["$too_slow", true] }, 1, 0] } },
                "boring": { "$sum": { "$cond": [{ "$eq": ["$boring", true] }, 1, 0] } },
                "bad_question_quality": { "$sum": { "$cond": [{ "$eq": ["$bad_question_quality", true] }, 1, 0] } },
            }
        },
    ];

    let mut cursor = coll
        .aggregate(pipeline, None)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "聚合失败".into()))?;

    let mut stats = doc! {
        "too_fast": 0_i32,
        "too_slow": 0_i32,
        "boring": 0_i32,
        "bad_question_quality": 0_i32,
    };

    if let Some(doc) = cursor.try_next().await.map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "读取聚合结果错误".into())
    })? {
        if let Ok(v) = doc.get_i32("too_fast") { stats.insert("too_fast", v); }
        if let Ok(v) = doc.get_i32("too_slow") { stats.insert("too_slow", v); }
        if let Ok(v) = doc.get_i32("boring") { stats.insert("boring", v); }
        if let Ok(v) = doc.get_i32("bad_question_quality") { stats.insert("bad_question_quality", v); }
    }

    Ok(RespJson(serde_json::json!({ "feedback_summary": stats })))
}

// GET /feedback/lecture/{lecture_id}/user/{user_id}/feedback
async fn get_user_feedback(
    State(client): State<AppState>,
    Path((lecture_id, user_id)): Path<(String, String)>,
) -> Result<RespJson<serde_json::Value>, (StatusCode, String)> {
    let coll = feedback_collection(&client);
    let lecture_oid = ObjectId::parse_str(&lecture_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid lecture_id".into()))?;
    let user_oid = ObjectId::parse_str(&user_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid user_id".into()))?;

    let doc = coll
        .find_one(
            doc! {
                "lecture_id": lecture_oid,
                "user_id": user_oid,
            },
            None,
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?
        .ok_or((StatusCode::NOT_FOUND, "未找到该用户的反馈信息".into()))?;

    let resp = serde_json::json!({
        "too_fast": doc.get_bool("too_fast").unwrap_or(false),
        "too_slow": doc.get_bool("too_slow").unwrap_or(false),
        "boring": doc.get_bool("boring").unwrap_or(false),
        "bad_question_quality": doc.get_bool("bad_question_quality").unwrap_or(false),
        "other": doc.get_str("other").unwrap_or("")
    });

    Ok(RespJson(resp))
}

// GET /feedback/lecture/{lecture_id}/feedback_details
async fn feedback_detail_comments(
    State(client): State<AppState>,
    Path(lecture_id): Path<String>,
) -> Result<RespJson<serde_json::Value>, (StatusCode, String)> {
    let fb_coll = feedback_collection(&client);
    let user_coll = user_collection(&client);
    let lecture_oid = ObjectId::parse_str(&lecture_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid lecture_id".into()))?;

    let mut cursor = fb_coll
        .find(
            doc! {
                "lecture_id": lecture_oid,
                "other": { "$ne": "" }
            },
            None,
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?;

    let mut comments = Vec::new();
    while let Some(fb) = cursor.try_next().await.map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "读取失败".into())
    })? {
        let user_oid = fb.get_object_id("user_id").map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "字段缺失".into())
        })?;
        let user = user_coll
            .find_one(doc! { "_id": user_oid }, None)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询用户失败".into()))?;

        let username = user
            .as_ref()
            .and_then(|u| u.get_str("username").ok())
            .unwrap_or("未知用户")
            .to_string();
        let avatar = user
            .as_ref()
            .and_then(|u| u.get_str("avatar").ok())
            .unwrap_or("")
            .to_string();

        comments.push(serde_json::json!({
            "user_id": user_oid.to_hex(),
            "username": username,
            "avatar": avatar,
            "comment": fb.get_str("other").unwrap_or("")
        }));
    }

    Ok(RespJson(serde_json::json!({ "feedback_comments": comments })))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/submit", post(submit_feedback))
        .route("/lecture/:lecture_id/feedback_summary", get(feedback_summary))
        .route("/lecture/:lecture_id/user/:user_id/feedback", get(get_user_feedback))
        .route("/lecture/:lecture_id/feedback_details", get(feedback_detail_comments))
}
