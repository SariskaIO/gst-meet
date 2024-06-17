FROM docker.io/library/alpine:3.18.2 AS builder

RUN apk --no-cache --update upgrade --ignore alpine-baselayout \
    && apk --no-cache add build-base gstreamer-dev gst-plugins-base-dev libnice-dev openssl-dev curl \
    && curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable \
    && source $HOME/.cargo/env \
    && rustup update stable

WORKDIR /app
COPY . .

RUN cargo build --release -p gst-meet

WORKDIR /app/rust-webserver
RUN cargo build --release -p rust-webserver

FROM docker.io/library/alpine:3.18.2

RUN apk --update --no-cache upgrade --ignore alpine-baselayout \
    && apk --no-cache add openssl gstreamer gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-libav libnice libnice-gstreamer

COPY --from=builder /app/target/release/gst-meet /usr/local/bin/
COPY --from=builder /app/rust-webserver/target/release/rust-webserver /usr/local/bin/

ENTRYPOINT ["/usr/local/bin/rust-webserver"]