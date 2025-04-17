use axum::{
    extract::{Path, Query, Json, State},
    http::StatusCode,
    response::Json as ResponseJson,
    routing::get,
    Router,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::{CorsLayer, Any};
use axum_server::tls_rustls::RustlsConfig;
use axum_server::Server;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::io::{AsyncWriteExt, AsyncBufReadExt, BufReader};

struct AppState {
    daemon_addr: String,
    connection: Mutex<TcpStream>,
}

async fn distribute(request: &str, state: &State<Arc<AppState>>) -> String {
    let mut result = String::from("{}");

    let mut stream = state.connection.lock().await;

    // Append packet end delimiter
    let message = request;

    // Send request
    if let Err(e) = stream.write_all(message.as_bytes()).await {
        eprintln!("Failed to send request: {}", e);
        return result;
    }

    // Create a buffered reader from the stream
    let mut reader = BufReader::new(&mut *stream);

    // Read until packet end delimiter
    let mut buf = Vec::new();
    if let Err(e) = reader.read_until(0x1E, &mut buf).await {
        eprintln!("Failed to read response: {}", e);
        return result;
    }

    // Remove the delimiter before parsing
    if buf.ends_with(&[0x1E]) {
        buf.pop();
    }

    result = String::from_utf8_lossy(&buf).to_string();
    result
}

async fn handle_get(
    Path(path): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    state: State<Arc<AppState>>,
) -> (StatusCode, ResponseJson<Value>) {
    let mut response = json!({
        "endpoint": path,
        "data": params,
    });

    if let Some(last) = path.split('/').last() {
        if last.parse::<i32>().is_ok() {
            response = json!({
                "endpoint": "coin",
                "data": { "id": last },
            });
        }
    }

    if path == "daemons" {
        return (StatusCode::OK, ResponseJson(json!({
            "daemons": [state.daemon_addr.clone()]
        })));
    }
    let response_str = serde_json::to_string(&response).unwrap();
    println!("{}", response_str);
    let result = distribute(&response_str, &state).await;

    match serde_json::from_str(&result) {
        Ok(val) => (StatusCode::OK, ResponseJson(val)),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, ResponseJson(json!({"error": "Invalid daemon response"}))),
    }
}

async fn handle_post(
    Path(path): Path<String>,
    state: State<Arc<AppState>>,
    Json(data): Json<Value>,
) -> (StatusCode, ResponseJson<Value>) {
    let mut response = json!({
        "endpoint": path,
        "data": data,
    });

    if let Some(last) = path.split('/').last() {
        if last.parse::<i32>().is_ok() {
            response = json!({
                "endpoint": "coin",
                "data": { "id": last },
            });
        }
    }

    let response_str = serde_json::to_string(&response).unwrap();
    let result = distribute(&response_str, &state).await;

    match serde_json::from_str(&result) {
        Ok(val) => (StatusCode::OK, ResponseJson(val)),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, ResponseJson(json!({"error": "Invalid daemon response"}))),
    }
}

pub async fn api(use_https: bool) {
    let daemon_addr = String::from("clc.ix.tc:6061");

    let stream = TcpStream::connect(&daemon_addr).await.expect("Failed to connect to daemon");

    let state = Arc::new(AppState {
        daemon_addr: daemon_addr.clone(),
        connection: Mutex::new(stream),
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/{*path}", get(handle_get).post(handle_post))
        .with_state(state)
        .layer(cors);

    let addr = SocketAddr::from(([0, 0, 0, 0], 7070));

    if use_https {
        let config = RustlsConfig::from_pem_file(
            "/etc/letsencrypt/live/clc.ix.tc/cert.pem",
            "/etc/letsencrypt/live/clc.ix.tc/privkey.pem"
        ).await.unwrap();

        axum_server::bind_rustls(addr, config)
            .serve(app.into_make_service())
            .await
            .unwrap();
    } else {
        Server::bind(addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
    }
}
