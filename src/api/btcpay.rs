use std::sync::Arc;

use log::debug;

use serde::Deserialize;

use rocket::{post, State};
use rocket::http::Status;
use rocket::request::{LenientForm, FromForm};
use rocket_contrib::json::Json;

use btcpay::{Invoice, InvoiceStatus};

use crate::db::{RedisMultiplexed, RedisEntity};
use crate::types::{BoostMessageInvoice, WsPacket, MessageExtra};
use crate::tasks;

#[derive(Debug, Deserialize)]
pub struct WebhookData {
    event: WebhookEvent,
    data: Invoice,
}

#[derive(Debug, Deserialize)]
pub struct WebhookEvent {
    code: usize,
    name: String,
}

#[post("/btcpay_webhook", data = "<input>")]
pub fn webhook(input: Json<WebhookData>, db: State<Arc<RedisMultiplexed>>) -> Status {
    // TODO: should check with the server

    match input.data.status {
        InvoiceStatus::Paid | InvoiceStatus::Completed | InvoiceStatus::Confirmed => {
            // TODO: nice TOCTOU here :)
            if let Some(invoice) = BoostMessageInvoice::sync_get(&db, input.data.id.clone()).unwrap() {
                // remove invoice
                invoice.sync_del(&db).unwrap();

                let amount = (input.data.btc_paid.parse::<f32>().unwrap() * 1e8) as u64;
                let duration = match amount {
                    0..=1000 => 20,
                    0..=10000 => 30,
                    0..=25000 => 60,
                    0..=50000 => 100,
                    _ => 120,
                };

                // publish message
                let extra = MessageExtra { amount, timestamp: 0, duration };
                let packet = WsPacket::ServerMessage { from: invoice.from, message: invoice.message, extra: Some(extra) };
                let _: () = redis::Cmd::publish(invoice.room, &serde_json::to_string(&packet).unwrap())
                    .query(&mut db.get_connection().unwrap())
                    .unwrap();
            }
        },
        _ => {},
    }

    Status::Ok
}
