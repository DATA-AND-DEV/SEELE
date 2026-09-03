# Empacota o instalador do Windows sem o GitHub Actions.
#
# Faz o mesmo que o job `windows` de `.github/workflows/release.yml`, na ordem
# dele e pelas mesmas razões. Existe porque o Tauri não cross-compila: o
# empacotador NSIS e o WebView2 precisam de Windows de verdade, e nenhuma
# máquina macOS ou Linux produz este arquivo.
#
#   .\empacotar\windows.ps1 -Versao 0.1.2
#
# Pré-requisitos, em `docs/windows.md` seção 1: Build Tools do Visual Studio com
# C++, Rust (MSVC) e Git. O NSIS o próprio Tauri baixa na primeira vez.
#
# Ao final, `entrega\` tem o instalador e o zip da CLI, com as somas SHA-256
# impressas para conferir contra o que for publicado.
#
# Para que o instalador saia atualizável, defina a chave do projeto antes:
#
#   $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content ~\.tauri\seele.key -Raw
#   $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '…'
#
# Sem ela sai o `.exe` de sempre, para baixar à mão.
# `docs/assinatura-e-atualizacao.md` diz de onde a chave vem.

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Versao
)

$ErrorActionPreference = "Stop"

# A raiz do repositório, a partir deste arquivo — assim o script roda de
# qualquer diretório.
$Raiz = Split-Path -Parent $PSScriptRoot
Set-Location $Raiz

# ---------------------------------------------------------------- a versão
#
# A mesma regra do workflow, e pelo mesmo motivo: o empacotador de MSI recusa o
# que o formato do instalador não sabe representar, e recusa **depois** de
# compilar. Conferir aqui custa nada. O NSIS é mais tolerante que o MSI, mas
# manter as duas regras iguais evita que um release saia com um número que o
# outro formato não aceitaria depois.
if ($Versao -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(-[0-9]+)?$') {
    Write-Error @"
a versão «$Versao» não serve para o instalador.
Aceito: X.Y.Z, ou X.Y.Z-N com N só de dígitos.
Não serve: -dev, -rc1, -beta.2, +metadados, ou o «v» na frente.
"@
}

# ------------------------------------------------- o linker existe mesmo?
#
# O Rust no Windows usa o linker da Microsoft, e sem ele a compilação morre —
# mas morre tarde, depois de baixar e compilar dezenas de crates. Numa máquina
# recém-preparada isso é um quarto de hora perdido para descobrir uma coisa que
# se sabe em um segundo.
#
# `link.exe` **não** é procurado no PATH de propósito: ele só está lá dentro de
# um Developer Command Prompt, e o Rust o encontra pelo registro. Quem sabe
# responder é o `vswhere`, que acompanha qualquer instalador moderno do Visual
# Studio.
$VsWhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $VsWhere) {
    $comCpp = & $VsWhere -products * -latest -property installationPath `
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 2>$null
    if (-not $comCpp) {
        Write-Error @"
o linker do MSVC não está instalado, e sem ele nada compila.

Baixe **Build Tools for Visual Studio** em
  https://visualstudio.microsoft.com/downloads/   (seção «Tools for Visual Studio»)

e no instalador marque:
  - Desenvolvimento para desktop com C++
  - dentro dele, confira que o Windows 10/11 SDK está marcado

São cerca de 3 GB. `docs/windows.md` seção 1.1 tem o passo a passo.
O VS Code não serve — é outro produto.
"@
    }
    Write-Host "→ linker do MSVC: ok" -ForegroundColor DarkGray
}
else {
    # Sem o `vswhere` não dá para afirmar nem que está nem que não está, e
    # barrar por não saber seria pior que deixar seguir.
    Write-Warning "não achei o vswhere; seguindo sem conferir o linker do MSVC."
}

# ------------------------------------------------- e o libclang, existe?
#
# A crate do Opus gera suas ligações com `bindgen`, que carrega `libclang.dll`
# em tempo de execução. O `llvm-tools` que o `rustup` instala **não** serve: ele
# traz as ferramentas tipo binutils que renomeiam símbolos, e não a biblioteca
# que o bindgen usa para ler cabeçalhos C.
#
# Isto não está na lista de pré-requisitos histórica porque o runner do GitHub
# já vem com LLVM — a mesma razão pela qual o contêiner do Linux também quebrou
# aqui. Quem tem uma máquina limpa não tem.
#
# A falha, quando vem, vem tarde: depois de baixar 52 MB de Opus e compilá-lo
# inteiro com o MSBuild, que é onde mais dói.
if (-not $env:LIBCLANG_PATH -or -not (Test-Path (Join-Path $env:LIBCLANG_PATH "libclang.dll"))) {
    $candidatos = @(
        "$env:ProgramFiles\LLVM\bin",
        "${env:ProgramFiles(x86)}\LLVM\bin"
    ) + @(
        # O componente «C++ Clang tools for Windows» do Build Tools põe o LLVM
        # dentro da instalação do Visual Studio, e não em Program Files.
        if ($comCpp) { Join-Path $comCpp "VC\Tools\Llvm\x64\bin" }
    )

    $achado = $candidatos | Where-Object { $_ -and (Test-Path (Join-Path $_ "libclang.dll")) } | Select-Object -First 1

    if ($achado) {
        # Definido só para este processo: mexer no ambiente da máquina é decisão
        # de quem usa a máquina, não de um script de empacotamento.
        $env:LIBCLANG_PATH = $achado
        Write-Host "→ libclang: $achado" -ForegroundColor DarkGray
    }
    else {
        Write-Error @"
o libclang não está instalado, e o codec Opus não compila sem ele.

  winget install LLVM.LLVM

ou baixe em https://releases.llvm.org/ . Depois abra um terminal novo e rode
este script de novo — ele acha o LLVM sozinho em C:\Program Files\LLVM.

Se instalou noutro lugar, aponte antes de rodar:
  `$env:LIBCLANG_PATH = 'D:\caminho\para\LLVM\bin'

Cuidado com a confusão comum: o ``llvm-tools`` que o rustup instala **não**
serve. Ele traz as ferramentas que renomeiam símbolos, não a biblioteca que o
bindgen carrega para ler cabeçalhos C. `docs/windows.md` seção 1.4.
"@
    }
}
else {
    Write-Host "→ libclang: $env:LIBCLANG_PATH" -ForegroundColor DarkGray
}

# --------------------------------------------------- escrever JSON sem BOM
#
# `Set-Content -Encoding UTF8` no Windows PowerShell 5.1 grava UTF-8 **com**
# BOM, e o Tauri recusa o `tauri.conf.json` assim: o parser tropeça no primeiro
# byte e diz "expected value at line 1 column 1", que não parece ter nada a ver.
#
# Este script precisa das duas coisas opostas ao mesmo tempo: o `.ps1` **tem**
# que ter BOM, ou o 5.1 lê os acentos como ANSI e imprime «compilaÃ§Ã£o»; o
# `.json` **não** pode ter. Usar o mesmo mecanismo para os dois é o erro que
# estava aqui.
#
# O caminho é absoluto de propósito: `[System.IO.File]` resolve relativo contra
# o diretório do processo .NET, que o `Set-Location` do PowerShell **não**
# altera. Com caminho relativo isto escreveria fora do repositório.
function Write-Utf8SemBom([string]$Caminho, [string]$Texto) {
    $semBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Caminho, $Texto, $semBom)
}

# ------------------------------------------------------ ler JSON como UTF-8
#
# E a leitura tinha o mesmo defeito da escrita, na direção contrária.
#
# `Get-Content -Raw` **sem `-Encoding`** no Windows PowerShell 5.1 lê o arquivo
# na página de código ANSI do sistema — cp1252 numa máquina brasileira. O
# `tauri.conf.json` é UTF-8 sem BOM, e sem BOM não há o que o 5.1 detecte: ele
# supõe ANSI e pronto.
#
# O estrago é silencioso e sobrevive ao empacotamento. O título da janela era
# `SEELE · Entry Plug`; o `·` é `C2 B7` em UTF-8, lido como cp1252 vira os dois
# caracteres `Â` e `·`, e a escrita — que está correta — grava esse par como
# UTF-8 de verdade. O arquivo passou a conter `SEELE Â· Entry Plug`, e foi assim
# que ele apareceu na barra de título de um Windows de verdade.
#
# O título é só `SEELE` desde o ADR 0035, sem nenhum byte fora do ASCII, então
# aquele caminho exato não se reproduz hoje. **A leitura continua tendo de ser
# UTF-8 explícita**: o defeito não é do `·`, é do 5.1 supondo ANSI num arquivo
# sem BOM, e o próximo acento que entrar no `tauri.conf.json` o traz de volta.
#
# O comentário logo acima já dizia que «o 5.1 lê os acentos como ANSI», sobre o
# próprio `.ps1`. O mesmo raciocínio valia uma linha abaixo e não foi aplicado.
#
# `[System.IO.File]::ReadAllText` decodifica UTF-8 e retira o BOM se houver, que
# é o que se quer nos dois casos. O caminho é absoluto pelo mesmo motivo escrito
# acima: o .NET não enxerga o `Set-Location` do PowerShell.
function Read-Utf8([string]$Caminho) {
    [System.IO.File]::ReadAllText($Caminho)
}

$Config = Join-Path $Raiz "apps\seele-app\tauri.conf.json"
$Original = Read-Utf8 $Config

try {
    # ------------------------------------------------------- gravar a versão
    #
    # É ela que aparece no instalador e em "Aplicativos e recursos". Sem isto o
    # arquivo baixado diria 0.0.0, que é a versão do workspace.
    Write-Host "→ gravando a versão $Versao" -ForegroundColor Cyan
    $json = $Original | ConvertFrom-Json
    $json.version = $Versao

    # ------------------------------------------- o pacote de atualização
    #
    # No Windows o `.exe` do NSIS **é** o pacote de atualização: o mesmo arquivo
    # que uma pessoa baixaria à mão. O que falta é a assinatura ao lado dele, e
    # ela só sai com as duas metades da chave do projeto — a pública, que vive
    # no repositório em `plugins.updater.pubkey`, e a privada, que vem do
    # ambiente. Uma só das duas quebra o empacotamento no último passo.
    $publica = ""
    if ($json.plugins -and $json.plugins.updater) { $publica = "$($json.plugins.updater.pubkey)".Trim() }
    $privada = "$env:TAURI_SIGNING_PRIVATE_KEY".Trim()
    if ($privada -and $publica) {
        # `Add-Member -Force` porque a chave pode não existir no arquivo: o
        # `ConvertFrom-Json` do 5.1 devolve um objeto fixo, e atribuir a uma
        # propriedade ausente falha em vez de criá-la.
        $json.bundle | Add-Member -NotePropertyName createUpdaterArtifacts -NotePropertyValue $true -Force
        Write-Host "→ com pacote de atualização" -ForegroundColor Cyan
    }
    elseif ($privada -or $publica) {
        # Qual metade falta, e o que fazer — não só que falta uma. A mensagem
        # antiga parava em «só metade», que é ter a informação e não entregá-la.
        if ($privada) {
            Write-Warning "a chave privada está no ambiente e a pública não está no tauri.conf.json (plugins.updater.pubkey)."
        }
        else {
            Write-Warning "a chave pública está no tauri.conf.json e a privada não está no ambiente. Para assinar:"
            # `-Encoding UTF8` na instrução, e não por causa do guarda: sem ele, uma
        # chave salva com BOM traz o BOM para dentro da string e o Tauri
        # recusa a assinatura. É a mesma família do `%` do zsh que já entrou
        # numa chave pública neste projeto.
        Write-Warning '  $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content ~\.tauri\seele.key -Raw -Encoding UTF8'
            Write-Warning '  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "…"'
        }
        Write-Warning "Este instalador funciona normal; ele só não serve de origem para atualização."
    }

    Write-Utf8SemBom $Config (($json | ConvertTo-Json -Depth 100) + "`n")

    # ------------------------------------------------------------------ CLI
    #
    # Antes do app, como no workflow: `seeled` e `plug` são o produto principal
    # (`specs/00-visao-geral.md`) e o que menos pode falhar. Eles entram
    # **dentro** do instalador, para que quem baixa o app ganhe o servidor
    # junto e não precise de um segundo arquivo para hospedar.
    Write-Host "→ compilando seeled (a primeira vez demora)" -ForegroundColor Cyan
    # A versão vai **para dentro** do binário, e não pelo `Cargo.toml`.
    #
    # A versão do workspace é `0.0.0` de propósito, e quem carimba o número de
    # verdade é este script — no `tauri.conf.json`, que é do app. O `seeled` não
    # passa por ali, então sem esta variável ele responderia
    # `local (sem versão carimbada)` num pacote de release. Ver a pendência 30 e
    # `seele_server::main::versao`; o `empacotar/macos.sh` e o workflow fazem o
    # mesmo, cada um na sintaxe da sua concha.
    $env:SEELE_VERSAO = $Versao
    cargo build --release --bin seeled
    if ($LASTEXITCODE -ne 0) { throw "a compilação da CLI falhou" }
    # Limpa a variável para o `cargo build` do app, logo abaixo, não a herdar.
    # Ele não a lê hoje, e uma variável de ambiente que sobra é a que aparece
    # num build futuro sem que ninguém a tenha posto ali.
    Remove-Item Env:\SEELE_VERSAO -ErrorAction SilentlyContinue

    # O Tauri procura acompanhantes pelo nome com o alvo no fim.
    $Binarios = "apps\seele-app\binaries"
    New-Item -ItemType Directory -Force -Path $Binarios | Out-Null
    foreach ($b in @("seeled")) {
        Copy-Item "target\release\$b.exe" "$Binarios\$b-x86_64-pc-windows-msvc.exe" -Force
    }

    # ------------------------------------------------------------------ app
    if (-not (Get-Command cargo-tauri -ErrorAction SilentlyContinue)) {
        Write-Host "→ instalando a CLI do Tauri" -ForegroundColor Cyan
        cargo install tauri-cli --version "^2" --locked
        if ($LASTEXITCODE -ne 0) { throw "não instalei a CLI do Tauri" }
    }

    # `.exe` do NSIS e não `.msi`: instala no perfil sem pedir administrador.
    # O `--config` é o que põe os acompanhantes dentro do bundle.
    Write-Host "→ empacotando o instalador" -ForegroundColor Cyan
    Push-Location "apps\seele-app"
    try {
        cargo tauri build --config tauri.release.conf.json --bundles nsis
        if ($LASTEXITCODE -ne 0) { throw "o empacotamento falhou" }
    }
    finally { Pop-Location }

    # ------------------------------------------------- o instalador que é nosso
    #
    # Ver ADR 0043. Ele convive com o do NSIS enquanto não estiver completo: o
    # NSIS continua sendo o que a versão publica, e este sai ao lado para ser
    # olhado. O dia em que trocarem de lugar está no ADR, e é depois de o modo
    # silencioso ter teste próprio.
    Write-Host "→ montando a carga do instalador próprio" -ForegroundColor Cyan
    $Carga = Join-Path $env:TEMP "seele-carga-$Versao"
    Remove-Item -Recurse -Force $Carga -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force -Path $Carga | Out-Null

    # O que o produto instalado precisa ter ao lado. É a mesma lista que o
    # bundle do Tauri leva: o app e o `seeled`, que é o sidecar. O sidecar perde
    # o sufixo do alvo — dentro da pasta instalada ele se chama `seeled.exe`,
    # que é como o README manda chamá-lo no PATH.
    Copy-Item "target\release\SEELE.exe" (Join-Path $Carga "SEELE.exe") -Force
    Copy-Item "target\release\seeled.exe" (Join-Path $Carga "seeled.exe") -Force

    $CargaTar = Join-Path $env:TEMP "seele-carga-$Versao.tar.gz"
    Remove-Item -Force $CargaTar -ErrorAction SilentlyContinue
    # O `tar` do Windows 10 em diante é o do libarchive e faz `.tar.gz` sozinho.
    tar -czf $CargaTar -C $Carga .
    if ($LASTEXITCODE -ne 0) { throw "não montei a carga do instalador" }

    Write-Host "→ compilando o instalador próprio" -ForegroundColor Cyan
    $env:SEELE_CARGA = $CargaTar
    try {
        cargo build --release -p seele-instalador
        if ($LASTEXITCODE -ne 0) { throw "o instalador próprio não compilou" }
    }
    finally { Remove-Item Env:SEELE_CARGA -ErrorAction SilentlyContinue }

    # -------------------------------------------------------------- reunir
    $Destino = "entrega"
    New-Item -ItemType Directory -Force -Path $Destino | Out-Null

    # Filtrado pela versão desta execução, e não «qualquer coisa que termine em
    # -setup.exe».
    #
    # O `target\bundle` acumula: empacotar 0.4.2 depois de 0.4.0 deixa os dois
    # lado a lado, e a conferência de «exatamente um» reprovava com os dois
    # presentes. Ela estava certa em recusar ambiguidade e errada em não saber
    # desfazê-la — a versão está no nome do arquivo, e é a resposta.
    #
    # Apagar o diretório antes seria a outra saída, e é pior: o instalador
    # anterior é de alguém, e um script de empacotamento que apaga entrega
    # passada é um que apaga a entrega que você ainda não subiu.
    $padrao = "*_${Versao}_*-setup.exe"
    $instaladores = @(Get-ChildItem -Path "target\release\bundle" -Recurse -Filter $padrao)
    if ($instaladores.Count -ne 1) {
        throw @"
esperava exatamente um instalador de $Versao e achei $($instaladores.Count).
Procurei por «$padrao» em target\release\bundle.
$(if ($instaladores.Count -eq 0) { 'Nenhum: o empacotamento disse que deu certo mas não deixou arquivo com esta versão.' } else { 'Mais de um: ' + ($instaladores.FullName -join ', ') })
"@
    }
    Copy-Item $instaladores[0].FullName $Destino -Force

    # A assinatura do atualizador fica ao lado do instalador, com o mesmo nome e
    # `.sig` no fim. Só existe quando a chave do projeto estava no ambiente.
    $assinatura = "$($instaladores[0].FullName).sig"
    if (Test-Path $assinatura) {
        Copy-Item $assinatura $Destino -Force
    }

    # O zip da CLI **não** é um segundo instalador: é o que o `install.ps1`
    # baixa. Sem ele o instalador de uma linha para de funcionar.
    Compress-Archive `
        -Path "target\release\seeled.exe" `
        -DestinationPath "$Destino\seele-cli-$Versao-windows-x86_64.zip" -Force

    # -------------------------------------------------------------- conferir
    # O nosso, com nome que não colide com o do NSIS: os dois convivem em
    # `entrega\` enquanto a troca não acontece, e um nome igual faria o segundo
    # sobrescrever o primeiro sem ninguém notar.
    $Proprio = "target\release\seele-instalador.exe"
    if (Test-Path $Proprio) {
        $NomeProprio = "SEELE_${Versao}_x64-instalador.exe"
        Copy-Item $Proprio (Join-Path $Destino $NomeProprio) -Force
        Write-Host "→ instalador próprio: $NomeProprio" -ForegroundColor Cyan
    }

    Write-Host "`n--- entrega ---" -ForegroundColor Green
    Get-ChildItem $Destino | ForEach-Object {
        $h = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
        "{0}  {1}" -f $h, $_.Name
    }
    Write-Host @"

Suba os arquivos no Releases. As somas acima são as que devem constar do
SHA256SUMS — confira-as depois do upload, baixando de volta.
"@ -ForegroundColor Green

    if (Test-Path (Join-Path $Destino "*.exe.sig")) {
        Write-Host @"
Depois de reunir os três sistemas em entrega\, monte o manifesto:
  python3 empacotar\manifesto.py entrega v$Versao
Sem ele o botão de atualizar do app não acha este release.
"@ -ForegroundColor Green
    }
}
finally {
    # A versão gravada é para o artefato, não para o repositório: deixá-la no
    # arquivo faria o próximo `git status` acusar uma mudança que ninguém pediu,
    # e um commit distraído fixaria no repositório o número de um release.
    Write-Utf8SemBom $Config $Original
    Write-Host "→ $Config devolvido ao que era" -ForegroundColor DarkGray
}
