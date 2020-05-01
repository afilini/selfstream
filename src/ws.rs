use std::error;
use std::net::SocketAddr;
use std::sync::Arc;

use rand::Rng;

use log::{debug, info, trace};

use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use futures::{SinkExt, StreamExt};

use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::protocol::Message;

use btcpay::*;

use crate::config::Config;
use crate::db::{RedisEntity, RedisMultiplexed};
use crate::types::{BoostMessageInvoice, Video, VideoStatus, WsPacket};

#[derive(Debug, Default)]
struct State {
    room: Option<String>,
}

#[derive(Debug)]
enum Action {
    None,
    Subscribe(String),
    Broadcast(String),
    CreateInvoice(u64, String),
    // CheckInvoice(String, String),
}

impl State {
    async fn apply(
        mut self,
        msg: WsPacket,
        db: Arc<RedisMultiplexed>,
    ) -> Result<(Self, Action), MyError> {
        debug!("State: {:?} applying: {:?}", self, msg);

        match (&mut self.room, &msg) {
            (ref mut self_room @ None, WsPacket::Join { room }) => {
                match Video::get(&db, room.clone()).await? {
                    Some(Video {
                        status: VideoStatus::Live { .. },
                        ..
                    })
                    | Some(Video {
                        status: VideoStatus::Scheduled { .. },
                        ..
                    }) => {
                        **self_room = Some(room.clone());
                        return Ok((self, Action::Subscribe(room.clone())));
                    }
                    _ => return Err(MyError::empty()),
                }
            }
            (None, _) => return Err(MyError::empty()),
            _ => {}
        }

        match msg {
            WsPacket::ClientMessage { message } if message.len() > 0 => {
                return Ok((self, Action::Broadcast(message)));
            }
            WsPacket::GetInvoice { amount, message } => {
                return Ok((self, Action::CreateInvoice(amount, message)));
            }
            _ => {}
        }

        Err(MyError::empty())
    }
}

#[derive(Debug)]
struct MyError {
    err: Box<dyn Send + std::fmt::Debug>,
}

impl MyError {
    fn empty() -> Self {
        MyError { err: Box::new(()) }
    }
}

impl<T: 'static + error::Error + Send> From<T> for MyError {
    fn from(other: T) -> Self {
        MyError {
            err: Box::new(other),
        }
    }
}

async fn handle_connection(
    db: Arc<RedisMultiplexed>,
    btcpay_client: Arc<BTCPayClient>,
    config: Arc<Config>,
    raw_stream: TcpStream,
    addr: SocketAddr,
) -> Result<(), MyError> {
    let id = format!("Anon{}", rand::thread_rng().gen::<u16>());
    let mut state = State::default();

    debug!("Incoming TCP connection from: {}. ID: {}", addr, id);

    let ws_stream = tokio_tungstenite::accept_async(raw_stream)
        .await
        .expect("Error during the websocket handshake occurred");
    debug!("WebSocket connection established: {}", addr);

    let mut connection = db.get_multiplexed_tokio_connection().await?;

    let (outgoing, mut incoming) = ws_stream.split();
    let (unbounded_tx, unbounded_rx) = unbounded::<Message>();
    tokio::spawn(unbounded_rx.map(Result::Ok).forward(outgoing));
    let mut outgoing = unbounded_tx;

    let broadcast_incoming = async {
        while let Some(Ok(msg)) = incoming.next().await {
            trace!("Received a message from {}: {}", addr, msg.to_text()?);

            let msg: WsPacket = serde_json::from_str(msg.to_text()?)?;
            let resp = state.apply(msg, db.clone()).await?;
            state = resp.0;

            match resp.1 {
                Action::None => {}
                Action::Subscribe(room) => {
                    let rx = db.subscribe(&id, &room);

                    let packet = WsPacket::AssignedUsername {
                        username: id.clone(),
                    };
                    outgoing
                        .send(Message::Text(serde_json::to_string(&packet)?))
                        .await?;

                    let receive_from_others = rx
                        .map(|msg| Ok(Message::Text(msg)))
                        .forward(outgoing.clone());
                    tokio::spawn(receive_from_others);
                }
                Action::Broadcast(message) => {
                    let packet = WsPacket::ServerMessage {
                        from: id.clone(),
                        message,
                        extra: None,
                    };
                    let _: () = redis::Cmd::publish(
                        state.room.as_ref().ok_or(MyError::empty())?,
                        &serde_json::to_string(&packet)?,
                    )
                    .query_async(&mut connection)
                    .await?;
                }
                Action::CreateInvoice(amount, message) => {
                    let amount_float = amount as f32 / 1e8;
                    let invoice = btcpay_client
                        .create_invoice(CreateInvoiceArgs {
                            currency: "BTC".to_string(),
                            price: amount_float,
                            notification_url: Some(config.btcpay.webhook.clone()),
                            full_notifications: Some(true),
                            extended_notifications: Some(true),
                            ..Default::default()
                        })
                        .await?;

                    let webhook_data = BoostMessageInvoice {
                        id: invoice.id.clone(),
                        message,
                        from: id.clone(),
                        room: state.room.clone().ok_or(MyError::empty())?,
                    };
                    webhook_data.save(&db).await?;

                    let packet = WsPacket::Invoice { id: invoice.id };
                    outgoing
                        .send(Message::Text(serde_json::to_string(&packet)?))
                        .await?;
                }
            }
        }

        Ok::<(), MyError>(())
    };
    if let Err(e) = broadcast_incoming.await {
        debug!("Error: {:?}", e);
    }

    debug!("{} disconnected", &addr);

    db.remove(&id);

    Ok(())
}

pub async fn start<S: tokio::net::ToSocketAddrs + std::fmt::Display>(
    addr: S,
    db: Arc<RedisMultiplexed>,
    btcpay_client: Arc<BTCPayClient>,
    config: Arc<Config>,
) {
    let try_socket = TcpListener::bind(&addr).await;
    let mut listener = try_socket.expect("Failed to bind");
    info!("WebSocket Listening on: {}", addr);

    while let Ok((stream, addr)) = listener.accept().await {
        tokio::spawn(handle_connection(
            db.clone(),
            btcpay_client.clone(),
            config.clone(),
            stream,
            addr,
        ));
    }
}
