FROM golang:1.26.4-bookworm AS sqlc

WORKDIR /work
RUN git clone https://github.com/sqlc-dev/sqlc
WORKDIR /work/sqlc/cmd/sqlc
RUN git checkout a95e91d70ad9e1181253c333a1cfdd75ae4b95a5
RUN go build

WORKDIR /work
RUN git clone https://github.com/fdietze/sqlc-gen-from-template
WORKDIR /work/sqlc-gen-from-template
RUN git checkout d438acdb75fd0ebfde8101ca7df3fa1c9027cba4
RUN go build

FROM rust:1.96 AS chef

RUN rustup component add rustfmt
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build

COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY --from=sqlc /work/sqlc/cmd/sqlc/sqlc /usr/local/bin/
COPY --from=sqlc /work/sqlc-gen-from-template/sqlc-gen-from-template /usr/local/bin/
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim AS run-deps

RUN apt-get update && apt-get install libsqlite3-0

FROM run-deps AS run

WORKDIR /app
COPY --from=build /app/target/release/harmonius_choir_lite /usr/local/bin
ENTRYPOINT ["/usr/local/bin/harmonius_choir_lite"]
