use std::io::Read;
use hyper::body::Buf;
use std::ops::DerefMut;
use std::time::Duration;
use anyhow::Context;
use arti_client::{BootstrapBehavior, TorClient};
use arti_client::config::{ClientAddrConfig, TorClientConfigBuilder};
use arti_hyper::ArtiHttpConnector;
use governor::{clock, RateLimiter, state::{InMemoryState, NotKeyed}, middleware::NoOpMiddleware, Quota, Jitter};
use nonzero_ext::nonzero;
use serde_json::Value;
use async_trait::async_trait;
use hyper::{Body, Client, Method, Request};
use tor_rtcompat::PreferredRuntime;
use crate::reddit::backend::RedditRequestBackend;
use tls_api::{TlsConnector as TlsConnectorTrait, TlsConnectorBuilder};
use tls_api_openssl::TlsConnector;
use tokio::sync::RwLock;
use tracing::info;

const REDDIT_TOR_HOST: &str = "www.reddittorjg6rue252oqsxryoxengawnmo46qy4kyii5wtqnwfj4ooad.onion";

fn create_hyper_client_from_tor_client(tor_client: &TorClient<PreferredRuntime>) -> anyhow::Result<Client<ArtiHttpConnector<PreferredRuntime, TlsConnector>>> {
    info!("Initializing new tor client...");
    let tls = TlsConnector::builder()
        .context("Unable to init OpenSSL Connector")?
        .build()
        .context("Unable to create OpenSSL Connector")?;
    let anon_client = tor_client.isolated_client();
    let tor = ArtiHttpConnector::new(anon_client, tls);
    let client = Client::builder().build(tor);
    Ok(client)
}

pub struct TorBackend {
    limiter: RateLimiter<NotKeyed, InMemoryState, clock::DefaultClock, NoOpMiddleware>,
    tor_client: TorClient<PreferredRuntime>,
    client: RwLock<Client<ArtiHttpConnector<PreferredRuntime, TlsConnector>>>,
}

impl TorBackend {
    pub fn new(rate_limit: f32) -> anyhow::Result<Box<Self>> {
        assert!(rate_limit > 0.0);
        let replenish_interval_ns = Duration::from_secs_f64(Duration::from_secs(1).as_secs_f64() / (rate_limit as f64));
        let limiter = RateLimiter::direct(Quota::with_period(replenish_interval_ns).unwrap().allow_burst(nonzero!(1u32)));

        let mut tor_config = TorClientConfigBuilder::default();
        *tor_config.address_filter() = ClientAddrConfig::builder().allow_onion_addrs(true).clone();
        let tor_client = TorClient::builder()
            .config(tor_config.build().unwrap_or_default())
            .bootstrap_behavior(BootstrapBehavior::OnDemand)
            .create_unbootstrapped()
            .context("Unable to create Tor client!")?;

        let client = RwLock::new(create_hyper_client_from_tor_client(&tor_client)?);

        Ok(Box::new(TorBackend {
            limiter,
            tor_client,
            client,
        }))
    }
}

#[async_trait]
impl RedditRequestBackend for TorBackend {
    async fn make_reddit_request(&self, rel_url: &str, query: Option<&[(String, String)]>) -> anyhow::Result<Value> {
        self.limiter.until_ready_with_jitter(Jitter::up_to(Duration::from_millis(1))).await;

        let uri = format!("https://{}/", REDDIT_TOR_HOST);
        let mut uri = url::Url::parse(&uri)?;
        uri.set_path(rel_url);
        if let Some(qp) = query
        {
            let mut q = uri.query_pairs_mut();
            q.clear();
            for (k, v) in qp {
                q.append_pair(k, v);
            }
        }
        let uri = uri;

        let request = Request::builder()
            .method(Method::GET)
            .uri(uri.as_str())
            .header("User-Agent", format!("web:reddark:{}", env!("CARGO_PKG_VERSION")))
            .header("Host", REDDIT_TOR_HOST)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8")
            .header("Accept-Encoding", /*if method == Method::GET { "gzip" } else { "identity" }*/ "identity")
            .header("Accept-Language", "en-US,en;q=0.5")
            .header("Connection", "keep-alive")
            .header("Cookie", "_options=%7B%22pref_quarantine_optin%22%3A%20true%2C%20%22pref_gated_sr_optin%22%3A%20true%7D")
            .body(Body::empty())?;

        let response = {
            let client = self.client.read().await;
            client.request(request).await?
        };

        if response.status().is_success() || response.status() == 403 || response.status() == 404 {
            let body = hyper::body::aggregate(response).await?;
            let value: Value = serde_json::from_reader(body.reader())?;
            Ok(value)
        } else {
            if response.status() == 429 {
                // Rate limit!
                // Cycle out circuit.
                {
                    let mut client = self.client.write().await;
                    let new_client = create_hyper_client_from_tor_client(&self.tor_client)?;
                    let old = std::mem::replace(client.deref_mut(), new_client);
                    drop(old);
                }
                // Retry.
                self.make_reddit_request(rel_url, query).await
            } else {
                let s = format!("{response:?}");
                let mut body = hyper::body::aggregate(response).await?.reader();
                let mut text = String::new();
                body.read_to_string(&mut text)?;
                Err(anyhow::anyhow!("Error querying reddit: {s} {}", text))
            }
        }
    }
}