ARG BINARY_NAME

FROM rust:1.79.0 AS chef

RUN rustup target add x86_64-unknown-linux-musl

RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
ARG BINARY_NAME

COPY Cargo.* ./
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG BUILD_MODE=release
ARG BINARY_NAME

RUN apt-get update
RUN apt-get install -y --no-install-recommends ca-certificates musl-tools
RUN update-ca-certificates
RUN rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json

RUN cargo chef cook --profile ${BUILD_MODE} --bin prisma --recipe-path recipe.json --target x86_64-unknown-linux-musl

COPY ./prisma /app/prisma
COPY ./src/bin/prisma.rs /app/src/bin/
RUN cargo run --profile ${BUILD_MODE} --target x86_64-unknown-linux-musl --bin prisma -- generate

COPY . .
RUN cargo build --profile ${BUILD_MODE} --features="prisma" --bin ${BINARY_NAME} --target x86_64-unknown-linux-musl

FROM alpine:latest AS runtime
ARG BUILD_DIRECTORY=release
ARG BINARY_NAME

COPY --from=builder /app/target/x86_64-unknown-linux-musl/${BUILD_DIRECTORY}/${BINARY_NAME} /usr/local/bin/app
ENTRYPOINT ["/usr/local/bin/app"]
