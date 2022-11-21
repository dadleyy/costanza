from ubuntu:bionic
env DEBIAN_FRONTEND noninteractive
run apt-get update
run apt-get install -y \
  libssl-dev \
  pkg-config \
  gcc-aarch64-linux-gnu \
  gcc-arm-linux-gnueabihf \
  curl \
  wget \
  libudev-dev \
  build-essential \
  git
# download + install rust
run curl --output /tmp/install-rust.sh --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs
run sh /tmp/install-rust.sh -y
run . /root/.cargo/env && rustup target add armv7-unknown-linux-gnueabihf
run . /root/.cargo/env && rustup component add --target armv7-unknown-linux-gnueabihf rust-src rust-std rustc-dev
# download + cross compile openssl
run mkdir -p /etc/arm-dependencies
run cd /etc/arm-dependencies && wget -O openssl-3.0.7.tar.gz https://www.openssl.org/source/openssl-3.0.7.tar.gz
run cd /etc/arm-dependencies && tar xvzf openssl-3.0.7.tar.gz
run cd /etc/arm-dependencies/openssl-3.0.7 && MACHINE=armv7 CC=arm-linux-gnueabihf-gcc ARCH=arm ./Configure linux-generic64 shared
run cd /etc/arm-dependencies/openssl-3.0.7 && MACHINE=armv7 CC=arm-linux-gnueabihf-gcc ARCH=arm make
run chsh -s /bin/bash root
run echo "source /root/.cargo/env" > /root/.bash_profile
entrypoint ["/bin/bash"]
