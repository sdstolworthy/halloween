# Porch eyes browser emulator. Build context: eyes/ (the Coolify app's base directory is /eyes).
# Base images are pinned by tag and digest; bump both together.

# 1. Build the wasm. The image tag pins the same Rust as rust-toolchain.toml does for local builds.
FROM rust:1.96.0-slim@sha256:c37af730be4fd8104cbf9aedbd6ab259e51ca2d5437817a0f8680edf66ac6c28 AS wasm
RUN rustup target add wasm32-unknown-unknown
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY eyes-core eyes-core
COPY eyes-render eyes-render
COPY eyes-hal eyes-hal
COPY eyes-emulator eyes-emulator
COPY eyes-web eyes-web
RUN ./eyes-web/build-wasm.sh

# 2. Refuse to ship a build whose pixels differ from the host renderer's (and so the firmware's).
FROM node:22.20-alpine@sha256:dbcedd8aeab47fbc0f4dd4bffa55b7c3c729a707875968d467aaaea42d6225af AS smoke
WORKDIR /web
COPY eyes-web/web/smoke.mjs ./
COPY --from=wasm /src/eyes-web/web/eyes_web.wasm ./
RUN node smoke.mjs && touch /smoke-passed

# 3. Serve the static site.
FROM nginx:1.29-alpine@sha256:5616878291a2eed594aee8db4dade5878cf7edcb475e59193904b198d9b830de
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY eyes-web/web/index.html eyes-web/web/main.js eyes-web/web/style.css /usr/share/nginx/html/
COPY --from=wasm /src/eyes-web/web/eyes_web.wasm /usr/share/nginx/html/
COPY --from=smoke /smoke-passed /tmp/smoke-passed
EXPOSE 80
# Coolify gates rolling deploys on this.
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD wget -qO- http://127.0.0.1/healthz >/dev/null 2>&1 || exit 1
