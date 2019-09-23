use futures::compat::Future01CompatExt;
use l337::{Config, Pool};
use l337_redis::RedisConnectionManager;

#[derive(Debug, Deserialize, Clone)]
pub struct RedisBackendConfig {
    pub uri: String,
    pub graph_name: String,
}

impl RedisBackendConfig {
    pub async fn connection_pool(&self) -> Pool<RedisConnectionManager> {
        let manager = RedisConnectionManager::new(self.uri.as_str()).unwrap();

        Pool::new(
            manager,
            Config {
                min_size: 3,
                max_size: 30,
            },
        )
        .compat()
        .await
        .unwrap()
    }
}
