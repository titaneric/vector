#!/usr/bin/env bash

set -o errexit
set -o pipefail
set -o nounset
#set -o xtrace

__dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOAK_ROOT="${__dir}/.."

display_usage() {
    echo ""
    echo "Usage: run_experiment [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --help: display this information"
    echo "  --soak: the experiment to run"
    echo "  --local-image: whether to use a local vector image or remote, local if true"
    echo "  --variant: the variation of test in play, either 'baseline' or 'comparison'"
    echo "  --tag: the tag this test covers"
    echo "  --capture-dir: the directory in which to write captures"
    echo "  --cpus: the total number of CPUs to dedicate to the soak minikube, default 7"
    echo "  --memory: the total amount of memory dedicate to the soak minikube, default 8g"
    echo "  --vector-cpus: the total number of CPUs to give to soaked vector"
    echo "  --warmup-seconds: the total number seconds to pause waiting for vector to warm up"
    echo ""
}

while [[ $# -gt 0 ]]; do
  key="$1"

  case $key in
      --soak)
          SOAK_NAME=$2
          shift # past argument
          shift # past value
          ;;
      --variant)
          VARIANT=$2
          shift # past argument
          shift # past value
          ;;
      --image)
          IMAGE=$2
          shift # past argument
          shift # past value
          ;;
      --capture-dir)
          CAPTURE_DIR=$2
          shift # past argument
          shift # past value
          ;;
      --vector-cpus)
          VECTOR_CPUS=$2
          shift # past argument
          shift # past value
          ;;
      --warmup-seconds)
          WARMUP_SECONDS=$2
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
          echo "unknown option: ${key}"
          display_usage
          exit 1
          ;;
  esac
done

TOTAL_SAMPLES=200
SOAK_CAPTURE_DIR="${CAPTURE_DIR}/${SOAK_NAME}"
SOAK_CAPTURE_FILE="${SOAK_CAPTURE_DIR}/${VARIANT}.captures"

pushd "${__dir}"
./boot_minikube.sh --cpus "${SOAK_CPUS}" --memory "${SOAK_MEMORY}"
mkdir --parents "${SOAK_CAPTURE_DIR}"
minikube image load "${IMAGE}"
# Mount the capture directory. This is where the samples captured from inside
# the minikube will be placed on the host.
minikube mount "${SOAK_CAPTURE_DIR}:/captures" &
CAPTURE_MOUNT_PID=$!
popd

pushd "${SOAK_ROOT}/tests/${SOAK_NAME}/terraform"
terraform init
terraform apply -var "experiment_name=${SOAK_NAME}" -var "type=${VARIANT}" \
          -var "vector_image=${IMAGE}" -var "vector_cpus=${VECTOR_CPUS}" \
          -var "lading_image=ghcr.io/blt/lading:sha-a3340ad8b31a7480bc73e0a2c9d7d8c7e2df5a9e" \
          -auto-approve -compact-warnings -input=false -no-color
echo "[${VARIANT}] Captures will be recorded into ${SOAK_CAPTURE_DIR}"
echo "[${VARIANT}] Sleeping for ${WARMUP_SECONDS} seconds to allow warm-up"
sleep "${WARMUP_SECONDS}"
echo "[${VARIANT}] Waiting for captures file to become available"
while [ ! -f "${SOAK_CAPTURE_FILE}" ]; do sleep 1; done
echo "[${VARIANT}] Recording captures to ${SOAK_CAPTURE_DIR}. Waiting for ${TOTAL_SAMPLES} sample periods."
periods=0
recorded_samples=0
while [ $periods -le $TOTAL_SAMPLES ]
do
    # Check that the capture file grows monotonically. If it shrinks this
    # indicates a serious problem.
    observed_samples=$(wc -l "${SOAK_CAPTURE_FILE}" | awk '{print $1}')
    # shellcheck disable=SC2086
    if [ $recorded_samples -gt $observed_samples ]; then
        echo "SAMPLES LOST. THIS IS A CATASTROPHIC, UNRECOVERABLE FAILURE."
        exit 1
    fi
    recorded_samples=$observed_samples
    (( periods = periods + 1 ))
    sleep 1
done
echo "[${VARIANT}] Recording captures to ${SOAK_CAPTURE_DIR} complete. At least ${recorded_samples} collected."
kill "${CAPTURE_MOUNT_PID}"
popd

pushd "${__dir}"
./shutdown_minikube.sh
popd
