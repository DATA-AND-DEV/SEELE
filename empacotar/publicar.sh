#!/bin/sh
# Gera os pacotes dos três sistemas e sobe o rascunho do release, sem Actions.
#
#   ./empacotar/publicar.sh 0.1.2
#
# É o `release.yml` inteiro numa máquina só: os três empacotadores de
# `empacotar/`, o `manifesto.py`, as somas de verificação e o rascunho no
# Releases. Ele **orquestra** os irmãos — não repete o que eles fazem, e cada
# armadilha que eles já pagaram continua morando neles.
#
#   macOS   `empacotar/macos.sh`    aqui, nativo, minutos
#   Windows `empacotar/windows.ps1` na sua máquina Windows, por SSH
#   Linux   `empacotar/linux.sh`    aqui, em Docker x86_64 emulado, 30 a 90 min
#
# ---------------------------------------------------------------- a ordem
#
# macOS, Windows, Linux — nessa ordem, e ela não é alfabética.
#
# O Linux é o mais caro dos três por uma ordem de grandeza, e é o único cuja
# falha custa hora e meia para aparecer. Se o que quebrou for o código — e é o
# que mais quebra —, ele quebra igual nos três, e o build nativo do Mac diz isso
# em minutos. Deixar o caro por último é o que transforma «noventa minutos
# perdidos» em «cinco».
#
# Sequencial, e não em paralelo: `macos.sh` e `linux.sh` reescrevem o **mesmo**
# `apps/seele-app/tauri.conf.json` e povoam o mesmo `apps/seele-app/binaries/`.
# Dois deles ao mesmo tempo é um gravando a versão enquanto o outro lê. O
# Windows está noutra máquina e poderia correr junto com um daqui — ganharia uns
# vinte minutos e pagaria com dois builds a interromper quando alguém aperta
# Ctrl-C, e com a saída dos dois embaralhada na mesma tela. O tempo é do
# computador; a clareza é de quem está olhando às duas da manhã.
#
# ------------------------------------------------------- conferir antes de tudo
#
# **Nada compila antes de todas as conferências passarem.** O pior resultado
# possível deste script é noventa minutos de Linux emulado terminando em «não
# consegui alcançar o Windows» — uma frase que custa um segundo para descobrir e
# que este script descobre antes do primeiro `cargo build`. `--conferir` roda só
# essa parte, e é o que se roda antes de sair para o almoço.
#
# ---------------------------------------------------------- o Windows por SSH
#
# O Windows 10/11 traz o **OpenSSH Server** como recurso opcional, desligado de
# fábrica. Ligá-lo é a seção 7 de `docs/windows.md`, e sem ele este script não
# tem como chegar lá.
#
# Duas coisas atravessam o canal, e nenhuma delas encosta no disco de lá:
#
#   1. O trecho de PowerShell a executar, em `-EncodedCommand`. **O shell padrão
#      do OpenSSH no Windows é o `cmd`, não o PowerShell** — tudo o que se manda
#      passa pelas regras de citação do `cmd` antes de chegar a qualquer lugar, e
#      elas comem aspas, `&`, `|`, `^` e acento. Base64 de UTF-16LE não tem
#      nenhum desses caracteres: não há o que o `cmd` possa interpretar.
#
#   2. A chave privada do projeto, **pela entrada padrão**, a cada build.
#
# A chave fica no Mac. Uma chave que existe em dois discos é uma chave com duas
# chances de vazar, e esta é a que deixa qualquer um assinar atualização para
# todo SEELE instalado — `docs/assinatura-e-atualizacao.md`, 1.3, diz o tamanho
# do estrago: quem a perde não consegue rotacionar, porque o app confere contra
# a chave compilada dentro dele.
#
# Ela não vai na linha de comando de propósito. No Windows a linha de comando de
# um processo é legível por outros processos da máquina, e o `sshd` com
# `LogLevel VERBOSE` a escreve no log de eventos. Pela entrada padrão ela existe
# só na memória daquele `powershell.exe`, enquanto ele durar.
#
# O que isso **não** compra, e é honesto dizer: se a máquina Windows estiver
# comprometida, ela vê a chave durante o build — o `cargo tauri` precisa dela em
# claro para assinar. O que se ganha é reduzir a exposição de «sempre» para «os
# minutos de cada empacotamento», e não deixar cópia para trás.
#
# ------------------------------------------------------------- `gh` ou `curl`
#
# `curl`, com um token. O `gh` não está instalado nesta máquina, e pedir que
# esteja é pedir uma instalação a mais para quatro chamadas HTTP — quatro,
# contadas: quem é o token, se ele pode escrever aqui, se já existe release
# nesta tag, e criar o rascunho (mais um POST por arquivo).
#
# O `release.yml` usa `gh` porque ele **já vem** no runner; a mesma frase que o
# justifica lá («nada de dependência a mais num workflow que cria releases»)
# aponta para o outro lado aqui, onde ele não vem. E há um ganho: com `curl` o
# token é um token — de escopo declarado, que se revoga numa tela — em vez do
# estado de sessão que o `gh auth login` guarda no chaveiro com escopos que
# ninguém releu depois.
#
# O token entra no `curl` por `--config -`, e não por `-H`. Argumento de linha de
# comando é público para qualquer `ps` da máquina.
#
#   export SEELE_GITHUB_TOKEN=github_pat_…
#
# Um token fino («fine-grained») com **Contents: Read and write** neste
# repositório basta. Um clássico precisa do escopo `repo`.
#
# ------------------------------------------------------------ o que sai daqui
#
# **Um rascunho, não um release publicado.** Não é timidez: o endereço gravado
# dentro de cada app é `releases/latest/download/latest.json`, e para o GitHub
# `releases/latest` é o último release **publicado**. Enquanto o rascunho for
# rascunho, nenhum SEELE do mundo o enxerga. Publicar é o gesto que faz o botão
# de atualizar de todo mundo ver a versão nova, e ele continua sendo de uma
# pessoa que baixou e abriu pelo menos um dos arquivos.
#
# E uma coisa este release perde por não vir do CI: **atestado de procedência.**
# O `release.yml` assina, com o GitHub, um documento que amarra cada arquivo ao
# commit e à execução que o produziu. Um pacote montado na sua mesa não tem isso
# e não há como ter — o que assina é a infraestrutura, não a chave de ninguém.
# O `SHA256SUMS` continua respondendo «o arquivo chegou inteiro»; os `.sig`
# continuam respondendo «veio de quem tem a chave do projeto»; o que ninguém
# consegue responder é «veio daquele código». Está dito na saída deste script e
# no corpo do release, para quem baixar não descobrir por um comando que falha.

set -u

# --------------------------------------------------------------- constantes

RAIZ="$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)"
CONFIG_TAURI="apps/seele-app/tauri.conf.json"
SISTEMAS="macos windows linux"
API="https://api.github.com"
ENVIO="https://uploads.github.com"

# --------------------------------------------------------------- as opções

VERSAO=""
REPO="${SEELE_REPO:-DATA-AND-DEV/SEELE}"
WINDOWS="${SEELE_WINDOWS_SSH:-}"
REPO_WINDOWS="${SEELE_WINDOWS_REPO:-C:\\SEELE}"
TOKEN="${SEELE_GITHUB_TOKEN:-${GITHUB_TOKEN:-${GH_TOKEN:-}}}"
PULAR=""
PARCIAL=nao
SEM_ASSINATURA=nao
SO_CONFERIR=nao

COMMIT=""
TOCOU_WINDOWS=nao
COMECOU_BUILD=nao
FAXINA_FEITA=nao
TEMPORARIO=""
CODIGO_HTTP=""
CORPO_API=""
RELEASE_ID=""
RELEASE_ESTADO=""
RELEASE_URL=""

# --------------------------------------------------------------- a conversa

passo() { printf '→ %s\n' "$1"; }

aviso() { printf '!  %s\n' "$1" >&2; }

# Morrer dizendo o que falhou **e** o que fazer.
#
# Não há `set -e` neste arquivo, e a ausência é deliberada. `set -e` transforma
# qualquer falha na mesma frase — nenhuma — e este script existe para atravessar
# duas horas de compilação: cada saída dele tem de dizer em qual passo parou e
# qual é o comando seguinte. Além disso ele **precisa** sobreviver à falha de um
# empacotador para tentar os outros dois, que é exatamente o que `set -e`
# proíbe.
morrer() {
    printf '\n' >&2
    while [ "$#" -gt 0 ]; do
        printf '%s\n' "$1" >&2
        shift
    done
    exit 1
}

uso() {
    printf '%s\n' \
"uso: $0 <versão> [opções]" \
"" \
"  --windows <usuário@máquina>  onde o Windows atende SSH (ou \$SEELE_WINDOWS_SSH)" \
"  --repo-windows <caminho>     o repositório lá (ou \$SEELE_WINDOWS_REPO)" \
"  --repo <dono/repositório>    onde publicar" \
"  --pular <lista>              macos,windows,linux — separados por vírgula" \
"  --parcial                    sobe o rascunho mesmo faltando sistema" \
"  --sem-assinatura             segue sem a chave do projeto (release que não" \
"                               atualiza ninguém)" \
"  --conferir                   só as conferências prévias; não compila nada" \
"  --decidir <pedidos> <falhas> a regra de publicação, sozinha, sem compilar" \
"" \
"ambiente:" \
"  SEELE_GITHUB_TOKEN                    token com escrita no repositório" \
"  TAURI_SIGNING_PRIVATE_KEY             a chave do projeto (fica só aqui)" \
"  TAURI_SIGNING_PRIVATE_KEY_PASSWORD    a senha dela" \
"  SEELE_WINDOWS_SSH                     usuário@máquina do Windows" \
"  SEELE_WINDOWS_REPO                    o repositório lá (hoje: C:\\SEELE)"
}

# ------------------------------------------------------------- a decisão

# Os pedidos menos as falhas — quem deu certo. Serve à frase de retomada.
sem_falhas() {
    sf_saida=""
    for sf_um in $1; do
        case " $2 " in
            *" $sf_um "*) continue ;;
        esac
        sf_saida="$sf_saida $sf_um"
    done
    printf '%s' "${sf_saida# }"
}

# O que fazer quando um dos três falhou e os outros deram certo.
#
# Está numa função de três argumentos, sem tocar em disco nem em rede, porque é
# aqui que os defeitos moram e um orquestrador que só se prova rodando por
# noventa minutos não se prova nunca. `--decidir` a expõe inteira:
#
#   ./empacotar/publicar.sh --decidir "macos windows linux" "windows"
#
# Imprime a decisão na primeira linha — `publicar`, `publicar-parcial` ou
# `abortar` — e a explicação depois. Devolve 0 quando há o que publicar.
decidir() {
    d_pedidos="$1"
    d_falhas="$2"
    d_parcial="$3"

    d_quantos_pedidos=$(printf '%s' "$d_pedidos" | wc -w | LC_ALL=C tr -d ' ')
    d_quantos_falhas=$(printf '%s' "$d_falhas" | wc -w | LC_ALL=C tr -d ' ')

    if [ "$d_quantos_falhas" -eq 0 ]; then
        echo publicar
        if [ "$d_quantos_pedidos" -eq 0 ]; then
            echo "Nenhum sistema foi pedido: sobe o que já estiver em entrega/."
        else
            printf 'Os sistemas pedidos ficaram prontos: %s.\n' "$d_pedidos"
        fi
        return 0
    fi

    if [ "$d_quantos_falhas" -ge "$d_quantos_pedidos" ]; then
        echo abortar
        printf 'Nenhum dos sistemas pedidos ficou pronto: %s.\n' "$d_falhas"
        echo "Não há pacote novo a publicar; o rascunho não é criado."
        return 1
    fi

    if [ "$d_parcial" = sim ]; then
        echo publicar-parcial
        printf 'Falharam: %s. O rascunho sobe assim mesmo, por --parcial.\n' "$d_falhas"
        echo "O latest.json vai sem esses sistemas: quem os usa aperta o botão de"
        echo "atualizar, não recebe versão nova e não é informado do porquê — fica"
        echo "para trás em silêncio até um release seguinte trazer o sistema de volta."
        return 0
    fi

    echo abortar
    printf 'Falharam: %s.\n' "$d_falhas"
    echo "O que deu certo continua em entrega/ — nada foi apagado."
    echo "Para retomar só o que falta, sem refazer o que já ficou pronto:"
    printf '  %s <versão> --pular %s\n' "$0" "$(sem_falhas "$d_pedidos" "$d_falhas" | LC_ALL=C tr ' ' ',')"
    echo "Se a intenção é publicar faltando sistema, é --parcial, e ela custa: o"
    echo "latest.json sai sem eles e quem os usa deixa de receber atualização."
    return 1
}

# ------------------------------------------------------------- o Windows

# PowerShell em base64 de UTF-16LE, que é o que `-EncodedCommand` come.
#
# Feito com o `python3` que este script já exige para o manifesto, em vez de com
# `iconv | base64`: as opções de quebra de linha do `base64` divergem entre BSD e
# GNU, e uma quebra no meio do argumento é o `cmd` recebendo dois argumentos.
codificar_ps() {
    printf '%s' "$1" | python3 -c 'import base64, sys
sys.stdout.write(base64.b64encode(sys.stdin.buffer.read().decode("utf-8").encode("utf-16-le")).decode())'
}

em_base64() {
    # A chave passa por aqui. `printf` é embutido do shell: nenhum processo novo
    # nasce com o segredo no `argv`.
    printf '%s' "$1" | python3 -c 'import base64, sys
sys.stdout.write(base64.b64encode(sys.stdin.buffer.read()).decode())'
}

# Roda um trecho de PowerShell no Windows e devolve a saída como texto, sem
# `\r`. A saída de erro entra junto de propósito: aqui ela é mensagem para uma
# pessoa ler, e não dado a processar.
no_windows() {
    nw_codificado=$(codificar_ps "$1")
    if [ -z "$nw_codificado" ]; then
        return 1
    fi
    ssh -o BatchMode=yes -o ConnectTimeout="${SEELE_SSH_TIMEOUT:-15}" "$WINDOWS" \
        powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass \
        -EncodedCommand "$nw_codificado" </dev/null 2>&1 | LC_ALL=C tr -d '\r'
}

# O mesmo, para quando a saída é **dado**: a de erro fica na de erro, e o
# resultado sai limpo. Misturar as duas aqui corromperia o base64 do zip com uma
# mensagem de aviso, e `b64decode` descarta o que não é do alfabeto — ou seja,
# corromperia em silêncio.
no_windows_dado() {
    nwd_codificado=$(codificar_ps "$1")
    if [ -z "$nwd_codificado" ]; then
        return 1
    fi
    ssh -o BatchMode=yes -o ConnectTimeout="${SEELE_SSH_TIMEOUT:-15}" "$WINDOWS" \
        powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass \
        -EncodedCommand "$nwd_codificado" </dev/null
}

# --------------------------------------------------------------- a faxina

# O que este script não pode deixar para trás.
#
# Os três empacotadores gravam a versão no `tauri.conf.json` e a devolvem na
# saída — cada um com o seu `trap`, cada um já mordido uma vez. Quando quem os
# chama morre no meio, esses `trap` costumam rodar (o Ctrl-C vai para o grupo
# inteiro), mas há um caso em que não rodam: a sessão SSH cair leva o
# `powershell.exe` embora sem executar o `finally` dele, e o `tauri.conf.json`
# **da máquina Windows** fica com o número do release gravado.
#
# É por isso que a árvore limpa é conferida antes de começar: qualquer diferença
# neste arquivo depois disso é nossa, e pode ser desfeita sem apagar trabalho de
# ninguém. `COMECOU_BUILD` é o que garante essa frase — enquanto nenhum
# empacotador rodou, este script **não** desfaz nada, porque aí a mudança seria
# de quem estava trabalhando. O teste `o_titulo_da_janela_atravessou_inteiro`
# existe porque isso já vazou para um commit uma vez.
faxina() {
    if [ "$FAXINA_FEITA" = sim ]; then
        return 0
    fi
    FAXINA_FEITA=sim

    if [ -n "$TEMPORARIO" ] && [ -d "$TEMPORARIO" ]; then
        rm -rf "$TEMPORARIO"
    fi

    if [ "$COMECOU_BUILD" = nao ]; then
        return 0
    fi

    if [ -n "$(git -C "$RAIZ" status --porcelain -- "$CONFIG_TAURI" 2>/dev/null)" ]; then
        git -C "$RAIZ" checkout -- "$CONFIG_TAURI" 2>/dev/null
        aviso "o $CONFIG_TAURI ficou com a versão gravada e foi devolvido ao que era."
    fi

    if [ "$TOCOU_WINDOWS" = sim ]; then
        fx_resposta=$(no_windows "\$ErrorActionPreference = 'SilentlyContinue'
Set-Location '$REPO_WINDOWS'
if (git status --porcelain -- '$CONFIG_TAURI') {
    git checkout -- '$CONFIG_TAURI'
    Write-Output 'restaurado'
}")
        case "$fx_resposta" in
            *restaurado*)
                aviso "no Windows o $CONFIG_TAURI também ficou sujo e foi devolvido."
                ;;
        esac
    fi
}

# ------------------------------------------------ as notas de uma versão

# Os escopos cujo conserto a pessoa que baixa o SEELE **sente**.
#
# A lista é escrita à mão de propósito. Um mapa automático precisaria de uma
# regra, e a regra seria sempre alguma variação de "o que está em crates/ é
# produto" — que é falsa: `ffi` é ponte e `xtask` é ferramenta, e os dois moram
# lá. Uma lista curta que alguém revisa quando um escopo novo aparece é mais
# honesta que uma heurística que erra em silêncio.
ESCOPOS_DE_PRODUTO="alcance admissao anexos app atualizador audio media
cages cascas chamada chegada cliente conformance convite core dogma encontro
enlace entrada escada fontes frases furo hospedagem interface marca medida
permissions mensagens moderar mods plug porta portaria proto rede seguranca server
sessao spike sync taxa tela telemetria tofu tui ui uri varredura voz"

# Os escopos que existem para **montar** o SEELE, e não para usá-lo.
#
# Continuam na página: quem publica, quem empacota noutra máquina e quem
# desconfia de um pacote têm o que fazer com eles. Só não lideram.
ESCOPOS_DE_FERRAMENTA="build ci deps empacotar ffi publicar release test
testes xtask"

# Em que seção um escopo entra.
#
# O padrão de um escopo desconhecido é **produto**, e a escolha tem um lado
# barato e um caro: deixar um conserto de empacotamento à vista custa uma linha
# feia numa página; enterrar uma mudança que a pessoa sente custa ela não saber
# que existe. O erro barato é que vira padrão.
#
# E o desconhecido **avisa**, em `secao_do_escopo` não, mas em quem chama: um
# padrão que decide calado é como uma tabela envelhece sem ninguém perceber.
secao_do_escopo() {
    for se_um in $ESCOPOS_DE_FERRAMENTA; do
        [ "$1" = "$se_um" ] && { printf 'ferramenta'; return 0; }
    done
    for se_um in $ESCOPOS_DE_PRODUTO; do
        [ "$1" = "$se_um" ] && { printf 'produto'; return 0; }
    done
    printf 'produto'
    return 1
}

# O corpo do release, a partir dos assuntos dos commits da faixa.
#
# Lê da **entrada padrão** e escreve na saída, sem tocar em git, em disco nem em
# rede. Isso não é purismo: a página de um release é o lugar onde ninguém
# percebe um defeito até ele estar publicado, e uma função de texto puro se
# prova alimentando texto — `--notas` a expõe inteira:
#
#   git log --no-merges --format='%s' v0.6.1..HEAD | ./empacotar/publicar.sh --notas
#
# Só `feat` e `fix` entram. `docs`, `test`, `chore` e `refactor` são verdade
# sobre o commit e não são mudança do produto; quem quiser a verdade completa
# tem o histórico, que continua sendo ela.
notas_das_mudancas() {
    ndm_produto=""
    ndm_ferramenta=""
    ndm_novos=""

    while IFS= read -r ndm_linha; do
        case "$ndm_linha" in
            feat\(*\):\ *|fix\(*\):\ *)
                ndm_resto="${ndm_linha#*\(}"
                ndm_escopo="${ndm_resto%%\)*}"
                ndm_assunto="${ndm_linha#*\): }"
                ;;
            feat:\ *|fix:\ *)
                ndm_escopo=""
                ndm_assunto="${ndm_linha#*: }"
                ;;
            *)
                continue
                ;;
        esac

        if [ -n "$ndm_escopo" ]; then
            ndm_item="- **$ndm_escopo** — $ndm_assunto"
            if ndm_secao=$(secao_do_escopo "$ndm_escopo"); then
                :
            else
                # Desconhecido: entra em produto e é nomeado para quem publica
                # classificar. Uma vez por escopo, não uma vez por commit.
                case " $ndm_novos " in
                    *" $ndm_escopo "*) : ;;
                    *) ndm_novos="$ndm_novos $ndm_escopo" ;;
                esac
            fi
        else
            # Sem escopo é forma legítima de conventional commit, e descartá-la
            # perderia mudança sem dizer nada.
            ndm_item="- $ndm_assunto"
            ndm_secao="produto"
        fi

        if [ "$ndm_secao" = "ferramenta" ]; then
            ndm_ferramenta="$ndm_ferramenta$ndm_item
"
        else
            ndm_produto="$ndm_produto$ndm_item
"
        fi
    done

    for ndm_um in $ndm_novos; do
        # As chaves não são estilo: sem elas o `»` (0xC2 0xBB) entra no nome da
        # variável em alguns shells, e o script morre com «unbound variable»
        # apontando para um nome que ninguém escreveu. É o mesmo defeito que o
        # empacotador do macOS já teve, e a mesma letra.
        aviso "«${ndm_um}» é um escopo que a tabela de $0 não conhece." \
            "Ele entrou em «O que mudou», que é o lado visível." \
            "Classifique-o em ESCOPOS_DE_PRODUTO ou ESCOPOS_DE_FERRAMENTA."
    done

    if [ -z "$ndm_produto" ] && [ -z "$ndm_ferramenta" ]; then
        # Uma versão só de papel e teste existe, e a página tem que dizer isso.
        # Uma seção vazia parece defeito de script para quem lê.
        printf '%s\n' \
"_Esta versão não traz nenhuma mudança de produto: só documentação, testes e" \
"arrumação interna. O histórico do repositório tem a lista completa._"
        return 0
    fi

    if [ -n "$ndm_produto" ]; then
        printf '%s\n\n' "## O que mudou"
        printf '%s' "$ndm_produto"
        printf '\n'
    fi
    if [ -n "$ndm_ferramenta" ]; then
        printf '%s\n\n' "## Por baixo"
        printf '%s' "$ndm_ferramenta"
        printf '\n'
    fi
}

# A tag da versão publicada antes desta.
#
# Lê a lista de tags da entrada padrão — de novo para ser testável sem
# repositório. Devolve vazio quando não há anterior, que é o caso da primeira
# publicação e **não** é erro: quem chama diz isso na página em vez de listar o
# histórico inteiro fingindo que é novidade.
tag_anterior() {
    ta_atual="v${1:-}"
    ta_melhor=""
    while IFS= read -r ta_uma; do
        case "$ta_uma" in
            v[0-9]*) : ;;
            *) continue ;;
        esac
        [ "$ta_uma" = "$ta_atual" ] && continue
        if [ -z "$ta_melhor" ]; then
            ta_melhor="$ta_uma"
            continue
        fi
        # A maior das duas, por versão e não por texto: sem isto v0.10.0
        # perderia para v0.9.0, e a faixa do release sairia errada justamente
        # quando o projeto passasse de nove.
        ta_maior=$(printf '%s\n%s\n' "${ta_melhor#v}" "${ta_uma#v}" \
            | sort -t. -k1,1n -k2,2n -k3,3n | tail -n 1)
        ta_melhor="v$ta_maior"
    done
    printf '%s' "$ta_melhor"
}

# --------------------------------------------------- os argumentos da linha

if [ "${1:-}" = "--notas" ]; then
    # O corpo do release a partir dos assuntos, sozinho: sem git e sem rede.
    notas_das_mudancas
    exit 0
fi

if [ "${1:-}" = "--tag-anterior" ]; then
    # A faixa, sozinha: recebe a lista de tags na entrada padrão.
    tag_anterior "${2:-}"
    exit 0
fi

if [ "${1:-}" = "--decidir" ]; then
    # A regra sozinha: sem repositório, sem rede, sem compilar nada.
    if [ "$#" -lt 3 ]; then
        morrer "uso: $0 --decidir \"<pedidos>\" \"<falhas>\" [--parcial]"
    fi
    if [ "${4:-}" = "--parcial" ]; then
        PARCIAL=sim
    fi
    decidir "$2" "$3" "$PARCIAL"
    exit "$?"
fi

while [ "$#" -gt 0 ]; do
    case "$1" in
        --windows)
            if [ "$#" -lt 2 ]; then
                morrer "--windows quer um destino, como alexandre@192.168.0.7"
            fi
            WINDOWS="$2"
            shift 2
            ;;
        --repo-windows)
            if [ "$#" -lt 2 ]; then
                morrer "--repo-windows quer um caminho, como C:\\SEELE"
            fi
            REPO_WINDOWS="$2"
            shift 2
            ;;
        --repo)
            if [ "$#" -lt 2 ]; then
                morrer "--repo quer dono/repositório"
            fi
            REPO="$2"
            shift 2
            ;;
        --pular)
            if [ "$#" -lt 2 ]; then
                morrer "--pular quer uma lista, como windows,linux"
            fi
            PULAR="$2"
            shift 2
            ;;
        --parcial) PARCIAL=sim; shift ;;
        --sem-assinatura) SEM_ASSINATURA=sim; shift ;;
        --conferir) SO_CONFERIR=sim; shift ;;
        -h|--help|--ajuda) uso; exit 0 ;;
        -*) uso >&2; morrer "não conheço a opção «${1}»." ;;
        *)
            if [ -n "$VERSAO" ]; then
                uso >&2
                morrer "recebi duas versões: «${VERSAO}» e «${1}»."
            fi
            VERSAO="$1"
            shift
            ;;
    esac
done

if [ -z "$VERSAO" ]; then
    uso >&2
    morrer "falta a versão."
fi

# ------------------------------------------------------- quais sistemas

for pulado in $(printf '%s' "$PULAR" | LC_ALL=C tr ',' ' '); do
    case " $SISTEMAS " in
        *" $pulado "*) ;;
        *) morrer "não conheço o sistema «${pulado}»." "Os que existem: $SISTEMAS." ;;
    esac
done

PEDIDOS=""
for sistema in $SISTEMAS; do
    case ",$PULAR," in
        *",$sistema,"*) continue ;;
    esac
    PEDIDOS="$PEDIDOS $sistema"
done
PEDIDOS="${PEDIDOS# }"

pedido() {
    case " $PEDIDOS " in
        *" $1 "*) return 0 ;;
    esac
    return 1
}

# O temporário nasce antes das conferências porque a primeira resposta do
# GitHub já precisa de um lugar para pousar; a faxina o remove em qualquer
# saída, inclusive nas que reprovam.
TEMPORARIO=$(mktemp -d "${TMPDIR:-/tmp}/seele-publicar.XXXXXX" 2>/dev/null)
if [ -z "$TEMPORARIO" ] || [ ! -d "$TEMPORARIO" ]; then
    morrer "não consegui criar um diretório temporário."
fi
CORPO_API="$TEMPORARIO/resposta"
trap 'faxina' EXIT
trap 'faxina; exit 130' INT
trap 'faxina; exit 143' TERM

# =========================================================================
# As conferências. Todas antes do primeiro build, da mais barata para a mais
# cara — o que é local e instantâneo primeiro, a rede por último.
# =========================================================================

conferir_versao() {
    # A mesma regra dos três irmãos, e pelo mesmo motivo: o formato do
    # instalador não sabe representar pré-lançamento por extenso, e recusa
    # **depois** de compilar.
    if ! printf '%s' "$VERSAO" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9]+)?$'; then
        morrer "a versão «${VERSAO}» não serve para o instalador." \
            "Aceito: X.Y.Z, ou X.Y.Z-N com N só de dígitos." \
            "Não serve: -dev, -rc1, -beta.2, +metadados, ou o «v» na frente."
    fi
    passo "versão $VERSAO: ok"
}

conferir_ferramentas() {
    # O `shasum` entra nesta lista por um motivo de tempo, e não de higiene: ele
    # só é usado no fim, depois de todos os builds. Descobrir a falta dele lá é
    # descobri-la duas horas tarde demais.
    for ferramenta in python3 curl git shasum; do
        if ! command -v "$ferramenta" >/dev/null 2>&1; then
            morrer "este script precisa do «${ferramenta}» e ele não está no PATH."
        fi
    done

    if [ ! -f "$RAIZ/empacotar/manifesto.py" ]; then
        morrer "não achei empacotar/manifesto.py." \
            "Sem ele o release sai sem latest.json e ninguém atualiza a partir dele."
    fi
    if pedido macos && [ ! -f "$RAIZ/empacotar/macos.sh" ]; then
        morrer "não achei empacotar/macos.sh." \
            "Este script orquestra os empacotadores; sem eles não há o que orquestrar."
    fi
    if pedido linux && [ ! -f "$RAIZ/empacotar/linux.sh" ]; then
        morrer "não achei empacotar/linux.sh."
    fi
    if pedido windows && ! command -v ssh >/dev/null 2>&1; then
        morrer "sem «ssh» não há como alcançar o Windows." \
            "Ou instale-o, ou rode com --pular windows."
    fi
    passo "ferramentas e empacotadores irmãos: ok"
}

conferir_arvore() {
    if ! git -C "$RAIZ" rev-parse --git-dir >/dev/null 2>&1; then
        morrer "«${RAIZ}» não é um repositório git." \
            "O release aponta para um commit, e sem repositório não há commit a apontar."
    fi

    # Só o que o empacotamento escreve, e não a árvore inteira.
    #
    # A conferência larga bloqueava um repositório onde havia trabalho em curso
    # em qualquer arquivo — inclusive um que o empacotamento nunca toca. Ela
    # parou o dono na primeira execução por causa de um documento de marca que
    # vive editado, e a saída que ela sugeria era `git stash` num arquivo dele.
    #
    # O motivo escrito logo abaixo é a medida certa do escopo: o que precisa
    # estar limpo é o que estes scripts reescrevem e devolvem, para que um resto
    # deixado por uma execução que morreu no meio seja distinguível do que já
    # estava aí. Fora desses caminhos, sujeira não confunde ninguém.
    sujeira=$(git -C "$RAIZ" status --porcelain -- \
        "$CONFIG_TAURI" \
        "apps/seele-app/tauri.release.conf.json" \
        "apps/seele-app/binaries" \
        "empacotar" 2>/dev/null)
    if [ -n "$sujeira" ]; then
        morrer "há trabalho não commitado no que o empacotamento escreve:" \
            "$sujeira" \
            "" \
            "Não é preciosismo. Os empacotadores gravam a versão no $CONFIG_TAURI e a" \
            "devolvem ao sair; se algo morrer no meio, a única forma de eu saber o que é" \
            "resto meu e o que é trabalho seu é ter começado do limpo nestes caminhos." \
            "O resto da árvore não me interessa: commite ou guarde só isto."
    fi

    COMMIT=$(git -C "$RAIZ" rev-parse HEAD 2>/dev/null)
    if [ -z "$COMMIT" ]; then
        morrer "não consegui ler o commit de HEAD."
    fi
    passo "o que o empacotamento escreve está limpo, em $COMMIT"
}

conferir_config_tauri() {
    ct_caminho="$RAIZ/$CONFIG_TAURI"
    if [ ! -f "$ct_caminho" ]; then
        morrer "não achei o $CONFIG_TAURI."
    fi
    # As duas metades do mesmo defeito, e as duas já aconteceram. Os testes de
    # `xtask/tests/empacotamento.rs` guardam o repositório; esta linha guarda
    # **este** empacotamento — começar com o arquivo já corrompido produziria um
    # instalador com «SEELE Â· Entry Plug» na barra de título.
    if ! python3 -c 'import sys
sys.exit(1 if open(sys.argv[1], "rb").read(3) == b"\xef\xbb\xbf" else 0)' "$ct_caminho"; then
        morrer "o $CONFIG_TAURI está com BOM, e o Tauri recusa o arquivo assim." \
            "A mensagem que ele dá — «expected value at line 1 column 1» — não parece" \
            "ter nada a ver com BOM nenhum."
    fi
    if ! grep -q 'SEELE · Entry Plug' "$ct_caminho"; then
        morrer "o título da janela não está inteiro no $CONFIG_TAURI." \
            "É o rastro de UTF-8 lido como Latin-1 em algum empacotamento anterior." \
            "Desfaça a mudança nesse arquivo antes de empacotar."
    fi
    passo "$CONFIG_TAURI íntegro"
}

limpar_entrega() {
    if [ ! -d "$RAIZ/entrega" ]; then
        passo "entrega/ ainda não existe; será criada"
        return 0
    fi
    # Restos de **outra** versão, e não restos em geral: retomar um sistema que
    # falhou é o caso normal deste script, e nele os arquivos dos que deram
    # certo têm de continuar ali. O que não pode é o `.dmg` de 0.4.0 subir junto
    # com o release de 0.4.1 — o `SHA256SUMS` cobriria os dois e a página
    # ofereceria duas versões com o mesmo nome de release.
    #
    # O `.DS_Store` fica de fora: ele não é entrega de ninguém, é um arquivo que
    # o Finder escreve sozinho em toda pasta que alguém abriu. Barrar por causa
    # dele manda a pessoa «mover a entrega passada» de um arquivo que ela não
    # criou e que vai voltar sozinho na próxima vez que ela olhar a pasta.
    ce_restos=$(find "$RAIZ/entrega" -type f ! -name "*$VERSAO*" ! -name ".DS_Store" 2>/dev/null)
    if [ -z "$ce_restos" ]; then
        passo "entrega/ sem restos de outra versão"
        return 0
    fi

    # Apagados, e não movidos para o lado. Esta decisão foi revertida em
    # 2026-08-20 a pedido de quem publica: antes o script parava aqui e mandava
    # mover à mão, com o argumento de que «quem apaga entrega passada apaga a
    # entrega que ainda não foi publicada».
    #
    # O argumento continua verdadeiro, e o preço foi aceito de olho aberto:
    # parar era um passo manual em **toda** publicação, e o que se perde é
    # reconstruível a partir do commit que o gerou. O que não foi aceito é
    # apagar calado — daí o nome de cada arquivo na saída, um por linha.
    passo "limpando entrega/ de outra versão"
    printf '%s\n' "$ce_restos" | while IFS= read -r ce_um; do
        [ -n "$ce_um" ] || continue
        printf '     apagado: %s\n' "${ce_um#"$RAIZ/"}"
        rm -f "$ce_um"
    done
}

conferir_chave() {
    ck_privada="${TAURI_SIGNING_PRIVATE_KEY:-}"
    ck_publica=$(python3 -c 'import json, sys
config = json.load(open(sys.argv[1], encoding="utf-8"))
print(config.get("plugins", {}).get("updater", {}).get("pubkey", "").strip())' \
        "$RAIZ/$CONFIG_TAURI" 2>/dev/null)

    if [ -n "$ck_privada" ] && [ -n "$ck_publica" ]; then
        passo "as duas metades da chave do projeto: ok"
        return 0
    fi

    if [ "$SEM_ASSINATURA" = sim ]; then
        aviso "seguindo sem assinar, por --sem-assinatura."
        aviso "Este release não terá latest.json: ninguém atualiza a partir dele."
        return 0
    fi

    # Falhar aqui, e não no fim: sem assinatura não há `latest.json`, e um
    # release sem manifesto deixa todo mundo sem atualização até o seguinte —
    # sem que quem o montou tenha como saber. Descobrir isso depois de duas
    # horas de compilação é descobrir tarde demais.
    if [ -z "$ck_publica" ]; then
        morrer "a chave **pública** não está no $CONFIG_TAURI (plugins.updater.pubkey)." \
            "docs/assinatura-e-atualizacao.md, parte 1.1, diz como colá-la lá." \
            "Para empacotar assim mesmo, e sem botão de atualizar: --sem-assinatura"
    fi
    morrer "a chave **privada** não está no ambiente. Para assinar:" \
        "  export TAURI_SIGNING_PRIVATE_KEY=\"\$(cat ~/.tauri/seele.key)\"" \
        "  export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=…" \
        "" \
        "A pública já está no repositório, então este release **deveria** atualizar." \
        "Para empacotar assim mesmo, e sem botão de atualizar: --sem-assinatura"
}

conferir_docker() {
    pedido linux || return 0
    if ! command -v docker >/dev/null 2>&1; then
        morrer "o pacote do Linux precisa do Docker e ele não está no PATH." \
            "Ou instale-o, ou rode com --pular linux."
    fi
    if ! docker info >/dev/null 2>&1; then
        morrer "o Docker está instalado e não responde — o daemon não está no ar." \
            "Abra o Docker Desktop e espere ele ficar verde, ou rode com --pular linux." \
            "Saber disto agora custa um segundo; saber daqui a uma hora custa a hora."
    fi
    passo "Docker responde"
}

conferir_windows() {
    pedido windows || return 0

    if [ -z "$WINDOWS" ]; then
        morrer "não sei onde fica o Windows." \
            "  export SEELE_WINDOWS_SSH=usuário@máquina    (ou --windows)" \
            "docs/windows.md, seção 7, liga o OpenSSH Server lá e mostra como conferir." \
            "Sem Windows não há instalador de Windows: para seguir sem ele, --pular windows."
    fi
    case "$REPO_WINDOWS" in
        *"'"*)
            morrer "o caminho do repositório no Windows tem uma aspa simples, e eu mando" \
                "o caminho dentro de uma string do PowerShell. Mova o repositório para um" \
                "caminho sem aspas."
            ;;
    esac

    passo "alcançando $WINDOWS por SSH"
    cw_resposta=$(no_windows "\$ErrorActionPreference = 'SilentlyContinue'
if (-not (Test-Path '$REPO_WINDOWS')) { Write-Output 'repositorio=ausente'; exit 0 }
Set-Location '$REPO_WINDOWS'
Write-Output 'repositorio=presente'
if (Test-Path 'empacotar\\windows.ps1') { Write-Output 'script=presente' } else { Write-Output 'script=ausente' }
if (Get-Command git) { Write-Output 'git=presente' } else { Write-Output 'git=ausente' }
if (Get-Command cargo) { Write-Output 'cargo=presente' } else { Write-Output 'cargo=ausente' }
Write-Output ('head=' + (git rev-parse HEAD))
# O arquivo que o windows.ps1 grava e devolve ao sair: uma rodada que morreu
# no meio o deixa editado, e essa é a sujeira que é nossa para desfazer.
if (git status --porcelain -- '$CONFIG_TAURI') { git checkout -- '$CONFIG_TAURI'; Write-Output 'restaurei=sim' } else { Write-Output 'restaurei=nao' }
# Depois de restaurar o que era nosso, o que sobrar é trabalho de quem está
# naquela máquina — e apagar trabalho de alguém sem perguntar é o único
# movimento daqui que não dá para desfazer.
if (git status --porcelain) { Write-Output 'sujo=sim' } else { Write-Output 'sujo=nao' }
\$restos = @(Get-ChildItem 'entrega' -File -ErrorAction SilentlyContinue | Where-Object { \$_.Name -notlike '*$VERSAO*' })
foreach (\$r in \$restos) { Write-Output ('apaguei=' + \$r.Name); Remove-Item \$r.FullName -Force }
Write-Output ('restos=' + \$restos.Count)")

    case "$cw_resposta" in
        *repositorio=*) ;;
        *)
            morrer "não consegui rodar nada em $WINDOWS." \
                "O que veio de lá:" \
                "$cw_resposta" \
                "" \
                "Os suspeitos, em ordem:" \
                "  1. o OpenSSH Server não está ligado lá — docs/windows.md, seção 7;" \
                "  2. a sua chave não está no authorized_keys **certo**: contas de" \
                "     administrador usam C:\\ProgramData\\ssh\\administrators_authorized_keys," \
                "     e não a pasta do usuário. É a pegadinha mais comum;" \
                "  3. o firewall de lá não deixa entrar na porta 22;" \
                "  4. o powershell.exe não está no PATH do serviço." \
                "" \
                "Confira à mão com:  ssh $WINDOWS powershell -Command \"echo ok\""
            ;;
    esac
    case "$cw_resposta" in
        *repositorio=ausente*)
            morrer "em $WINDOWS não há repositório em «${REPO_WINDOWS}»." \
                "Clone-o lá, ou aponte para onde ele está:" \
                "  --repo-windows 'D:\\caminho\\SEELE'"
            ;;
    esac
    case "$cw_resposta" in
        *script=ausente*)
            morrer "o repositório de lá não tem empacotar\\windows.ps1." \
                "Ele está velho demais: atualize-o antes."
            ;;
    esac
    case "$cw_resposta" in
        *git=ausente*)
            morrer "não há git no PATH de $WINDOWS." \
                "docs/windows.md, seção 1.4, instala-o."
            ;;
    esac
    case "$cw_resposta" in
        *cargo=ausente*)
            morrer "não há cargo no PATH de $WINDOWS." \
                "docs/windows.md, seção 1.2, instala o Rust com o alvo MSVC." \
                "Ele precisa estar no PATH **do serviço de SSH**, e não só no do seu" \
                "terminal: instale como o usuário que atende o SSH, ou reinicie a máquina" \
                "depois de instalar."
            ;;
    esac
    case "$cw_resposta" in
        *restaurei=sim*)
            passo "em $WINDOWS: $CONFIG_TAURI restaurado (resto de uma rodada interrompida)"
            ;;
    esac
    case "$cw_resposta" in
        *sujo=sim*)
            # Já restauramos o que era nosso, então isto é trabalho de quem está
            # naquela máquina.
            #
            # Antes isto matava o release, e o argumento escrito era: um
            # `reset --hard` apagaria sem volta, e um `stash` deixaria sedimento
            # que ninguém limpa. A primeira metade continua valendo e é por isso
            # que não há `reset` aqui. A segunda caiu na prática: parar custava
            # uma viagem até a outra máquina **depois** de o SSH já estar de pé,
            # e a árvore de lá chega suja quase sempre, porque o próprio
            # empacotamento regenera arquivos nela.
            #
            # E há um motivo que o argumento antigo não via: uma árvore suja no
            # commit certo **compila diferente do commit**. O release deixaria
            # de sair do código que a tag aponta, e isso é pior que o sedimento.
            #
            # O sedimento é tratado, e não ignorado: o stash leva o número do
            # release no nome, e a contagem da pilha é impressa. Uma pilha que
            # cresce aparece na tela em vez de crescer calada.
            #
            # Só o rastreado, e isso é o **padrão** de `git stash push`: sem
            # `-u` ele não toca em arquivo não rastreado. É o que se quer —
            # aquele arquivo não impede `checkout` nem muda o que compila, e
            # arrastá-lo tiraria da outra máquina coisa que ninguém pediu para
            # guardar.
            #
            # Houve aqui um `--untracked-files=no`, escrito por simetria com o
            # `git status` da linha de baixo. **Aquela bandeira não existe** em
            # `git stash push`, e o git respondia com o texto de uso — o stash
            # não acontecia e o `checkout` seguinte falhava. Encontrado rodando
            # de verdade contra a máquina Windows, e não por leitura.
            passo "há trabalho não commitado em $WINDOWS; guardando num stash"
            cw_guarda=$(no_windows "\$ErrorActionPreference = 'Stop'
Set-Location '$REPO_WINDOWS'
git stash push --quiet --message 'seele: antes do release $VERSAO'
Write-Output ('sobrou=' + (git status --porcelain --untracked-files=no).Length)
Write-Output ('pilha=' + (git stash list | Measure-Object).Count)")
            case "$cw_guarda" in
                *sobrou=0*|*sobrou=*[!0-9]*) ;;
                *)
                    morrer "não consegui guardar o trabalho solto em $WINDOWS." \
                        "Não vou compilar um release a partir de uma árvore que não é o commit." \
                        "Rode lá:  git -C '$REPO_WINDOWS' status" \
                        "" \
                        "O que veio de lá:" \
                        "$cw_guarda"
                    ;;
            esac
            printf '%s\n' "$cw_guarda" | sed -n 's/^pilha=/     stashes em '"$WINDOWS"' agora: /p' | tr -d '\r'
            printf '     recupere lá com:  git stash list  e  git stash pop\n'
            ;;
    esac
    printf '%s\n' "$cw_resposta" | sed -n 's/^apaguei=/     apagado em '"$WINDOWS"': /p' | tr -d '\r'
    case "$cw_resposta" in
        *restos=0*) ;;
        *restos=*)
            passo "entrega\\ de $WINDOWS limpo de outra versão"
            ;;
    esac

    cw_head=$(printf '%s\n' "$cw_resposta" | sed -n 's/^head=//p' | tr -d '\r')
    if [ "$cw_head" != "$COMMIT" ]; then
        # O Windows tem de estar no **mesmo commit**, e não na ponta do ramo: um
        # release cujos três pacotes vêm de códigos diferentes é três releases
        # com o mesmo número.
        #
        # É por isso que não é um `git pull` do outro lado. `pull` traz a ponta
        # do ramo remoto, que só coincide com este `HEAD` por sorte — e, se este
        # commit ainda não saiu daqui, ele **não existe no remoto** e nenhum
        # `pull` o alcança. Daí a ordem ser: garantir que ele está lá, depois
        # mandar buscar exatamente ele.
        if ! git -C "$RAIZ" ls-remote --exit-code origin "$COMMIT" >/dev/null 2>&1; then
            passo "empurrando $COMMIT para o origin (o Windows não alcança o que não saiu daqui)"
            if ! git -C "$RAIZ" push origin HEAD >/dev/null 2>&1; then
                morrer "não consegui empurrar $COMMIT para o origin." \
                    "O Windows busca o commit pelo remoto, e ele não está lá." \
                    "Rode:  git -C '$RAIZ' push origin HEAD"
            fi
        fi

        passo "levando $WINDOWS ao commit $COMMIT"
        # Sem stash aqui: a árvore já foi limpa acima, no `sujo=sim`, e um
        # segundo `stash` neste ponto nunca teria o que guardar. Código que não
        # dá para observar rodando é código que ninguém percebe quando quebra.
        cw_troca=$(no_windows "\$ErrorActionPreference = 'Stop'
Set-Location '$REPO_WINDOWS'
git fetch --all --quiet
git checkout --quiet $COMMIT
Write-Output ('head=' + (git rev-parse HEAD))")
        cw_head=$(printf '%s\n' "$cw_troca" | sed -n 's/^head=//p' | tr -d '\r')
        if [ "$cw_head" != "$COMMIT" ]; then
            morrer "não consegui levar $WINDOWS ao commit deste release." \
                "  aqui: $COMMIT" \
                "  lá:   ${cw_head:-desconhecido}" \
                "" \
                "O que veio de lá:" \
                "$cw_troca"
        fi
    fi
    passo "Windows pronto, no mesmo commit"
}

# ------------------------------------------------------------ o GitHub

# Uma chamada à API. Deixa o corpo em `$CORPO_API` e o código em `$CODIGO_HTTP`.
#
# O corpo vai para arquivo, e não para a saída padrão, por um motivo de shell e
# não de gosto: `x=$(chamar_api …)` roda a função numa subshell, e a atribuição
# de `$CODIGO_HTTP` lá dentro morre com ela. Quem lesse o corpo assim ficaria
# com o código da chamada **anterior** — e o erro seria «funcionou» quando não
# funcionou.
#
# O token entra por `--config -`, e não por `-H`: em `ps` a linha de comando do
# `curl` é pública, e um token nela é um token vazado para quem quer que
# compartilhe a máquina.
chamar_api() {
    ca_metodo="$1"
    ca_url="$2"
    ca_corpo="${3:-}"

    if [ -n "$ca_corpo" ]; then
        ca_bruto=$(printf 'header = "Authorization: Bearer %s"\n' "$TOKEN" | curl --config - \
            -sS -X "$ca_metodo" \
            -H 'Accept: application/vnd.github+json' \
            -H 'X-GitHub-Api-Version: 2022-11-28' \
            -H 'Content-Type: application/json' \
            --data-binary "@$ca_corpo" \
            -w '\n%{http_code}' "$ca_url" 2>&1)
    else
        ca_bruto=$(printf 'header = "Authorization: Bearer %s"\n' "$TOKEN" | curl --config - \
            -sS -X "$ca_metodo" \
            -H 'Accept: application/vnd.github+json' \
            -H 'X-GitHub-Api-Version: 2022-11-28' \
            -w '\n%{http_code}' "$ca_url" 2>&1)
    fi

    CODIGO_HTTP=$(printf '%s\n' "$ca_bruto" | tail -n 1)
    printf '%s\n' "$ca_bruto" | sed '$d' > "$CORPO_API"
}

conferir_github() {
    if [ -z "$TOKEN" ]; then
        morrer "não há token para falar com o GitHub." \
            "  export SEELE_GITHUB_TOKEN=…" \
            "" \
            "Um token fino («fine-grained») com **Contents: Read and write** neste" \
            "repositório basta; um clássico precisa do escopo «repo»." \
            "github.com/settings/tokens"
    fi

    passo "conferindo o token no GitHub"
    chamar_api GET "$API/user"
    if [ "$CODIGO_HTTP" != "200" ]; then
        morrer "o GitHub não aceitou o token (HTTP $CODIGO_HTTP)." \
            "$(head -c 500 "$CORPO_API")" \
            "" \
            "Se for «Bad credentials», ele expirou ou foi revogado."
    fi
    cg_quem=$(python3 -c 'import json, sys
print(json.load(open(sys.argv[1], encoding="utf-8")).get("login", "?"))' "$CORPO_API" 2>/dev/null)

    chamar_api GET "$API/repos/$REPO"
    if [ "$CODIGO_HTTP" != "200" ]; then
        morrer "não consegui ler $REPO (HTTP $CODIGO_HTTP)." \
            "$(head -c 500 "$CORPO_API")"
    fi
    if ! python3 -c 'import json, sys
dados = json.load(open(sys.argv[1], encoding="utf-8"))
sys.exit(0 if dados.get("permissions", {}).get("push") else 1)' "$CORPO_API" 2>/dev/null; then
        morrer "o token de «${cg_quem}» enxerga $REPO mas não pode escrever nele." \
            "Criar release é escrita. Dê-lhe **Contents: Read and write**."
    fi
    passo "token de $cg_quem, com escrita em $REPO"

    # O commit tem de existir lá.
    #
    # O rascunho nasce apontando para ele, e é ele que vira a tag na hora de
    # publicar. Um rascunho apontando para um commit que o GitHub não conhece é
    # um release que ninguém consegue rastrear.
    chamar_api GET "$API/repos/$REPO/commits/$COMMIT"
    if [ "$CODIGO_HTTP" != "200" ]; then
        morrer "o commit $COMMIT não está no GitHub (HTTP $CODIGO_HTTP)." \
            "É dele que este release sairia, e é ele que vira a tag ao publicar." \
            "Empurre-o antes:  git push"
    fi
    passo "o commit está no GitHub"

    # Rascunho **não** cria tag, então `releases/tags/<tag>` não o encontra: é
    # preciso listar. É a única coisa que este script faz diferente do
    # `release.yml`, onde o `gh release view` esconde essa distinção.
    chamar_api GET "$API/repos/$REPO/releases?per_page=100"
    if [ "$CODIGO_HTTP" != "200" ]; then
        morrer "não consegui listar os releases de $REPO (HTTP $CODIGO_HTTP)." \
            "$(head -c 500 "$CORPO_API")"
    fi
    cg_achado=$(python3 -c 'import json, sys
alvo = sys.argv[2]
for release in json.load(open(sys.argv[1], encoding="utf-8")):
    if release.get("tag_name") == alvo:
        print(release["id"])
        print("rascunho" if release.get("draft") else "publicado")
        print(release.get("html_url", ""))
        break' "$CORPO_API" "v$VERSAO" 2>/dev/null)

    RELEASE_ID=$(printf '%s\n' "$cg_achado" | sed -n 1p)
    RELEASE_ESTADO=$(printf '%s\n' "$cg_achado" | sed -n 2p)
    RELEASE_URL=$(printf '%s\n' "$cg_achado" | sed -n 3p)

    if [ "$RELEASE_ESTADO" = publicado ]; then
        # A mesma regra do release.yml, e a mesma razão: rascunho se substitui
        # sem cerimônia, publicado é decisão que uma pessoa tomou.
        morrer "já existe um release **publicado** em v$VERSAO, e eu não o apago." \
            "  $RELEASE_URL" \
            "" \
            "Publique noutra versão, ou remova-o à mão se a intenção é mesmo substituí-lo."
    fi
    if [ "$RELEASE_ESTADO" = rascunho ]; then
        aviso "já há um rascunho em v$VERSAO; ele será substituído no fim."
    fi
    passo "v$VERSAO livre no Releases"
}

# =========================================================================

printf -- '--- conferindo antes de compilar ---\n'
conferir_versao
conferir_ferramentas
conferir_arvore
conferir_config_tauri
limpar_entrega
conferir_chave
conferir_docker
conferir_windows
conferir_github
printf -- '--- tudo conferido ---\n'

if [ "$SO_CONFERIR" = sim ]; then
    printf '\n'
    printf '%s\n' "Nada foi compilado — era --conferir." \
        "Para valer, e reservando o resto da tarde:" \
        "  $0 $VERSAO"
    exit 0
fi

# =========================================================================
# Os builds.
# =========================================================================

COMECOU_BUILD=sim
FALHAS=""

empacotar_macos() {
    passo "macOS: empacotando aqui (nativo)"
    "$RAIZ/empacotar/macos.sh" "$VERSAO"
}

empacotar_linux() {
    passo "Linux: empacotando em Docker x86_64 (emulado; 30 a 90 minutos)"
    "$RAIZ/empacotar/linux.sh" "$VERSAO"
}

empacotar_windows() {
    TOCOU_WINDOWS=sim
    passo "Windows: empacotando em $WINDOWS"

    # A chave atravessa aqui, e só aqui: pela entrada padrão, dentro do canal
    # cifrado, em base64 para não haver quebra de linha a interpretar. Ela não
    # toca no disco de lá e não aparece em `argv` nenhum.
    ew_envelope=""
    if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
        ew_envelope="chave=$(em_base64 "$TAURI_SIGNING_PRIVATE_KEY")"
    fi
    if [ -n "${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}" ]; then
        ew_envelope="$ew_envelope
senha=$(em_base64 "$TAURI_SIGNING_PRIVATE_KEY_PASSWORD")"
    fi

    # `[char]10` e não a crase de escape do PowerShell: este texto está dentro
    # de aspas duplas de shell, onde crase abre substituição de comando.
    ew_condutor="\$ErrorActionPreference = 'Stop'
foreach (\$linha in ([Console]::In.ReadToEnd() -split ([char]10))) {
    \$linha = \$linha.Trim()
    if (-not \$linha) { continue }
    \$par = \$linha.Split('=', 2)
    \$valor = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String(\$par[1]))
    if (\$par[0] -eq 'chave') { \$env:TAURI_SIGNING_PRIVATE_KEY = \$valor }
    if (\$par[0] -eq 'senha') { \$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = \$valor }
}
Set-Location '$REPO_WINDOWS'
& .\\empacotar\\windows.ps1 -Versao '$VERSAO'
\$codigo = \$LASTEXITCODE
if (\$null -ne \$codigo -and \$codigo -ne 0) { exit \$codigo }
exit 0"

    ew_codificado=$(codificar_ps "$ew_condutor")
    if [ -z "$ew_codificado" ]; then
        aviso "não consegui codificar o comando para o Windows."
        return 1
    fi
    printf '%s\n' "$ew_envelope" | ssh -o BatchMode=yes "$WINDOWS" \
        powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass \
        -EncodedCommand "$ew_codificado" || return 1

    # Trazer os arquivos de volta.
    #
    # Zip pela **saída padrão em base64**, e não `scp`: com o `cmd` de shell
    # padrão não há expansão de `entrega\*` do outro lado, e a versão do `scp`
    # que o Windows traz varia entre a que fala SFTP e a que não fala. Isto aqui
    # depende só de `ssh` e `powershell`, que é o mínimo que já precisamos ter.
    # Custa um terço a mais de bytes numa rede local, o que é nada.
    #
    # Só o que casa com esta versão: o `entrega\` de lá é do dono da máquina, e
    # arrastar tudo traria o instalador da versão passada para dentro deste
    # release.
    passo "Windows: trazendo o que saiu de lá"
    no_windows_dado "\$ErrorActionPreference = 'Stop'
Set-Location '$REPO_WINDOWS'
\$saida = Join-Path \$env:TEMP 'seele-entrega.zip'
if (Test-Path \$saida) { Remove-Item \$saida -Force }
\$arquivos = @(Get-ChildItem 'entrega' -File | Where-Object { \$_.Name -like '*$VERSAO*' })
if (\$arquivos.Count -eq 0) { throw 'nada em entrega com esta versao' }
Compress-Archive -Path \$arquivos.FullName -DestinationPath \$saida -Force
[Console]::Out.Write([Convert]::ToBase64String([IO.File]::ReadAllBytes(\$saida)))
Remove-Item \$saida -Force" > "$TEMPORARIO/entrega.b64" || return 1

    if ! python3 -c 'import base64, sys
open(sys.argv[2], "wb").write(base64.b64decode(open(sys.argv[1], encoding="ascii").read().strip()))' \
        "$TEMPORARIO/entrega.b64" "$TEMPORARIO/entrega.zip" 2>/dev/null; then
        aviso "o que veio do Windows não é base64 — a saída de lá veio misturada."
        return 1
    fi

    mkdir -p "$RAIZ/entrega"
    if ! python3 -m zipfile -e "$TEMPORARIO/entrega.zip" "$RAIZ/entrega"; then
        aviso "o zip que veio do Windows não abriu."
        return 1
    fi

    # Chegou mesmo o que interessa? O resto do script trata a ausência do
    # instalador como «o Windows não entrou», e é melhor saber agora.
    if [ -z "$(find "$RAIZ/entrega" -type f -name "*_${VERSAO}_*-setup.exe" 2>/dev/null)" ]; then
        aviso "veio arquivo do Windows, mas nenhum instalador de $VERSAO."
        return 1
    fi
    passo "Windows: instalador em entrega/"
}

for sistema in $SISTEMAS; do
    pedido "$sistema" || continue

    printf '\n=== %s ===\n' "$sistema"
    resultado=0
    case "$sistema" in
        macos) empacotar_macos || resultado=1 ;;
        windows) empacotar_windows || resultado=1 ;;
        linux) empacotar_linux || resultado=1 ;;
    esac

    if [ "$resultado" -ne 0 ]; then
        FALHAS="$FALHAS $sistema"
        aviso "$sistema falhou. Sigo com os outros — o que já saiu fica em entrega/."
    fi
done
FALHAS="${FALHAS# }"

# =========================================================================

printf '\n--- o que fazer com o que saiu ---\n'
VEREDITO=$(decidir "$PEDIDOS" "$FALHAS" "$PARCIAL")
ESTADO="$?"
printf '%s\n' "$VEREDITO" | sed 1d

if [ "$ESTADO" -ne 0 ]; then
    # Sair por aqui não é sair mal: entrega/ está de pé, a árvore vai ser
    # devolvida ao que era pela faxina, e nada foi ao GitHub.
    exit 1
fi

if [ -z "$(find "$RAIZ/entrega" -type f 2>/dev/null)" ]; then
    morrer "entrega/ está vazia; não há o que publicar."
fi

# =========================================================================
# A publicação.
# =========================================================================

printf '\n--- publicando ---\n'

passo "montando o latest.json"
if ! python3 "$RAIZ/empacotar/manifesto.py" "$RAIZ/entrega" "v$VERSAO" --repo "$REPO"; then
    morrer "o manifesto não saiu." \
        "Sem ele o botão de atualizar do app não acha este release."
fi

passo "somando os arquivos"
rm -f "$RAIZ/entrega/SHA256SUMS"
if ! (cd "$RAIZ/entrega" && shasum -a 256 * > SHA256SUMS); then
    morrer "não consegui somar os arquivos de entrega/."
fi

# O corpo do release: as notas de sempre, mais a verdade sobre estes pacotes.
#
# No corpo, e não só na tela deste script, porque quem baixa não vê esta tela.
# As NOTAS-DE-RELEASE ensinam a conferir a procedência com
# `gh attestation verify`; para um release montado à mão esse comando **não
# encontra atestado**, e uma pessoa cuidadosa que o rode sem este aviso conclui
# adulteração onde só houve ausência de CI.
NOTAS="$TEMPORARIO/corpo.md"

# A faixa desta versão, e as mudanças dentro dela.
#
# Antes disto a página abria com 259 linhas explicando o que é o `plug` e onde
# ele mora — texto que não muda de uma versão para a outra e que já está no
# README. Quem chegava na página de uma versão nova para saber **o que mudou**
# não achava em lugar nenhum, e o histórico do repositório, que é onde a
# resposta sempre esteve, não é o lugar onde alguém procura depois de clicar num
# link de download.
ANTERIOR=$(git -C "$RAIZ" tag --list 'v*' | tag_anterior "$VERSAO")
if [ -n "$ANTERIOR" ]; then
    FAIXA="$ANTERIOR..HEAD"
else
    # A primeira publicação. Dizer isso é melhor que listar o histórico inteiro
    # como se tudo fosse novidade desta versão.
    FAIXA=""
fi

{
    if [ -n "$FAIXA" ]; then
        git -C "$RAIZ" log --no-merges --format='%s' "$FAIXA" | notas_das_mudancas
    else
        printf '%s\n' \
"_Primeira versão publicada: não há uma anterior contra a qual comparar. O" \
"histórico do repositório tem tudo o que veio antes._"
        printf '\n'
    fi
    if [ -f "$RAIZ/.github/NOTAS-DE-RELEASE.md" ]; then
        printf '\n---\n\n'
        cat "$RAIZ/.github/NOTAS-DE-RELEASE.md"
    fi
    printf '\n---\n\n'
    printf '%s\n' \
"## Como esta versão foi montada" \
"" \
"**Fora da integração contínua.** Os pacotes desta página foram construídos nas" \
"máquinas de quem publica — macOS e Linux num Mac, Windows numa máquina Windows" \
"alcançada por SSH — a partir do commit \`$COMMIT\`." \
"" \
"A consequência, e ela muda o que conferir: **não há atestado de procedência do" \
"GitHub para esta versão.** \`gh attestation verify\` vai responder que não" \
"encontrou atestado para estes arquivos, e isso é o esperado aqui — não é sinal" \
"de adulteração. O atestado é assinado pela infraestrutura que compila, e uma" \
"mesa não é essa infraestrutura." \
"" \
"O que continua valendo:" \
"" \
"- o \`SHA256SUMS\` responde **«o arquivo chegou inteiro»**;" \
"- os \`.sig\`, quando estiverem aqui, respondem **«veio de quem tem a chave do" \
"  projeto»** — e é essa a assinatura que o próprio SEELE confere antes de" \
"  instalar uma atualização." \
"" \
"O que ninguém consegue responder sobre esta versão é **«veio daquele código»**." \
"Versões saídas do workflow respondem, e a seção acima explica como."
} > "$NOTAS"

passo "criando o rascunho de v$VERSAO"
PEDIDO="$TEMPORARIO/pedido.json"
if ! python3 -c 'import json, sys
tag, commit, notas, destino = sys.argv[1:5]
with open(destino, "w", encoding="utf-8") as saida:
    json.dump({
        "tag_name": tag,
        "name": "SEELE " + tag,
        "target_commitish": commit,
        "draft": True,
        "prerelease": False,
        "body": open(notas, encoding="utf-8").read(),
    }, saida)' "v$VERSAO" "$COMMIT" "$NOTAS" "$PEDIDO"; then
    morrer "não consegui montar o pedido de criação do release."
fi

# O rascunho anterior desta tag sai antes, para não acumular duplicata. Só o
# rascunho: o publicado já foi barrado lá atrás, nas conferências.
if [ "$RELEASE_ESTADO" = rascunho ] && [ -n "$RELEASE_ID" ]; then
    passo "apagando o rascunho anterior de v$VERSAO"
    chamar_api DELETE "$API/repos/$REPO/releases/$RELEASE_ID"
    if [ "$CODIGO_HTTP" != "204" ]; then
        aviso "não consegui apagar o rascunho anterior (HTTP $CODIGO_HTTP); sigo."
    fi
fi

chamar_api POST "$API/repos/$REPO/releases" "$PEDIDO"
if [ "$CODIGO_HTTP" != "201" ]; then
    morrer "o GitHub recusou criar o release (HTTP $CODIGO_HTTP)." \
        "$(head -c 500 "$CORPO_API")" \
        "" \
        "Os arquivos continuam em entrega/. Para tentar só a publicação de novo:" \
        "  $0 $VERSAO --pular $(printf '%s' "$SISTEMAS" | LC_ALL=C tr ' ' ',')"
fi
NOVO_ID=$(python3 -c 'import json, sys
print(json.load(open(sys.argv[1], encoding="utf-8"))["id"])' "$CORPO_API")
NOVA_URL=$(python3 -c 'import json, sys
print(json.load(open(sys.argv[1], encoding="utf-8")).get("html_url", ""))' "$CORPO_API")

for arquivo in "$RAIZ"/entrega/*; do
    [ -f "$arquivo" ] || continue
    nome=$(basename "$arquivo")
    # Nome que precise de escape na URL é nome que eu não sei subir sem
    # corrompê-lo, e os empacotadores nunca produziram um.
    case "$nome" in
        *[!A-Za-z0-9._-]*)
            morrer "o arquivo «${nome}» tem caractere que eu não sei pôr numa URL." \
                "Renomeie-o antes de publicar."
            ;;
    esac

    passo "subindo $nome"
    envio_bruto=$(printf 'header = "Authorization: Bearer %s"\n' "$TOKEN" | curl --config - \
        -sS -X POST \
        -H 'Accept: application/vnd.github+json' \
        -H 'Content-Type: application/octet-stream' \
        --data-binary "@$arquivo" \
        -w '\n%{http_code}' \
        "$ENVIO/repos/$REPO/releases/$NOVO_ID/assets?name=$nome" 2>&1)
    envio_codigo=$(printf '%s\n' "$envio_bruto" | tail -n 1)
    if [ "$envio_codigo" != "201" ]; then
        morrer "o envio de «${nome}» falhou (HTTP $envio_codigo)." \
            "$(printf '%s\n' "$envio_bruto" | sed '$d' | head -c 500)" \
            "" \
            "O rascunho ficou pela metade em:" \
            "  $NOVA_URL" \
            "Apague-o lá e rode de novo — os arquivos continuam em entrega/."
    fi
done

# =========================================================================
# O que ficou faltando, dito por extenso.
# =========================================================================

printf '\n--- pronto, e ainda não está publicado ---\n\n'
printf '%s\n' "O rascunho está em:" "  $NOVA_URL" ""
printf '%s\n' \
"**Ele é rascunho, e rascunho ninguém enxerga.** O endereço gravado dentro de" \
"cada SEELE instalado é releases/latest/download/latest.json, e «latest» é o" \
"último release **publicado**. Enquanto este não for publicado à mão, nenhum" \
"app do mundo vê a versão nova — é de propósito, e é o que mantém a decisão de" \
"lançar com uma pessoa." \
"" \
"Antes de apertar o botão:" \
"  1. baixe pelo menos um dos arquivos e abra-o;" \
"  2. confira as somas contra o SHA256SUMS desta página;" \
"  3. leia o corpo do release até o fim — a última seção dele é sobre esta" \
"     versão em particular." \
"" \
"O que este release **perdeu** por não ter vindo do CI: atestado de procedência." \
"O release.yml assina, com o GitHub, um documento que amarra cada arquivo ao" \
"commit e à execução que o produziu; nada montado numa mesa consegue isso. Quem" \
"rodar «gh attestation verify» nestes arquivos vai receber que não há atestado," \
"e isso é o esperado, não adulteração. O SHA256SUMS continua respondendo «o" \
"arquivo chegou inteiro» e os .sig continuam respondendo «veio de quem tem a" \
"chave do projeto». «Veio daquele código» é a pergunta que fica sem resposta, e" \
"ela está escrita no corpo do release para quem baixar não descobrir sozinho."

if [ -n "$FALHAS" ]; then
    printf '\n'
    aviso "faltam sistemas neste release: $FALHAS."
    aviso "Quem os usa não recebe esta atualização, e não é avisado do porquê."
fi

if [ -n "$(find "$RAIZ/entrega" -type f -name '*.app.tar.gz' 2>/dev/null)" ]; then
    printf '\n'
    printf '%s\n' \
"O .dmg desta entrega saiu com a arquitetura **desta** máquina, e só ela — é o" \
"que empacotar/macos.sh faz, e está escrito no cabeçalho dele. O latest.json" \
"oferece o pacote do Mac só ao alvo que casa; Macs da outra arquitetura não" \
"recebem esta versão. Para os dois, o caminho é o workflow."
fi

exit 0
