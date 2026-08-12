# Trunnion as a container.
#
# `CLAUDE.md` opens by saying this ships as a container, and until this file
# existed that was a claim with nothing behind it. Two stages: build against
# the full toolchain, run on a slim base carrying the binary and nothing that
# compiles.
#
# Linux, which is the platform the Landlock backend covers. The isolation is
# real inside a plain unprivileged `docker run` with no flags, no
# `--privileged` and no added capabilities, because Landlock is unprivileged
# and kernel-native: that property is the reason it was chosen over bubblewrap
# or a namespace sandbox, both of which would silently no-op here. The host
# kernel still has to provide it. On a kernel below 5.13, or one with Landlock
# off the boot LSM list, `trunnion` records the backend as `none` and the
# isolation claim is honestly unmet rather than quietly assumed.

FROM rust:1-slim-bookworm AS build
WORKDIR /src

# The manifests alone first, so a source-only change does not re-download and
# re-compile every dependency.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() {}' > src/main.rs && echo '' > src/lib.rs \
    && cargo build --release --locked \
    && rm -rf src

COPY . .
# The touch is load-bearing: cargo decides by mtime, and the layer above left
# a newer artifact than the real sources that just replaced it.
RUN touch src/main.rs src/lib.rs && cargo build --release --locked

FROM debian:bookworm-slim AS runtime

# ca-certificates because the gateway reaches a provider over TLS when it is
# pointed at one. git because `trunnion project add <git-url>` shells out to it,
# and the workspace registry is most of the reason to run this in a container
# at all. Nothing else: no compiler, no shell tooling, no package manager
# cache.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates git \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /src/target/release/trunnion /usr/local/bin/trunnion
# The bundled starting harness. `trunnion template init /usr/share/trunnion/templates/laptop <dir>`
# writes a working policy, scoring rules, sensors and a freshly generated
# actor key into an empty directory, so the image can produce a harness
# without the repository.
COPY templates /usr/share/trunnion/templates

# An unprivileged user, because nothing here needs root and the sandbox
# explicitly does not: an isolation backend that required privileges to apply
# would be one that no-ops under a hardened runtime.
RUN useradd --create-home --uid 10001 trunnion
USER trunnion
WORKDIR /harness

# The harness a run reads (config/policy.json, config/scoring.json, sensors)
# and the ledger it writes are the operator's, not the image's, so they are
# mounted rather than baked. An image carrying a ledger would ship an actor
# key every install shared.
VOLUME ["/harness"]

ENTRYPOINT ["trunnion"]
CMD []
