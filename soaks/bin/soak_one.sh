#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
#set -o xtrace

__dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

display_usage() {
    echo ""
    echo "Usage: $0 USE_LOCAL_IMAGE SOAK_NAME VARIANT TAG CAPTURE_DIR"
}

USE_LOCAL_IMAGE="${1}"
SOAK_NAME="${2}"
VARIANT="${3}"
TAG="${4}"
CAPTURE_DIR="${5}"

pushd "${__dir}"

IMAGE="vector:${TAG}"
if [ "${USE_LOCAL_IMAGE}" = "true" ]; then
    echo "Building images locally..."

    ./build_container.sh "${TAG}" "${IMAGE}"
else
    REMOTE_IMAGE="ghcr.io/vectordotdev/vector/soak-vector:${TAG}"
    docker pull "${REMOTE_IMAGE}"
    docker image tag "${REMOTE_IMAGE}" "${IMAGE}"
fi

./run_experiment.sh "${CAPTURE_DIR}" "${VARIANT}" "${IMAGE}" "${SOAK_NAME}"

popd
