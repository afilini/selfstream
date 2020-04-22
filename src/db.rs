use std::collections::{HashMap, HashSet};
use std::ops::DerefMut;
use std::sync::{Mutex, RwLock};

use log::debug;

use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};

use redis::{Client, Cmd, IntoConnectionInfo, RedisResult};

type Tx = UnboundedSender<String>;
type Rx = UnboundedReceiver<String>;

pub struct RedisMultiplexed {
    client: Client,

    subscriber_client: Mutex<Client>,
    buckets: RwLock<HashMap<String, HashMap<String, Tx>>>,
}

impl RedisMultiplexed {
    pub fn new<I: IntoConnectionInfo + std::clone::Clone>(params: I) -> RedisResult<Self> {
        let client = Client::open(params.clone())?;
        let subscriber_client = Mutex::new(Client::open(params.clone())?);

        Ok(RedisMultiplexed {
            client,

            subscriber_client,
            buckets: RwLock::new(HashMap::new()),
        })
    }

    pub fn subscribe(&self, id: &str, channel: &str) -> Rx {
        let mut buckets = self.buckets.write().unwrap();

        if !buckets.contains_key(channel) {
            buckets.insert(channel.to_string(), HashMap::new());
        }

        let (tx, rx) = unbounded();
        buckets.get_mut(channel).unwrap().insert(id.to_string(), tx);

        rx
    }

    pub fn remove(&self, id: &str) {
        for (_, channel) in self.buckets.write().unwrap().deref_mut() {
            channel.retain(|c_id, _| c_id != &id);
        }
    }

    pub fn subscribed_count(&self, channel: &str) -> usize {
        self.buckets.read().unwrap().get(channel).map(HashMap::len).unwrap_or(0)
    }

    pub fn mainloop(&self) -> RedisResult<()> {
        let redis_client = self.subscriber_client.lock().unwrap();
        let mut connection = redis_client.get_connection()?;
        let mut pubsub = connection.as_pubsub();

        let mut currently_subscribed: HashSet<String> = HashSet::new();
        pubsub.set_read_timeout(Some(std::time::Duration::from_millis(1000)))?;

        loop {
            for (channel, _) in self
                .buckets
                .read()
                .unwrap()
                .iter()
                .filter(|(channel, clients)| {
                    !clients.is_empty() && !currently_subscribed.contains(channel.clone())
                })
                .collect::<Vec<_>>()
            {
                debug!("Subscribing to {:?}", channel);

                pubsub.subscribe(channel)?;
                currently_subscribed.insert(channel.clone());
            }

            if let Ok(msg) = pubsub.get_message() {
                let payload: String = msg.get_payload()?;
                let channel = msg.get_channel_name();
                let mut to_remove = Vec::new();

                match self.buckets.read().unwrap().get(channel) {
                    Some(clients) if clients.is_empty() => {
                        debug!("Unsubscribing from {:?}", channel);

                        pubsub.unsubscribe(channel)?;
                        currently_subscribed.remove(channel);
                    }
                    Some(clients) => {
                        for (key, tx) in clients {
                            if let Err(_) = tx.unbounded_send(payload.clone()) {
                                to_remove.push(key.clone());
                            }
                        }
                    }
                    None => {}
                }

                for id in to_remove {
                    self.remove(&id);
                }
            }
        }
    }
}

impl std::ops::Deref for RedisMultiplexed {
    type Target = Client;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

#[derive(Debug)]
pub enum RedisFetchError {
    Redis(redis::RedisError),
    JSON(serde_json::Error),
}

impl std::fmt::Display for RedisFetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for RedisFetchError {
}

impl From<redis::RedisError> for RedisFetchError {
    fn from(other: redis::RedisError) -> Self {
        RedisFetchError::Redis(other)
    }
}

impl From<serde_json::Error> for RedisFetchError {
    fn from(other: serde_json::Error) -> Self {
        RedisFetchError::JSON(other)
    }
}

#[async_trait]
pub trait RedisEntity
where
    Self: Clone + Sized + serde::de::DeserializeOwned + serde::Serialize,
    Self::Id: Eq + Send + Clone + std::hash::Hash + redis::ToRedisArgs + redis::FromRedisValue,
{
    type Id;

    fn key() -> &'static str;
    fn id(&self) -> &Self::Id;

    fn list_cmd() -> Cmd {
        Cmd::hgetall(Self::key())
    }

    fn get_cmd(id: Self::Id) -> Cmd {
        Cmd::hget(Self::key(), id)
    }

    fn save_cmd(&self) -> Result<Cmd, RedisFetchError> {
        Ok(Cmd::hset(
            Self::key(),
            self.id().clone(),
            serde_json::to_string(self)?,
        ))
    }

    fn del_cmd(&self) -> Cmd {
        Cmd::hdel(Self::key(), self.id().clone())
    }

    fn sync_list(client: &RedisMultiplexed) -> Result<HashMap<Self::Id, Self>, RedisFetchError> {
        let mut con = client.get_connection()?;
        let data: HashMap<Self::Id, String> = Self::list_cmd().query(&mut con)?;

        Ok(data
            .into_iter()
            .map(|(key, data)| -> Result<_, RedisFetchError> {
                Ok((key, serde_json::from_str(&data)?))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .collect::<HashMap<_, _>>())
    }

    fn sync_get(client: &RedisMultiplexed, id: Self::Id) -> Result<Option<Self>, RedisFetchError> {
        let mut con = client.get_connection()?;
        let data: Option<String> = Self::get_cmd(id).query(&mut con)?;

        Ok(data
           .and_then(|data| Some(serde_json::from_str(&data)))
           .transpose()?)
    }

    fn sync_save(&self, client: &RedisMultiplexed) -> Result<usize, RedisFetchError> {
        let mut con = client.get_connection()?;

        Ok(self.save_cmd()?.query(&mut con)?)
    }

    fn sync_del(&self, client: &RedisMultiplexed) -> Result<usize, RedisFetchError> {
        let mut con = client.get_connection()?;

        Ok(self.del_cmd().query(&mut con)?)
    }

    async fn list(client: &RedisMultiplexed) -> Result<HashMap<Self::Id, Self>, RedisFetchError> {
        let mut con = client.get_multiplexed_tokio_connection().await?;
        let data: HashMap<Self::Id, String> = Self::list_cmd().query_async(&mut con).await?;

        Ok(data
            .into_iter()
            .map(|(key, data)| -> Result<_, RedisFetchError> {
                Ok((key, serde_json::from_str(&data)?))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .collect::<HashMap<_, _>>())
    }

    async fn get(client: &RedisMultiplexed, id: Self::Id) -> Result<Option<Self>, RedisFetchError> {
        let mut con = client.get_multiplexed_tokio_connection().await?;
        let data: Option<String> = Self::get_cmd(id).query_async(&mut con).await?;

        Ok(data
           .and_then(|data| Some(serde_json::from_str(&data)))
           .transpose()?)
    }

    async fn save(&self, client: &RedisMultiplexed) -> Result<usize, RedisFetchError> {
        let mut con = client.get_multiplexed_tokio_connection().await?;

        Ok(self.save_cmd()?.query_async(&mut con).await?)
    }

    async fn del(&self, client: &RedisMultiplexed) -> Result<usize, RedisFetchError> {
        let mut con = client.get_multiplexed_tokio_connection().await?;

        Ok(self.del_cmd().query_async(&mut con).await?)
    }
}
