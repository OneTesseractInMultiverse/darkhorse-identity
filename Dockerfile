FROM node:24.19.0-bookworm-slim@sha256:a9f5f7c91a432850b2a8a7797adf5eadb6c733ceed61167806cee7ea7fbc29df AS console
WORKDIR /build
RUN npm install --global pnpm@11.19.0 --ignore-scripts
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/console/package.json apps/console/package.json
RUN pnpm install --frozen-lockfile
COPY apps/console apps/console
RUN pnpm --filter @darkhorse/console build

FROM rust:1.97.1-bookworm@sha256:0e2bcaef56d041a486784e54104a81aebe0da44bd03019bd70bc0401e42e4a97 AS backend
WORKDIR /build
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates crates
COPY apps/server apps/server
RUN cargo build --locked --release -p darkhorse-server

FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171
COPY --from=backend /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=backend /build/target/release/darkhorse-server /usr/local/bin/darkhorse-server
COPY --from=console /build/apps/console/build /app/console
USER 10001:10001
WORKDIR /app
ENV DARKHORSE_HTTP_HOST=0.0.0.0 DARKHORSE_STATIC_DIR=/app/console
EXPOSE 3001
ENTRYPOINT ["/usr/local/bin/darkhorse-server"]
CMD ["serve"]
