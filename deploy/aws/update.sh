#!/bin/bash
# dregg gateway node + Discord bot update path. This script intentionally
# refuses to discard local changes on the host.
#
# Knobs (env):
#   GATEWAY_ONLY=1   update + restart ONLY the gateway node (skip the Discord
#                    bot build/restart, site deploy, and bot preflight). Use
#                    for node-semantics redeploys (e.g. the dregg3 cutover)
#                    where the bot/site are unchanged or updated separately.
#   SKIP_SITE=1      skip the static-site deploy leg only.
#
# STATE PRESERVATION: the node's durable state lives in /opt/dregg-data
# (node.key, genesis.json, the redb store). This script NEVER writes there —
# it only ffwd-merges the repo, rebuilds, reinstalls units, and restarts.
# Genesis is installed by setup.sh ONLY when /opt/dregg-data/genesis.json is
# absent, so a redeploy keeps the running devnet's chain state. A redeploy
# that CHANGES the protocol semantics (VK / commitment / wire bumps) may make
# the preserved state unloadable — deciding to wipe /opt/dregg-data and
# re-genesis is an explicit OPERATOR action, never this script's.
set -euo pipefail

REPO_DIR="${REPO_DIR:-/opt/dregg}"
ENV_FILE="${ENV_FILE:-/etc/dregg/discord-bot.env}"
REMOTE="${REMOTE:-origin}"
BRANCH="${BRANCH:-main}"
REMOTE_REF="$REMOTE/$BRANCH"
GATEWAY_ONLY="${GATEWAY_ONLY:-0}"
SKIP_SITE="${SKIP_SITE:-0}"

if [[ "$GATEWAY_ONLY" == "1" ]]; then
  echo "=== Updating dregg gateway node (GATEWAY_ONLY) ==="
else
  echo "=== Updating dregg gateway node and Discord bot ==="
fi

cd "$REPO_DIR"

if [[ -n "$(git status --porcelain)" ]]; then
  echo "refusing to update: $REPO_DIR has local changes or untracked files" >&2
  echo "commit, stash, remove, or inspect them before deploying" >&2
  exit 1
fi

git fetch "$REMOTE" "$BRANCH"
git merge --ff-only "$REMOTE_REF"

deploy/aws/install-descriptor-tools.sh
if ! aws sts get-caller-identity >/dev/null 2>&1; then
  echo "descriptor hydration needs AWS credentials; attach the descriptor-store deployment instance profile" >&2
  exit 1
fi
python3 scripts/descriptor_store.py fetch

if [[ "$GATEWAY_ONLY" != "1" && ! -f "$ENV_FILE" ]]; then
  echo "missing bot env file: $ENV_FILE" >&2
  echo "copy deploy/aws/discord-bot.env.example to $ENV_FILE and fill secrets" >&2
  exit 1
fi

echo "Building..."
if [[ "$GATEWAY_ONLY" == "1" ]]; then
  cargo build --release -p dregg-node
else
  cargo build --release -p dregg-node -p dregg-discord-bot
fi

echo "Installing systemd units..."
sudo cp deploy/aws/dregg-gateway.service /etc/systemd/system/dregg-gateway.service
if [[ "$GATEWAY_ONLY" != "1" ]]; then
  sudo cp deploy/aws/dregg-discord-bot.service /etc/systemd/system/dregg-discord-bot.service
fi
sudo systemctl daemon-reload

echo "Restarting gateway..."
sudo systemctl restart dregg-gateway

if [[ "$GATEWAY_ONLY" != "1" ]]; then
  echo "Restarting Discord bot..."
  sudo install -d -o dregg -g dregg /var/lib/dregg-discord-bot
  sudo systemctl restart dregg-discord-bot
fi

if [[ "$GATEWAY_ONLY" != "1" && "$SKIP_SITE" != "1" ]]; then
  echo "Deploying static site..."
  deploy/aws/deploy-site.sh
fi

if [[ "$GATEWAY_ONLY" != "1" ]]; then
  echo "Running preflight..."
  deploy/aws/preflight-discord-bot.sh
fi

echo "=== Update complete ==="
sudo systemctl status dregg-gateway --no-pager -l | head -20
if [[ "$GATEWAY_ONLY" != "1" ]]; then
  sudo systemctl status dregg-discord-bot --no-pager -l | head -20
fi
