#!/bin/zsh
# Ciclo de reversão nº 6 — o que a revisão independente cobrou: a reprovação
# por PRAZO, sob `cargo test --workspace`, com a permissão desarmada, e sob a
# carga que de fato reproduz a §29 (servidores QUIC concorrentes fora do cargo,
# `rodada3.sh`), e não sob queimador de processador, que está medido como
# incapaz de reproduzi-la.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
W="/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/299c2412-1abd-4516-9748-226ce1950e3e"
V="$W/crates/seele-conformance/tests/vaga/mod.rs"
B=/tmp/o29/vaga_ARMADO6.backup.rs
cd "$W"
while pgrep -x cargo >/dev/null 2>&1; do sleep 10; done

# --- desarma: uma permissão de largura ilimitada nunca bloqueia ---
perl -pi -e 's/^const VAGAS: usize = 1;$/const VAGAS: usize = usize::MAX;/' "$V"
grep -n 'const VAGAS' "$V"
cargo test --workspace --no-run >/dev/null 2>&1
QUEIMADORES=8 /tmp/o29/ate_falhar3.sh revws 5 cargo test --workspace
echo "--- fim das rodadas desarmadas ---"

# --- restaura e confere byte a byte ---
cp "$B" "$V"
shasum -a 256 "$B" "$V"
cmp "$B" "$V" && echo "RESTAURADO: identico byte-a-byte ao backup"
grep -n 'const VAGAS' "$V"
cargo test --workspace --no-run >/dev/null 2>&1
QUEIMADORES=8 /tmp/o29/rodada3.sh pos_rev6 cargo test --workspace
