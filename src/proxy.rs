use std::collections::HashMap;
use tokio::io::AsyncWriteExt;
use tokio::io::AsyncReadExt;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::Mutex;
use std::sync::Arc;
use tokio::time::timeout;

async fn send_large_data(stream: &mut tokio::net::TcpStream, data: &[u8]) -> std::io::Result<()> {
    let chunk_size = 1024; // 1 KB chunk size
    let mut start = 0;

    // Send data in chunks
    while start < data.len() {
        let end = std::cmp::min(start + chunk_size, data.len());
        let chunk = &data[start..end];
        stream.write_all(chunk).await?;  // Write each chunk
        start = end;
    }

    Ok(())
}

async fn receive_large_data(stream: &mut tokio::net::TcpStream) -> std::io::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut chunk = vec![0; 1024]; // Buffer to read in chunks

    loop {
        let bytes_read = stream.read(&mut chunk).await?;
        if bytes_read == 0 {
            // No more data to read
            break;
        }
        buffer.extend_from_slice(&chunk[..bytes_read]);
        if bytes_read < 1024 {
            break; // End of data, since we've received fewer bytes than a full chunk
        }
    }

    Ok(buffer)
}

pub async fn distribute_proxy(request: &str, result: &mut String, proxies: &Vec<String>) {
    let responses = Arc::new(Mutex::new(HashMap::<String, usize>::new()));
    let mut handles = vec![];

    for proxy in proxies.iter().cloned() {
        let responses = Arc::clone(&responses);
        let request = request.to_string();
        
        let handle = tokio::spawn(async move {
            let proxy_addr: SocketAddr = match proxy.parse() {
                Ok(addr) => addr,
                Err(e) => {
                    eprintln!("Invalid proxy address {}: {}", proxy, e);
                    return;
                }
            };

            let mut stream = match timeout(Duration::from_millis(500), tokio::net::TcpStream::connect(proxy_addr)).await {
                Ok(Ok(stream)) => stream,  // Both timeout and connection succeed
                Ok(Err(e)) => {  // Connection failed but within timeout
                    eprintln!("Failed to connect to proxy {}: {}", proxy, e);
                    return;
                }
                Err(e) => {  // Timeout occurred
                    eprintln!("Connection to proxy {} timed out: {}", proxy, e);
                    return;
                }
            };

            if let Err(e) = send_large_data(&mut stream, request.as_bytes()).await {
                eprintln!("Failed to send data to proxy {}: {}", proxy, e);
                return;
            }

            let received_data = match receive_large_data(&mut stream).await {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("Failed to receive data from proxy {}: {}", proxy, e);
                    return;
                }
            };

            let response = match String::from_utf8(received_data) {
                Ok(response) => response,
                Err(e) => {
                    eprintln!("Failed to convert data to UTF-8 for proxy {}: {}", proxy, e);
                    return;
                }
            };

            let mut responses = responses.lock().await;
            let counter = responses.entry(response.clone()).or_insert(0);
            *counter += 1;
        });

        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    let responses = responses.lock().await;
    let mut best_count: usize = 0;
    for (key, &count) in responses.iter() {
        if count > best_count {
            best_count = count;
            *result = key.clone();
        }
    }
}
