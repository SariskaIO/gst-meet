FROM docker.io/library/alpine:3.18.2 AS builder

# Install rustup and necessary dependencies
RUN apk --no-cache --update upgrade --ignore alpine-baselayout \
 && apk --no-cache add build-base gstreamer-dev gst-plugins-base-dev libnice-dev openssl-dev curl \
 && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
 && source $HOME/.cargo/env \
 && rustup toolchain install 1.72.0 \
 && rustup default 1.72.0

COPY . .
RUN source $HOME/.cargo/env && cargo build --release -p gst-meet

# Continue the build in the same stage
COPY ./rust-webserver .
WORKDIR ./rust-webserver
RUN source $HOME/.cargo/env && cargo build --release -p rust-webserver

# Create the final image
FROM docker.io/library/alpine:3.18.2
RUN apk --update --no-cache upgrade --ignore alpine-baselayout \
 && apk --no-cache add openssl gstreamer gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-libav libnice libnice-gstreamer

# Copy the built binaries from the previous stage
COPY --from=builder /target/release/gst-meet /usr/local/bin/
COPY --from=builder /rust-webserver/target/release/rust-webserver /usr/local/bin/

# Set the entrypoint
ENTRYPOINT ["/usr/local/bin/rust-webserver"]