FROM rust:1.93.0-slim-bookworm@sha256:776861219cd851131c1cec3bbd7cbeb16b99a794048097eb69ad9682a8ed0d57
RUN apt-get update && apt-get install -y --no-install-recommends git tmux asciinema python3 patch procps ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock .gitignore README.md SPEC.md ./
COPY src ./src
RUN git init && git -c user.name=Benchmark -c user.email=benchmark@localhost add . && git -c user.name=Benchmark -c user.email=benchmark@localhost commit -m 'Initial candidate workspace'
RUN cargo build --locked --offline
CMD ["/app/target/debug/clob-sim"]
