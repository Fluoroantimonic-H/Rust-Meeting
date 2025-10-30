// src/routes/user.rs
use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, put},
    Router,
};
use bcrypt::{hash, verify, DEFAULT_COST};
use bson::{doc, oid::ObjectId, Document};
use futures_util::stream::StreamExt;
use mongodb::Client;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// use crate::db::USER_COLLECTION;
use crate::db::user_collection;

// 共享状态
type AppState = Arc<Client>;

// ==================== Pydantic 模型 → Rust Structs ====================

#[derive(Deserialize)]
struct UserCreate {
    username: String,
    email: String,
    password: String,
    role: i32,
}

#[derive(Deserialize)]
struct UserLogin {
    email: String,
    password: String,
}

#[derive(Deserialize, Default)]
struct UserUpdate {
    username: Option<String>,
    gender: Option<i32>,
    age: Option<i32>,
    motto: Option<String>,
}

// ==================== 工具函数 ====================

fn hash_password(password: &str) -> Result<String, StatusCode> {
    hash(password, DEFAULT_COST).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn verify_password(plain: &str, hashed: &str) -> Result<bool, StatusCode> {
    verify(plain, hashed).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn validate_email(email: &str) -> bool {
    let re = Regex::new(r"^[a-zA-Z0-9_.+-]+@[a-zA-Z0-9-]+\.[a-zA-Z0-9-.]+$").unwrap();
    re.is_match(email)
}

// ==================== 路由函数 ====================

async fn register(
    State(client): State<AppState>,
    Json(payload): Json<UserCreate>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let collection = user_collection(&client);

    // 校验邮箱格式
    if !validate_email(&payload.email) {
        return Err((StatusCode::BAD_REQUEST, "Invalid email format".to_string()));
    }

    // 校验用户名/邮箱是否重复
    if collection.find_one(doc! { "username": &payload.username }, None).await.unwrap().is_some() {
        return Err((StatusCode::BAD_REQUEST, "用户名已被使用".to_string()));
    }
    if collection.find_one(doc! { "email": &payload.email }, None).await.unwrap().is_some() {
        return Err((StatusCode::BAD_REQUEST, "邮箱已被注册".to_string()));
    }

    let hashed = hash_password(&payload.password).map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "密码加密失败".to_string())
    })?;

    let user_doc = doc! {
        "username": &payload.username,
        "email": &payload.email,
        "password": hashed,
        "role": payload.role,
        "avatar": "/static/uploads/ad08e97b84354e6b9720e877072f28c4.png",
        "background": "/static/uploads/aa486fc11bd94ab3bd9ef02baa48e357.jpg",
    };

    collection.insert_one(user_doc, None).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "数据库错误".to_string()))?;

    Ok(Json(serde_json::json!({
        "message": "User successfully created",
        "username": payload.username
    })))
}

async fn login(
    State(client): State<AppState>,
    Json(payload): Json<UserLogin>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let collection = user_collection(&client);

    let user = collection.find_one(doc! { "email": &payload.email }, None).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "数据库错误".to_string()))?
        .ok_or((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()))?;

    let hashed = user.get_str("password").map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "密码字段缺失".to_string())
    })?;

    if !verify_password(&payload.password, hashed).map_err(|_| {
        (StatusCode::INTERNAL_SERVER_ERROR, "密码验证失败".to_string())
    })? {
        return Err((StatusCode::UNAUTHORIZED, "Invalid credentials".to_string()));
    }

    let id = user.get_object_id("_id").unwrap().to_hex();

    Ok(Json(serde_json::json!({
        "message": "Login successful",
        "user": {
            "id": id,
            "email": payload.email,
            "username": user.get_str("username").unwrap_or(""),
            "role": user.get_i32("role").unwrap_or(0),
        }
    })))
}

async fn get_all_users(
    State(client): State<AppState>,
) -> Result<Json<Vec<serde_json::Value>>, (StatusCode, String)> {
    let collection = user_collection(&client);

    let mut cursor = collection.find(doc! {}, None).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".to_string()))?;

    let mut users = Vec::new();
    while let Some(result) = cursor.next().await {
        let mut doc = result.map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "读取错误".to_string()))?;
        doc.remove("password");
        let id = doc.get_object_id("_id").unwrap().to_hex();
        doc.insert("id", id);
        doc.remove("_id");
        users.push(serde_json::to_value(doc).unwrap());
    }

    Ok(Json(users))
}

async fn get_user(
    State(client): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let collection = user_collection(&client);

    let obj_id = ObjectId::parse_str(&user_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效的用户ID".to_string()))?;

    let user = collection.find_one(doc! { "_id": obj_id }, None).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "用户未找到".to_string()))?;

    let mut user: serde_json::Value = bson::from_document(user)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "序列化错误".to_string()))?;

    let obj = user.as_object_mut().unwrap();
    obj.remove("password");
    
    
    // let id = obj.get("_id").unwrap().as_str().unwrap().to_string(); // _id 已经是 hex 字符串
    // obj.insert("id".to_string(), serde_json::Value::String(id));
    // obj.remove("_id");
    
    // 替换有问题的部分：
    let id = match obj.get("_id") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(other_value) => other_value.to_string(),
        None => user_id.clone(), // 如果没有 _id，使用传入的 user_id
    };
    obj.insert("id".to_string(), serde_json::Value::String(id));
    obj.remove("_id");

    Ok(Json(user))
}

const UPLOAD_DIR: &str = "static/uploads";

async fn update_user_with_files(
    State(client): State<AppState>,
    Path(user_id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let collection = user_collection(&client);

    let obj_id = ObjectId::parse_str(&user_id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "无效的用户ID".to_string()))?;

    let db_user = collection.find_one(doc! { "_id": obj_id }, None).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "查询失败".to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "用户未找到".to_string()))?;

    let mut update_data = doc! {};
    let mut paths = doc! { "avatar": null, "background": null };

    let current_username = db_user.get_str("username").ok().map(|s| s.to_string());

    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "username" => {
                let username = field.text().await.unwrap_or_default();
                if username.is_empty() {
                    return Err((StatusCode::BAD_REQUEST, "用户名不能为空".to_string()));
                }
                if Some(&username) != current_username.as_ref() {
                    if collection.find_one(doc! { "username": &username }, None).await.unwrap().is_some() {
                        return Err((StatusCode::BAD_REQUEST, "用户名已被使用".to_string()));
                    }
                }
                update_data.insert("username", username);
            }
            "gender" => {
                if let Ok(text) = field.text().await {
                    if let Ok(g) = text.parse::<i32>() {
                        update_data.insert("gender", g);
                    }
                }
            }
            "age" => {
                if let Ok(text) = field.text().await {
                    if let Ok(a) = text.parse::<i32>() {
                        update_data.insert("age", a);
                    }
                }
            }
            "motto" => {
                let motto = field.text().await.unwrap_or_default();
                if !motto.is_empty() {
                    update_data.insert("motto", motto);
                }
            }
            "avatar" | "background" => {
                let filename = field.file_name().unwrap_or("unknown").to_string();
                let ext = std::path::Path::new(&filename)
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                let new_filename = format!("{}{}", Uuid::new_v4().to_string(), ext);
                let path = format!("{}/{}", UPLOAD_DIR, new_filename);

                let mut file = std::fs::File::create(&path)
                    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "无法保存文件".to_string()))?;
                let bytes = field.bytes().await
                    .map_err(|_| (StatusCode::BAD_REQUEST, "读取文件失败".to_string()))?;
                std::io::copy(&mut bytes.as_ref(), &mut file)
                    .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "写入文件失败".to_string()))?;

                let url = format!("/static/uploads/{}", new_filename);
                if name == "avatar" {
                    update_data.insert("avatar", &url);
                    paths.insert("avatar", url);
                } else {
                    update_data.insert("background", &url);
                    paths.insert("background", url);
                }
            }
            _ => {}
        }
    }

    if update_data.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "没有可更新的字段".to_string()));
    }

    collection.update_one(doc! { "_id": obj_id }, doc! { "$set": update_data.clone() }, None).await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "更新失败".to_string()))?;

    Ok(Json(serde_json::json!({
        "message": "用户信息已更新",
        "updated_fields": update_data.keys().collect::<Vec<_>>(),
        "paths": paths
    })))
}

// ==================== Router ====================

pub fn router() -> Router<AppState> {
    std::fs::create_dir_all(UPLOAD_DIR).expect("无法创建上传目录");

    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/", get(get_all_users))
        .route("/:user_id", get(get_user))
        .route("/update/:user_id", put(update_user_with_files))
}

