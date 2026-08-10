# Instala o `plug` e o `seeled` no Windows a partir de um release do GitHub.
#
#   irm https://raw.githubusercontent.com/DATA-AND-DEV/SEELE/main/install.ps1 | iex
#
# Antes de rodar isso: você está prestes a executar um script vindo da rede.
# Num produto cujo argumento é não depender de terceiros, isso merece um
# segundo de atenção. Se preferir:
#
#   1. Baixe, leia, e só então rode:
#        irm https://raw.githubusercontent.com/DATA-AND-DEV/SEELE/main/install.ps1 -OutFile install.ps1
#        notepad install.ps1
#        .\install.ps1
#
#   2. Sem script: pegue o .zip na aba Releases, confira a soma contra o
#      SHA256SUMS publicado ao lado, e descompacte onde quiser.
#
# O que este script faz: baixa o pacote da versão pedida, **confere a soma
# SHA-256** contra o arquivo publicado no mesmo release, e copia dois
# executáveis para uma pasta. Não mexe no registro, não pede administrador,
# não instala serviço.
#
# Variáveis:
#   $env:SEELE_VERSION  versão a instalar (padrão: a última publicada)
#   $env:SEELE_BIN      onde instalar (padrão: %LOCALAPPDATA%\SEELE\bin)

$ErrorActionPreference = 'Stop'

$repo = 'DATA-AND-DEV/SEELE'
$destino = if ($env:SEELE_BIN) { $env:SEELE_BIN } else { "$env:LOCALAPPDATA\SEELE\bin" }

function Falhar($mensagem) {
    Write-Host ''
    Write-Host "erro: $mensagem" -ForegroundColor Red
    exit 1
}

if ([Environment]::Is64BitOperatingSystem -eq $false) {
    Falhar 'só há pacote para Windows x86_64.'
}

# ------------------------------------------------------------------- versão

$versao = $env:SEELE_VERSION
if (-not $versao) {
    Write-Host 'procurando a última versão... ' -NoNewline
    try {
        $ultimo = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest"
        $versao = $ultimo.tag_name
    } catch {
        Falhar @"
não achei nenhuma versão publicada.

       Se o repositório for privado, ou se ainda não houver release
       publicado (um rascunho não conta), este script não tem de onde
       baixar. Compile do código-fonte — ver docs\windows.md.
"@
    }
    Write-Host $versao
}

$numero = $versao -replace '^v', ''
$pacote = "seele-cli-$numero-windows-x86_64.zip"
$base = "https://github.com/$repo/releases/download/$versao"

# --------------------------------------------------------------------- baixa

$trabalho = Join-Path ([System.IO.Path]::GetTempPath()) "seele-$([guid]::NewGuid())"
New-Item -ItemType Directory -Path $trabalho -Force | Out-Null

try {
    Write-Host "baixando $pacote"
    try {
        Invoke-WebRequest "$base/$pacote" -OutFile "$trabalho\$pacote"
    } catch {
        Falhar "não consegui baixar $pacote. Confira se a versão $versao tem pacote para Windows."
    }

    # ------------------------------------------------------------- confere

    Write-Host 'conferindo a soma... ' -NoNewline
    try {
        Invoke-WebRequest "$base/SHA256SUMS" -OutFile "$trabalho\SHA256SUMS"
    } catch {
        Falhar @"
este release não publica SHA256SUMS.

       Sem soma não há o que conferir, e instalar um binário sem conferir
       é exatamente o que este script deveria evitar.
"@
    }

    $esperada = (Get-Content "$trabalho\SHA256SUMS" |
        Where-Object { $_ -match [regex]::Escape($pacote) } |
        Select-Object -First 1) -split '\s+' | Select-Object -First 1
    if (-not $esperada) { Falhar "o SHA256SUMS não menciona $pacote." }

    $obtida = (Get-FileHash "$trabalho\$pacote" -Algorithm SHA256).Hash.ToLower()
    if ($esperada.ToLower() -ne $obtida) {
        Falhar @"
A SOMA NÃO CONFERE.

       esperada: $esperada
       obtida:   $obtida

       O arquivo baixado não é o que foi publicado. Não instale.
       Pode ser corrupção no caminho — ou não.
"@
    }
    Write-Host 'confere'

    # ------------------------------------------------------------- instala

    Expand-Archive "$trabalho\$pacote" -DestinationPath $trabalho -Force
    New-Item -ItemType Directory -Path $destino -Force | Out-Null

    foreach ($programa in @('plug.exe', 'seeled.exe')) {
        $origem = Get-ChildItem -Path $trabalho -Filter $programa -Recurse |
            Select-Object -First 1
        if (-not $origem) { Falhar "o pacote não traz $programa." }
        Copy-Item $origem.FullName (Join-Path $destino $programa) -Force
    }
} finally {
    Remove-Item $trabalho -Recurse -Force -ErrorAction SilentlyContinue
}

Write-Host ''
Write-Host "instalado em $destino"
Write-Host '  plug.exe    o cliente de terminal'
Write-Host '  seeled.exe  o servidor'

$noCaminho = ($env:PATH -split ';') -contains $destino
if (-not $noCaminho) {
    Write-Host ''
    Write-Host "  $destino não está no seu PATH. Para acrescentar, permanentemente:"
    Write-Host "    [Environment]::SetEnvironmentVariable('PATH', `"`$env:PATH;$destino`", 'User')"
    Write-Host '  e abra um terminal novo.'
}

Write-Host ''
Write-Host 'a porta 8383 e UDP, nao TCP. Para o servidor aceitar conexoes,'
Write-Host 'num PowerShell de administrador:'
Write-Host '  New-NetFirewallRule -DisplayName SEELE -Direction Inbound -Protocol UDP -LocalPort 8383 -Action Allow'
Write-Host ''
Write-Host 'para comecar:'
Write-Host '  seeled 0.0.0.0:8383      numa maquina'
Write-Host '  plug --server <ip>:8383  na outra'
