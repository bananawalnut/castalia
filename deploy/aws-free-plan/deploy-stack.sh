#!/usr/bin/env bash
set -euo pipefail

if [[ "${CASTALIA_AWS_FREE_PLAN_CONFIRMED:-}" != "YES" ]]; then
  echo "Refusing to deploy: first confirm the AWS console shows Free account plan." >&2
  echo "Then export CASTALIA_AWS_FREE_PLAN_CONFIRMED=YES without upgrading the account." >&2
  exit 2
fi

if [[ "$#" -lt 2 || "$#" -gt 3 ]]; then
  echo "usage: $0 <ec2-key-name> <operator-ip-cidr> [stack-name]" >&2
  exit 2
fi

KEY_NAME="$1"
OPERATOR_CIDR="$2"
STACK_NAME="${3:-castalia-dregg-free-plan}"
AWS_REGION="${AWS_REGION:-us-west-2}"
INSTANCE_TYPE="${CASTALIA_INSTANCE_TYPE:-t3.small}"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

case "$INSTANCE_TYPE" in
  t3.small|c7i-flex.large) ;;
  *) echo "unsupported free-plan instance type: $INSTANCE_TYPE" >&2; exit 2 ;;
esac

ACCOUNT_ID="$(aws sts get-caller-identity --query Account --output text)"
echo "AWS account: $ACCOUNT_ID"
echo "Region: $AWS_REGION"
echo "Instance: $INSTANCE_TYPE"
echo "Free Account Plan confirmation: recorded for this deployment"

aws cloudformation deploy \
  --region "$AWS_REGION" \
  --stack-name "$STACK_NAME" \
  --template-file "$SCRIPT_DIR/template.yml" \
  --no-fail-on-empty-changeset \
  --parameter-overrides \
    "KeyName=$KEY_NAME" \
    "OperatorCidr=$OPERATOR_CIDR" \
    "InstanceType=$INSTANCE_TYPE" \
  --tags \
    "Project=Castalia" \
    "CostBoundary=AWSFreeAccountPlan" \
    "FreePlanConfirmedAt=$(date -u +%Y-%m-%dT%H:%M:%SZ)"

aws cloudformation describe-stacks \
  --region "$AWS_REGION" \
  --stack-name "$STACK_NAME" \
  --query 'Stacks[0].Outputs' \
  --output table
