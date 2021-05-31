#!/usr/bin/env bash
set -euo pipefail

# deploy-public-chart-test.sh
#
# SUMMARY
#
#   Deploys a public chart into Kubernetes for testing purposes.
#   Uses the same installation method our users would use.
#
#   This script implements cli interface required by the kubernetes E2E
#   tests.
#
# USAGE
#
#   Deploy:
#
#   $ CHART_REPO=https://helm.testmaterial.tld scripts/deploy-public-chart-test.sh up test-namespace-qwerty chart
#
#   Teardown:
#
#   $ scripts/deploy-public-chart-test.sh down test-namespace-qwerty chart
#

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Command to perform.
COMMAND="${1:?"Specify the command (up/down) as the first argument"}"

# A Kubernetes namespace to deploy to.
NAMESPACE="${2:?"Specify the namespace as the second argument"}"

# The helm chart to manage
HELM_CHART="${3:?"Specify the helm chart name as the third argument"}"

# Allow overriding kubectl with something like `minikube kubectl --`.
VECTOR_TEST_KUBECTL="${VECTOR_TEST_KUBECTL:-"kubectl"}"

# Allow overriding helm with a custom command.
VECTOR_TEST_HELM="${VECTOR_TEST_HELM:-"helm"}"

# Allow optionally installing custom resource configs.
CUSTOM_RESOURCE_CONFIGS_FILE="${CUSTOM_RESOURCE_CONFIGS_FILE:-""}"

# Allow optionally passing custom Helm values.
CUSTOM_HELM_VALUES_FILE="${CUSTOM_HELM_VALUES_FILE:-""}"

# Allow overriding the local repo name, useful to use multiple external repo
CUSTOM_HELM_REPO_LOCAL_NAME="${CUSTOM_HELM_REPO_LOCAL_NAME:-"local_repo"}"

up() {
  $VECTOR_TEST_HELM repo add "$CUSTOM_HELM_REPO_LOCAL_NAME" "$CHART_REPO" || true
  $VECTOR_TEST_HELM repo update

  $VECTOR_TEST_KUBECTL create namespace "$NAMESPACE"

  if [[ -n "$CUSTOM_RESOURCE_CONFIGS_FILE" ]]; then
    $VECTOR_TEST_KUBECTL create --namespace "$NAMESPACE" -f "$CUSTOM_RESOURCE_CONFIGS_FILE"
  fi


  HELM_VALUES=()
  if [[ -n "$CUSTOM_HELM_VALUES_FILE" ]]; then
    HELM_VALUES=(--values "$CUSTOM_HELM_VALUES_FILE")
  fi

  set -x
  $VECTOR_TEST_HELM install \
    --atomic \
    --namespace "$NAMESPACE" \
    "${HELM_VALUES[@]}" \
    "$HELM_CHART" \
    "$CUSTOM_HELM_REPO_LOCAL_NAME/$HELM_CHART"
  { set +x; } &>/dev/null
}

down() {
  if [[ -n "$CUSTOM_RESOURCE_CONFIGS_FILE" ]]; then
    $VECTOR_TEST_KUBECTL delete --namespace "$NAMESPACE" -f "$CUSTOM_RESOURCE_CONFIGS_FILE"
  fi

  if $VECTOR_TEST_HELM status --namespace "$NAMESPACE" "$HELM_CHART" &>/dev/null; then
    $VECTOR_TEST_HELM delete --namespace "$NAMESPACE" "$HELM_CHART"
  fi

  $VECTOR_TEST_KUBECTL delete namespace "$NAMESPACE"
}

case "$COMMAND" in
  up|down)
    "$COMMAND" "$@"
    ;;
  *)
    echo "Invalid command: $COMMAND" >&2
    exit 1
esac
