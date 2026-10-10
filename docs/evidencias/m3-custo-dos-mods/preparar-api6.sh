#!/bin/sh
# M3 da 0.16, a campanha da API 6 (tarefa 11 do plano da F3) — monta a pasta
# de trabalho do zero, sem tocar no repositório de onde é chamado.
#
#   sh preparar-api6.sh <pasta-de-trabalho> [commit]    (commit padrão: o HEAD de quem chama)
#
# Nesta ordem:
#   1. copia o código do commit por `git archive` para <pasta>/fonte, e anota
#      o commit em <pasta>/fonte/COMMIT_DA_MEDICAO;
#   2. põe ali os exemplos da medição (o servidor, o falante e a conferência do
#      MOD de carga) e aplica os remendos da medição (`remendar-api6.py`: a API 6
#      aceita só nesta cópia, o jitter no seele.log, o servidor na 0.16);
#   3. compila o app em <pasta>/alvo/release/bundle/macos/SEELE.app (`cargo tauri
#      build`, assinatura ad-hoc), e o servidor, o falante e a conferência em
#      `--release`;
#   4. compila o `ax` e o `sessao`;
#   5. baixa os três pacotes oficiais da base (PERFIS 3.3.0, MESA 3.1.0, ESTILO
#      3.1.0), como o `preparar.sh`;
#   6. gera as variantes do MOD de carga (`gerar-carga.py`), confere cada uma
#      contra o prelúdio da cópia (`conferir-carga`) e as põe no cache por hash
#      (<pasta>/cache/mod-packages): elas não estão no indexador, e o app só as
#      acha ali;
#   7. copia o roteiro para <pasta>/roteiro.
set -eu
PASTA=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
AQUI=$(cd "$(dirname "$0")" && pwd)
REPO=$(git -C "$AQUI" rev-parse --show-toplevel)
COMMIT=${2:-$(git -C "$REPO" rev-parse HEAD)}
mkdir -p "$PASTA/fonte" "$PASTA/roteiro" "$PASTA/pacotes" "$PASTA/medidas" "$PASTA/cache/mod-packages"

git -C "$REPO" archive "$COMMIT" | tar -x -C "$PASTA/fonte"
echo "$COMMIT" > "$PASTA/fonte/COMMIT_DA_MEDICAO"
cp "$AQUI/m3-servidor.rs" "$PASTA/fonte/crates/seele-server/examples/"
# O falante como exemplo do `seele-core`, e não do `seele-conformance` (como na
# base): as `[dev-dependencies]` deste ligam a feature `recusar-encontro-de-
# producao`, que a 0.16 recusa num build otimizado (`compile_error!`, medido na
# primeira preparação).
mkdir -p "$PASTA/fonte/crates/seele-core/examples" "$PASTA/fonte/crates/seele-motor/examples"
cp "$AQUI/m3-falante.rs" "$PASTA/fonte/crates/seele-core/examples/"
cp "$AQUI/conferir-carga.rs" "$PASTA/fonte/crates/seele-motor/examples/"
python3 -I "$AQUI/remendar-api6.py" "$PASTA/fonte"

( cd "$PASTA/fonte/apps/seele-app" \
  && CARGO_TARGET_DIR="$PASTA/alvo" cargo tauri build --bundles app )
( cd "$PASTA/fonte" \
  && CARGO_TARGET_DIR="$PASTA/alvo" cargo build --release -p seele-server --example m3-servidor \
  && CARGO_TARGET_DIR="$PASTA/alvo" cargo build --release -p seele-core --example m3-falante \
  && CARGO_TARGET_DIR="$PASTA/alvo" cargo build --release -p seele-motor --example conferir-carga )

swiftc -O "$AQUI/ax.swift" -o "$PASTA/ax"
swiftc -O "$AQUI/sessao.swift" -o "$PASTA/sessao"
cp "$AQUI/m3.py" "$AQUI/xctrace_tabela.py" "$AQUI/campanha-api6.sh" "$PASTA/roteiro/"

curl -sSf -o "$PASTA/pacotes/catalogo.json" https://mods.seele.app.br/catalogo.json
python3 -I - "$PASTA" <<'PY'
import json, os, subprocess, sys
pasta = sys.argv[1]
catalogo = json.load(open(os.path.join(pasta, "pacotes/catalogo.json")))
escolha = {"seele/perfis": "3.3.0", "seele/mesa": "3.1.0", "seele/estilo": "3.1.0"}
for mod in catalogo["mods"]:
    versao = escolha.get(mod["id"])
    if not versao:
        continue
    v = next(x for x in mod["versoes"] if x["versao"] == versao)
    destino = os.path.join(pasta, "pacotes", mod["id"].replace("/", "-") + "-" + versao)
    for relativo in v["arquivos"]:
        caminho = os.path.join(destino, relativo)
        os.makedirs(os.path.dirname(caminho), exist_ok=True)
        subprocess.run(["curl", "-sSf", "-o", caminho,
                        f"https://mods.seele.app.br/mods/{mod['id']}/{versao}/{relativo}"], check=True)
    print(mod["id"], versao, "hash do catálogo", v["hash"])
PY

# O MOD de carga: gerado, conferido contra o prelúdio da cópia, e no cache.
python3 -I "$AQUI/gerar-carga.py" "$PASTA/pacotes"
for variante in "$PASTA"/pacotes/m3-carga-*; do
  "$PASTA/alvo/release/examples/conferir-carga" "$variante/cliente/main.js" > /dev/null \
    || { echo "o MOD de carga $(basename "$variante") tropeçou no prelúdio da cópia"; exit 1; }
  # O hash pelo próprio servidor da medição, como na conferência dos oficiais.
  "$PASTA/alvo/release/examples/m3-servidor" 8391 "$PASTA/conferencia" "$variante" 2>/dev/null > "$PASTA/conferencia.txt" &
  SERVIDOR=$!
  for _ in 1 2 3 4 5 6 7 8 9 10; do [ -s "$PASTA/conferencia.txt" ] && break; sleep 1; done
  kill "$SERVIDOR" 2>/dev/null || true
  wait "$SERVIDOR" 2>/dev/null || true
  HASH=$(sed -n 's/.*#\([0-9a-f]\{64\}\).*/\1/p' "$PASTA/conferencia.txt")
  [ -n "$HASH" ] || { echo "sem hash para $(basename "$variante")"; exit 1; }
  rm -rf "$PASTA/conferencia" "$PASTA/conferencia.txt" "$PASTA/cache/mod-packages/$HASH"
  cp -R "$variante" "$PASTA/cache/mod-packages/$HASH"
  echo "$(basename "$variante"): conferido, no cache como $HASH"
done
echo "pronto: sh $PASTA/roteiro/campanha-api6.sh"
