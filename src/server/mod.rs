use std::collections::{BTreeSet, HashMap, HashSet};
use std::ops::Index;
use std::sync::Arc;
use std::time::Duration;
use axum::routing::get;
use axum::Server;
use redis::{AsyncCommands, Msg};
use serde::Serialize;
use serde_json::Value;
use socketioxide::{Namespace, SocketIoLayer};
use tokio::sync::Mutex;
use tracing::{error, info};
use tower_http::services::{ServeDir, ServeFile};
use crate::reddit::{Subreddit, SubredditDelta, SubredditState};
use futures_util::stream::StreamExt;

#[derive(Serialize, Debug, Clone)]
struct InitSRListEntry {
    name: String,
    status: String,
}


async fn compute_initial_reddits_list(con: Arc<Mutex<redis::aio::Connection>>) -> anyhow::Result<HashMap<String, Vec<InitSRListEntry>>> {
    let srs: HashMap<String, String> = con.lock().await.hgetall("subreddit").await?;
    let mut result: HashMap<String, Vec<InitSRListEntry>> = HashMap::new();

    for sr in srs.values() {
        let sr: Subreddit = serde_json::from_str(sr)?;
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

pub async fn server(con: Arc<Mutex<redis::aio::Connection>>, cli: &crate::Cli, listen: &str) -> anyhow::Result<()> {
    info!("Starting server");
    let active_sockets = Arc::new(Mutex::new(Vec::new()));

    let ns = {
        let con = con.clone();
        let active_sockets = active_sockets.clone();
        Namespace::builder()
            .add("/", move |socket| {
                let con = con.clone();
                let active_sockets = active_sockets.clone();
                async move {
                    info!("Socket connected on / namespace with id: {}", socket.sid);
                    match compute_initial_reddits_list(con.clone()).await {
                        Ok(d) => {
                            info!("Sent subreddits!");
                            socket.emit("subreddits", d);
                        }

                        Err(e) => {
                            error!("Error fetching initial subreddits list: {}", e);
                        }
                    }

                    active_sockets.lock().await.push(socket.clone());
                }
            })
            .build()
    };

    let periodic_subreddits = async {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            match compute_initial_reddits_list(con.clone()).await {
                Ok(d) => {
                    info!("Periodic send subreddits!");
                    let mut s = active_sockets.lock().await;

                    //TODO: Figure out a way to clean up old handlers.

                    let mut clean_up = Vec::new();
                    for (idx, sock) in s.iter().enumerate() {
                        let res = sock.emit("subreddits", d.clone());
                        if let Err(_) = res {
                            clean_up.push(idx);
                        }
                    }

                    for i in clean_up.iter().rev() {
                        s.remove(*i);
                    }
                }

                Err(e) => {
                    error!("Error fetching initial subreddits list: {}", e);
                }
            }
        };



    };


    let pubsub = async {
        let client = redis::Client::open(&*cli.redis_url).unwrap();
        let con = client.get_async_connection().await?;
        let mut pubsub = con.into_pubsub();

        pubsub.subscribe("subreddit_updates").await?;

        let mut s =  pubsub.into_on_message();

        while let Some(item) = s.next().await {
            let item: Msg = item;
            let delta: String = item.get_payload()?;
            let delta: SubredditDelta = serde_json::from_str(&delta)?;

            let mut msg: HashMap<String, String> = HashMap::new();
            msg.insert("name".to_string(), delta.subreddit.name.clone());
            msg.insert("status".to_string(), delta.subreddit.state.to_string());

            let mut s = active_sockets.lock().await;

            //TODO: Figure out a way to clean up old handlers.

            let mut clean_up = Vec::new();
            for (idx, sock) in s.iter().enumerate() {
                let res = sock.emit("updatenew", msg.clone());
                if let Err(_) = res {
                    clean_up.push(idx);
                }
            }

            for i in clean_up.iter().rev() {
                s.remove(*i);
            }
        }

        anyhow::Ok(())
    };

    let serve_dir = ServeDir::new("public").not_found_service(ServeFile::new("public/index.html"));


    let app = axum::Router::new()
//        .route("/", get(|| async { "Hello, World!" }))
        .nest_service("/", serve_dir.clone())
        .layer(SocketIoLayer::new(ns));

    let server = Server::bind(&listen.parse().unwrap())
        .serve(app.into_make_service());

    tokio::join!(pubsub, server, periodic_subreddits);
    Ok(())
}