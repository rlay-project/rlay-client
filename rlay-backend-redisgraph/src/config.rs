use redis::{aio::SharedConnection, Client};

#[derive(Debug, Deserialize, Clone)]
pub struct RedisgraphBackendConfig {
    pub uri: String,
    pub graph_name: String,
}

impl RedisgraphBackendConfig {
    pub async fn connection_pool(&self) -> SharedConnection {
        trace!("Creating new Redis connection");
        let client = Client::open(self.uri.as_str()).unwrap();
        client.get_shared_async_connection().await.unwrap()
    }
}
