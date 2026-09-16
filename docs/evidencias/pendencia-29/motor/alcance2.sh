#!/bin/zsh
# Critério 2 medido sobre o ARQUIVO ENTREGUE.
# Três medidas, não duas, porque «antes/depois» aqui é fraco de propósito:
#   (a) prova estrutural — nenhum arquivo que o `seele-server` compila mudou;
#   (b) `cargo test -p seele-server` sobre a árvore entregue, sob carga;
#   (c) o contrafactual: o MESMO binário com --test-threads=1, que é o preço
#       que o critério 2 proíbe pagar. É ele que mostra que não foi pago.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
W="/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/299c2412-1abd-4516-9748-226ce1950e3e"
cd "$W"
while pgrep -x cargo >/dev/null 2>&1 || pgrep -f final29.sh >/dev/null 2>&1; do sleep 15; done

echo "--- ARQUIVO SOB MEDIÇÃO ---"
shasum -a 256 crates/seele-conformance/tests/vaga/mod.rs /tmp/o29/vaga_ARMADO7.backup.rs
echo "--- (a) PROVA ESTRUTURAL: o que o seele-server compila mudou desde a base 3c59eec? ---"
git diff --stat 3c59eec -- crates/seele-server/ .cargo/ Cargo.toml Cargo.lock
echo "(nada impresso acima = nenhum byte de diferença)"
echo "--- (b) paralelo, árvore entregue, sob carga ---"
while pgrep -x cargo >/dev/null 2>&1; do sleep 5; done
QUEIMADORES=8 /tmp/o29/rodada5.sh srv2_paralelo cargo test -p seele-server
echo "--- (c) contrafactual serializado, mesmo binário, mesma carga ---"
while pgrep -x cargo >/dev/null 2>&1; do sleep 5; done
QUEIMADORES=8 /tmp/o29/rodada5.sh srv2_serie cargo test -p seele-server -- --test-threads=1
echo "FIM DO ALCANCE 2"
