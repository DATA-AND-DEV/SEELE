#!/bin/zsh
set -u
export PATH="$HOME/.cargo/bin:$PATH"
W="/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/299c2412-1abd-4516-9748-226ce1950e3e"
cd "$W" || exit 9

# --- ANTES: árvore original, sem vaga nenhuma ---
git checkout -- crates/seele-conformance/tests .github/workflows/ci.yml
rm -rf crates/seele-conformance/tests/vaga
echo "estado ANTES: vaga presente? $(ls crates/seele-conformance/tests/vaga 2>/dev/null | wc -l) arquivos; git status: $(git status --porcelain | wc -l) modificados"
cargo test -p seele-server --no-run >/dev/null 2>&1
/tmp/o29/rodada.sh srv_antes cargo test -p seele-server

# --- restaura a árvore armada e confere byte-a-byte ---
rm -rf crates/seele-conformance/tests
cp -R /tmp/o29/armado crates/seele-conformance/tests
cp /tmp/o29/ci_armado.yml .github/workflows/ci.yml
cd /tmp/o29/armado && H=$(shasum -a 256 *.rs vaga/mod.rs | shasum -a 256)
cd "$W/crates/seele-conformance/tests" && H2=$(shasum -a 256 *.rs vaga/mod.rs | shasum -a 256)
echo "hash backup=$H"
echo "hash arvore=$H2"
[[ "$H" == "$H2" ]] && echo "RESTAURACAO CONFERIDA byte-a-byte" || { echo "RESTAURACAO DIVERGIU"; exit 8; }
cd "$W"
grep -n 'VAGAS: usize' crates/seele-conformance/tests/vaga/mod.rs

# --- DEPOIS: árvore armada ---
cargo test -p seele-server --no-run >/dev/null 2>&1
/tmp/o29/rodada.sh srv_depois cargo test -p seele-server
