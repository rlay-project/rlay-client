FROM rust:1.40.0 AS builder

WORKDIR /usr/src/app
COPY . .

RUN cd rlay-client && \
    cargo build \
      --release \
      --features backend_neo4j

# Ubuntu 18.04 as runner image
FROM ubuntu@sha256:5f4bdc3467537cbbe563e80db2c3ec95d548a9145d64453b06939c4592d67b6d

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get -y install ca-certificates libssl-dev && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/rlay-client /rlay-client
COPY docker/rlay.config.toml .

ENTRYPOINT ["/rlay-client"]
CMD ["client"]
