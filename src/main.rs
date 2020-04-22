#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use]
extern crate async_trait;

use std::env;
use std::sync::Arc;

use tokio::task;

use btcpay::*;

mod api;
mod db;
mod monitor;
mod encoder;
mod probe;
mod types;
mod ws;
mod tasks;
mod config;

#[tokio::main]
async fn main() {
    env_logger::init();

    let config = config::Config::new().await.unwrap();
    let config = Arc::new(config);

    let db = db::RedisMultiplexed::new(config.redis_server.as_str()).unwrap();
    let db = Arc::new(db);

    let nginx_monitor = Arc::new(monitor::NginxMonitor::new(&config.stat_url));

    let cloned_db = db.clone();
    std::thread::spawn(move || {
        cloned_db.mainloop().unwrap();
    });

    let cloned_config = config.clone();
    let cloned_db = db.clone();
    std::thread::spawn(move || {
        api::start(cloned_db, cloned_config);
    });

    let cloned_config = config.clone();
    let cloned_db = db.clone();
    let cloned_monitor = nginx_monitor.clone();
    task::spawn(async move {
        tasks::live_monitor::monitor_live_streams(cloned_db, cloned_monitor, cloned_config).await;
    });

    let btcpay_key = SecretKey::from_slice(
        &Vec::<u8>::from_hex(
            &config.btcpay.key
        )
        .unwrap(),
    )
    .unwrap();
    let btcpay_keypair: KeyPair = btcpay_key.into();

    let btcpay_client = BTCPayClient::new(
        &config.btcpay.url,
        btcpay_keypair,
        Some(&config.btcpay.merchant),
    )
    .unwrap();

    ws::start(&config.listen, db.clone(), Arc::new(btcpay_client), config.clone()).await;
}
