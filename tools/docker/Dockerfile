# The image of a release: the `rustc4` the release already built, on the
# least that runs it. Nothing is compiled here. The release workflow
# (.github/workflows/release.yml) builds it, from a directory holding
#
#   linux-amd64/rustc4   linux-arm64/rustc4
#
# out of the two Linux archives, and so can anyone:
#
#   docker build -f tools/docker/Dockerfile -t rustc4 <that directory>
#
# Both images are named by digest, so the same Dockerfile builds the same
# image; to take a newer one, `docker buildx imagetools inspect <name>`.

# The one thing the binary wants besides a C library: a font to draw the
# text of a PNG with (cli/src/png.rs). Liberation Sans is one a diagram
# asks for by name, and has the widths of the Helvetica it was laid out for.
# Fonts are the same files for every architecture, so this stage runs on
# the machine that builds, whatever it builds for.
FROM --platform=$BUILDPLATFORM debian:13-slim@sha256:8aa5a7bf5a1955bddbe7d74fcdd0b91ab30e74362a8b73af672b0481b35b44a0 AS fonts
RUN apt-get update \
    && apt-get install --yes --no-install-recommends fonts-liberation \
    && mkdir /fonts \
    && cp /usr/share/fonts/truetype/liberation/LiberationSans-*.ttf /fonts/ \
    && cp /usr/share/doc/fonts-liberation/copyright /fonts/LICENSE

# glibc and nothing else to speak of: no shell, no package manager, no
# OpenSSL. The binary is linked against glibc (libc, libm and the names
# that are now part of it) and against nothing more, so `static` and
# `scratch` are too little and `cc` is more than it uses.
FROM gcr.io/distroless/base-nossl-debian13@sha256:f21fdfe4ede467d9ea8a6caaec1f8381ea4502243a3e8bb1c43df80c6fbaaeae
ARG TARGETARCH
COPY --from=fonts /fonts /usr/share/fonts/liberation
COPY linux-${TARGETARCH}/rustc4 /usr/local/bin/rustc4

# Inside a container, this machine only is nobody: `serve` listens to
# whoever the port is published to.
ENV RUSTC4_HOST=0.0.0.0
EXPOSE 8080

# Where a model is mounted: `docker run -v "$PWD:/work" <image> check .`
WORKDIR /work
ENTRYPOINT ["/usr/local/bin/rustc4"]
CMD ["--help"]
