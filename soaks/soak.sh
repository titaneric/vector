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
    echo "  --soak: the experiment to run, default all; space delimited"
    echo "  --build-image: build the soak image if needed, default true"
    echo "  --baseline: the baseline SHA to compare against"
    echo "  --comparison: the SHA to compare against 'baseline'"
    echo "  --cpus: the total number of CPUs to dedicate to the soak minikube, default 7"
    echo "  --memory: the total amount of memory dedicate to the soak minikube, default 8g"
    echo "  --vector-cpus: the total number of CPUs to give to soaked vector, default 4"
    echo ""
}

BUILD_IMAGE="true"
SOAK_CPUS="7"
SOAK_MEMORY="8g"
VECTOR_CPUS="4"
WARMUP_SECONDS="30"
SOAKS=""

while [[ $# -gt 0 ]]; do
  key="$1"

  case $key in
      --soak)
          SOAKS=$2
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
      --no-build-image)
          BUILD_IMAGE="true"
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

if [ -z "${SOAKS}" ]; then
    SOAKS=$(ls tests/)
fi

for SOAK_NAME in ${SOAKS}; do
    echo "########"
    echo "########"
    echo "########    ${SOAK_NAME}"
    echo "########"
    echo "########"
    capture_dir_replica_one="${capture_dir}/1"
    capture_dir_replica_two="${capture_dir}/2"
    capture_dir_replica_three="${capture_dir}/3"
    # shellcheck disable=SC2015
    ./bin/soak_one.sh --build-image "${BUILD_IMAGE}" \
                      --soak "${SOAK_NAME}" \
                      --variant "baseline" \
                      --tag "${BASELINE}" \
                      --capture-dir "${capture_dir_replica_one}" \
                      --cpus "${SOAK_CPUS}" \
                      --memory "${SOAK_MEMORY}" \
                      --warmup-seconds "${WARMUP_SECONDS}" \
                      --vector-cpus "${VECTOR_CPUS}" && \
    ./bin/soak_one.sh --build-image "${USE_BUILD_IMAGE}" \
                      --soak "${SOAK_NAME}" \
                      --variant "baseline" \
                      --tag "${BASELINE}" \
                      --capture-dir "${capture_dir_replica_two}" \
                      --cpus "${SOAK_CPUS}" \
                      --memory "${SOAK_MEMORY}" \
                      --warmup-seconds "${WARMUP_SECONDS}" \
                      --vector-cpus "${VECTOR_CPUS}" && \
    ./bin/soak_one.sh --build-image "${USE_BUILD_IMAGE}" \
                      --soak "${SOAK_NAME}" \
                      --variant "baseline" \
                      --tag "${BASELINE}" \
                      --capture-dir "${capture_dir_replica_three}" \
                      --cpus "${SOAK_CPUS}" \
                      --memory "${SOAK_MEMORY}" \
                      --warmup-seconds "${WARMUP_SECONDS}" \
                      --vector-cpus "${VECTOR_CPUS}" && \
    ./bin/soak_one.sh --build-image "${USE_BUILD_IMAGE}" \
                      --soak "${SOAK_NAME}" \
                      --variant "comparison" \
                      --tag "${COMPARISON}" \
                      --capture-dir "${capture_dir_replica_one}" \
                      --cpus "${SOAK_CPUS}" \
                      --memory "${SOAK_MEMORY}" \
                      --warmup-seconds "${WARMUP_SECONDS}" \
                      --vector-cpus "${VECTOR_CPUS}" && \
    ./bin/soak_one.sh --build-image "${USE_BUILD_IMAGE}" \
                      --soak "${SOAK_NAME}" \
                      --variant "comparison" \
                      --tag "${COMPARISON}" \
                      --capture-dir "${capture_dir_replica_two}" \
                      --cpus "${SOAK_CPUS}" \
                      --memory "${SOAK_MEMORY}" \
                      --warmup-seconds "${WARMUP_SECONDS}" \
                      --vector-cpus "${VECTOR_CPUS}" && \
    ./bin/soak_one.sh --build-image "${USE_BUILD_IMAGE}" \
                      --soak "${SOAK_NAME}" \
                      --variant "comparison" \
                      --tag "${COMPARISON}" \
                      --capture-dir "${capture_dir_replica_three}" \
                      --cpus "${SOAK_CPUS}" \
                      --memory "${SOAK_MEMORY}" \
                      --warmup-seconds "${WARMUP_SECONDS}" \
                      --vector-cpus "${VECTOR_CPUS}" || true
done

# Aggregate all captures and analyze them.
./bin/analyze_experiment --capture-dir "${capture_dir}" \
                         --baseline-sha "${BASELINE}" \
                         --comparison-sha "${COMPARISON}" \
                         --vector-cpus "${VECTOR_CPUS}" \
                         --warmup-seconds "${WARMUP_SECONDS}" \
                         --p-value 0.05 # 95% confidence

popd
