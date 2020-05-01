use std::ops::Deref;
use std::sync::Arc;

use rocket::routes;
use rocket_contrib::templates::handlebars::handlebars_helper;
use rocket_contrib::templates::Template;

use crate::config::Config;
use crate::db::RedisMultiplexed;

mod btcpay;
mod pages;
mod rtmp;

fn json_merge(a: &mut serde_json::Value, b: &serde_json::Value) {
    match (a, b) {
        (&mut serde_json::Value::Object(ref mut a), &serde_json::Value::Object(ref b)) => {
            for (k, v) in b {
                json_merge(a.entry(k.clone()).or_insert(serde_json::Value::Null), v);
            }
        }
        (a, b) => {
            *a = b.clone();
        }
    }
}

pub struct GlobalContext(serde_json::Value);

impl GlobalContext {
    fn new<T: serde::Serialize>(globals: &T) -> Self {
        let json = serde_json::to_value(globals).unwrap();

        GlobalContext(json)
    }

    fn extend<T: serde::Serialize>(&self, data: &T) -> serde_json::Value {
        let mut a = self.0.clone();
        let b = serde_json::to_value(data).unwrap();

        json_merge(&mut a, &b);

        a
    }
}

handlebars_helper!(streq: |x: str, y: str| x == y);

pub fn start(db: Arc<RedisMultiplexed>, config: Arc<Config>) {
    rocket::ignite()
        .manage(db)
        .manage(Arc::new(GlobalContext::new(config.deref())))
        .mount(
            "/",
            routes![
                pages::index,
                pages::watch,
                rtmp::callback_on_publish,
                btcpay::webhook,
            ],
        )
        .attach(Template::custom(|engines| {
            engines.handlebars.register_helper("streq", Box::new(streq));
        }))
        .launch();
}
