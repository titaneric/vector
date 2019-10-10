#!/usr/bin/env bash

# release-docker.sh
#
# SUMMARY
#
#   Builds and pushes Vector docker images

set -eu

CHANNEL=$(scripts/util/release-channel.sh)

#
# Build
#

./scripts/build-docker.sh

#
# Push
#

echo "Pushing timberio/vector Docker images"

docker login -u "$DOCKER_USERNAME" -p "$DOCKER_PASSWORD"

if [[ "$CHANNEL" == "latest" ]]; then
  docker push timberio/vector:$VERSION-alpine
  docker push timberio/vector:$VERSION-debian
  docker push timberio/vector:latest-alpine
  docker push timberio/vector:latest-debian
elif [[ "$CHANNEL" == "nightly" ]]; then
  docker push timberio/vector:nightly-alpine
  docker push timberio/vector:nightly-debian
fi
