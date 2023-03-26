FROM rust:1.67.0 AS chef

RUN rustup target add x86_64-unknown-linux-musl

RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner

COPY Cargo.* ./
COPY prisma-cli/Cargo.toml ./prisma-cli/Cargo.toml
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG BUILD_MODE=release

RUN apt-get update
RUN apt-get install -y --no-install-recommends ca-certificates musl-tools
RUN update-ca-certificates
RUN rm -rf /var/lib/apt/lists/*
COPY --from=planner /app/recipe.json recipe.json

RUN cargo chef cook --profile ${BUILD_MODE} --workspace --recipe-path recipe.json --target x86_64-unknown-linux-musl

COPY ./prisma-cli ./
COPY ./prisma ./
RUN cargo run --bin prisma -p prisma-cli -- generate

COPY . .
RUN ls && echo 213132 && cat Cargo.toml
RUN cargo build --profile ${BUILD_MODE} --bin ozb --target x86_64-unknown-linux-musl

FROM alpine:latest AS runtime
ARG BUILD_DIRECTORY=release

COPY --from=builder /app/target/x86_64-unknown-linux-musl/${BUILD_DIRECTORY}/ozb /usr/local/bin
ENTRYPOINT ["/usr/local/bin/ozb"]
