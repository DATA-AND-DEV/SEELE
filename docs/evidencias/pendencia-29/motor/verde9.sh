#!/bin/zsh
# Bateria final sobre o arquivo REALMENTE entregue (vaga/mod.rs, armado,
# SHA-256 1515219068d8…), depois do ciclo de reversão nº 7.
#  1. fmt e clippy sobre o conteúdo entregue (o achado 4 da revisão: da outra
#     vez o clippy caiu no meio do experimento, com o arquivo desarmado);
#  2. cinco rodadas seguidas de `cargo test --workspace` sob a MESMA carga da
#     série verde8 (8 queimadores + 6 laços de conformidade fora do cargo),
#     para que a comparação com a rodada reprovada seja de igual para igual.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
W="/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/299c2412-1abd-4516-9748-226ce1950e3e"
V="$W/crates/seele-conformance/tests/vaga/mod.rs"
cd "$W"
while pgrep -x cargo >/dev/null 2>&1; do sleep 10; done

echo "--- ARQUIVO SOB MEDIÇÃO ---"
shasum -a 256 "$V" /tmp/o29/vaga_ARMADO7.backup.rs
grep -n 'const VAGAS' "$V"

echo "--- cargo fmt --all -- --check ---"
cargo fmt --all -- --check >/tmp/o29/fmt_final.log 2>&1
echo "fmt SAIDA=$?"
echo "--- cargo clippy --workspace --all-targets --all-features -- -D warnings ---"
cargo clippy --workspace --all-targets --all-features -- -D warnings >/tmp/o29/clippy_final.log 2>&1
echo "clippy SAIDA=$?"

cargo test --workspace --no-run >/dev/null 2>&1
for r in 1 2 3 4 5; do
  while pgrep -x cargo >/dev/null 2>&1; do sleep 5; done
  QUEIMADORES=8 /tmp/o29/rodada4.sh "verde9_${r}" cargo test --workspace
done
echo "FIM DA BATERIA FINAL"
