#!/bin/zsh
# Custo do critério 5, medido com a carga CASADA — o que faltava.
# As rodadas desarmadas que existiam (`revws7_1..3`) foram feitas sob a carga do
# `rodada4.sh`, mais pesada que a do `rodada5.sh` usada nas rodadas armadas do
# aceite. Comparar parede entre as duas é comparar duas cargas, não duas
# permissões. Aqui as duas metades correm sob `rodada5.sh`.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
W="/Users/dev-alexandre/Documents/Obsidian Vault/Órbita/.orbita/worktrees/299c2412-1abd-4516-9748-226ce1950e3e"
V="$W/crates/seele-conformance/tests/vaga/mod.rs"
B=/tmp/o29/vaga_ARMADO7.backup.rs
cd "$W"
while pgrep -x cargo >/dev/null 2>&1 || pgrep -f 'final29.sh|alcance2.sh' >/dev/null 2>&1; do sleep 15; done

echo "--- ANTES DE DESARMAR ---"; shasum -a 256 "$V" "$B"
perl -pi -e 's/^const VAGAS: usize = 1;$/const VAGAS: usize = usize::MAX;/' "$V"
grep -n 'const VAGAS' "$V"
cargo test --workspace --no-run >/dev/null 2>&1
for r in 1 2 3; do
  while pgrep -x cargo >/dev/null 2>&1; do sleep 5; done
  QUEIMADORES=8 /tmp/o29/rodada5.sh "custo8_desarmada_${r}" cargo test --workspace
done
cp "$B" "$V"
echo "--- RESTAURACAO ---"; shasum -a 256 "$B" "$V"
cmp "$B" "$V" && echo "RESTAURADO: identico byte-a-byte ao backup"
grep -n 'const VAGAS' "$V"
cargo test --workspace --no-run >/dev/null 2>&1
while pgrep -x cargo >/dev/null 2>&1; do sleep 5; done
QUEIMADORES=8 /tmp/o29/rodada5.sh custo8_pos cargo test --workspace
echo "--- CONFERENCIA FINAL DO ARQUIVO ENTREGUE ---"; shasum -a 256 "$V" "$B"; cmp "$B" "$V" && echo IDENTICO
echo "FIM DO CUSTO 8"
