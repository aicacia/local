ARG RUST_IMAGE=rust:1.98-trixie
FROM ${RUST_IMAGE} AS chef

RUN apt-get update && apt-get -yq upgrade
RUN apt-get -yq install musl-tools

WORKDIR /app

ARG TARGETPLATFORM
RUN set -e; \
  case "${TARGETPLATFORM}" in \
  linux/amd64) TARGET_TRIPLE="x86_64-unknown-linux-musl" ;; \
  linux/arm64) TARGET_TRIPLE="aarch64-unknown-linux-musl" ;; \
  linux/arm/v7) TARGET_TRIPLE="armv7-unknown-linux-musleabihf" ;; \
  linux/arm/v6) TARGET_TRIPLE="arm-unknown-linux-musleabi" ;; \
  linux/386) TARGET_TRIPLE="i686-unknown-linux-musl" ;; \
  *) TARGET_TRIPLE="x86_64-unknown-linux-musl" ;; \
  esac; \
  echo "${TARGET_TRIPLE}" > /tmp/target

RUN rustup target add $(cat /tmp/target)

RUN cargo install cargo-chef --locked


FROM chef AS planner

WORKDIR /app

COPY . .
RUN cargo chef prepare --recipe-path recipe.json


FROM chef AS builder

WORKDIR /app

COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --target $(cat /tmp/target) --recipe-path recipe.json

ARG PROJECT=local

COPY . .
RUN rustup target add $(cat /tmp/target)
RUN cargo build -p ${PROJECT} --target $(cat /tmp/target) --release --bin ${PROJECT}


FROM scratch
LABEL org.opencontainers.image.source=https://github.com/aicacia/rs-local

WORKDIR /app

ARG PROJECT=local

COPY --from=builder /app/target/*/release/${PROJECT} /app/run

CMD ["/app/run", "-c", "/app/config.yaml"]
