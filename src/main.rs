mod api;
mod proxy;

#[tokio::main]
async fn main() {
    api::api(false).await;
}