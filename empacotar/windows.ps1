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

$Config = "apps\seele-app\tauri.conf.json"
$Original = Get-Content $Config -Raw

try {
    # ------------------------------------------------------- gravar a versão
    #
    # É ela que aparece no instalador e em "Aplicativos e recursos". Sem isto o
    # arquivo baixado diria 0.0.0, que é a versão do workspace.
    Write-Host "→ gravando a versão $Versao" -ForegroundColor Cyan
    $json = $Original | ConvertFrom-Json
    $json.version = $Versao
    $json | ConvertTo-Json -Depth 100 | Set-Content $Config -Encoding UTF8

    # ------------------------------------------------------------------ CLI
    #
    # Antes do app, como no workflow: `seeled` e `plug` são o produto principal
    # (`specs/00-visao-geral.md`) e o que menos pode falhar. Eles entram
    # **dentro** do instalador, para que quem baixa o app ganhe as duas metades
    # e não precise de um segundo arquivo para hospedar ou usar o terminal.
    Write-Host "→ compilando seeled e plug (a primeira vez demora)" -ForegroundColor Cyan
    cargo build --release --bin seeled --bin plug
    if ($LASTEXITCODE -ne 0) { throw "a compilação da CLI falhou" }

    # O Tauri procura acompanhantes pelo nome com o alvo no fim.
    $Binarios = "apps\seele-app\binaries"
    New-Item -ItemType Directory -Force -Path $Binarios | Out-Null
    foreach ($b in @("plug", "seeled")) {
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

    # -------------------------------------------------------------- reunir
    $Destino = "entrega"
    New-Item -ItemType Directory -Force -Path $Destino | Out-Null

    $instaladores = @(Get-ChildItem -Path "target\release\bundle" -Recurse -Filter "*-setup.exe")
    if ($instaladores.Count -ne 1) {
        throw "esperava exatamente um instalador e achei $($instaladores.Count). Veja target\release\bundle."
    }
    Copy-Item $instaladores[0].FullName $Destino -Force

    # O zip da CLI **não** é um segundo instalador: é o que o `install.ps1`
    # baixa. Sem ele o instalador de uma linha para de funcionar.
    Compress-Archive `
        -Path "target\release\seeled.exe", "target\release\plug.exe" `
        -DestinationPath "$Destino\seele-cli-$Versao-windows-x86_64.zip" -Force

    # -------------------------------------------------------------- conferir
    Write-Host "`n--- entrega ---" -ForegroundColor Green
    Get-ChildItem $Destino | ForEach-Object {
        $h = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower()
        "{0}  {1}" -f $h, $_.Name
    }
    Write-Host @"

Suba os dois arquivos no Releases. As somas acima são as que devem constar do
SHA256SUMS — confira-as depois do upload, baixando de volta.
"@ -ForegroundColor Green
}
finally {
    # A versão gravada é para o artefato, não para o repositório: deixá-la no
    # arquivo faria o próximo `git status` acusar uma mudança que ninguém pediu,
    # e um commit distraído fixaria no repositório o número de um release.
    Set-Content $Config -Value $Original -NoNewline -Encoding UTF8
    Write-Host "→ $Config devolvido ao que era" -ForegroundColor DarkGray
}
