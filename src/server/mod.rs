use std::collections::{HashMap};
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use axum::Server;
use futures_util::{TryStreamExt, TryFutureExt};
use serde::Serialize;
use socketioxide::{Namespace, Socket, SocketIoLayer};
use socketioxide::adapter::LocalAdapter;
use tokio::sync::{Mutex};
use tower_http::services::{ServeDir, ServeFile};
use tracing::{error, info};

use crate::redis_helper::RedisHelper;

#[derive(Serialize, Debug, Clone)]
struct InitSRListEntry {
    name: String,
    status: String,
}

async fn compute_initial_reddits_list(redis_helper: &RedisHelper) -> anyhow::Result<HashMap<String, Vec<InitSRListEntry>>> {
    let mut result: HashMap<String, Vec<InitSRListEntry>> = HashMap::new();
    let subreddits = redis_helper.get_current_state().await?;

    for sr in subreddits {
        if !result.contains_key(&sr.section) {
            result.insert(sr.section.clone(), Vec::new());
        }

        let status = sr.state.to_string();

        result.get_mut(&sr.section).unwrap().push(InitSRListEntry {
            name: sr.name.clone(),
            status,
        });
    }

    Ok(result)
}

struct SocketManager {
    active_sockets: Mutex<Vec<Arc<Socket<LocalAdapter>>>>,
}

impl SocketManager {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            active_sockets: Mutex::new(Vec::new()),
        })
    }

    pub async fn add_socket(&self, socket: Arc<Socket<LocalAdapter>>) -> anyhow::Result<()> {
        self.active_sockets.lock().await.push(socket);
        Ok(())
    }

    pub async fn emit_all(&self, event: impl Into<String>, data: impl Serialize + Clone) -> anyhow::Result<()> {
        let mut clean_up = Vec::new();
        {
            let s: Vec<_> = self.active_sockets.lock().await.clone();
            let event = event.into();
            for (idx, sock) in s.iter().enumerate() {
                let res = sock.emit(event.clone(), data.clone());
                if let Err(_) = res {
                    clean_up.push(idx);
                }
            }
        }

        {
            let mut s = self.active_sockets.lock().await;
            for i in clean_up.iter().rev() {
                s.remove(*i);
            }
        }

        Ok(())
    }
}

async fn start_server(redis_helper: RedisHelper, socket_manager: Arc<SocketManager>, listen: &str) -> anyhow::Result<impl Future<Output=anyhow::Result<()>>> {
    let ns = {
        Namespace::builder()
            .add("/", move |socket| {
                let redis_helper = redis_helper.clone();
                let socket_manager = socket_manager.clone();
                async move {
                    info!("Socket connected on / namespace with id: {}", socket.sid);
                    match compute_initial_reddits_list(&redis_helper).await {
                        Ok(d) => {
                            info!("Sent subreddits!");
                            let res = socket.emit("subreddits", d);
                            if let Ok(_) = res {
                                let _ = socket_manager.add_socket(socket.clone()).await;
                            }
                        }
                        Err(e) => {
                            error!("Error fetching initial subreddits list: {}", e);
                        }
                    }
                }
            })
            .build()
    };
    let serve_dir = ServeDir::new("public").not_found_service(ServeFile::new("public/index.html"));

    let app = axum::Router::new()
        .nest_service("/", serve_dir.clone())
        .layer(SocketIoLayer::new(ns));

    Ok(
        Server::bind(&listen.parse().unwrap())
            .serve(app.into_make_service())
            .map_err(|e| anyhow::Error::from(e))
    )
}

async fn start_periodic_job(redis_helper: RedisHelper, socket_manager: Arc<SocketManager>) -> anyhow::Result<impl Future<Output=anyhow::Result<()>>> {
    Ok(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            // Wait period.
            interval.tick().await;

            match compute_initial_reddits_list(&redis_helper).await {
                Ok(d) => {
                    info!("Periodic send subreddits!");
                    socket_manager.emit_all("subreddits", d).await?;
                }

                Err(e) => {
                    error!("Error fetching subreddits list: {}", e);
                }
            }
        }
        // Hint to type system
        #[allow(unreachable_code)]
        anyhow::Ok(())
    })
}

async fn start_pubsub(cli: &crate::Cli, socket_manager: Arc<SocketManager>) -> anyhow::Result<impl Future<Output=anyhow::Result<()>>> {
    let mut stream = crate::redis_helper::new_delta_stream(cli).await?;
    let socket_manager = socket_manager.clone();
    Ok(async move {
        while let Some(delta) = stream.try_next().await? {
            let mut msg: HashMap<String, String> = HashMap::new();
            msg.insert("name".to_string(), delta.subreddit.name.clone());
            msg.insert("status".to_string(), delta.subreddit.state.to_string());

            socket_manager.emit_all("updatenew", msg).await?;
        }

        anyhow::Ok(())
    })
}

pub async fn server(cli: &crate::Cli, listen: &str) -> anyhow::Result<()> {
    info!("Starting server");
    let socket_manager = SocketManager::new();
    let redis_helper = RedisHelper::new(cli).await?;

    let server = start_server(redis_helper.clone(), socket_manager.clone(), listen).await?;
    let periodic_subreddits = start_periodic_job(redis_helper.clone(), socket_manager.clone()).await?;
    let pubsub = start_pubsub(cli, socket_manager.clone()).await?;

    tokio::select! {
        val = pubsub => {
            val?;
        }
        val = server => {
            val?;
        }
        val = periodic_subreddits => {
            val?;
        }
    }

    info!("Exited!");

    Ok(())
}