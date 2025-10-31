use axum::{
    extract::{Path, State, Json},
    routing::{get, post, put, delete},
    Router,
};
use axum::response::Json as RespJson;
use bson::{doc, oid::ObjectId, Document};
use mongodb::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::db::{invitation_collection, lecture_collection};
use futures_util::TryStreamExt;

type AppState = Arc<Client>;

#[derive(Deserialize)]
struct InvitationCreate {
    lecture_id: String,
    speaker_id: String,
    status: i32,
}

#[derive(Serialize)]
struct InvitationResponse {
    id: String,
    lecture_id: String,
    speaker_id: String,
    status: i32,
}

async fn create_invitation(
    State(client): State<AppState>,
    Json(payload): Json<InvitationCreate>,
) -> Result<RespJson<InvitationResponse>, (axum::http::StatusCode, String)> {
    let coll = invitation_collection(&client);

    // 验证并转换为 ObjectId 存库
    let lec_oid = ObjectId::parse_str(&payload.lecture_id)
        .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, "Invalid lecture_id format".into()))?;
    let spk_oid = ObjectId::parse_str(&payload.speaker_id)
        .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, "Invalid speaker_id format".into()))?;

    let doc = doc! {
        "lecture_id": lec_oid,
        "speaker_id": spk_oid,
        "status": payload.status,
    };

    let result = coll.insert_one(doc, None)
        .await
        .map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "创建邀请失败".into()))?;

    let id = result.inserted_id.as_object_id().unwrap().to_hex();
    Ok(RespJson(InvitationResponse {
        id,
        lecture_id: payload.lecture_id,
        speaker_id: payload.speaker_id,
        status: payload.status,
    }))
}

// GET /invitation/ -> 全部邀请
async fn get_all_invitations(
    State(client): State<AppState>,
) -> Result<RespJson<Vec<InvitationResponse>>, (axum::http::StatusCode, String)> {
    let coll = invitation_collection(&client);
    let mut cursor = coll
        .find(doc! {}, None)
        .await
        .map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?;
    let mut items = Vec::new();
    while let Some(doc) = cursor.try_next().await.map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "读取失败".into()))? {
        let id = doc.get_object_id("_id").map(|o| o.to_hex()).unwrap_or_default();
        let lecture_id = doc.get_object_id("lecture_id").map(|o| o.to_hex()).unwrap_or_default();
        let speaker_id = doc.get_object_id("speaker_id").map(|o| o.to_hex()).unwrap_or_default();
        let status = doc.get_i32("status").unwrap_or(0);
        items.push(InvitationResponse { id, lecture_id, speaker_id, status });
    }
    Ok(RespJson(items))
}

// GET /invitation/:invitation_id
async fn get_invitation(
    State(client): State<AppState>,
    Path(invitation_id): Path<String>,
) -> Result<RespJson<InvitationResponse>, (axum::http::StatusCode, String)> {
    let coll = invitation_collection(&client);
    let oid = ObjectId::parse_str(&invitation_id)
        .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, "Invalid invitation_id format".into()))?;
    let doc = coll
        .find_one(doc! { "_id": oid }, None)
        .await
        .map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?
        .ok_or((axum::http::StatusCode::NOT_FOUND, "Invitation not found".into()))?;
    let lecture_id = doc.get_object_id("lecture_id").map(|o| o.to_hex()).unwrap_or_default();
    let speaker_id = doc.get_object_id("speaker_id").map(|o| o.to_hex()).unwrap_or_default();
    let status = doc.get_i32("status").unwrap_or(0);
    Ok(RespJson(InvitationResponse { id: invitation_id, lecture_id, speaker_id, status }))
}

// PUT /invitation/:invitation_id
async fn update_invitation(
    State(client): State<AppState>,
    Path(invitation_id): Path<String>,
    Json(payload): Json<InvitationCreate>,
) -> Result<RespJson<InvitationResponse>, (axum::http::StatusCode, String)> {
    let coll = invitation_collection(&client);
    let oid = ObjectId::parse_str(&invitation_id)
        .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, "Invalid ID format".into()))?;
    let lec_oid = ObjectId::parse_str(&payload.lecture_id)
        .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, "Invalid lecture_id format".into()))?;
    let spk_oid = ObjectId::parse_str(&payload.speaker_id)
        .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, "Invalid speaker_id format".into()))?;

    let update = doc! {
        "$set": { "lecture_id": lec_oid, "speaker_id": spk_oid, "status": payload.status }
    };
    let result = coll
        .update_one(doc! { "_id": oid }, update, None)
        .await
        .map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "更新失败".into()))?;
    if result.matched_count == 0 { return Err((axum::http::StatusCode::NOT_FOUND, "Invitation not found".into())); }
    Ok(RespJson(InvitationResponse { id: invitation_id, lecture_id: payload.lecture_id, speaker_id: payload.speaker_id, status: payload.status }))
}

// DELETE /invitation/:invitation_id
async fn delete_invitation(
    State(client): State<AppState>,
    Path(invitation_id): Path<String>,
) -> Result<RespJson<serde_json::Value>, (axum::http::StatusCode, String)> {
    let coll = invitation_collection(&client);
    let oid = ObjectId::parse_str(&invitation_id)
        .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, "Invalid invitation_id format".into()))?;
    let result = coll
        .delete_one(doc! { "_id": oid }, None)
        .await
        .map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "删除失败".into()))?;
    if result.deleted_count == 0 { return Err((axum::http::StatusCode::NOT_FOUND, "Invitation not found".into())); }
    Ok(RespJson(serde_json::json!({"message": format!("Invitation {} deleted successfully", invitation_id)})))
}

// GET /invitation/byspeaker/:speaker_id -> 该讲者的邀请列表
async fn get_invitations_by_speaker(
    State(client): State<AppState>,
    Path(speaker_id): Path<String>,
) -> Result<RespJson<Vec<InvitationResponse>>, (axum::http::StatusCode, String)> {
    let coll = invitation_collection(&client);
    let spk_oid = ObjectId::parse_str(&speaker_id)
        .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, "Invalid speaker_id format".into()))?;
    let mut cursor = coll
        .find(doc! { "speaker_id": spk_oid }, None)
        .await
        .map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?;
    let mut items = Vec::new();
    while let Some(doc) = cursor.try_next().await.map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "读取失败".into()))? {
        let id = doc.get_object_id("_id").map(|o| o.to_hex()).unwrap_or_default();
        let lecture_id = doc.get_object_id("lecture_id").map(|o| o.to_hex()).unwrap_or_default();
        let speaker_id = doc.get_object_id("speaker_id").map(|o| o.to_hex()).unwrap_or_default();
        let status = doc.get_i32("status").unwrap_or(0);
        items.push(InvitationResponse { id, lecture_id, speaker_id, status });
    }
    Ok(RespJson(items))
}

// PUT /invitation/accept/:invitation_id -> 接受邀请，并把 speaker_id 写入 lecture（以字符串十六进制存储）
async fn accept_invitation(
    State(client): State<AppState>,
    Path(invitation_id): Path<String>,
) -> Result<RespJson<InvitationResponse>, (axum::http::StatusCode, String)> {
    let inv_coll = invitation_collection(&client);
    let lec_coll = lecture_collection(&client);
    let oid = ObjectId::parse_str(&invitation_id)
        .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, "Invalid invitation ID".into()))?;

    // 找邀请
    let invite = inv_coll
        .find_one(doc! { "_id": oid }, None)
        .await
        .map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?
        .ok_or((axum::http::StatusCode::NOT_FOUND, "Invitation not found".into()))?;

    let lecture_oid = invite.get_object_id("lecture_id").map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "字段缺失".into()))?;
    let speaker_oid = invite.get_object_id("speaker_id").map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "字段缺失".into()))?;

    // 更新邀请状态
    inv_coll
        .update_one(doc! { "_id": oid }, doc! { "$set": { "status": 1 } }, None)
        .await
        .map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "更新失败".into()))?;

    // 同步更新 lecture 的 speaker_id（存 hex 字符串，兼容现有 lecture 结构）
    lec_coll
        .update_one(
            doc! { "_id": lecture_oid },
            doc! { "$set": { "speaker_id": speaker_oid.to_hex() } },
            None,
        )
        .await
        .map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "更新演讲失败".into()))?;

    Ok(RespJson(InvitationResponse {
        id: invitation_id,
        lecture_id: lecture_oid.to_hex(),
        speaker_id: speaker_oid.to_hex(),
        status: 1,
    }))
}


// DELETE /invitation/lid/:lecture_id
async fn delete_invitation_by_lid(
    State(client): State<AppState>,
    Path(lecture_id): Path<String>,
) -> Result<RespJson<serde_json::Value>, (axum::http::StatusCode, String)> {
    let coll = invitation_collection(&client);
    let oid = ObjectId::parse_str(&lecture_id)
        .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, "Invalid invitation_id format".into()))?;
    let result = coll
        .delete_many(doc! { "lecture_id": oid }, None)
        .await
        .map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "删除失败".into()))?;
    // if result.deleted_count == 0 { return Err((axum::http::StatusCode::NOT_FOUND, "Invitation not found".into())); }
    Ok(RespJson(serde_json::json!({"message": format!("Invitation which lecture_id is {} deleted successfully", lecture_id)})))
}


pub fn router() -> Router<AppState> {
    Router::new()
        .route("/create", post(create_invitation))
        .route("/", get(get_all_invitations))
        .route("/:invitation_id", get(get_invitation))
        .route("/:invitation_id", put(update_invitation))
        .route("/:invitation_id", delete(delete_invitation))
        .route("/byspeaker/:speaker_id", get(get_invitations_by_speaker))
        .route("/accept/:invitation_id", put(accept_invitation))
        .route("/lid/:lecture_id", delete(delete_invitation_by_lid))
}

