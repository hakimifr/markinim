FROM rust:1-slim AS builder

WORKDIR /code
COPY Cargo.toml Cargo.lock ./
# Build the dependency tree first so code edits don't invalidate the cache
RUN mkdir src && echo "pub fn stub() {}" > src/lib.rs && echo "fn main() {}" > src/main.rs && \
    cargo build --release && rm -rf src

COPY . .
RUN cargo build --release && cp target/release/markinim /code/markinim

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /code
COPY --from=builder /code/markinim /code/markinim

CMD [ "./markinim" ]
