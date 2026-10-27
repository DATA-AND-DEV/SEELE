#!/bin/sh
# Pages: fonte Rust + UI -> target/pacote-web. Não publica nem usa credenciais.
set -eu
raiz=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
cd "$raiz"
falhar() { echo "cloudflare-web: $*" >&2; exit 1; }
[ "$(uname -s)/$(uname -m)" = Linux/x86_64 ] || falhar 'este bootstrap exige Linux x86_64 (Pages v3)'
for ferramenta in curl tar gzip sha256sum cc; do
  command -v "$ferramenta" >/dev/null 2>&1 || falhar "falta $ferramenta na imagem de build"
done
[ -f Cargo.lock ] || falhar 'Cargo.lock precisa estar versionado'
# Os caminhos do pacote-web são relativos ao target da raiz, inclusive o motor.
[ -z "${CARGO_TARGET_DIR:-}" ] || [ "$CARGO_TARGET_DIR" = "$raiz/target" ] || falhar 'CARGO_TARGET_DIR deve ser o target da raiz'
export CARGO_TARGET_DIR="$raiz/target"
export CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export RUSTUP_TOOLCHAIN=1.97.1
[ "$(sed -n 's/^channel = "\([^"]*\)"$/\1/p' rust-toolchain.toml)" = "$RUSTUP_TOOLCHAIN" ] || falhar 'revisar o bootstrap: rust-toolchain.toml mudou'
mkdir -p target
trava=target/.cloudflare-web.lock
mkdir "$trava" 2>/dev/null || falhar 'outro build usa este checkout (ou deixou target/.cloudflare-web.lock); não apagar durante um build'
trap 'rmdir "$trava"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
lock_antes=$(sha256sum Cargo.lock)
# Nada de curl | sh nem rustup update: instalador fixo, verificado antes de rodar.
if ! command -v rustup >/dev/null 2>&1; then
  instalador=target/rustup-init-1.28.2
  curl --fail --show-error --location --retry 3 --proto '=https' --tlsv1.2 \
    https://static.rust-lang.org/rustup/archive/1.28.2/x86_64-unknown-linux-gnu/rustup-init -o "$instalador"
  echo "20a06e644b0d9bd2fbdbfd52d42540bdde820ea7df86e92e533c073da0cdd43c  $instalador" | sha256sum -c -
  chmod +x "$instalador"
  "$raiz/$instalador" -y --no-modify-path --profile minimal --default-toolchain none
  rm "$instalador"
  PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
  export PATH
fi
rustup toolchain install "$RUSTUP_TOOLCHAIN" --profile minimal --component rustfmt,clippy,llvm-tools
rustup target add --toolchain "$RUSTUP_TOOLCHAIN" wasm32-unknown-unknown wasm32-wasip1
rustc --version
cargo --version
# Hash do SDK24 Linux em xtask/src/wasi_sdk.rs. Cache local reaproveitado só
# pelo preparador verificado; pacote-web não faz uma segunda descarga implícita.
cargo run --quiet --locked --package xtask -- wasi-sdk
cargo run --quiet --locked --package xtask -- pacote-web
[ "$(sha256sum Cargo.lock)" = "$lock_antes" ] || falhar 'Cargo.lock mudou durante o build'
for arquivo in index.html seele_web.wasm seele_motor_web.wasm _headers; do
  [ -s "target/pacote-web/$arquivo" ] || falhar "saída ausente: $arquivo"
done
echo 'cloudflare-web: pacote conferido em target/pacote-web (sem publicação)'
