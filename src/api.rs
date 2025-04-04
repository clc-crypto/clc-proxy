use axum::{
    extract::{Path, Query, Json, State},
    http::StatusCode,
    response::Json as ResponseJson,
    routing::get,
    Router,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use tower_http::cors::{CorsLayer, Any};
use std::sync::Arc;
use axum_server::tls_rustls::RustlsConfig;
use std::net::SocketAddr;

use crate::proxy;

async fn distribute(request: &str, state: State<Arc<AppState>>) -> String {
    let mut result = String::from("{}");
    proxy::distribute_proxy(request, &mut result, &state.proxies).await;
    result
}

async fn handle_get(Path(path): Path<String>, Query(params): Query<HashMap<String, String>>, state: State<Arc<AppState>>) -> (StatusCode, ResponseJson<Value>) {
    let mut response = serde_json::to_string(&json!({
        "endpoint": path,
        "data": params,
    })).unwrap();
    if path.split("/").last().is_some() && path.split("/").last().unwrap().parse::<i32>().is_ok() { // Handle depricated /coin/<id>
        response = serde_json::to_string(&json!({
            "endpoint": "coin",
            "data": {
                "id": path.split("/").last()
            },
        })).unwrap();
    }
    if path == "daemons" {
        return (StatusCode::OK, ResponseJson(json!({
            "daemons": state.proxies
        })));
    }
    let result = distribute(&response, state).await;
    (StatusCode::OK, ResponseJson(serde_json::from_str(&result).expect("Error parsing daemon response")))
}

async fn handle_post(Path(path): Path<String>, state: State<Arc<AppState>>, Json(data): Json<Value>) -> (StatusCode, ResponseJson<Value>) {
    let mut response = serde_json::to_string(&json!({
        "endpoint": path,
        "data": data,
    })).unwrap();
    if path.split("/").last().is_some() && path.split("/").last().unwrap().parse::<i32>().is_ok() { // Handle depricated /coin/<id>
        response = serde_json::to_string(&json!({
            "endpoint": "coin",
            "data": {
                "id": path.split("/").last()
            },
        })).unwrap();
    }
    let result = distribute(&response, state).await;
    (StatusCode::OK, ResponseJson(serde_json::from_str(&result).expect("err")))
}

struct AppState {
    proxies: Vec<String>
}

pub async fn api() {
    let state = Arc::new(AppState {
        proxies: vec![String::from("174.138.101.187:6061")]
    });

    // Create a CORS layer that allows all origins, methods, and headers
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let config = RustlsConfig::from_pem_file(
        "/etc/letsencrypt/live/clc.ix.tc/cert.pem",
        "/etc/letsencrypt/live/clc.ix.tc/privkey.pem"
    ).await.unwrap();

    // Build our application with two routes, one for GET and one for POST
    let app = Router::new()
        .route("/{*path}", get(handle_get).post(handle_post))
        .with_state(state)
        .layer(cors);

        let addr = SocketAddr::from(([0, 0, 0, 0], 7070));
    axum_server::bind_rustls(addr, config)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
