# Entorno reproducible: mismas herramientas que CI (tests + llvm-cov al 95 %).
FROM rust:1.85-bookworm

RUN apt-get update \
  && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    libgit2-dev libssl-dev pkg-config cmake llvm libclang-dev clang \
  && rm -rf /var/lib/apt/lists/*

RUN rustup component add llvm-tools-preview \
  && cargo install cargo-llvm-cov --locked --version 0.6.16

WORKDIR /app
ENV CARGO_TERM_COLOR=always

# El código se monta por volumen (docker compose); aquí solo dependencias.
CMD ["cargo", "test", "--release"]
