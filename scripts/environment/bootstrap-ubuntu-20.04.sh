#! /usr/bin/env bash
set -e -o verbose

export DEBIAN_FRONTEND=noninteractive

echo 'APT::Acquire::Retries "5";' > /etc/apt/apt.conf.d/80-retries

apt update --yes

apt install --yes \
  software-properties-common \
  apt-utils \
  apt-transport-https

# This is a workaround for GH https://github.com/actions/virtual-environments/issues/1605
# This fix will be removed when GH addresses the issue.
# What's happening here? Well we use this script inside CI and Docker containers.
# It'll run fine in CI because update-grub can find a root parition.
# Inside a container it'll fail. Either way we don't care about the outcome of the command
# so we ignore its exit.

set +e
apt-get install --yes grub-efi
update-grub
set -e

apt upgrade --yes

# Deps
apt install --yes \
    build-essential \
    cmake \
    pkg-config \
    libssl-dev \
    python3-pip \
    jq \
    shellcheck \
    locales \
    ca-certificates \
    curl \
    gnupg-agent \
    gnuplot \
    nodejs \
    npm \
    ruby-bundler \
    libsasl2-dev \
    gnupg2 \
    wget \
    gawk \
    yarn \
    sudo \
    cmark-gfm \
    rename \
    rpm

# Cue
TEMP=$(mktemp -d)
curl \
    -L https://github.com/cuelang/cue/releases/download/v0.3.0-alpha5/cue_0.3.0-alpha5_Linux_x86_64.tar.gz \
    -o "${TEMP}/cue_0.3.0-alpha5_Linux_x86_64.tar.gz"
tar \
    -xvf "${TEMP}/cue_0.3.0-alpha5_Linux_x86_64.tar.gz" \
    -C "${TEMP}"
cp "${TEMP}/cue" /usr/bin/cue

# Grease
# Grease is used for the `make release-github` task.
TEMP=$(mktemp -d)
curl \
    -L https://github.com/timberio/grease/releases/download/v1.0.1/grease-1.0.1-linux-amd64.tar.gz \
    -o "${TEMP}/grease-1.0.1-linux-amd64.tar.gz"
tar \
    -xvf "${TEMP}/grease-1.0.1-linux-amd64.tar.gz" \
    -C "${TEMP}"
cp "${TEMP}/grease/bin/grease" /usr/bin/grease

# Locales
locale-gen en_US.UTF-8
dpkg-reconfigure locales

# Rust
curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal

if ! [ -x "$(command -v docker)" ]; then
    # Docker
    curl -fsSL https://download.docker.com/linux/ubuntu/gpg | apt-key add -
    add-apt-repository \
        "deb [arch=amd64] https://download.docker.com/linux/ubuntu \
        xenial \
        stable"
    # Install those new things
    apt update --yes
    apt install --yes docker-ce docker-ce-cli containerd.io
fi

# Apt cleanup
apt clean
