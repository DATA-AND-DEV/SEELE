#!/bin/sh
# Empacota o `.dmg` do macOS sem o GitHub Actions.
#
# Faz o mesmo que o job `macos` de `.github/workflows/release.yml`, com uma
# diferença deliberada: **só a arquitetura desta máquina**, sem universal.
#
#   ./empacotar/macos.sh 0.1.2
#
# O workflow compila para `aarch64` e `x86_64` e junta os dois com `lipo`,
# porque um `.dmg` publicado precisa rodar em Mac Intel também. Este script não:
# ele existe para quando o CI está indisponível e o pacote é para máquinas
# conhecidas. **Um `.dmg` feito aqui num Apple Silicon não abre num Mac Intel** —
# se o release for público, o alvo Intel tem que entrar, e a forma de fazer isso
# está no workflow.

set -eu

VERSAO="${1:-}"
if [ -z "$VERSAO" ]; then
    echo "uso: $0 <versão>    (por exemplo: $0 0.1.2)" >&2
    exit 1
fi

if ! printf '%s' "$VERSAO" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9]+)?$'; then
    echo "a versão «$VERSAO» não serve para o instalador." >&2
    echo "Aceito: X.Y.Z, ou X.Y.Z-N com N só de dígitos." >&2
    exit 1
fi

RAIZ="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$RAIZ"

ALVO="$(rustc -vV | sed -n 's/^host: //p')"
echo "→ alvo desta máquina: $ALVO"

# Absoluto de propósito. Aqui os `cd` acontecem só dentro de subshells, então o
# relativo funcionaria — mas o script irmão do Linux tinha exatamente esta linha
# em relativo, trocava de diretório antes de sair, e a restauração errava o alvo
# em silêncio, deixando a versão do release gravada no repositório. Não vale
# manter a fragilidade só porque neste arquivo ela ainda não disparou.
CONFIG="$RAIZ/apps/seele-app/tauri.conf.json"
cp "$CONFIG" "$CONFIG.original"
# A versão gravada é para o artefato, não para o repositório: um commit
# distraído fixaria no código o número de um release.
trap 'mv "$CONFIG.original" "$CONFIG"' EXIT INT TERM

python3 - "$VERSAO" "$CONFIG" <<'PY'
import json, pathlib, sys
caminho = pathlib.Path(sys.argv[2])
config = json.loads(caminho.read_text())
config["version"] = sys.argv[1]
caminho.write_text(json.dumps(config, indent=2, ensure_ascii=False) + "\n")
print("→ versão gravada:", config["version"])
PY

# A CLI antes do app, como no workflow: `seeled` e `plug` são o produto
# principal e o que menos pode falhar. Eles entram **dentro** do `.app`, para
# que quem baixa ganhe as duas metades num arquivo só.
echo "→ compilando seeled e plug"
cargo build --release --bin seeled --bin plug --target "$ALVO"

mkdir -p apps/seele-app/binaries
for b in plug seeled; do
    cp "target/$ALVO/release/$b" "apps/seele-app/binaries/$b-$ALVO"
done

if ! command -v cargo-tauri >/dev/null 2>&1; then
    echo "→ instalando a CLI do Tauri"
    cargo install tauri-cli --version "^2" --locked
fi

# `signingIdentity: "-"` já está no `tauri.conf.json`: é assinatura ad-hoc, que
# não vale para o Gatekeeper — só a notarização vale — mas **é o que faz a
# permissão de microfone grudar**. Sem assinatura de bundle o macOS não tem a
# que identidade prender a autorização, e pergunta de novo a cada abertura sem
# nunca lembrar.
echo "→ empacotando o .dmg"
(cd apps/seele-app && cargo tauri build --config tauri.release.conf.json --bundles dmg)

DESTINO=entrega
mkdir -p "$DESTINO"
# Filtrado pela versão, e não por horário de modificação.
#
# O `-newer` funcionava e funcionava por sorte: ele responde «apareceu depois
# que comecei», que é quase sempre a mesma coisa que «é o desta execução» e não
# é a mesma pergunta. O `target/bundle` acumula versões, e o irmão deste script
# no Windows reprovou exatamente assim — dois instaladores lado a lado, um de
# cada versão. A versão está no nome do arquivo; é a resposta direta.
padrao="*_${VERSAO}_*.dmg"
encontrados=$(find target -type f -name "$padrao" | wc -l | tr -d ' ')
if [ "$encontrados" != "1" ]; then
    echo "esperava exatamente um .dmg de $VERSAO e achei $encontrados." >&2
    echo "Procurei por «$padrao» em target/." >&2
    find target -type f -name "$padrao" >&2
    exit 1
fi
find target -type f -name "$padrao" -exec cp {} "$DESTINO/" \;

tar -czf "$DESTINO/seele-cli-$VERSAO-macos.tar.gz" -C "target/$ALVO/release" seeled plug

echo
echo "--- entrega ---"
(cd "$DESTINO" && shasum -a 256 *.dmg "seele-cli-$VERSAO-macos.tar.gz")
echo
echo "Este .dmg roda só em $ALVO. Para Mac Intel, o workflow é o caminho."
