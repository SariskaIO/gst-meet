FROM docker.io/library/alpine:3.19 AS builder

# Add edge repository for newer GStreamer version
RUN echo "http://dl-cdn.alpinelinux.org/alpine/edge/main" >> /etc/apk/repositories && \
    echo "http://dl-cdn.alpinelinux.org/alpine/edge/community" >> /etc/apk/repositories

# Update and install dependencies
RUN apk --no-cache --update upgrade --ignore alpine-baselayout \
    && apk --no-cache add \
        build-base \
        gstreamer-dev=1.24.10-r0 \
        gst-plugins-base-dev \
        glib-dev \
        libnice-dev \
        openssl-dev \
        cargo

COPY . .
RUN cargo build --release -p gst-meet

# Continue the build in the same stage
COPY ./rust-webserver .
WORKDIR ./rust-webserver
RUN cargo build --release -p rust-webserver

# Create the final image
FROM docker.io/library/alpine:3.19

# Add edge repository
RUN echo "http://dl-cdn.alpinelinux.org/alpine/edge/main" >> /etc/apk/repositories && \
    echo "http://dl-cdn.alpinelinux.org/alpine/edge/community" >> /etc/apk/repositories

RUN apk --update --no-cache upgrade --ignore alpine-baselayout \
    && apk --no-cache add \
        openssl \
        gstreamer=1.24.10-r0 \
        gst-plugins-good \
        gst-plugins-bad \
        gst-plugins-ugly \
        gst-libav \
        glib \
        libnice \
        libnice-gstreamer

# Copy the built binaries from the previous stage
COPY --from=builder target/release/gst-meet /usr/local/bin/
COPY --from=builder rust-webserver/target/release/rust-webserver /usr/local/bin/

# Set the entrypoint
ENTRYPOINT ["/usr/local/bin/rust-webserver"]