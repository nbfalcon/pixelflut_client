# Debian 13 (trixie)
FROM docker.io/debian:trixie

ENV DEBIAN_FRONTEND=noninteractive

# ---- System dependencies (as root) ----
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    build-essential \
    pkg-config \
    libssl-dev \
    libgstreamer1.0-dev \
    libgstreamer-plugins-base1.0-dev \
    gstreamer1.0-tools \
    gstreamer1.0-plugins-base \
    gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-bad \
    gstreamer1.0-plugins-ugly \
    sudo

# ---- Create non-root user ----
RUN useradd -m -s /bin/bash run

# ---- Switch to non-root user ----
USER run
WORKDIR /home/run

# ---- Rust (user-local install) ----
ENV RUSTUP_HOME=/home/run/.rustup \
    CARGO_HOME=/home/run/.cargo \
    PATH=/home/run/.cargo/bin:$PATH

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path \
    && rustup toolchain install nightly \
    && rustup default nightly

# ---- Optional sanity checks ----
RUN rustc --version && cargo --version && gst-launch-1.0 --version

WORKDIR /workspace
