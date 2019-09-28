use redis::{aio::SharedConnection, Client};

#[derive(Debug, Deserialize, Clone)]
pub struct RedisBackendConfig {
    pub uri: String,
    pub graph_name: String,
}

impl RedisBackendConfig {
    pub async fn connection_pool(&self) -> SharedConnection {
        let client = Client::open(self.uri.as_str()).unwrap();
        client.get_shared_async_connection().await.unwrap()
    }
}
