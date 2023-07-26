use std::num::NonZeroU32;
use std::time::Duration;
use anyhow::Result;
use tracing::info;
use crate::Cli;
use crate::redis_helper::RedisHelper;

pub async fn update_list(cli: &Cli, period: Option<NonZeroU32>) -> Result<()> {
    let reddit = cli.new_reddit_backend().await?;
    let redis_helper = RedisHelper::new(cli).await?;

    let mut timer = period.map(|p| tokio::time::interval(Duration::from_secs(p.get() as u64)));

    loop {
        info!("Fetching subreddits...");
        let (sections, subs) = reddit.fetch_subreddits().await?;
        let existing_subs = redis_helper.get_current_state().await?;

        redis_helper.set_sections(sections).await?;

        for sub in subs {
            let existing = existing_subs.iter().find(|s| s.name == sub.name);

            if let Some(existing) = existing {
                if existing.section != sub.section {
                    info!("Subreddit {} already exists! Updating section to {}...", sub.name, sub.section);
                    let mut new = existing.clone();
                    new.section = sub.section.clone();
                    redis_helper.update_subreddit(&new).await?;
                } else {
                    info!("Subreddit {} already exists!", sub.name);
                }
            }  else {
                info!("Adding subreddit {}...", sub.name);
                redis_helper.update_subreddit(&sub).await?;
            }
        }
        info!("Done!");

        if let Some(t) = timer.as_mut() {
            info!("Awaiting tick...");
            t.tick().await;
        } else {
            break
        }
    }
    Ok(())
}