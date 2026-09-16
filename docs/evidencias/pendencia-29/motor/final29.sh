#!/bin/zsh
# Bateria de fechamento, toda sobre o arquivo ENTREGUE (vaga/mod.rs armado).
# Parte A — critério 1: 5 rodadas de `cargo test --workspace` sob carga
#   concorrente deliberada que não burla o alcance do conserto (rodada5.sh).
# Parte B — discriminador: o mesmo conjunto de conformidade que reprovou armado
#   (`tela_por_um_par`) rodado 6 vezes sob cada uma das duas cargas, para
#   separar «o conserto não segura» de «a carga roda servidores QUIC fora do
#   processo, onde a permissão não alcança».
set -u
export PATH="$HOME/.cargo/bin:$PATH"
W="/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/299c2412-1abd-4516-9748-226ce1950e3e"
cd "$W"
while pgrep -x cargo >/dev/null 2>&1; do sleep 10; done

echo "--- ARQUIVO SOB MEDIÇÃO ---"
shasum -a 256 crates/seele-conformance/tests/vaga/mod.rs /tmp/o29/vaga_ARMADO7.backup.rs
grep -n 'const VAGAS' crates/seele-conformance/tests/vaga/mod.rs

echo "--- PARTE A: 5 rodadas de --workspace sob carga de queimadores ---"
for r in 1 2 3 4 5; do
  while pgrep -x cargo >/dev/null 2>&1; do sleep 5; done
  QUEIMADORES=8 /tmp/o29/rodada5.sh "verde10_${r}" cargo test --workspace
done

echo "--- PARTE B: discriminador de carga sobre tela_por_um_par ---"
for r in 1 2 3 4 5 6; do
  while pgrep -x cargo >/dev/null 2>&1; do sleep 5; done
  QUEIMADORES=8 /tmp/o29/rodada5.sh "disc_queimador_${r}" \
    cargo test -p seele-conformance --test tela_por_um_par
done
for r in 1 2 3 4 5 6; do
  while pgrep -x cargo >/dev/null 2>&1; do sleep 5; done
  QUEIMADORES=8 /tmp/o29/rodada4.sh "disc_fora_do_processo_${r}" \
    cargo test -p seele-conformance --test tela_por_um_par
done
echo "FIM DA BATERIA DE FECHAMENTO"
