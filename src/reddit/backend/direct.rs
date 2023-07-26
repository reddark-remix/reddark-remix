use std::time::Duration;
use governor::{clock, RateLimiter, state::{InMemoryState, NotKeyed}, middleware::NoOpMiddleware, Quota, Jitter};
use nonzero_ext::nonzero;
use serde_json::Value;
use async_trait::async_trait;
use crate::reddit::backend::RedditRequestBackend;

pub struct DirectBackend {
    limiter: RateLimiter<NotKeyed, InMemoryState, clock::DefaultClock, NoOpMiddleware>,
}

impl DirectBackend {
    pub fn new(rate_limit: f32) -> anyhow::Result<Box<Self>> {
        assert!(rate_limit > 0.0);
        let replenish_interval_ns = Duration::from_secs_f64(Duration::from_secs(1).as_secs_f64() / (rate_limit as f64));
        let limiter = RateLimiter::direct(Quota::with_period(replenish_interval_ns).unwrap().allow_burst(nonzero!(1u32)));
        Ok(Box::new(DirectBackend {
            limiter,
        }))
    }
}

#[async_trait]
impl RedditRequestBackend for DirectBackend {
    async fn make_reddit_request(&self, rel_url: &str, query: Option<&[(String, String)]>) -> anyhow::Result<Value> {
        self.limiter.until_ready_with_jitter(Jitter::up_to(Duration::from_millis(1))).await;
        let client = reqwest::Client::builder();
        let client = client.user_agent("Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/114.0");
        let client = client.build()?;
        let req = client.get(format!("https://old.reddit.com/{}", rel_url));
        let req = if let Some(q) = query {
            req.query(q)
        } else {
            req
        };
        let req = req.header("Range", "bytes=0-50");
        //info!("Sending request! {req:?}");
        let resp = req.send().await?;
        if resp.status().is_success() || resp.status() == 403 || resp.status() == 404 {
            Ok(resp.json().await?)
        } else {
            let s = format!("{resp:?}");
            Err(anyhow::anyhow!("Error querying reddit: {s} {}", resp.text().await?))
        }
    }
}