FROM rust:bullseye AS build

WORKDIR /src

RUN apt update && apt install -y ca-certificates pkg-config libssl-dev libclang-dev
RUN rustup default stable && rustup update

COPY ./Cargo.* /src/
COPY ./components/lumaLang-cli /src/components/lumaLang-cli
COPY ./components/lumaLang-types-rs /src/components/lumaLang-types-rs
COPY ./components/lumaLang-sdk /src/components/lumaLang-sdk

WORKDIR /src/components/lumaLang-cli

RUN mkdir /out
RUN cargo build --features release --release
RUN cp /src/target/release/lumaLang /out

FROM debian:bullseye-slim

RUN apt update && apt install -y ca-certificates libssl-dev

COPY --from=build /out/ /bin/

WORKDIR /workspace

ENTRYPOINT ["lumaLang"]
