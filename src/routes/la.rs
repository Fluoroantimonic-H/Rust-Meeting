use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post},
    Router,
};
use bson::{doc, oid::ObjectId, Document};
use futures_util::stream::StreamExt;
use mongodb::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use chrono::Utc;

use crate::db::{la_collection, user_collection};

type AppState = Arc<Client>;

// ==================== 模型 ====================

#[derive(Deserialize)]
struct LARecord {
    lecture_id: String,
    audience_id: String,
    is_present: Option<bool>,
    joined_at: Option<i64>,
}

#[derive(Deserialize)]
struct LACreateRequest {
    lecture_id: String,
    audience_id: String,
}

#[derive(Serialize)]
struct LAResponse {
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    la_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    joined_at: Option<i64>,
}

#[derive(Deserialize)]
struct UpdateIsPresent {
    lecture_id: String,
    audience_id: String,
    is_present: bool,
}

// ==================== 工具函数 ====================

fn convert_doc_ids(doc: &mut Document) -> Result<(), (StatusCode, String)> {
    let id = doc.get_object_id("_id")
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "Invalid _id".into()))?
        .to_hex();
    doc.insert("_id", id);

    if let Ok(oid) = doc.get_object_id("lecture_id") {
        doc.insert("lecture_id", oid.to_hex());
    }
    if let Ok(oid) = doc.get_object_id("audience_id") {
        doc.insert("audience_id", oid.to_hex());
    }
    Ok(())
}

// ==================== 路由 ====================

async fn add_la(
    State(client): State<AppState>,
    Json(payload): Json<LARecord>,
) -> Result<Json<LAResponse>, (StatusCode, String)> {
    let coll = la_collection(&client);

    let lecture_oid = ObjectId::parse_str(&payload.lecture_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效的 lecture_id".into()))?;
    let audience_oid = ObjectId::parse_str(&payload.audience_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效的 audience_id".into()))?;

    let doc = doc! {
        "lecture_id": lecture_oid,
        "audience_id": audience_oid,
        "is_present": payload.is_present.unwrap_or(false),
        "joined_at": payload.joined_at.unwrap_or_else(|| Utc::now().timestamp_millis()),
    };

    coll.insert_one(doc, None).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "插入失败".into()))?;

    Ok(Json(LAResponse {
        message: "加入成功".into(),
        la_id: None,
        joined_at: None,
    }))
}

async fn delete_la(
    State(client): State<AppState>,
    query: Query<std::collections::HashMap<String, String>>,
) -> Result<Json<LAResponse>, (StatusCode, String)> {
    let coll = la_collection(&client);
    let lecture_id = query.get("lecture_id").ok_or((StatusCode::BAD_REQUEST, "缺少 lecture_id".into()))?;
    let audience_id = query.get("audience_id").ok_or((StatusCode::BAD_REQUEST, "缺少 audience_id".into()))?;

    let lecture_oid = ObjectId::parse_str(lecture_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效的 lecture_id".into()))?;
    let audience_oid = ObjectId::parse_str(audience_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效的 audience_id".into()))?;

    let result = coll.delete_one(doc! {
        "lecture_id": lecture_oid,
        "audience_id": audience_oid,
    }, None).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".into()))?;

    if result.deleted_count == 0 {
        return Err((StatusCode::NOT_FOUND, "记录未找到".into()));
    }

    Ok(Json(LAResponse {
        message: "删除成功".into(),
        la_id: None,
        joined_at: None,
    }))
}

async fn delete_la_by_lid(
    State(client): State<AppState>,
    Path(lecture_id): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    let coll = la_collection(&client);
    let oid = ObjectId::parse_str(&lecture_id)
        .map_err(|_| (axum::http::StatusCode::BAD_REQUEST, "Invalid lecture_id format".into()))?;
    let result = coll
        .delete_many(doc! { "lecture_id": oid }, None)
        .await
        .map_err(|_| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, "删除失败".into()))?;
    // if result.deleted_count == 0 { return Err((axum::http::StatusCode::NOT_FOUND, "Invitation not found".into())); }
    Ok(Json(serde_json::json!({"message": format!("LA which lecture_id is {} deleted successfully", lecture_id)})))
}

async fn get_by_lecture(
    State(client): State<AppState>,
    query: Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let coll = la_collection(&client);
    let lecture_id = query.get("lecture_id").ok_or((StatusCode::BAD_REQUEST, "缺少 lecture_id".into()))?;
    let oid = ObjectId::parse_str(lecture_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效的 lecture_id".into()))?;

    let mut cursor = coll.find(doc! { "lecture_id": oid }, None).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?;

    let mut records = Vec::new();
    while let Some(mut doc) = cursor.next().await {
        let mut doc = doc.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "读取错误".into()))?;
        convert_doc_ids(&mut doc)?;
        records.push(doc);
    }

    Ok(Json(serde_json::json!({ "records": records })))
}

async fn get_by_audience(
    State(client): State<AppState>,
    query: Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let coll = la_collection(&client);
    let audience_id = query.get("audience_id").ok_or((StatusCode::BAD_REQUEST, "缺少 audience_id".into()))?;
    let oid = ObjectId::parse_str(audience_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效的 audience_id".into()))?;

    let mut cursor = coll.find(doc! { "audience_id": oid }, None).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?;

    let mut records = Vec::new();
    while let Some(mut doc) = cursor.next().await {
        let mut doc = doc.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "读取错误".into()))?;
        convert_doc_ids(&mut doc)?;
        records.push(doc);
    }

    Ok(Json(serde_json::json!({ "records": records })))
}

async fn get_present_users(
    State(client): State<AppState>,
    query: Query<std::collections::HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let coll = la_collection(&client);
    let user_coll = user_collection(&client);
    let lecture_id = query.get("lecture_id").ok_or((StatusCode::BAD_REQUEST, "缺少 lecture_id".into()))?;

    let lecture_oid = match ObjectId::parse_str(lecture_id) {
        Ok(oid) => oid,
        Err(_) => return Ok(Json(serde_json::json!({ "error": "无效的 lecture_id" }))),
    };

    let mut cursor = coll.find(doc! {
        "lecture_id": lecture_oid,
        "is_present": true,
    }, None).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?;

    let mut user_ids = Vec::new();
    while let Some(doc) = cursor.next().await {
        let doc = doc.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "读取错误".into()))?;
        if let Ok(oid) = doc.get_object_id("audience_id") {
            user_ids.push(oid);
        }
    }

    if user_ids.is_empty() {
        return Ok(Json(serde_json::json!({ "users": [] })));
    }

    let mut user_cursor = user_coll.find(doc! {
        "_id": { "$in": user_ids }
    }, None).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询用户失败".into()))?;

    let mut users = Vec::new();
    while let Some(mut doc) = user_cursor.next().await {
        let mut doc = doc.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "读取用户错误".into()))?;
        if let Ok(oid) = doc.get_object_id("_id") {
            doc.insert("_id", oid.to_hex());
        }
        users.push(doc);
    }

    Ok(Json(serde_json::json!({ "users": users })))
}


// async fn update_is_present(
//     State(client): State<AppState>,
//     Json(payload): Json<UpdateIsPresent>,
// ) -> Result<Json<LAResponse>, (StatusCode, String)> {
//     let coll = la_collection(&client);

//     let lecture_oid = ObjectId::parse_str(&payload.lecture_id)
//         .map_err(|_| (StatusCode::BAD_REQUEST, "无效的 lecture_id".into()))?;
//     let audience_oid = ObjectId::parse_str(&payload.audience_id)
//         .map_err(|_| (StatusCode::BAD_REQUEST, "无效的 audience_id".into()))?;

//     let result = coll.update_one(
//         doc! {
//             "lecture_id": lecture_oid,
//             "audience_id": audience_oid,
//         },
//         doc! { "$set": { "is_present": payload.is_present } },
//         None,
//     ).await
//         .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "更新失败".into()))?;

//     if result.matched_count == 0 {
//         return Err((StatusCode::NOT_FOUND, "记录未找到".into()));
//     }

//     Ok(Json(LAResponse {
//         message: format!("is_present 已更新为 {}", payload.is_present),
//         la_id: None,
//         joined_at: None,
//     }))
// }
use mongodb::options::UpdateOptions;

async fn update_is_present(
    State(client): State<AppState>,
    Json(payload): Json<UpdateIsPresent>,
) -> Result<Json<LAResponse>, (StatusCode, String)> {
    let coll = la_collection(&client);

    let lecture_oid = ObjectId::parse_str(&payload.lecture_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效的 lecture_id".into()))?;
    let audience_oid = ObjectId::parse_str(&payload.audience_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效的 audience_id".into()))?;

    let now = Utc::now().timestamp_millis();

    let update_doc = doc! {
        "$set": {
            "is_present": payload.is_present,
            "joined_at": now  // 即使是更新，也刷新 joined_at（可选）
        },
        "$setOnInsert": {
            "lecture_id": lecture_oid,
            "audience_id": audience_oid,
        }
    };

    let options = UpdateOptions::builder().upsert(true).build();

    let result = coll.update_one(
        doc! {
            "lecture_id": lecture_oid,
            "audience_id": audience_oid,
        },
        update_doc,
        options,
    ).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "操作失败".into()))?;

    let message = if result.matched_count > 0 {
        "is_present 已更新".to_string()
    } else if result.upserted_id.is_some() {
        "新记录已创建".to_string()
    } else {
        "操作成功".to_string()
    };

    Ok(Json(LAResponse {
        message,
        la_id: result.upserted_id.map(|id| id.as_object_id().unwrap().to_hex()),
        joined_at: Some(now),
    }))
}

async fn create_la_entry(
    State(client): State<AppState>,
    Json(data): Json<LACreateRequest>,
) -> Result<Json<LAResponse>, (StatusCode, String)> {
    let coll = la_collection(&client);

    if !ObjectId::parse_str(&data.lecture_id).is_ok() || !ObjectId::parse_str(&data.audience_id).is_ok() {
        return Err((StatusCode::BAD_REQUEST, "无效的 lecture_id 或 audience_id".into()));
    }

    let lecture_oid = ObjectId::parse_str(&data.lecture_id).unwrap();
    let audience_oid = ObjectId::parse_str(&data.audience_id).unwrap();

    let la_doc = doc! {
        "lecture_id": lecture_oid,
        "audience_id": audience_oid,
        "is_present": false,
        "joined_at": Utc::now().timestamp_millis(),
    };

    let result = coll.insert_one(la_doc, None).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "创建失败".into()))?;

    let la_id = result.inserted_id.as_object_id()
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "插入ID无效".into()))?
        .to_hex();

    Ok(Json(LAResponse {
        message: "成功加入演讲".into(),
        la_id: Some(la_id),
        joined_at: Some(Utc::now().timestamp_millis()),
    }))
}

async fn get_lectures_by_user(
    State(client): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<Document>>, (StatusCode, String)> {
    let coll = la_collection(&client);
    let oid = ObjectId::parse_str(&user_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid user_id".into()))?;

    let mut cursor = coll.find(doc! { "audience_id": oid }, None).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?;

    let mut lectures = Vec::new();
    while let Some(mut doc) = cursor.next().await {
        let mut doc = doc.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "读取错误".into()))?;
        convert_doc_ids(&mut doc)?;
        lectures.push(doc);
    }

    Ok(Json(lectures))
}

// ==================== Router ====================

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/add", post(add_la))
        .route("/delete", delete(delete_la))
        .route("/by-lecture", get(get_by_lecture))
        .route("/by-audience", get(get_by_audience))
        .route("/present", get(get_present_users))
        .route("/update_is_present", post(update_is_present))
        .route("/create", post(create_la_entry))
        .route("/lectures_by_user/:user_id", get(get_lectures_by_user))
        .route("/deletela/:lecture_id", delete(delete_la_by_lid))
}
