use bb8_cypher::CypherConnectionManager;
use futures::compat::Future01CompatExt;
use l337::{Config, Pool};

#[derive(Debug, Deserialize, Clone)]
pub struct Neo4jBackendConfig {
    pub uri: String,
}

impl Neo4jBackendConfig {
    pub async fn connection_pool(&self) -> Pool<CypherConnectionManager> {
        let manager = CypherConnectionManager {
            url: self.uri.to_owned(),
        };

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
