use async_trait::async_trait;

pub mod direct;
pub mod tor;

#[async_trait]
pub trait RedditRequestBackend: Sync + Send {
    async fn make_reddit_request(&self, rel_url: &str, query: Option<&[(String, String)]>) -> anyhow::Result<serde_json::Value>;
}
