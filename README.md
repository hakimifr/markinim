# Markinim
This is the @MarkinimBot Telegram bot's source code. It learns from your messages and tries to formulate its own sentences. It uses sqlite, and the bot itself is written in Rust.

## Deploy
> For docker instructions, [skip here](#deploy-with-docker)

Install the Rust toolchain (e.g. with [rustup](https://rustup.rs)), then build:

```shell
$ cargo build --release
```

Then create a `secret.ini` file which looks like this, where admin is your Telegram user id, and token is the bot token obtainable from @BotFather.

```ini
[config]
token = "1234:abcdefg"
admin = 123456
logging = 1
```

You can also add a `keeplast = 1500` parameter to the configuration, to avoid ram overloads by processing a maximum of keeplast messages per session (default: `1500`)

Every setting can also be provided through the environment (`BOT_TOKEN`, `ADMIN_ID`, `LOGGING`, `KEEP_LAST`), which takes effect when the key is missing from `secret.ini`.

```shell
$ ./target/release/markinim
```

The bot reads and writes `data/markov.db` relative to the working directory.

## Deploy (with docker)
- Copy `.env.sample` to `.env`
- Edit `BOT_TOKEN` and `ADMIN_ID`
- If needed, edit `KEEP_LAST` (default: `1500`. Read above)
- Build and run the image with `docker compose up -d --build`
- Run the bot with `docker compose up -d`

## Tests

```shell
$ cargo test
```

## Backups
> ⚠️ **WARNING**: This is an experimental backup script. It's not well-tested yet. Use it at your own risk. I am not responsible for any data loss. I don't know if it works.
- Setup [`syncthing`](https://syncthing.net/) if you want to sync the backups to another device
- Copy `tools/backup_script.example.sh` to `tools/backup_script.sh` and edit it to set the correct values for `root_dir`, `backup_directory` and `backup_filename`
- Optionally, edit `TELEGRAM_ID` to receive a notification when the backup is done
- Copy `tools/backup.example.sh` to `tools/backup.sh` and edit it if you want to change the container name
- Run a cronjob to run `tools/backup.sh` every 4h (or whatever you want)
  - Open crontab with `crontab -e`
  - Add `0 */4 * * * /path/to/markinim/tools/backup.sh`
  - Save and exit
- Done! Now you should have a backup every 4h in the specified directory

## Credits
- The quote image fonts (Lora, Oswald, Montserrat) are under the Open Fonts License
- The emojipasta feature ports [EmojipastaBot](https://github.com/Kevinpgalligan/EmojipastaBot)'s EmojipastaGenerator (MIT)
