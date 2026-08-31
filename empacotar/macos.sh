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
#
# Para que o pacote saia atualizável, exporte a chave do projeto antes:
#
#   export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/seele.key)"
#   export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=…
#
# Sem ela sai o `.dmg` de sempre, para baixar à mão.
# `docs/assinatura-e-atualizacao.md` diz de onde a chave vem.

set -eu

VERSAO="${1:-}"
if [ -z "$VERSAO" ]; then
    echo "uso: $0 <versão>    (por exemplo: $0 0.1.2)" >&2
    exit 1
fi

if ! printf '%s' "$VERSAO" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9]+)?$'; then
    echo "a versão «${VERSAO}» não serve para o instalador." >&2
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
import json, os, pathlib, sys
caminho = pathlib.Path(sys.argv[2])
config = json.loads(caminho.read_text())
config["version"] = sys.argv[1]

# O pacote que o botão de atualizar baixa, quando esta máquina tem a chave do
# projeto. As duas metades ou nenhuma: a pública vive no repositório, em
# `plugins.updater.pubkey`, e a privada vem do ambiente. Ligar isto com só uma
# delas quebra o empacotamento no último passo.
privada = os.environ.get("TAURI_SIGNING_PRIVATE_KEY", "").strip()
publica = config.get("plugins", {}).get("updater", {}).get("pubkey", "").strip()
if privada and publica:
    config["bundle"]["createUpdaterArtifacts"] = True
    print("→ com pacote de atualização")
elif privada or publica:
    # Qual metade falta, e o que fazer — não só que falta uma.
    #
    # A mensagem antiga dizia «só metade da chave» e parava aí. Quem a lia sabia
    # que algo estava incompleto e não sabia o quê nem onde, o que é a mesma
    # falha que este produto passou o dia consertando na tela: a mensagem tem a
    # informação e não a entrega.
    if privada:
        print("→ a chave privada está no ambiente e a pública não está no "
              "tauri.conf.json (plugins.updater.pubkey).")
    else:
        print("→ a chave pública está no tauri.conf.json e a privada não está "
              "no ambiente. Para assinar:")
        print("→   export TAURI_SIGNING_PRIVATE_KEY=\"$(cat ~/.tauri/seele.key)\"")
        print("→   export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=…")
    print("→ Este .dmg funciona normal; ele só não serve de origem para atualização.")

caminho.write_text(json.dumps(config, indent=2, ensure_ascii=False) + "\n")
print("→ versão gravada:", config["version"])
PY

# A CLI antes do app, como no workflow: `seeled` e `plug` são o produto
# principal e o que menos pode falhar. Eles entram **dentro** do `.app`, para
# que quem baixa ganhe as duas metades num arquivo só.
echo "→ compilando seeled"
# A versão vai **para dentro** do binário aqui, e não pelo `Cargo.toml`.
#
# A versão do workspace é `0.0.0` de propósito, e quem a carimba é este script —
# no `tauri.conf.json`, que é do app. O `seeled` não passa por ali, então sem
# esta linha ele não teria como responder `--versao` com a verdade. Ver a
# pendência 30 e `seele_server::main::versao`.
SEELE_VERSAO="$VERSAO" cargo build --release --bin seeled --target "$ALVO"

mkdir -p apps/seele-app/binaries
for b in seeled; do
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
#
# `app,dmg` e não só `dmg`, pela mesma razão do workflow: o `.app` sempre é
# construído — o `.dmg` é uma imagem com ele dentro —, mas pedi-lo por nome é o
# que faz sair o `.app.tar.gz` que o atualizador baixa.
echo "→ empacotando o .dmg"
(cd apps/seele-app && cargo tauri build --config tauri.release.conf.json --bundles app,dmg)

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
# O `rw.NNNNN.<nome>.dmg` fica de fora.
#
# É a imagem de leitura e escrita que o `bundle_dmg.sh` monta para copiar o
# `.app` para dentro, e que deveria sumir no fim. Quando o empacotamento é
# interrompido — ou quando o `hdiutil` não desmonta —, ela sobra, casa com este
# mesmo padrão, e vira um segundo «pacote» que não é pacote nenhum.
#
# Aconteceu, e o script parou dizendo que achou dois `.dmg` de 0.6.3 depois de a
# assinatura já ter dado certo — a pior hora possível para parar.
encontrados=$(find target -type f -name "$padrao" ! -name 'rw.*' | wc -l | tr -d ' ')
if [ "$encontrados" != "1" ]; then
    echo "esperava exatamente um .dmg de $VERSAO e achei $encontrados." >&2
    echo "Procurei por «${padrao}» em target/." >&2
    find target -type f -name "$padrao" ! -name 'rw.*' >&2
    exit 1
fi
find target -type f -name "$padrao" ! -name 'rw.*' -exec cp {} "$DESTINO/" \;

# O pacote de atualização e a assinatura dele, quando este empacotamento teve a
# chave. O nome leva o alvo desta máquina de propósito: `empacotar/manifesto.py`
# lê a arquitetura daí, e um pacote só-ARM oferecido a um Mac Intel instala um
# app que não abre.
# Um, e exatamente um — nunca «o primeiro que aparecer».
#
# Este arquivo **não leva versão no nome**: ele é sempre `SEELE.app.tar.gz`. O
# `head -n 1` que estava aqui pegava o primeiro que o `find` devolvesse em toda
# a árvore, e o renomeava com a versão desta execução. Uma sobra de um
# empacotamento antigo — um universal, um alvo diferente — seria publicada como
# se fosse esta versão, assinada, e o atualizador a instalaria sem reclamar,
# porque a assinatura estaria certa: errado é o *conteúdo*, e nenhuma
# conferência do caminho pergunta isso.
#
# É o mesmo defeito que o `.dmg` e o `.deb` tiveram, na sua forma mais cara: lá
# a contagem errada **parava** o empacotamento; aqui ela entregava o pacote
# errado calada.
tarballs=$(find target/release/bundle/macos -type f -name '*.app.tar.gz' 2>/dev/null)
quantos_tarballs=$(printf '%s' "$tarballs" | grep -c . || true)
if [ "$quantos_tarballs" -gt 1 ]; then
    echo "achei $quantos_tarballs pacotes de atualização e não sei qual é o desta versão:" >&2
    echo "$tarballs" >&2
    echo "Eles não levam versão no nome. Limpe target/release/bundle/macos e" >&2
    echo "empacote de novo." >&2
    exit 1
fi
tarball=$(printf '%s' "$tarballs" | head -n 1)
if [ -n "$tarball" ]; then
    cp "$tarball" "$DESTINO/SEELE_${VERSAO}_${ALVO}.app.tar.gz"
    cp "$tarball.sig" "$DESTINO/SEELE_${VERSAO}_${ALVO}.app.tar.gz.sig"
    echo "→ pacote de atualização em $DESTINO"
fi

tar -czf "$DESTINO/seele-cli-$VERSAO-macos.tar.gz" -C "target/$ALVO/release" seeled

echo
echo "--- entrega ---"
(cd "$DESTINO" && shasum -a 256 *)
echo
echo "Este .dmg roda só em $ALVO. Para Mac Intel, o workflow é o caminho."
if [ -n "$tarball" ]; then
    echo
    echo "Depois de reunir os três sistemas em entrega/, monte o manifesto:"
    echo "  python3 empacotar/manifesto.py entrega v$VERSAO"
    echo "Sem ele o botão de atualizar do app não acha este release."
fi
