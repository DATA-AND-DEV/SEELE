#!/bin/sh
# Instala o `plug` e o `seeled` a partir de um release do GitHub.
#
#   curl -fsSL https://raw.githubusercontent.com/DATA-AND-DEV/SEELE/main/install.sh | sh
#
# Antes de rodar isso: você está prestes a executar um script que veio da rede.
# Num produto cujo argumento é não depender de terceiros, isso merece um
# segundo de atenção. Duas alternativas melhores, se preferir:
#
#   1. Baixe, leia, e só então rode:
#        curl -fsSLO https://raw.githubusercontent.com/DATA-AND-DEV/SEELE/main/install.sh
#        less install.sh && sh install.sh
#
#   2. Não use script nenhum: pegue o `.tar.gz` na aba Releases, confira a soma
#      contra o `SHA256SUMS` publicado ao lado, e descompacte onde quiser.
#
# O que este script faz, e nada além: descobre o sistema, baixa o pacote da
# versão pedida, **confere a soma SHA-256** contra o arquivo publicado no mesmo
# release, e copia dois binários para um diretório. Não mexe no seu shell, não
# escreve em `/usr`, não pede `sudo`, não manda nada para lugar nenhum.
#
# Variáveis:
#   SEELE_VERSION  versão a instalar (padrão: a última publicada)
#   SEELE_BIN      onde instalar (padrão: ~/.local/bin)
#   SEELE_BASE     de onde baixar (padrão: os releases do GitHub). Serve para
#                  espelho interno — e é o que torna este script testável, em
#                  vez de só escrito.

set -eu

REPO="DATA-AND-DEV/SEELE"
BIN="${SEELE_BIN:-$HOME/.local/bin}"

erro() {
    printf '\nerro: %s\n' "$1" >&2
    exit 1
}

precisa() {
    command -v "$1" >/dev/null 2>&1 || erro "preciso de \`$1\` e não achei."
}

precisa curl
precisa tar

# ---------------------------------------------------------------- plataforma

case "$(uname -s)" in
    Linux)  SISTEMA=linux ;;
    Darwin) SISTEMA=macos ;;
    *)      erro "sistema não suportado por este script: $(uname -s).
       No Windows use o install.ps1, ou baixe o .zip na aba Releases." ;;
esac

# Só o macOS publica pacote universal; no Linux o pacote é x86_64.
if [ "$SISTEMA" = linux ]; then
    case "$(uname -m)" in
        x86_64|amd64) : ;;
        *) erro "no Linux só há pacote para x86_64, e esta máquina é $(uname -m).
       Compile do código-fonte: cargo build --release --bin seeled --bin seele" ;;
    esac
fi

# ------------------------------------------------------------------- versão

if [ -n "${SEELE_VERSION:-}" ]; then
    VERSAO="$SEELE_VERSION"
else
    printf 'procurando a última versão... '
    # Sem `jq`: a API devolve o campo numa linha previsível, e depender de mais
    # uma ferramenta para ler um número seria pior.
    VERSAO=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
        | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n1)
    [ -n "$VERSAO" ] || erro "não achei nenhuma versão publicada.

       Se o repositório for privado, ou se ainda não houver release publicado
       (um rascunho não conta), este script não tem de onde baixar.
       Compile do código-fonte:

         git clone https://github.com/$REPO && cd SEELE
         cargo build --release --bin seeled --bin seele"
    printf '%s\n' "$VERSAO"
fi

NUMERO="${VERSAO#v}"
PACOTE="seele-cli-${NUMERO}-${SISTEMA}.tar.gz"
BASE="${SEELE_BASE:-https://github.com/$REPO/releases/download/$VERSAO}"

# --------------------------------------------------------------------- baixa

TRABALHO=$(mktemp -d)
# Sai limpo mesmo se algo falhar no meio: um diretório temporário com binários
# pela metade é o tipo de coisa que confunde na próxima tentativa.
trap 'rm -rf "$TRABALHO"' EXIT INT TERM

printf 'baixando %s\n' "$PACOTE"
curl -fsSL -o "$TRABALHO/$PACOTE" "$BASE/$PACOTE" \
    || erro "não consegui baixar $PACOTE.
       Confira se a versão $VERSAO tem pacote para $SISTEMA."

# ------------------------------------------------------------------ confere

printf 'conferindo a soma... '
if ! curl -fsSL -o "$TRABALHO/SHA256SUMS" "$BASE/SHA256SUMS"; then
    erro "este release não publica SHA256SUMS.

       Sem soma não há o que conferir, e instalar um binário sem conferir é
       exatamente o que este script deveria evitar. Baixe manualmente se
       aceitar o risco conscientemente."
fi

ESPERADA=$(grep " \*\{0,1\}$PACOTE\$" "$TRABALHO/SHA256SUMS" | awk '{print $1}' | head -n1)
[ -n "$ESPERADA" ] || erro "o SHA256SUMS não menciona $PACOTE."

if command -v sha256sum >/dev/null 2>&1; then
    OBTIDA=$(sha256sum "$TRABALHO/$PACOTE" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
    OBTIDA=$(shasum -a 256 "$TRABALHO/$PACOTE" | awk '{print $1}')
else
    erro "preciso de \`sha256sum\` ou \`shasum\` para conferir a soma."
fi

if [ "$ESPERADA" != "$OBTIDA" ]; then
    erro "A SOMA NÃO CONFERE.

       esperada: $ESPERADA
       obtida:   $OBTIDA

       O arquivo baixado não é o que foi publicado. Não instale.
       Pode ser corrupção no caminho — ou não."
fi
printf 'confere\n'

# ------------------------------------------------------------------ instala

tar -xzf "$TRABALHO/$PACOTE" -C "$TRABALHO"
mkdir -p "$BIN"

for programa in seele seeled; do
    [ -f "$TRABALHO/$programa" ] || erro "o pacote não traz \`$programa\`."
    # Copia e depois renomeia: substituir um binário em uso falha no meio se
    # for escrito por cima.
    cp "$TRABALHO/$programa" "$BIN/$programa.novo"
    chmod +x "$BIN/$programa.novo"
    mv -f "$BIN/$programa.novo" "$BIN/$programa"
done

# O macOS põe em quarentena o que veio da rede e recusa abrir sem isso.
if [ "$SISTEMA" = macos ] && command -v xattr >/dev/null 2>&1; then
    xattr -d com.apple.quarantine "$BIN/seele" "$BIN/seeled" 2>/dev/null || true
fi

printf '\ninstalado em %s\n' "$BIN"
printf '  seele   o cliente de terminal\n'
printf '  seeled  o servidor\n'

case ":$PATH:" in
    *":$BIN:"*) ;;
    *)
        printf '\n  %s não está no seu PATH. Acrescente:\n' "$BIN"
        printf '    export PATH="%s:$PATH"\n' "$BIN"
        ;;
esac

printf '\npara começar:\n'
printf '  seeled 0.0.0.0:8383      numa máquina\n'
printf '  seele --server <ip>:8383  na outra\n'
printf '\npara remover: rm %s/seele %s/seeled\n' "$BIN" "$BIN"
