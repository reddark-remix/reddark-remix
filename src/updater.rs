use std::num::NonZeroU32;
use std::time::Duration;
use itertools::Itertools;
use tracing::{error, info};
use crate::Cli;
use crate::reddit::{Subreddit, SubredditDelta, SubredditState};
use crate::redis_helper::RedisHelper;

pub async fn updater(cli: &Cli, period: Option<NonZeroU32>) -> anyhow::Result<()> {
    let reddit = cli.new_reddit_backend().await?;
    let redis_helper = RedisHelper::new(cli).await?;

    let mut timer = period.map(|p| tokio::time::interval(Duration::from_secs(p.get() as u64)));

    loop {
        let start = std::time::Instant::now();
        let redis_subreddits = redis_helper.get_current_state().await?;

        let oliver_subs = reddit.get_oliver_list().await?;
        let oliver_subs = oliver_subs.into_iter().map(|s| s.to_lowercase().to_string()).collect::<Vec<String>>();

        // Spawn out all the subreddits.
        let fns = redis_subreddits.into_iter().chunks(100).into_iter().map(|subreddits| {
            let reddit = reddit.clone();
            let redis_helper = redis_helper.clone();
            let subreddits: Vec<Subreddit> = subreddits.collect();
            let oliver_subs = oliver_subs.clone();

            let name = subreddits.iter().map(|s| &s.name).join(",");

            let f = async move {
                let srs: Vec<String> = subreddits.iter().map(|s| s.name.to_string()).collect();
                info!("Updating subreddits {}...", srs.join(","));
                let states = reddit.get_subreddit_state_bulk(&srs).await?;

                for prev_state in subreddits.iter() {
                    let lname = prev_state.name.to_lowercase();
                    let mut delta = SubredditDelta::from(prev_state.clone());
                    let state = states.get(&lname).cloned().unwrap_or(SubredditState::UNKNOWN);
                    delta.subreddit.state = if oliver_subs.contains(&lname) { SubredditState::OLIVER } else { state };

                    if delta.prev_state != delta.subreddit.state {
                        info!("Change happend! Subreddit {} has gone from {:?} to {:?}.", delta.subreddit.name, delta.prev_state, delta.subreddit.state);
                    }

                    redis_helper.apply_delta(&delta).await?;
                }

                anyhow::Ok(())
            };

            (name, tokio::spawn(f))
        }).collect::<Vec<_>>();

        // Wait for parallel work to finish.
        let mut failed_subs = 0usize;
        let total_subs = fns.len();
        for (n, h) in fns {
            let result = h.await?;
            if let Err(e) = result {
                error!("Request failed for {n}: {e}");
                failed_subs += 1;
            }
        }

        redis_helper.trim_history().await?;

        let stop = std::time::Instant::now();
        let taken = stop.duration_since(start);
        let perc = (((total_subs - failed_subs) as f32) / (total_subs as f32)) * 100.0;
        info!("Done! Update took {} seconds. {failed_subs} out of {total_subs} requests failed to fetch. Success rate is: {perc:.2}%", taken.as_secs_f32());

        if let Some(t) = timer.as_mut() {
            info!("Awaiting tick...");
            t.tick().await;
        } else {
            break;
        }
    }

    Ok(())
}
