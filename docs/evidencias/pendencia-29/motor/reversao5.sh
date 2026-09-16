#!/bin/zsh
set -u
export PATH="$HOME/.cargo/bin:$PATH"
W="/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/299c2412-1abd-4516-9748-226ce1950e3e"
V="$W/crates/seele-conformance/tests/vaga/mod.rs"
# espera a máquina sair da troca de páginas provocada pela sobrecarga anterior
while [ "$(uptime | sed 's/.*averages: //' | awk '{print int($1)}')" -gt 10 ]; do sleep 60; done
while pgrep -x cargo >/dev/null 2>&1; do sleep 10; done
cd "$W"
# --- referência armada, para comparar na mesma máquina e no mesmo minuto ---
/tmp/o29/rodada.sh base_armado cargo test -p seele-conformance
# --- desarma ---
perl -pi -e 's/^const VAGAS: usize = 1;$/const VAGAS: usize = usize::MAX;/' "$V"
grep -n 'const VAGAS' "$V"
cargo test -p seele-conformance --no-run >/dev/null 2>&1
/tmp/o29/ate_falhar.sh revconf2 6 cargo test -p seele-conformance
echo "--- fim das rodadas desarmadas ---"
# --- restaura e confere ---
cp /tmp/o29/vaga_ARMADO.backup.rs "$V"
shasum -a 256 /tmp/o29/vaga_ARMADO.backup.rs "$V"
cmp /tmp/o29/vaga_ARMADO.backup.rs "$V" && echo "RESTAURADO: identico byte-a-byte ao backup"
grep -n 'const VAGAS' "$V"
cargo test -p seele-conformance --no-run >/dev/null 2>&1
/tmp/o29/rodada.sh pos_armado cargo test -p seele-conformance
