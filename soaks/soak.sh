#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
#set -o xtrace

__dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

display_usage() {
    echo ""
    echo "Usage: soak [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --help: display this information"
    echo "  --soak: the experiment to run"
    echo "  --remote-image|--local-image: whether to use a local vector image or remote"
    echo "  --baseline: the baseline SHA to compare against"
    echo "  --comparison: the SHA to compare against 'baseline'"
    echo "  --cpus: the total number of CPUs to dedicate to the soak minikube, default 7"
    echo "  --memory: the total amount of memory dedicate to the soak minikube, default 8g"
    echo "  --vector-cpus: the total number of CPUs to give to soaked vector, default 4"
    echo ""
}

USE_LOCAL_IMAGE="true"
SOAK_CPUS="7"
SOAK_MEMORY="8g"
VECTOR_CPUS="4"

while [[ $# -gt 0 ]]; do
  key="$1"

  case $key in
      --soak)
          SOAK_NAME=$2
          shift # past argument
          shift # past value
          ;;
      --baseline)
          BASELINE=$2
          shift # past argument
          shift # past value
          ;;
      --comparison)
          COMPARISON=$2
          shift # past argument
          shift # past value
          ;;
      --remote-image)
          USE_LOCAL_IMAGE="false"
          shift # past argument
          ;;
      --local-image)
          USE_LOCAL_IMAGE="true"
          shift # past argument
          ;;
      --vector-cpus)
          VECTOR_CPUS=$2
          shift # past argument
          shift # past value
          ;;
      --cpus)
          SOAK_CPUS=$2
          shift # past argument
          shift # past value
          ;;
      --memory)
          SOAK_MEMORY=$2
          shift # past argument
          shift # past value
          ;;
      --help)
          display_usage
          exit 0
          ;;
      *)
          echo "unknown option"
          display_usage
          exit 1
          ;;
  esac
done

pushd "${__dir}"

capture_dir=$(mktemp -d /tmp/soak-captures.XXXXXX)
echo "Captures will be recorded into ${capture_dir}"

./bin/soak_one.sh --local-image "${USE_LOCAL_IMAGE}" \
                  --soak "${SOAK_NAME}" \
                  --variant "baseline" \
                  --tag "${BASELINE}" \
                  --capture-dir "${capture_dir}" \
                  --cpus "${SOAK_CPUS}" \
                  --memory "${SOAK_MEMORY}" \
                  --vector-cpus "${VECTOR_CPUS}"
./bin/soak_one.sh --local-image "${USE_LOCAL_IMAGE}" \
                  --soak "${SOAK_NAME}" \
                  --variant "comparison" \
                  --tag "${COMPARISON}" \
                  --capture-dir "${capture_dir}" \
                  --cpus "${SOAK_CPUS}" \
                  --memory "${SOAK_MEMORY}" \
                  --vector-cpus "${VECTOR_CPUS}"

./bin/analyze_experiment.sh --capture-dir "${capture_dir}" \
                            --baseline "${BASELINE}" \
                            --comparison "${COMPARISON}" \
                            --vector-cpus "${VECTOR_CPUS}"

popd
