use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use log::{trace, debug, info};

use rocket::http::Status;

use crate::db::{RedisEntity, RedisMultiplexed};
use crate::monitor::NginxMonitor;
use crate::encoder::Encoder;
use crate::config::Config;

use crate::types::{Video, VideoStatus, WsPacket};

pub async fn monitor_live_streams(db: Arc<RedisMultiplexed>, monitor: Arc<NginxMonitor>, config: Arc<Config>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    loop {
        interval.tick().await;

        let status = monitor.get_newer_than(Duration::from_secs(5)).await.unwrap();
        let src_app = status.get_application("src").unwrap();

        for (id, mut video) in Video::list(&db).await.unwrap() {
            if let VideoStatus::Live { started_timestamp, .. } = video.status {
                let live_for = SystemTime::now().duration_since(UNIX_EPOCH.checked_add(Duration::from_secs(started_timestamp)).unwrap()).unwrap();
                let stream = src_app.get_stream(&id);

                // update the viewers count
                let packet = WsPacket::UpdateViewers { viewers: db.subscribed_count(&id) };
                let _: () = redis::Cmd::publish(&id, &serde_json::to_string(&packet).unwrap())
                    .query_async(&mut db.get_multiplexed_tokio_connection().await.unwrap())
                    .await
                    .unwrap();


                debug!("Currently live: {} ~{:?}", id, live_for);
                trace!("{:#?}", stream);

                // clean dead streams or streams completed
                if live_for > Duration::from_secs(60) && (stream.is_none() || stream.unwrap().bw_in == 0) {
                    // TODO: cleanup message_invoices in redis

                    video.status = VideoStatus::Processing;
                    video.save(&db).await.unwrap();

                    let cloned_config = Arc::clone(&config);
                    let cloned_db = Arc::clone(&db);
                    tokio::spawn(async move {
                        let encoder = Encoder::new(id, &cloned_config).await.unwrap();
                        let result = encoder.encode().await.unwrap();

                        debug!("Encoding completed with result: {:?}", result);

                        video.status = VideoStatus::Published {
                            timestamp: started_timestamp,
                            duration: result.duration,
                            variants: result.variants,
                            views: 0,
                        };
                        video.save(&cloned_db).await.unwrap();
                    });
                }
            }
        }
    }
}

pub fn publish_stream(db: Arc<RedisMultiplexed>, name: String) -> Status {
    info!("Published stream {}", name);

    match Video::sync_get(&db, name).unwrap() {
        None => Status::NotFound,
        Some(mut video @ Video { status: VideoStatus::Scheduled{ .. }, .. }) => {
            video.status = VideoStatus::Live {
                started_timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
                viewers: 0,
            };
            video.sync_save(&db).unwrap();

            Status::Ok
        }
        _ => Status::NotAcceptable,
    }
}
