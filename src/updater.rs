use std::num::NonZeroU32;
use tracing::info;
use crate::Cli;
use crate::reddit::{Reddit, SubredditDelta, SubredditState};
use crate::redis_helper::RedisHelper;

pub async fn updater(cli: &Cli, rate_limit: NonZeroU32) -> anyhow::Result<()> {
    let reddit = Reddit::new(rate_limit);
    let redis_helper = RedisHelper::new(cli).await?;

    {
        let redis_subreddits = redis_helper.get_current_state().await?;

        // Spawn out all the subreddits.
        let fns = redis_subreddits.into_iter().map(|subreddit| {
            let reddit = reddit.clone();
            let redis_helper = redis_helper.clone();

            let f = async move {
                info!("Updating subreddit {}...", subreddit.name);

                let mut delta = SubredditDelta {
                    prev_state: subreddit.state.clone(),
                    subreddit: subreddit.clone(),
                };

                let is_private = reddit.is_subreddit_private(&subreddit.name).await?;

                delta.subreddit.state = if is_private { SubredditState::PRIVATE } else { SubredditState::PUBLIC };

                if delta.prev_state != delta.subreddit.state {
                    info!("Change happend! Subreddit {} has gone from {:?} to {:?}.", delta.subreddit.name, delta.prev_state, delta.subreddit.state);
                }

                redis_helper.apply_delta(&delta).await?;

                anyhow::Ok(())
            };

            tokio::spawn(f)
        }).collect::<Vec<_>>();

        // Wait for parallel work to finish.
        for h in fns {
            h.await??;
        }
    }

    Ok(())
}