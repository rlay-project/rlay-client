use bb8::Pool;
use bb8_cypher::CypherConnectionManager;

#[derive(Debug, Deserialize, Clone)]
pub struct Neo4jBackendConfig {
    pub uri: String,
}

impl Neo4jBackendConfig {
    pub fn connection_pool(&self) -> Pool<CypherConnectionManager> {
        let mut rt = tokio_core::reactor::Core::new().unwrap();
        let manager = CypherConnectionManager {
            url: self.uri.to_owned(),
        };
        let res: Result<Pool<_>, _> = rt.run(futures01::future::lazy(|| {
            Pool::builder()
                .min_idle(Some(10))
                .max_size(20)
                .build(manager)
        }));
        res.unwrap()
    }
}
