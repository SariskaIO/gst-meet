FROM docker.io/library/alpine:3.18.2 AS builder
RUN apk --no-cache --update upgrade --ignore alpine-baselayout \
 && apk --no-cache add build-base gstreamer-dev gst-plugins-base-dev libnice-dev openssl-dev cargo
COPY . .
RUN cargo build --release -p gst-meet

# Continue with the build in the same stage
COPY ./rust-webserver .
WORKDIR ./rust-webserver

# Update `actix-http` to a version compatible with Rust 1.71.1
RUN cargo add --package rust-webserver actix-http@=3.4.0

RUN cargo build --release -p rust-webserver

# Create the final image
FROM docker.io/library/alpine:3.18.2

# Install necessary runtime dependencies
RUN apk --update --no-cache upgrade --ignore alpine-baselayout \
 && apk --no-cache add openssl gstreamer gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-libav libnice libnice-gstreamer

# Copy the built binaries from the previous stage
COPY --from=builder target/release/gst-meet /usr/local/bin/
COPY --from=builder rust-webserver/target/release/rust-webserver /usr/local/bin/

# Set the entrypoint
ENTRYPOINT ["/usr/local/bin/rust-webserver"]