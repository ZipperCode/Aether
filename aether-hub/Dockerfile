# syntax=docker/dockerfile:1

FROM rust:1.85-slim AS builder
WORKDIR /build/aether-hub

# 可选：配置国内 Cargo 镜像源（本地构建时传 --build-arg CARGO_MIRROR=1）
ARG CARGO_MIRROR
RUN if [ -n "$CARGO_MIRROR" ]; then \
    printf '[source.crates-io]\nreplace-with = "tuna"\n\n[source.tuna]\nregistry = "sparse+https://mirrors.tuna.tsinghua.edu.cn/crates.io-index/"\n' \
    > /usr/local/cargo/config.toml; \
    fi

# 先构建依赖层，最大化后续代码变更时的缓存命中
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && printf 'fn main() {}\n' > src/main.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/build/aether-hub/target,sharing=locked \
    cargo build --release --locked
RUN rm -rf src

COPY src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/build/aether-hub/target,sharing=locked \
    cargo build --release --locked && \
    cp target/release/aether-hub /tmp/aether-hub

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*
COPY --from=builder /tmp/aether-hub /usr/local/bin/aether-hub

EXPOSE 8085
ENTRYPOINT ["/usr/local/bin/aether-hub"]
CMD ["--bind", "0.0.0.0:8085"]
