#!/bin/zsh
# A rodada armada do `custo8` reprovou em
# `moderacao::um_operador_modera_pessoas_e_nao_o_comandante`. Esta medida
# pergunta a pergunta certa antes de concluir: o conjunto passa sozinho, e passa
# sozinho SOB A MESMA CARGA? Se passar, a reprovação é contenção que sobrou; se
# reprovar sozinho, é instabilidade do teste, e cai na causa 1 da §29.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
W="/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/299c2412-1abd-4516-9748-226ce1950e3e"
cd "$W"
while pgrep -x cargo >/dev/null 2>&1; do sleep 10; done
echo "--- ARQUIVO SOB MEDIÇÃO ---"; shasum -a 256 crates/seele-conformance/tests/vaga/mod.rs
grep -n 'const VAGAS' crates/seele-conformance/tests/vaga/mod.rs
for r in 1 2 3 4; do
  while pgrep -x cargo >/dev/null 2>&1; do sleep 5; done
  QUEIMADORES=8 /tmp/o29/rodada5.sh "moder_so_${r}" cargo test -p seele-conformance --test moderacao
done
echo "FIM DA MODERACAO"
