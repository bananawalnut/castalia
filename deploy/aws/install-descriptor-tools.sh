#!/usr/bin/env bash
# Ensure deployment hosts can hydrate private descriptor compiler inputs.
set -euo pipefail

if command -v apt-get >/dev/null 2>&1; then
  if ! command -v python3 >/dev/null 2>&1 || ! command -v aws >/dev/null 2>&1; then
    sudo apt-get update
    sudo apt-get install -y python3 awscli
  fi
elif command -v dnf >/dev/null 2>&1; then
  if ! command -v python3 >/dev/null 2>&1; then
    sudo dnf install -y python3
  fi
  if ! command -v aws >/dev/null 2>&1; then
    if ! sudo dnf install -y awscli2; then
      sudo dnf install -y awscli
    fi
  fi
else
  echo "unsupported package manager: install Python 3 and AWS CLI before deployment" >&2
  exit 1
fi

command -v python3 >/dev/null 2>&1 || {
  echo "python3 is required for descriptor hydration" >&2
  exit 1
}
command -v aws >/dev/null 2>&1 || {
  echo "AWS CLI is required for descriptor hydration" >&2
  exit 1
}
