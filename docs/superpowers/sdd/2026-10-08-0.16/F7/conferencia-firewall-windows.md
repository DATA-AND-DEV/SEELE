# D-sala-1 no Windows: a conferência sem tela (15/10)

Roteiro para a orquestradora rodar no PC de testes pelo SSH (`C:\SEELE`, com
Rust). Ninguém mexe no firewall à mão: quem cria a regra é o instalador, e o
caso «sem regra» vem do pacote portátil. A pergunta na tela fica com o dono.

## O que o código diz (lido na `versao/0.16`, `95ff2689`, e na v0.15.1 publicada)

- **Quem cria a regra é o instalador próprio** (ADR 0043, `apps/seele-instalador`),
  e não mais o NSIS, que saiu da entrega. Ele não mudou entre a v0.15.1
  (`54f542f9`) e a `versao/0.16`.
  - Nome **`SEELE`**, entrada, UDP, `profile=any`, ligada, **pelo programa**
    `C:\Program Files\SEELE\SEELE.exe`, sem porta. Por isso ela cobre a 8385 da
    sala e a 8383 do servidor guardado.
  - **Por padrão, sim**: a escolha fica em `HKLM\Software\DATA-AND-DEV\SEELE`,
    valor `FirewallUDP`. Sem o valor, o instalador cria a regra. Com `0`
    guardado por uma instalação anterior, o modo silencioso respeita o `0`.
  - Antes de criar, ele apaga toda regra de entrada chamada `SEELE`.
- **Quem lê é o app** (`firewall_nao_cobre_este_executavel`, `main.rs`), com
  três leituras, nesta ordem:
  1. `netsh advfirewall firewall show rule name=SEELE dir=in`: só o código de
     saída. Sem regra, para aí;
  2. `reg query "HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\SEELE" /v InstallLocation`;
  3. o executável que roda está dentro dessa pasta?
- **No `seele.log`** a resposta sai numa linha só, a do estado da sala:
  `INFO seele_app::sala: sala pessoal estado=NoAr { registro: …, firewall_nao_cobre: … }`.
  O log mora em `%APPDATA%\tech.datadev.seele\seele.log`, ou em `%SEELE_HOME%\seele.log`.

## Passo 0 — o estado do PC, só leitura

```powershell
reg query "HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\SEELE" /v DisplayVersion
reg query "HKLM\Software\Microsoft\Windows\CurrentVersion\Uninstall\SEELE" /v InstallLocation
reg query "HKLM\Software\DATA-AND-DEV\SEELE"
Get-NetFirewallRule -DisplayName "SEELE" -ErrorAction SilentlyContinue |
  Format-List DisplayName,Enabled,Direction,Action,Profile
Get-NetFirewallRule -DisplayName "SEELE" -ErrorAction SilentlyContinue |
  Get-NetFirewallApplicationFilter | Format-List Program
Get-NetFirewallRule -DisplayName "SEELE" -ErrorAction SilentlyContinue |
  Get-NetFirewallPortFilter | Format-List Protocol,LocalPort
# toda regra que cita o programa, de qualquer origem: o aviso do próprio Windows
# cria regras de BLOQUEIO quando alguém aperta Cancelar
Get-NetFirewallApplicationFilter |
  Where-Object { $_.Program -like '*\SEELE.exe' -or $_.Program -like '*\seele-app.exe' } |
  Get-NetFirewallRule | Format-Table DisplayName,Enabled,Direction,Action,Profile -AutoSize
# a sessão do SSH está elevada? o instalador pede administrador
whoami /groups | findstr /C:"S-1-16-12288"
Get-NetUDPEndpoint -LocalPort 8383,8385 -ErrorAction SilentlyContinue
```

**Pare aqui e me mande a saída** se: o `findstr` não achar o `S-1-16-12288` (a
sessão não está elevada, e o instalador não roda pelo SSH); houver regra de
bloqueio para o `SEELE.exe`; ou houver uma instalação cujos dados o dono queira
preservar (a conferência instala a 0.16 por cima).

## Passo 1 — o instalador da 0.16, gerado no PC

Escreve só em `target\` e em `entrega\` da cópia. O `windows.ps1` instala o
`tauri-cli` se faltar, e devolve o `tauri.conf.json` como estava.

**A `versao/0.16` não está no `origin`** (conferido em 15/10): ela tem de chegar
a `C:\SEELE` do jeito que a 0.15.2 chegou para o teste do app. O código desta
conferência é o de `95ff2689` ou mais novo: a sala sobe sozinha desde a 12ª, e o
instalador e a leitura da regra não mudaram depois.

```powershell
cd C:\SEELE
git status --short
git rev-parse --short HEAD           # me mande este hash
.\empacotar\windows.ps1 -Versao 0.16.0
Get-ChildItem entrega | Format-Table Name,Length
```

Saem `entrega\SEELE_0.16.0_x64-instalador.exe` e
`entrega\SEELE_0.16.0_x64-portatil.tar.gz`. Sem a chave de assinatura, o `.exe`
sai sem a assinatura do atualizador, e isso não pesa aqui.

## Passo 2 — «sem regra», se o Passo 0 não achou instalação nem regra

Só neste caso: o portátil numa máquina sem regra nenhuma é o primeiro ramo do
app. **Com instalação ou regra de pé, pule para o Passo 3.**

```powershell
New-Item -ItemType Directory -Force C:\SEELE\entrega\portatil | Out-Null
tar -xzf C:\SEELE\entrega\SEELE_0.16.0_x64-portatil.tar.gz -C C:\SEELE\entrega\portatil
$env:SEELE_HOME = 'C:\SEELE\entrega\casa-sem-regra'; $env:RUST_LOG = 'seele_app=info'
$app = Start-Process C:\SEELE\entrega\portatil\SEELE.exe -PassThru
Start-Sleep 25
Select-String -Path "$env:SEELE_HOME\seele.log" -Pattern 'sala pessoal estado|could not start|janela pronta'
Get-NetUDPEndpoint -LocalPort 8385 -ErrorAction SilentlyContinue
Stop-Process -Id $app.Id
```

**Esperado:**
```
sala pessoal estado=NoAr { registro: …, firewall_nao_cobre: Some("não há regra de firewall para o SEELE nesta máquina, então conexão de fora não chega — nem de quem está na mesma rede. Reinstale marcando a caixa «abrir a porta 8383 UDP» no passo 02.") }
```

## Passo 3 — instalar em silêncio

```powershell
$inst = 'C:\SEELE\entrega\SEELE_0.16.0_x64-instalador.exe'
$p = Start-Process $inst -ArgumentList '/S' -Wait -PassThru `
  -RedirectStandardOutput C:\SEELE\entrega\instalar.out `
  -RedirectStandardError C:\SEELE\entrega\instalar.err
$p.ExitCode
Get-Content C:\SEELE\entrega\instalar.out, C:\SEELE\entrega\instalar.err
```

**Esperado:** código `0`. Na saída, um passo por linha, entre eles «porta 8383
UDP aberta no firewall, em qualquer perfil de rede». Se aparecer «firewall:
regra não pedida», havia `FirewallUDP = 0` guardado (Passo 0). Sem o `/R`, o
instalador não abre o app.

## Passo 4 — a regra, só leitura

Os mesmos comandos de regra do Passo 0, mais o que o app pergunta:

```powershell
netsh advfirewall firewall show rule name=SEELE dir=in | Out-Null; $LASTEXITCODE
reg query "HKLM\Software\DATA-AND-DEV\SEELE" /v FirewallUDP
```

**Esperado:**
- `DisplayName SEELE`, `Enabled True`, `Direction Inbound`, `Action Allow`, `Profile Any`;
- `Program C:\Program Files\SEELE\SEELE.exe`;
- `Protocol UDP`, `LocalPort Any`;
- `$LASTEXITCODE` `0`, e `FirewallUDP 0x1`;
- uma regra `SEELE` só.

## Passo 5 — a primeira abertura, com a regra

Opcional pelo SSH: a janela é criada **antes** de a sala subir. Se o Windows
não deixar criar a janela nesta sessão, o log diz «the desktop shell could not
start», e esta parte fica para o dono, na tela.

```powershell
$env:SEELE_HOME = 'C:\SEELE\entrega\casa-instalado'; $env:RUST_LOG = 'seele_app=info'
$app = Start-Process 'C:\Program Files\SEELE\SEELE.exe' -PassThru
Start-Sleep 25
Select-String -Path "$env:SEELE_HOME\seele.log" -Pattern 'sala pessoal estado|could not start|janela pronta'
Get-NetUDPEndpoint -LocalPort 8385 -ErrorAction SilentlyContinue
Stop-Process -Id $app.Id
```

**Esperado:** `janela pronta`, depois `sala pessoal estado=Subindo` e
`sala pessoal estado=NoAr { registro: …, firewall_nao_cobre: None }`, e a 8385
com o PID do app.

## Passo 6 — a mesma regra, outra cópia: o portátil com a instalação de pé

É o «sem regra» que não pede mexer em nada: a regra é pelo programa
instalado, e esta cópia não é ele.

```powershell
New-Item -ItemType Directory -Force C:\SEELE\entrega\portatil | Out-Null
tar -xzf C:\SEELE\entrega\SEELE_0.16.0_x64-portatil.tar.gz -C C:\SEELE\entrega\portatil
$env:SEELE_HOME = 'C:\SEELE\entrega\casa-portatil'; $env:RUST_LOG = 'seele_app=info'
$app = Start-Process C:\SEELE\entrega\portatil\SEELE.exe -PassThru
Start-Sleep 25
Select-String -Path "$env:SEELE_HOME\seele.log" -Pattern 'sala pessoal estado|could not start|janela pronta'
Stop-Process -Id $app.Id
```

**Esperado:**
```
sala pessoal estado=NoAr { registro: …, firewall_nao_cobre: Some("este SEELE está rodando de C:\SEELE\entrega\portatil\SEELE.exe, e a regra de firewall cobre o que está instalado em C:\Program Files\SEELE. Conexões de fora não chegam a esta cópia.") }
```

**Atenção:** um programa sem regra que escuta UDP pode fazer o Windows mostrar o
aviso «permitir acesso» na tela do PC. Ninguém deve responder. Cancelar cria
regras de **bloqueio** para essa cópia, e elas apareceriam no último comando do
Passo 0.

## O que me mandar

- a saída de cada passo, com o hash do Passo 1;
- os três `seele.log` inteiros: `casa-sem-regra` (se rodou), `casa-instalado` e `casa-portatil`.

## O que a leitura do código já aponta, para a conferência confirmar ou não

1. **A frase diz a porta errada para a sala.** «Reinstale marcando a caixa
   "abrir a porta 8383 UDP" no passo 02» aparece no PERFIL, na linha da sala,
   que escuta na **8385**. A regra é pelo programa e cobre as duas portas; a
   frase e o «porta 8383 UDP aberta» do instalador falam só da 8383. O comentário
   que diz que a caixa «nasce desmarcada» também está velho: o padrão é sim.
2. **O app só pergunta se existe uma regra chamada `SEELE`.** Não confere se
   ela está ligada, se permite ou bloqueia, nem o programa. O aviso do próprio
   Windows cria regras com o nome do produto. Se alguém apertou Cancelar nele,
   pode existir um **bloqueio** chamado `SEELE`, e o app diria que está tudo
   coberto. O último comando do Passo 0 mostra se isso acontece no PC.
3. **A D-sala-1 diz «pergunta», e a casca tem uma linha no PERFIL** com a frase
   do Rust, o que a 8385 é e um botão que pede a regra
   (`abrir_a_porta_no_firewall`, que chama o instalador com `--abrir-a-porta`).
   Não há pergunta ao abrir o app. Isso o dono vê na tela, e é dele rever.
4. O `alcance::firewall` do `seele-server`, cuja documentação diz ser quem lê a
   regra, não é chamado por ninguém fora dos testes. Quem lê é o `main.rs`.
