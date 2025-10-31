// src/routes/lecture.rs
use axum::{
    extract::{Path, State, Json},
    http::StatusCode,
    routing::{get, post},
    Router,
};
use axum::response::Json as RespJson;
use bson::{doc, oid::ObjectId, Document};
use futures_util::TryStreamExt;
use mongodb::Client;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::db::lecture_collection;

type AppState = Arc<Client>;

// ==================== 请求模型 ====================

#[derive(Deserialize)]
struct LectureCreate {
    topic: String,
    // 前端传 ISO8601 字符串，如 2025-01-01T10:00:00.000Z
    start_time: String,
    duration: i32,
    description: Option<String>,
    // 前端可能传空字符串，按 None 处理
    speaker_id: Option<String>,
    organizer_id: String,
    status: i32,
}

#[derive(Serialize)]
struct Lecture {
    id: String,
    topic: String,
    start_time: i64,
    duration: i32,
    description: String,
    speaker_id: Option<String>,
    organizer_id: Option<String>,
    lecturecode: i32,
    status: i32,
}

#[derive(Deserialize, Default)]
struct LectureUpdate {
    topic: Option<String>,
    start_time: Option<serde_json::Value>,
    duration: Option<i32>,
    description: Option<String>,
    speaker_id: Option<String>,
    organizer_id: Option<String>,
    status: Option<i32>,
}

// ==================== 工具函数 ====================

async fn generate_unique_lecturecode(coll: &mongodb::Collection<Document>) -> i32 {
    loop {
        let code: i32 = {
            let mut rng = rand::thread_rng();
            rng.gen_range(100000..=999999)
        };
        if coll
            .find_one(doc! { "lecturecode": code }, None)
            .await
            .unwrap()
            .is_none()
        {
            return code;
        }
    }
}

// ==================== 路由 ====================

async fn create_lecture(
    State(client): State<AppState>,
    Json(payload): Json<LectureCreate>,
) -> Result<RespJson<Lecture>, (StatusCode, String)> {
    let coll = lecture_collection(&client);

    let topic = payload.topic;
    // 解析 ISO 字符串为 ms
    let start_time = chrono::DateTime::parse_from_rfc3339(&payload.start_time)
        .map_err(|_| (StatusCode::BAD_REQUEST, "start_time 无效".into()))?
        .timestamp_millis();
    let duration = payload.duration;
    let description = payload.description.unwrap_or_default();
    let status = payload.status;

    let speaker_id = payload
        .speaker_id
        .and_then(|s| {
            let s = s.trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        })
        .and_then(|s| ObjectId::parse_str(&s).ok().map(|oid| oid.to_hex()));
    let organizer_id = ObjectId::parse_str(&payload.organizer_id)
        .ok()
        .map(|oid| oid.to_hex())
        .ok_or((StatusCode::BAD_REQUEST, "organizer_id 无效".into()))?;

    let lecturecode = generate_unique_lecturecode(&coll).await;

    let lecture_doc = doc! {
        "topic": &topic,
        "start_time": start_time,
        "duration": duration,
        "description": &description,
        "speaker_id": speaker_id.as_ref(),
        "organizer_id": &organizer_id,
        "lecturecode": lecturecode,
        "status": status,
    };

    let result = coll
        .insert_one(lecture_doc, None)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "数据库插入失败".into()))?;

    let inserted_id = result
        .inserted_id
        .as_object_id()
        .ok_or((StatusCode::INTERNAL_SERVER_ERROR, "插入ID无效".into()))?
        .to_hex();

    Ok(RespJson(Lecture {
        id: inserted_id,
        topic,
        start_time,
        duration,
        description,
        speaker_id,
        organizer_id: Some(organizer_id),
        lecturecode,
        status,
    }))
}


// =============== 列表：按组织者 ===============
async fn list_by_organizer(
    State(client): State<AppState>,
    Path(organizer_id): Path<String>,
) -> Result<RespJson<Vec<serde_json::Value>>, (StatusCode, String)> {
    let coll = lecture_collection(&client);
    // organizer_id 存库为 hex 字符串
    let filter = doc! { "organizer_id": &organizer_id };
    let mut cursor = coll
        .find(filter, None)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?;

    let mut items = Vec::new();
    while let Some(doc) = cursor
        .try_next()
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "读取失败".into()))?
    {
        let id_hex = doc
            .get_object_id("_id")
            .map(|o| o.to_hex())
            .unwrap_or_default();
        let mut v: serde_json::Value = bson::from_document(doc)
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "序列化错误".into()))?;
        if let Some(obj) = v.as_object_mut() {
            obj.remove("_id");
            obj.insert("id".to_string(), serde_json::Value::String(id_hex));
        }
        items.push(v);
    }

    Ok(RespJson(items))
}

// =============== 列表：全部 ===============
async fn list_all(
    State(client): State<AppState>,
) -> Result<RespJson<Vec<serde_json::Value>>, (StatusCode, String)> {
    let coll = lecture_collection(&client);
    let mut cursor = coll
        .find(doc! {}, None)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?;

    let mut items = Vec::new();
    while let Some(doc) = cursor
        .try_next()
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "读取失败".into()))?
    {
        let id_hex = doc
            .get_object_id("_id")
            .map(|o| o.to_hex())
            .unwrap_or_default();
        let mut v: serde_json::Value = bson::from_document(doc)
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "序列化错误".into()))?;
        if let Some(obj) = v.as_object_mut() {
            obj.remove("_id");
            obj.insert("id".to_string(), serde_json::Value::String(id_hex));
        }
        items.push(v);
    }
    Ok(RespJson(items))
}

// =============== 详情：按 ID ===============
// async fn get_lecture(
//     State(client): State<AppState>,
//     Path(lecture_id): Path<String>,
// ) -> Result<RespJson<serde_json::Value>, (StatusCode, String)> {
//     let coll = lecture_collection(&client);
//     let oid = ObjectId::parse_str(&lecture_id)
//         .map_err(|_| (StatusCode::BAD_REQUEST, "无效的 lecture_id".into()))?;
//     let doc = coll
//         .find_one(doc! { "_id": oid }, None)
//         .await
//         .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?
//         .ok_or((StatusCode::NOT_FOUND, "Lecture not found".into()))?;

//     let mut v: serde_json::Value = bson::from_document(doc)
//         .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "序列化错误".into()))?;
//     if let Some(obj) = v.as_object_mut() {
//         // 使用真实 hex id
//         let id_hex = obj
//             .remove("_id")
//             .and_then(|oid| match oid { serde_json::Value::String(s) => Some(s), other => Some(other.to_string()) })
//             .unwrap_or(lecture_id);
//         obj.insert("id".to_string(), serde_json::Value::String(id_hex));
//     }
//     Ok(RespJson(v))
// }
async fn get_lecture(
    State(client): State<AppState>,
    Path(lecture_id): Path<String>,
) -> Result<RespJson<serde_json::Value>, (StatusCode, String)> {
    let coll = lecture_collection(&client);
    let oid = ObjectId::parse_str(&lecture_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效的 lecture_id".into()))?;
    
    let doc = coll
        .find_one(doc! { "_id": oid }, None)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?
        .ok_or((StatusCode::NOT_FOUND, "Lecture not found".into()))?;

    // 正确提取 id 为字符串
    let id_hex = oid.to_hex();

    let mut v: serde_json::Value = bson::from_document(doc)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "序列化错误".into()))?;
    
    if let Some(obj) = v.as_object_mut() {
        obj.remove("_id");  // 移除原始 _id
        obj.insert("id".to_string(), serde_json::Value::String(id_hex)); // 插入字符串 id
    }
    
    Ok(RespJson(v))
}

// =============== 更新：按 ID ===============
async fn update_lecture(
    State(client): State<AppState>,
    Path(lecture_id): Path<String>,
    Json(mut payload): Json<LectureUpdate>,
) -> Result<RespJson<serde_json::Value>, (StatusCode, String)> {
    let coll = lecture_collection(&client);
    let oid = ObjectId::parse_str(&lecture_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效的 lecture_id".into()))?;

    let mut set_doc = doc! {};
    if let Some(topic) = payload.topic.take() { set_doc.insert("topic", topic); }
    if let Some(description) = payload.description.take() { set_doc.insert("description", description); }
    if let Some(duration) = payload.duration.take() { set_doc.insert("duration", duration); }
    if let Some(status) = payload.status.take() { set_doc.insert("status", status); }
    if let Some(sid) = payload.speaker_id.take() {
        let sid = sid.trim().to_string();
        if !sid.is_empty() { set_doc.insert("speaker_id", sid); } else { set_doc.insert("speaker_id", bson::Bson::Null); }
    }
    if let Some(oid_str) = payload.organizer_id.take() {
        let oid_str = oid_str.trim().to_string();
        if !oid_str.is_empty() { set_doc.insert("organizer_id", oid_str); }
    }
    if let Some(st) = payload.start_time.take() {
        let ts_ms: i64 = match st {
            serde_json::Value::String(s) => chrono::DateTime::parse_from_rfc3339(&s)
                .map_err(|_| (StatusCode::BAD_REQUEST, "start_time 无效".into()))?
                .timestamp_millis(),
            serde_json::Value::Number(n) => n.as_i64().ok_or((StatusCode::BAD_REQUEST, "start_time 无效".into()))?,
            _ => return Err((StatusCode::BAD_REQUEST, "start_time 无效".into())),
        };
        set_doc.insert("start_time", ts_ms);
    }

    if set_doc.is_empty() { return Err((StatusCode::BAD_REQUEST, "无可更新字段".into())); }

    let result = coll
        .update_one(doc! { "_id": oid }, doc! { "$set": set_doc.clone() }, None)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "更新失败".into()))?;
    if result.matched_count == 0 { return Err((StatusCode::NOT_FOUND, "Lecture not found".into())); }

    // 返回最新
    let doc = coll
        .find_one(doc! { "_id": oid }, None)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?
        .ok_or((StatusCode::NOT_FOUND, "Lecture not found".into()))?;
    let mut v: serde_json::Value = bson::from_document(doc)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "序列化错误".into()))?;
    if let Some(obj) = v.as_object_mut() {
        obj.insert("id".to_string(), serde_json::Value::String(lecture_id));
        obj.remove("_id");
    }
    Ok(RespJson(v))
}

// =============== 删除：按 ID ===============
async fn delete_lecture(
    State(client): State<AppState>,
    Path(lecture_id): Path<String>,
) -> Result<String, (StatusCode, String)> {
    let coll = lecture_collection(&client);
    let oid = ObjectId::parse_str(&lecture_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效的 lecture_id".into()))?;
    let result = coll
        .delete_one(doc! { "_id": oid }, None)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "删除失败".into()))?;
    if result.deleted_count == 0 { return Err((StatusCode::NOT_FOUND, "Lecture not found".into())); }
    Ok(format!("Lecture with ID {} has been deleted", lecture_id))
}

// =============== 详情：按 lecturecode ===============
async fn get_by_code(
    State(client): State<AppState>,
    Path(code): Path<i32>,
) -> Result<RespJson<serde_json::Value>, (StatusCode, String)> {
    let coll = lecture_collection(&client);
    let doc = coll
        .find_one(doc! { "lecturecode": code }, None)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?
        .ok_or((StatusCode::NOT_FOUND, "Lecture not found".into()))?;
    let mut v: serde_json::Value = bson::from_document(doc)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "序列化错误".into()))?;
    if let Some(obj) = v.as_object_mut() {
        // let id_hex = obj
        //     .remove("_id")
        //     .and_then(|oid| match oid { serde_json::Value::String(s) => Some(s), other => Some(other.to_string()) })
        //     .unwrap_or_default();
        // obj.insert("id".to_string(), serde_json::Value::String(id_hex));

        // let id = match obj.get("_id") {
        //     Some(serde_json::Value::String(s)) => s.clone(),
        //     Some(other_value) => other_value.to_string(),
        //     None => "error".to_string().clone(), // 如果没有 _id，使用传入的 user_id
        // };
        // obj.insert("id".to_string(), serde_json::Value::String(id));
        // obj.remove("_id");
        let id = match obj.get("_id") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Object(map)) => {
                // 处理 MongoDB 扩展 JSON 格式: {"$oid": "xxx"}
                if let Some(serde_json::Value::String(oid_str)) = map.get("$oid") {
                    oid_str.clone()
                } else {
                    "error".to_string()
                }
            }
            Some(other) => other.to_string(),
            None => "error".to_string(),
        };
        
        obj.insert("id".to_string(), serde_json::Value::String(id));
        obj.remove("_id");
    }
    Ok(RespJson(v))
}

// =============== 按 speaker_id 查询（新增）===============
async fn get_by_speaker(
    State(client): State<AppState>,
    Path(speaker_id): Path<String>,
) -> Result<RespJson<Vec<serde_json::Value>>, (StatusCode, String)> {
    let coll = lecture_collection(&client);
    let filter = doc! { "speaker_id": &speaker_id };
    let mut cursor = coll
        .find(filter, None)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".into()))?;

    let mut items = Vec::new();
    while let Some(doc) = cursor.try_next().await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "读取失败".into()))?
    {
        let id_hex = doc.get_object_id("_id")
            .map(|o| o.to_hex())
            .unwrap_or_default();
        let mut v: serde_json::Value = bson::from_document(doc)
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "序列化错误".into()))?;
        if let Some(obj) = v.as_object_mut() {
            obj.remove("_id");
            obj.insert("id".to_string(), serde_json::Value::String(id_hex));
        }
        items.push(v);
    }

    Ok(RespJson(items))
}



// ==================== Router ====================


pub fn router() -> Router<AppState> {
    Router::new()
        .route("/create", post(create_lecture))
        .route("/by_organizer/:organizer_id", get(list_by_organizer))
        .route("/", get(list_all))
        .route("/:lecture_id", get(get_lecture))
        .route("/:lecture_id", axum::routing::put(update_lecture))
        .route("/:lecture_id", axum::routing::delete(delete_lecture))
        .route("/by_code/:code", get(get_by_code))
        .route("/by_speaker/:speaker_id", get(get_by_speaker))
}
