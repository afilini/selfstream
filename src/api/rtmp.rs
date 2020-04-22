use std::sync::Arc;

use rocket::{post, State};
use rocket::http::Status;
use rocket::request::{LenientForm, FromForm};

use crate::db::RedisMultiplexed;
use crate::types::{Video, VideoStatus};
use crate::tasks;

#[derive(Debug, FromForm)]
pub struct OnPublishForm {
    app: String,
    flashver: String,
    tcurl: String,
    addr: String,
    clientid: usize,
    call: String,
    name: String,
}

#[post("/callback/on_publish", data = "<on_publish>")]
pub fn callback_on_publish(on_publish: LenientForm<OnPublishForm>, db: State<Arc<RedisMultiplexed>>) -> Status {
    // TODO: special name to allocate a new id and redirect to it

    tasks::live_monitor::publish_stream(Arc::clone(db.inner()), on_publish.name.clone())
}


