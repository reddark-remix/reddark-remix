# Reddark Remix

I got annoyed by the backend of https://github.com/Tanza3D/reddark crashing. So I wrote my own backend.
Frontend is still from there and very much theirs.

This code is awful and was written hastily.

## Subreddits
Reddark pulls the list of participating subreddits from the [threads on r/ModCoord](https://reddit.com/r/ModCoord/comments/1401qw5/incomplete_and_growing_list_of_participating/). If you are the moderator of a sub that is going dark and that is not displayed on Reddark, reply to the aforementioned thread to be counted as participating.

> **SubManagerBot**
> 
> If you have already commented your sub below or your sub is already on the list and now going private, please do NOT send a modmail - if you comment here, your sub will be on the list.

## Running it yourself

You'll need a redis instance running at localhost.
Also have rust installed and working. (https://rustup.rs is good)
Then run the process to import the subreddit list from /r/ModCoord:
```sh
cargo run --release -- update-subreddit-list
```
Next you run the process that checks for subreddit updates:
```sh
cargo run --release -- updater --rate-limit 500
```
Finally, the web server:
```sh
cargo run --release -- server
```

The latter process stays running. The other two exit.
Run the updater again to process updated status and get events to fire to frontend.
If you want to edit the templates, you have to restart the webserver after each edit.

To keep the updater running, you can use the `--period` parameter to define how often polling is done in seconds.
```sh
cargo run --release -- updater --rate-limit 500 --period 30
```
Unlike before, this will keep running and repeat every 30 seconds.
