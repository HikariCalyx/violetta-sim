# Violetta PM userbot

The MTProto user account that gates private messages. It sends the verification photo,
blocks and archives a stranger, and later polls the Worker to delete the message and
unblock/unarchive them after a pass.

## Requirements

- A Linux host with outbound HTTPS/TCP 443 to Telegram and to the Worker.
- Python 3.10+ (`python3-venv` on Debian/Ubuntu).
- The Worker deployed with `USERBOT_API_HASH` set, and the four values below.
- **Only one process may use the `USERBOT_SESSION` at a time.** Disable the GitHub
  Actions workflow (Actions → Run the userbot → ⋯ → Disable workflow) before starting
  this service.

## Install on a VPS

```sh
sudo apt update && sudo apt install -y python3 python3-venv git
sudo useradd --system --user-group --create-home --home-dir /opt/violetta --shell /usr/sbin/nologin violetta
sudo -u violetta git clone https://github.com/HikariCalyx/violetta-sim.git /opt/violetta
sudo -u violetta git -C /opt/violetta checkout main
sudo -u violetta python3 -m venv /opt/violetta/.venv
sudo -u violetta /opt/violetta/.venv/bin/pip install -r /opt/violetta/userbot/requirements.txt
```

Reuse the existing `USERBOT_SESSION`, or create one once as the service user (it prompts
for the phone, the login code and the 2FA password):

```sh
sudo -u violetta /opt/violetta/.venv/bin/python /opt/violetta/userbot/login.py
```

Create the environment file. systemd reads it; the Python script does not load files:

```sh
sudo install -m 600 -o root -g root /opt/violetta/userbot/userbot.env.example /etc/violetta-userbot.env
sudo nano /etc/violetta-userbot.env
```

Required keys:

```ini
USERBOT_API_ID=123456
API_HASH_SECRET=0123456789abcdef0123456789abcdef
USERBOT_SESSION=1BQANOTEuMTA4LjU2LjE2OAG7...
```

Optional:

```ini
WORKER_ORIGIN=https://violetta.trill.workers.dev
MAX_RUNTIME_SECONDS=0
TRIGGER_MAX_AGE_SECONDS=1800
```

`API_HASH_SECRET` must be the same api_hash the Worker has as `USERBOT_API_HASH`.

Install and start the unit:

```sh
sudo cp /opt/violetta/userbot/violetta-userbot.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now violetta-userbot
sudo systemctl status violetta-userbot
sudo journalctl -u violetta-userbot -f
```

The first log line should be `userbot running as …`.

## Updating

```sh
sudo -u violetta git -C /opt/violetta pull
sudo -u violetta /opt/violetta/.venv/bin/pip install -r /opt/violetta/userbot/requirements.txt
sudo systemctl restart violetta-userbot
```

## Local test (no systemd)

```sh
export USERBOT_API_ID=...
export API_HASH_SECRET=...
export USERBOT_SESSION=...
python userbot/main.py
```

The process writes nothing to disk: the session is held in a `StringSession`. It needs
`Resources/lieDetector.npcSpecial.stand.0.png` from the repository checkout.

## Security

- Treat `USERBOT_SESSION` as a password for the whole account. Never commit it; keep the
  environment file `chmod 600`.
- Use a dedicated account, not your main one.
- The account should be able to receive messages from non-contacts.
