#!/usr/bin/env bash
# SPDX-License-Identifier: BUSL-1.1
# The viewer's browser journeys: a headless Chromium driving the bundle a real
# FerroTERM serves, over WebDriver (https://www.w3.org/TR/webdriver2/).
#
#   scripts/ui-e2e.sh
#   scripts/ui-e2e.sh --base-url URL --webdriver URL
#   scripts/ui-e2e.sh --base-url URL --webdriver URL \
#     --signed-in-base-url URL --issuer URL
#   scripts/ui-e2e.sh --docs-shots
#
# The battery must not change a tracked file. The documentation capture pass is
# the one exception, and it runs only when --docs-shots asks for it: it drives
# the same deployment and writes one PNG per viewer screen into
# website/book/src/operate/img/viewer, which the book embeds. An ordinary run,
# on a pull request or on a laptop, never rewrites an image.
#
# Without arguments the script owns everything it drives. It builds the bundle
# with Trunk, builds the server with the bundle inside it, builds the image
# from docker/Dockerfile the way the release lane stages it, and runs that
# image and a pinned Chromium beside it on a private container network. The
# journeys then run against the image the project actually ships.
#
# The signed-in journeys need a second deployment, because a server either
# configures an identity provider or does not and the read-only journeys assert
# the second shape. The managed mode puts every name the browser addresses on
# one private container network, so every run renders the same addresses:
#
#   ferroterm         the server image with no issuer, on http port 8080. The
#                     read-only journeys drive it.
#   ferroterm-origin  the same image with an issuer: FERROTERM_OIDC_ISSUER,
#                     FERROTERM_BASE_URL=https://ferroterm-smart, a viewer
#                     client id, SSL_CERT_FILE naming the run's certificate
#                     authority, and its resource store on a tmpfs.
#   ferroterm-smart   Caddy, terminating TLS with a certificate that authority
#                     signed, with ferroterm-origin:8080 as its upstream. The
#                     browser signs in here, because PKCE needs a secure context.
#   issuer            the stub identity provider (e2e/src/bin/stub_issuer.rs),
#                     run on the host over TLS. It writes the authority and the
#                     Caddy certificate. The second server and the browser both
#                     reach it as issuer through --add-host issuer:host-gateway,
#                     so both use one address and it runs without --public.
#   the browser       the pinned Chromium, trusting the authority through a
#                     Chromium policy. Its WebDriver port is published to the
#                     host, which is where the journeys run.
#
# In the manual mode you start the pair yourself and name them with
# --signed-in-base-url and --issuer; without them those journeys skip and say
# so. The managed mode builds linux binaries for the image, so on macOS this is
# the way to run the whole battery. With the servers on the host and the
# browser in a container, the server reaches the issuer as localhost and the
# browser reaches it as host.docker.internal, and --public is that split. With
# Docker Desktop, from the repository root, after the two trunk builds below and
# a `cargo build --release -p ferroterm-server --features ui` that names them in
# FERROTERM_UI_BUNDLE and FERROTERM_UI_EDITOR_BUNDLE:
#
#   cargo run --manifest-path e2e/Cargo.toml --bin stub-issuer -- \
#     --listen 0.0.0.0:18443 --issuer https://localhost:18443 \
#     --public https://host.docker.internal:18443 \
#     --name localhost --name host.docker.internal \
#     --ca /tmp/e2e/ca.pem --serve-name ferroterm-smart --serve-dir /tmp/e2e/certs
#   FERROTERM_USAGE_REPORT=off FERROTERM_LISTEN=0.0.0.0:8080 FERROTERM_UI=on \
#     FERROTERM_CODESYSTEMS=e2e/fixtures/codesystems target/release/ferroterm
#   FERROTERM_USAGE_REPORT=off FERROTERM_LISTEN=0.0.0.0:8081 FERROTERM_UI=on \
#     FERROTERM_CODESYSTEMS=e2e/fixtures/codesystems \
#     FERROTERM_BASE_URL=https://ferroterm-smart \
#     FERROTERM_OIDC_ISSUER=https://localhost:18443 \
#     FERROTERM_VIEWER_CLIENT_ID=ferroterm-viewer-e2e \
#     FERROTERM_RESOURCES=/tmp/e2e/resources.redb SSL_CERT_FILE=/tmp/e2e/ca.pem \
#     target/release/ferroterm
#
# then a network with Caddy aliased ferroterm-smart on it, serving
# /tmp/e2e/certs/cert.pem and key.pem with host.docker.internal:8081 as its
# upstream, and the browser container on the same network with the authority
# as a Chromium policy, both started the way the managed mode below starts them.
# Pass --base-url http://host.docker.internal:8080, --webdriver
# http://127.0.0.1:4444, --signed-in-base-url https://ferroterm-smart and
# --issuer https://host.docker.internal:18443, which is the --public address
# because --issuer is where the BROWSER reaches the issuer. On a Linux host,
# give both containers --add-host host.docker.internal:host-gateway, which
# Docker Desktop provides by itself.
#
# With --base-url and --webdriver it builds and starts nothing and drives what
# you already have. Both are required together, because a browser that cannot
# reach the address is a red lane with no defect behind it: a browser in a
# container reaches a server on the host as host.docker.internal, not as
# 127.0.0.1. A server started that way serves the journeys' own fixture only if
# it was pointed at e2e/fixtures/codesystems with FERROTERM_CODESYSTEMS, which
# the tree journey needs and the capture pass needs on every screen.
#
# FERROTERM_UI_E2E_SHOTS_DIR sends the capture somewhere other than the book,
# for looking at a shot without touching the checkout.
#
# A journey that fails writes what it was looking at into
# target/ui-e2e-failures: a screenshot and the whole document, one pair per
# failed wait. A passing run leaves the directory empty, and the ui-e2e CI job
# uploads it as an artifact only when the run failed.
#
# A capture renders the FHIR base the viewer read, so whatever address the
# browser used lands in the images. The managed mode gives the server the fixed
# network alias below; a manual capture reproduces the same images by serving on
# 8080 and starting the browser container with
# --add-host ferroterm:host-gateway, then passing --base-url http://ferroterm:8080.
#
# The image stages linux binaries, so the managed mode needs a Linux host.
# Anywhere else it says so, names the manual recipe above, and stops rather
# than reporting a lane it did not run.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

# The browser, pinned by index digest so every run drives the same Chromium and
# the same chromedriver. Selenium publishes the pair in one image and keeps
# them in step; the tag records which pair this digest is.
readonly BROWSER_IMAGE="selenium/standalone-chromium:4.48.0-20260905@sha256:fcf9eef47b9546a2252937481a8298ce0958d20c9d91e040d480184e80b41c76"

# The tag the locally built server image is loaded under. It is never pushed.
readonly SERVER_IMAGE="ferroterm-ui-e2e:local"

# The name the browser addresses the server by on the private network.
readonly SERVER_HOST="ferroterm"

# The name the browser addresses the second deployment by: the one with a SMART
# issuer configured, which the signed-in journeys drive. One server cannot be
# both, because the read-only journeys assert that a deployment without an
# issuer offers no sign-in.
#
# It is a TLS terminator in front of the server, the way a deployment runs one,
# because a browser gives `crypto.subtle` to a secure context alone
# (https://developer.mozilla.org/en-US/docs/Web/Security/Secure_Contexts) and
# the PKCE verifier is drawn with it.
readonly SIGNED_IN_HOST="ferroterm-smart"

# The name that terminator forwards to, which nothing else addresses.
readonly SIGNED_IN_ORIGIN_HOST="ferroterm-origin"

# The terminator, pinned by digest, and the same image compose.yaml puts in
# front of a proxied deployment.
readonly PROXY_IMAGE="docker.io/library/caddy:2.11.4-alpine@sha256:5f5c8640aae01df9654968d946d8f1a56c497f1dd5c5cda4cf95ab7c14d58648"

# The name both containers address the stub identity provider by. It runs on
# the host, so each container reaches it through the host gateway, and the
# certificate is issued for this name.
readonly ISSUER_HOST="issuer"

# The OAuth client the second server publishes for the viewer to present.
readonly VIEWER_CLIENT_ID="ferroterm-viewer-e2e"

# Seconds to wait for the server container to answer /health, and for the
# browser container to report itself ready.
readonly READY_TIMEOUT=120

# How many journeys drive the browser at once. The Selenium image allows one
# session per container by default, so a second journey would sit in the grid's
# new-session queue until the first quit; the image's own README says to raise
# the ceiling with SE_NODE_MAX_SESSIONS plus SE_NODE_OVERRIDE_MAX_SESSIONS, and
# not to exceed the available processors
# (https://github.com/SeleniumHQ/docker-selenium).
# Four is the ubuntu-latest runner's processor count. The same number is the
# test-thread count below, so the journeys never ask for a session the browser
# cannot open. A browser you started yourself has its own ceiling; give it at
# least this many or the journeys queue behind each other.
readonly BROWSER_SESSIONS=4

# Where a failing journey writes its evidence: a screenshot and the whole
# document. The ui-e2e CI job uploads this directory when the run fails, and a
# passing run leaves it empty, so nothing is attached for a green lane.
readonly FAILURES_DIR="$root/target/ui-e2e-failures"

base_url=""
webdriver=""
docs_shots=""
signed_in_base_url=""
issuer_url=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --base-url) base_url=$2; shift 2 ;;
    --webdriver) webdriver=$2; shift 2 ;;
    --docs-shots) docs_shots=1; shift ;;
    --signed-in-base-url) signed_in_base_url=$2; shift 2 ;;
    --issuer) issuer_url=$2; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

if { [[ -n "$base_url" ]] && [[ -z "$webdriver" ]]; } ||
   { [[ -z "$base_url" ]] && [[ -n "$webdriver" ]]; }; then
  echo "ui-e2e: --base-url and --webdriver are given together or not at all" >&2
  exit 2
fi

if { [[ -n "$signed_in_base_url" ]] && [[ -z "$issuer_url" ]]; } ||
   { [[ -z "$signed_in_base_url" ]] && [[ -n "$issuer_url" ]]; }; then
  echo "ui-e2e: --signed-in-base-url and --issuer are given together or not at all" >&2
  exit 2
fi

if [[ -z "$base_url" ]] && [[ -n "$signed_in_base_url" ]]; then
  echo "ui-e2e: --signed-in-base-url belongs to the manual mode; the managed mode" >&2
  echo "  starts its own issuer and its own second server." >&2
  exit 2
fi

need() {
  command -v "$1" >/dev/null 2>&1 ||
    { echo "ui-e2e: $1 is not installed; $2" >&2; exit 1; }
}

need cargo "install the toolchain in rust-toolchain.toml"
need cargo-nextest "cargo install cargo-nextest --locked"
need curl "the readiness probes ask the server and the browser whether they answer"

# Whether anything is listening on a local port, so a published container port
# never lands on a socket another process already holds.
port_taken() {
  (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null || return 1
  exec 3<&-
  return 0
}

# The first free port at or above $1.
free_port() {
  local candidate
  for candidate in $(seq "$1" "$(($1 + 40))"); do
    if ! port_taken "$candidate"; then
      echo "$candidate"
      return 0
    fi
  done
  echo "ui-e2e: no free port in $1..$(($1 + 40))" >&2
  return 1
}

network=""
server=""
signed_in=""
proxy=""
browser=""
issuer_pid=""
issuer_dir=""
cleanup() {
  [[ -z "$browser" ]] || docker rm -f "$browser" >/dev/null 2>&1 || true
  [[ -z "$proxy" ]] || docker rm -f "$proxy" >/dev/null 2>&1 || true
  [[ -z "$signed_in" ]] || docker rm -f "$signed_in" >/dev/null 2>&1 || true
  [[ -z "$server" ]] || docker rm -f "$server" >/dev/null 2>&1 || true
  [[ -z "$network" ]] || docker network rm "$network" >/dev/null 2>&1 || true
  [[ -z "$issuer_pid" ]] || kill "$issuer_pid" >/dev/null 2>&1 || true
  [[ -z "$issuer_dir" ]] || rm -rf "$issuer_dir"
}

# Waits until the container named $1 answers /health on the published port $2.
#
# A container that exits before it is ready is reported with its own log,
# because a server that refused its configuration says why there and nowhere
# else.
await_health() {
  local container=$1 port=$2 waited
  echo "== waiting for $container on 127.0.0.1:$port"
  for waited in $(seq 1 "$((READY_TIMEOUT * 5))"); do
    if [[ "$(docker inspect -f '{{.State.Running}}' "$container" 2>/dev/null)" != true ]]; then
      echo "ui-e2e: $container exited before it was ready (after $waited probes)" >&2
      docker logs "$container" >&2 || true
      return 1
    fi
    if curl -sf "http://127.0.0.1:$port/health" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.2
  done
  echo "ui-e2e: $container did not answer /health within ${READY_TIMEOUT}s" >&2
  docker logs "$container" >&2 || true
  return 1
}

if [[ -z "$base_url" ]]; then
  need docker "the managed mode runs the server and the browser as containers"
  need trunk "the pin is in docs/VERSIONS.md; the bundle is what the journeys drive"
  docker info >/dev/null 2>&1 ||
    { echo "ui-e2e: docker is installed but not running" >&2; exit 1; }
  if [[ "$(uname -s)" != Linux ]]; then
    echo "ui-e2e: the managed mode builds linux binaries; on macOS run the manual recipe in this script's header." >&2
    exit 1
  fi

  case "$(uname -m)" in
    x86_64 | amd64) arch=amd64 ;;
    aarch64 | arm64) arch=arm64 ;;
    *) echo "ui-e2e: no image architecture for $(uname -m)" >&2; exit 1 ;;
  esac

  echo "== the two viewer bundles"
  # `locked = true` in Trunk.toml already refuses a stale lock file; the flag
  # says so at the call site too. The editor bundle is the same crate built
  # with its feature on, into its own directory and under its own mount.
  (cd app/ferroterm-viewer && trunk build --release --locked)
  (cd app/ferroterm-viewer &&
    trunk build --release --locked --features editor \
      --dist dist-editor --public-url /ui/editor/)

  # The image base is distroless/static, which carries no dynamic loader, so a
  # glibc-linked binary copies in and then fails `exec` with "no such file or
  # directory". release-image.yml maps the image architecture to a musl triple
  # for the same reason; this mirrors it rather than inventing a second answer.
  case "$arch" in
    amd64) target=x86_64-unknown-linux-musl ;;
    arm64) target=aarch64-unknown-linux-musl ;;
    *) echo "ui-e2e: no musl target for $arch" >&2; exit 1 ;;
  esac
  if ! rustup target list --installed | grep -qx "$target"; then
    echo "== adding the $target target"
    rustup target add "$target"
  fi

  echo "== the server, with the bundle inside it"
  # Naming the bundle directory is what makes a missing bundle fail the build:
  # the server's build script refuses when the variable names a directory that
  # does not read, and only warns when it falls back to the default. Without it
  # the journeys would drive a viewer-less binary and nothing would say so.
  FERROTERM_UI_BUNDLE="$root/app/ferroterm-viewer/dist" \
    FERROTERM_UI_EDITOR_BUNDLE="$root/app/ferroterm-viewer/dist-editor" \
    cargo build --release --locked --target "$target" -p ferroterm-server --features ui
  cargo build --release --locked --target "$target" -p ferroterm-build

  echo "== the image, staged the way the release lane stages it"
  # .dockerignore admits nothing but dist/, so the repository root is a cheap
  # build context even with a populated target/.
  rm -rf "dist/linux/$arch"
  mkdir -p "dist/linux/$arch"
  cp "target/$target/release/ferroterm" "target/$target/release/ferroterm-build" \
    "dist/linux/$arch/"
  docker build --file docker/Dockerfile --tag "$SERVER_IMAGE" \
    --build-arg "TARGETOS=linux" --build-arg "TARGETARCH=$arch" .

  run="ferroterm-ui-e2e-$$"
  network="$run-net"
  server="$run-server"
  browser="$run-browser"
  trap cleanup EXIT
  docker network create "$network" >/dev/null

  # The server port is published as well as networked: the readiness probe and
  # anyone debugging a failure reach it from the host, while the browser
  # resolves the container by name on the private network.
  server_port="$(free_port 8140)"
  # The registry systems the server ships with are flat, so a deployment
  # carrying only them declares no child-of filter operator and the viewer's
  # taxonomy tree has nothing to draw. e2e/fixtures/codesystems holds one
  # shaped, synthetic CodeSystem resource so the tree journey drives a real
  # hierarchy; it is mounted read-only and never baked into the image.
  # The alias is what the browser addresses the server by, and it is fixed
  # while the container name carries this run's pid. A screenshot the capture
  # pass takes renders the FHIR base it read, so an address with a pid in it
  # would change every image on every run.
  docker run --detach --name "$server" --network "$network" \
    --network-alias "$SERVER_HOST" \
    --env FERROTERM_UI=on --env FERROTERM_LOG_FORMAT=json --env FERROTERM_USAGE_REPORT=off \
    --env FERROTERM_CODESYSTEMS=/fixtures/codesystems \
    --volume "$root/e2e/fixtures/codesystems:/fixtures/codesystems:ro" \
    --publish "127.0.0.1:$server_port:8080" "$SERVER_IMAGE" >/dev/null
  await_health "$server" "$server_port"
  # A binary built without a bundle serves no /ui route, and the journeys would
  # then fail on a missing element rather than on the missing bundle.
  curl -sf "http://127.0.0.1:$server_port/ui/" >/dev/null 2>&1 || {
    echo "ui-e2e: the server serves no /ui, so this binary carries no viewer bundle" >&2
    docker logs "$server" >&2 || true
    exit 1
  }
  curl -sf "http://127.0.0.1:$server_port/ui/editor/" >/dev/null 2>&1 || {
    echo "ui-e2e: the server serves no /ui/editor, so this binary carries no editor bundle" >&2
    docker logs "$server" >&2 || true
    exit 1
  }

  # The identity provider the signed-in journeys sign in to: a stub over TLS,
  # because the server refuses a non-loopback issuer over plain HTTP. It runs
  # on the host, and both containers reach it through the host gateway under
  # the name its certificate is issued for.
  echo "== the stub identity provider"
  cargo build --manifest-path e2e/Cargo.toml --locked --bin stub-issuer
  issuer_dir="$(mktemp -d)"
  issuer_ca="$issuer_dir/ca.pem"
  issuer_port="$(free_port 18443)"
  issuer_url="https://$ISSUER_HOST:$issuer_port"
  # The same authority issues the terminator's certificate, so installing one
  # authority in the browser covers the issuer and the viewer alike.
  "$root/e2e/target/debug/stub-issuer" --listen "0.0.0.0:$issuer_port" \
    --issuer "$issuer_url" --name "$ISSUER_HOST" --ca "$issuer_ca" \
    --serve-name "$SIGNED_IN_HOST" --serve-dir "$issuer_dir/certs" &
  issuer_pid=$!
  ready=""
  for _ in $(seq 1 "$((READY_TIMEOUT * 5))"); do
    if [[ -s "$issuer_ca" ]] &&
       curl -sf --cacert "$issuer_ca" --resolve "$ISSUER_HOST:$issuer_port:127.0.0.1" \
         "$issuer_url/.well-known/openid-configuration" >/dev/null 2>&1; then
      ready=1
      break
    fi
    sleep 0.2
  done
  if [[ -z "$ready" ]]; then
    echo "ui-e2e: the stub issuer did not answer its discovery document" >&2
    exit 1
  fi

  # The second server: the same image, configured to ask for a token. The
  # first one configures no issuer, and the read-only journeys assert that it
  # offers no sign-in, so the two deployments cannot be one container.
  #
  # SSL_CERT_FILE is what makes the container trust the run's own authority:
  # the server reads its roots through rustls-native-certs, which takes that
  # variable over the platform store
  # (https://docs.rs/rustls-native-certs/0.8/rustls_native_certs/).
  signed_in="$run-signed-in"
  signed_in_base_url="https://$SIGNED_IN_HOST"
  signed_in_port="$(free_port 8180)"
  docker run --detach --name "$signed_in" --network "$network" \
    --network-alias "$SIGNED_IN_ORIGIN_HOST" \
    --add-host "$ISSUER_HOST:host-gateway" \
    --env FERROTERM_UI=on --env FERROTERM_LOG_FORMAT=json --env FERROTERM_USAGE_REPORT=off \
    --env FERROTERM_CODESYSTEMS=/fixtures/codesystems \
    --env "FERROTERM_BASE_URL=$signed_in_base_url" \
    --env "FERROTERM_OIDC_ISSUER=$issuer_url" \
    --env "FERROTERM_VIEWER_CLIENT_ID=$VIEWER_CLIENT_ID" \
    --env SSL_CERT_FILE=/run/issuer/ca.pem \
    --env FERROTERM_RESOURCES=/run/state/resources.redb \
    --tmpfs /run/state:mode=1777 \
    --volume "$root/e2e/fixtures/codesystems:/fixtures/codesystems:ro" \
    --volume "$issuer_ca:/run/issuer/ca.pem:ro" \
    --publish "127.0.0.1:$signed_in_port:8080" "$SERVER_IMAGE" >/dev/null
  await_health "$signed_in" "$signed_in_port"

  # The TLS in front of it, with the certificate the issuer's own authority
  # signed. `auto_https off` keeps Caddy from reaching for a public authority
  # it could never reach (https://caddyserver.com/docs/caddyfile/options).
  echo "== the TLS terminator in front of the signed-in server"
  cat > "$issuer_dir/Caddyfile" <<CADDY
{
	admin off
	auto_https off
}

https://$SIGNED_IN_HOST {
	tls /certs/cert.pem /certs/key.pem
	reverse_proxy $SIGNED_IN_ORIGIN_HOST:8080
}
CADDY
  proxy="$run-proxy"
  proxy_port="$(free_port 8543)"
  docker run --detach --name "$proxy" --network "$network" \
    --network-alias "$SIGNED_IN_HOST" \
    --volume "$issuer_dir/Caddyfile:/etc/caddy/Caddyfile:ro" \
    --volume "$issuer_dir/certs:/certs:ro" \
    --publish "127.0.0.1:$proxy_port:443" "$PROXY_IMAGE" >/dev/null
  echo "== waiting for the terminator on 127.0.0.1:$proxy_port"
  ready=""
  for _ in $(seq 1 "$((READY_TIMEOUT * 5))"); do
    if curl -sf --cacert "$issuer_ca" \
      --resolve "$SIGNED_IN_HOST:$proxy_port:127.0.0.1" \
      "https://$SIGNED_IN_HOST:$proxy_port/health" >/dev/null 2>&1; then
      ready=1
      break
    fi
    sleep 0.2
  done
  if [[ -z "$ready" ]]; then
    echo "ui-e2e: the TLS terminator did not answer within ${READY_TIMEOUT}s" >&2
    docker logs "$proxy" >&2 || true
    exit 1
  fi

  # The browser trusts the same authority through an enterprise policy, which
  # is how Chromium takes a certificate authority without a user profile
  # (https://chromeenterprise.google/policies/#CACertificates). The two paths
  # are the policy directories of the Chromium and the Chrome packaging; a
  # directory the image does not use is simply never read.
  issuer_policy="$issuer_dir/ca-policy.json"
  printf '{"CACertificates": ["%s"]}\n' \
    "$(grep -v CERTIFICATE "$issuer_ca" | tr -d '\n')" > "$issuer_policy"

  # Chromium needs more than the default 64 MB /dev/shm; without this it
  # crashes on a page of any size
  # (https://developer.chrome.com/docs/chromium/headless).
  webdriver_port="$(free_port 4444)"
  docker run --detach --name "$browser" --network "$network" --shm-size 2g \
    --add-host "$ISSUER_HOST:host-gateway" \
    --env "SE_NODE_MAX_SESSIONS=$BROWSER_SESSIONS" \
    --env SE_NODE_OVERRIDE_MAX_SESSIONS=true \
    --volume "$issuer_policy:/etc/chromium/policies/managed/ferroterm-e2e-ca.json:ro" \
    --volume "$issuer_policy:/etc/opt/chrome/policies/managed/ferroterm-e2e-ca.json:ro" \
    --publish "127.0.0.1:$webdriver_port:4444" "$BROWSER_IMAGE" >/dev/null
  echo "== waiting for the browser on 127.0.0.1:$webdriver_port"
  ready=""
  for _ in $(seq 1 "$((READY_TIMEOUT * 5))"); do
    if [[ "$(docker inspect -f '{{.State.Running}}' "$browser" 2>/dev/null)" != true ]]; then
      echo "ui-e2e: the browser container exited before it was ready" >&2
      docker logs "$browser" >&2 || true
      exit 1
    fi
    if curl -sf "http://127.0.0.1:$webdriver_port/status" 2>/dev/null | grep -q '"ready": *true'; then
      ready=1
      break
    fi
    sleep 0.2
  done
  if [[ -z "$ready" ]]; then
    echo "ui-e2e: the browser did not report itself ready within ${READY_TIMEOUT}s" >&2
    docker logs "$browser" >&2 || true
    exit 1
  fi

  base_url="http://$SERVER_HOST:8080"
  webdriver="http://127.0.0.1:$webdriver_port"
fi

# A previous run's evidence would be uploaded beside this run's, so the
# directory starts empty and is only written to by a journey that fails.
rm -rf "$FAILURES_DIR"
mkdir -p "$FAILURES_DIR"

echo "== the journeys, against $base_url through $webdriver"
if [[ -n "$signed_in_base_url" ]]; then
  echo "   and the signed-in ones against $signed_in_base_url through $issuer_url"
else
  echo "   the signed-in journeys skip: nothing names a deployment with an issuer"
fi
# The journeys live outside the workspace, for the reason e2e/Cargo.toml
# records, so they are run by manifest path rather than by package.
#
# The capture pass is excluded by a nextest set difference, so it never runs
# beside the journeys and never writes an image nobody asked for
# (https://nexte.st/docs/filtersets/).
FERROTERM_UI_E2E_BASE_URL="$base_url" \
  FERROTERM_UI_E2E_WEBDRIVER="$webdriver" \
  FERROTERM_UI_E2E_FAILURES="$FAILURES_DIR" \
  FERROTERM_UI_E2E_SIGNED_IN_BASE_URL="$signed_in_base_url" \
  FERROTERM_UI_E2E_ISSUER="$issuer_url" \
  cargo nextest run --manifest-path e2e/Cargo.toml --locked \
    --test-threads "$BROWSER_SESSIONS" \
    -E 'binary(it) - test(/^docs_shots::/)'

# The documentation capture pass, which is the one thing here that writes into
# the checkout. It runs after the journeys, so an image is only ever taken of a
# viewer the journeys have just found working.
if [[ -n "$docs_shots" ]]; then
  echo "== the documentation screenshots, into website/book/src/operate/img/viewer"
  FERROTERM_UI_E2E_BASE_URL="$base_url" \
    FERROTERM_UI_E2E_WEBDRIVER="$webdriver" \
    FERROTERM_UI_E2E_FAILURES="$FAILURES_DIR" \
    FERROTERM_UI_E2E_DOCS_SHOTS=1 \
    cargo nextest run --manifest-path e2e/Cargo.toml --locked \
      -E 'test(/^docs_shots::/)'
fi
