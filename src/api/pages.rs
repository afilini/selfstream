use std::sync::Arc;

use rocket::{get, State};
use rocket::http::Status;
use rocket::response::{Redirect, Responder};
use rocket_contrib::templates::Template;

use crate::db::{RedisMultiplexed, RedisEntity};
use crate::types::{Video, VideoStatus};
use super::GlobalContext;

#[get("/")]
pub fn index(db: State<Arc<RedisMultiplexed>>) -> FullResponse {
    let context = Video::sync_list(&db).unwrap();
    Template::render("index", &context).into()
}

#[get("/watch?<v>")]
pub fn watch(db: State<Arc<RedisMultiplexed>>, globals: State<Arc<GlobalContext>>, v: String) -> FullResponse {
    match Video::sync_get(&db, v).unwrap() {
        Some(v @ Video { status: VideoStatus::Live { .. }, .. }) => Template::render("watch-live", &globals.extend(&v)).into(),
        Some(v @ Video { status: VideoStatus::Processing, .. }) => Template::render("watch-live", &globals.extend(&v)).into(),
        Some(v @ Video { status: VideoStatus::Published { .. }, .. }) => Template::render("watch-published", &globals.extend(&v)).into(),
        Some(v @ Video { status: VideoStatus::Scheduled { .. }, .. }) => Template::render("watch-live", &globals.extend(&v)).into(),
        _ => Status::NotFound.into(),
    }
}

#[derive(Debug, Responder)]
pub enum FullResponse {
    Template(Template),
    Redirect(Redirect),
    Status(Status)
}

impl From<Template> for FullResponse {
    fn from(other: Template) -> FullResponse {
        FullResponse::Template(other)
    }
}

impl From<Redirect> for FullResponse {
    fn from(other: Redirect) -> FullResponse {
        FullResponse::Redirect(other)
    }
}

impl From<Status> for FullResponse {
    fn from(other: Status) -> FullResponse {
        FullResponse::Status(other)
    }
}
