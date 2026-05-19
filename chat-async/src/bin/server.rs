use futures_util::sink::SinkExt;
use futures_util::stream::StreamExt;
use serde_json::json;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast::{Sender, channel}, Mutex};
use tokio_websockets::{Message, ServerBuilder, WebSocketStream};

type SharedUsers = Arc<Mutex<std::collections::HashMap<SocketAddr, String>>>;

async fn handle_connection(
    addr: SocketAddr,
    mut ws_stream: WebSocketStream<TcpStream>,
    bcast_tx: Sender<String>,
    users: SharedUsers,
    next_id: Arc<AtomicU64>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut bcast_rx = bcast_tx.subscribe();

    loop {
        tokio::select! {
            msg = ws_stream.next() => {
                let Some(msg) = msg else {
                    // connection closed — remove user and broadcast user list
                    let mut users_locked = users.lock().await;
                    users_locked.remove(&addr);
                    let users_list: Vec<String> = users_locked.values().cloned().collect();
                    let outer = json!({"messageType":"users","dataArray": users_list});
                    let _ = bcast_tx.send(outer.to_string());
                    return Ok(());
                };

                let msg = msg?;

                if let Some(text) = msg.as_text() {
                    // try parse as JSON matching the YewChat protocol
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(text) {
                        if let Some(msg_type) = val.get("messageType").and_then(|v| v.as_str()) {
                            match msg_type {
                                "register" => {
                                    let username = val.get("data").and_then(|d| d.as_str()).unwrap_or("").to_string();
                                    {
                                        let mut users_locked = users.lock().await;
                                        users_locked.insert(addr, username.clone());
                                        let users_list: Vec<String> = users_locked.values().cloned().collect();
                                        let outer = json!({"messageType":"users","dataArray": users_list});
                                        let _ = bcast_tx.send(outer.to_string());
                                    }
                                }
                                "message" => {
                                    let body = val.get("data").and_then(|d| d.as_str()).unwrap_or("");
                                    let id = next_id.fetch_add(1, Ordering::SeqCst);
                                    let sender = {
                                        let users_locked = users.lock().await;
                                        users_locked.get(&addr).cloned().unwrap_or_else(|| addr.to_string())
                                    };
                                    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
                                    let inner = json!({"id": id, "from": sender, "message": body, "time": now});
                                    let outer = json!({"messageType":"message","data": serde_json::to_string(&inner).unwrap()});
                                    let _ = bcast_tx.send(outer.to_string());
                                }
                                "reaction" => {
                                    let sender = {
                                        let users_locked = users.lock().await;
                                        users_locked.get(&addr).cloned().unwrap_or_else(|| addr.to_string())
                                    };
                                    if let Some(data_str) = val.get("data").and_then(|d| d.as_str()) {
                                        if let Ok(reaction_val) = serde_json::from_str::<serde_json::Value>(data_str) {
                                            let message_id = reaction_val.get("messageId").and_then(|v| v.as_u64()).unwrap_or(0);
                                            let emoji = reaction_val.get("emoji").and_then(|v| v.as_str()).unwrap_or("");
                                            let inner = json!({"messageId": message_id, "emoji": emoji, "from": sender});
                                            let outer = json!({"messageType":"reaction","data": serde_json::to_string(&inner).unwrap()});
                                            let _ = bcast_tx.send(outer.to_string());
                                        }
                                    }
                                }
                                "thread" => {
                                    let sender = {
                                        let users_locked = users.lock().await;
                                        users_locked.get(&addr).cloned().unwrap_or_else(|| addr.to_string())
                                    };
                                    if let Some(data_str) = val.get("data").and_then(|d| d.as_str()) {
                                        if let Ok(thread_val) = serde_json::from_str::<serde_json::Value>(data_str) {
                                            let message_id = thread_val.get("messageId").and_then(|v| v.as_u64()).unwrap_or(0);
                                            let message = thread_val.get("message").and_then(|v| v.as_str()).unwrap_or("");
                                            let inner = json!({"messageId": message_id, "from": sender, "message": message, "time": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()});
                                            let outer = json!({"messageType":"thread","data": serde_json::to_string(&inner).unwrap()});
                                            let _ = bcast_tx.send(outer.to_string());
                                        }
                                    }
                                }
                                _ => {
                                    // ignore unknown message types
                                }
                            }
                        }
                    } else {
                        // not JSON — ignore or optionally broadcast raw
                        println!("client {addr:?} sent (raw): {text}");
                    }
                }
            }
            msg = bcast_rx.recv() => {
                let Ok(message) = msg else {
                    continue;
                };

                ws_stream.send(Message::text(message)).await?;
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let (bcast_tx, _) = channel(64);
    let users: SharedUsers = Arc::new(Mutex::new(std::collections::HashMap::new()));
    let next_id = Arc::new(AtomicU64::new(1));

    let listener = TcpListener::bind("127.0.0.1:8080").await?;
    println!("listening on port 8080");

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("New connection from {addr:?}");
        let bcast_tx = bcast_tx.clone();
        let users = users.clone();
        let next_id = next_id.clone();
        tokio::spawn(async move {
            // Wrap the raw TCP stream into a websocket.
            let (_req, ws_stream) = ServerBuilder::new().accept(socket).await?;

            handle_connection(addr, ws_stream, bcast_tx, users, next_id).await
        });
    }
}
