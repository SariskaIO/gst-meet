FROM docker.io/library/alpine:3.18.2 AS builder

# Install build dependencies
RUN apk --no-cache --update upgrade --ignore alpine-baselayout && \
    apk --no-cache add build-base meson ninja pkgconf gstreamer-dev gst-plugins-base-dev gst-plugins-bad-dev libsrt-dev libsrt glib-dev python3 openssl-dev cargo

# Build gst-plugins-bad with SRT support
RUN wget https://gstreamer.freedesktop.org/src/gst-plugins-bad/gst-plugins-bad-1.24.10.tar.xz && \
    tar xf gst-plugins-bad-1.24.10.tar.xz && \
    cd gst-plugins-bad-1.24.10 && \
    meson setup builddir -Dsrt=enabled -Dgpl=enabled && \
    ninja -C builddir && \
    ninja -C builddir install

# Rest of your build process
COPY . .
RUN cargo build --release -p gst-meet

COPY ./rust-webserver .
WORKDIR ./rust-webserver
RUN cargo build --release -p rust-webserver

# Create the final image
FROM docker.io/library/alpine:3.18.2
RUN apk --update --no-cache upgrade --ignore alpine-baselayout \
 && apk --no-cache add openssl gstreamer gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-libav libnice libnice-gstreamer

# Copy the built binaries from the previous stage
COPY --from=builder target/release/gst-meet /usr/local/bin/
COPY --from=builder rust-webserver/target/release/rust-webserver /usr/local/bin/

# Set the entrypoint
ENTRYPOINT ["/usr/local/bin/rust-webserver"]
