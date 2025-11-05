use axum::{
    routing::{get, get_service},
    Router,
    response::{Redirect},
    http::StatusCode,
};
use std::net::SocketAddr;
use tower_http::{
    services::ServeDir,
    cors::{CorsLayer, Any},
    normalize_path::NormalizePathLayer,
};

mod db;
mod routes;

use crate::db::get_db;
use routes::{
    user, lecture, invitation, feedback, la, discussion,
};

#[tokio::main]
async fn main() {
    // 获取 MongoDB 客户端（Arc<Client>）
    let client = get_db().await;

    // 静态文件服务：/static/* → ./static/*
    let static_files_service = get_service(ServeDir::new("static"))
        .handle_error(|error| async move {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("文件加载错误: {}", error),
            )
        });

    // 构建路由
    let app = Router::new()
        // === API 路由 ===
        .nest("/user", user::router())
        .nest("/lecture", lecture::router())
        .nest("/invitation", invitation::router())
        .nest("/feedback", feedback::router())
        .nest("/LA", la::router())
        .nest("/discussion", discussion::router())

        // === 首页重定向 ===
        .route("/", get(|| async { Redirect::to("/static/login.html") }))

        // === 静态资源 ===
        .nest_service("/static", static_files_service)

        // === 中间件 ===
        .layer(NormalizePathLayer::trim_trailing_slash())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)     // 开发环境允许所有来源
                .allow_methods(Any)
                .allow_headers(Any),
        )

        // === 注入共享状态（MongoDB Client）===
        .with_state(client);

    // 启动服务器
    let addr = SocketAddr::from(([127, 0, 0, 1], 8000));
    println!("服务器已启动: http://{}", addr);

    axum::serve(
        tokio::net::TcpListener::bind(addr).await.unwrap(),
        app,
    )
    .await
    .unwrap();
}