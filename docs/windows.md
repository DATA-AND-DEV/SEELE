# SEELE no Windows

> **O caminho curto:** se houver um release publicado, baixe
> `seele-cli-<versão>-windows-x86_64.zip` na aba **Releases**, descompacte e
> pule direto para o passo 3. Ele traz `seeled.exe` e `connection.exe` prontos, e
> nenhuma das etapas de instalação abaixo é necessária — nem Rust, nem Build
> Tools. O Windows vai reclamar que o arquivo não é assinado; **Mais
> informações** → **Executar assim mesmo**.
>
> O resto desta página é para compilar do código-fonte, que é o que você quer
> se for mexer no código ou se não confiar num binário baixado.

**Nada disto foi executado.** Não tenho uma máquina Windows, e a matriz de CI de
três sistemas nunca rodou por falta de repositório remoto. O que está aqui é
derivado de ler o que cada dependência exige para compilar, e cada exigência
está justificada abaixo para que você possa julgar quando algo divergir.

A parte que **está** verificada: o binário compila e roda em macOS aarch64, dois
clientes conversam por texto e voz sintética entre si, e nenhuma dependência
nativa da árvore exige ferramenta que o Windows não tenha ou que os passos
abaixo não instalem.

Se algo falhar, o mais útil é a mensagem exata e em qual passo.

---

## 1 · Instalar o que compila

Em **cada um dos dois PCs**.

### 1.1 Build Tools do Visual Studio

O Rust no Windows usa o linker da Microsoft, e duas dependências compilam C:
o SQLite embutido do PERSISTENCE e partes do QUIC.

Baixe **Build Tools for Visual Studio** em
<https://visualstudio.microsoft.com/downloads/> (seção *Tools for Visual
Studio*) e, no instalador, marque:

- **Desenvolvimento para desktop com C++**
- Dentro dele, confira que **Windows 10/11 SDK** está marcado

Cerca de 3 GB. Se você já tem o Visual Studio completo com C++, já está feito.

### 1.2 Rust

<https://rustup.rs> → baixe e rode `rustup-init.exe`. Aceite o padrão
(`x86_64-pc-windows-msvc`).

Feche e reabra o terminal. Confira:

```powershell
rustc --version
```

O projeto fixa a versão do toolchain e os componentes de que precisa em
`rust-toolchain.toml` — o rustup instala tudo sozinho na primeira compilação,
inclusive o `llvm-tools`, que o codec Opus usa para renomear símbolos.

**O `llvm-tools` não dispensa a seção 1.3.** São coisas diferentes com nomes
parecidos, e a confusão custa uma compilação inteira.

### 1.3 LLVM

```powershell
winget install LLVM.LLVM
```

Ou <https://releases.llvm.org/>. Depois **feche e reabra o terminal**.

O codec Opus gera suas ligações com `bindgen`, que carrega `libclang.dll` em
tempo de execução. O `llvm-tools` do rustup traz as ferramentas tipo binutils
que renomeiam símbolos — não traz essa biblioteca.

Sem ela a compilação falha assim, e falha **tarde**: depois de baixar 52 MB de
Opus e compilá-lo inteiro com o MSBuild.

```
Unable to find libclang: "couldn't find any valid shared libraries matching:
['clang.dll', 'libclang.dll'], set the `LIBCLANG_PATH` environment variable…"
```

Se instalou fora do padrão, aponte antes de compilar:

```powershell
$env:LIBCLANG_PATH = 'D:\caminho\para\LLVM\bin'
```

`empacotar\windows.ps1` procura sozinho nos lugares usuais — inclusive dentro do
Visual Studio, se você marcou *C++ Clang tools for Windows* — e reprova na
largada se não achar, em vez de deixar você descobrir depois do Opus.

> Isto entrou nesta lista tarde. Os runners do GitHub já vêm com LLVM, então o
> CI nunca precisou dizer que ele era necessário, e a lista foi escrita a partir
> do que o CI instalava. A mesma ausência derrubou o contêiner de empacotamento
> do Linux, pelo mesmo motivo.

### 1.4 Git

<https://git-scm.com/download/win>, padrões.

> **Não é necessário:** CMake, NASM, Python, Node. O `aws-lc-rs`, que exigiria
> CMake e NASM, foi removido da árvore — usamos só o provider `ring`, que é o
> que o código instala em tempo de execução de qualquer forma.
>
> **Já vem no Windows 10 (1803+) e 11:** `curl`, `tar` e `certutil`. O build do
> Opus usa os três para baixar e conferir a biblioteca pré-compilada. Numa
> máquina muito antiga, é o primeiro lugar a olhar se o build falhar.

---

## 2 · Compilar

**Clone perto da raiz do disco.** `C:\SEELE` serve; `C:\Users\voce\Downloads\SEELE`
não.

O Windows corta caminho em 260 caracteres por padrão, e a árvore de dependências
do Tauri é funda: `target\release\build\<crate>-<hash>\out\...` some com uns
cento e poucos sozinho. Passar do limite **não** dá um erro sobre caminho — dá
este, que aponta para o lugar errado:

```
error[E0463]: can't find crate for `tauri`
```

O crate está no `Cargo.lock`, é dependência comum e sem condicional de
plataforma, e a mensagem faz procurar em tudo isso antes de desconfiar da pasta.
Foi encontrado assim: a mesma árvore compilava no macOS e falhava aqui, e mover
de `Downloads` para `C:\SEELE` resolveu sem mais nada.

Quem quiser manter caminhos longos pode ligar o suporte a eles no Windows 10
1607+ (`LongPathsEnabled` na política de sistema de arquivos), mas nem toda
ferramenta da cadeia o respeita. Clonar perto da raiz é a resposta que não
depende disso.

Nos dois PCs:

```powershell
git clone <url-do-repo> C:\SEELE
cd C:\SEELE
cargo build --release --bin seeled --bin connection
```

A primeira compilação demora — dez a vinte minutos é normal, e o Opus baixa uma
biblioteca pré-compilada no meio do caminho.

Para o cliente gráfico, no PC onde quiser testá-lo:

```powershell
cargo build --release -p seele-app
```

O Tauri usa o **WebView2**, que já vem no Windows 11 e na maioria dos Windows 10
atualizados. Se reclamar, o runtime está em
<https://developer.microsoft.com/microsoft-edge/webview2/>.

---

## 3 · Liberar a porta no firewall

**O passo que mais provavelmente vai faltar.** A porta 8383 é **UDP**, porque o
transporte é QUIC — e a regra que se escreve de cabeça é sempre TCP.

Só no PC que vai rodar o servidor. PowerShell **como administrador**:

```powershell
New-NetFirewallRule -DisplayName "SEELE" -Direction Inbound `
  -Protocol UDP -LocalPort 8383 -Action Allow
```

Se ao rodar o `seeled` o Windows perguntar, aceite também — mas ele costuma
perguntar só para TCP, e é por isso que a regra acima é explícita.

---

## 4 · Subir o servidor

No **PC A**:

```powershell
.\target\release\seeled.exe 0.0.0.0:8383
```

Ele imprime, entre outras coisas:

```
na outra máquina:
  connection --server 192.168.x.x:8383

certificate fingerprint: 50217d68c6...
```

**Anote a impressão digital.** É o que o cliente vai fixar no primeiro contato
(ADR 0003) e conferir depois.

Se a linha "na outra máquina" não aparecer, o servidor não achou um endereço de
rede. Confira com `ipconfig`.

O `seeled` cria o `seele.db` na pasta onde foi executado. Para começar do zero,
pare o servidor e apague o arquivo.

---

## 5 · Conectar os dois

No **PC A**, num segundo terminal:

```powershell
.\target\release\connection.exe --server 127.0.0.1:8383 --nick alexandre
```

No **PC B**:

```powershell
.\target\release\connection.exe --server 192.168.x.x:8383 --nick outro
```

Use o endereço que o `seeled` imprimiu, e **apelidos diferentes**.

Na primeira conexão o cliente mostra `PRIMEIRO CONTATO — CHAVE FIXADA` com uma
impressão digital. Confira contra a do passo 4. Se conferir, é aquele servidor.

> Dois clientes com o mesmo `$SEELE_HOME` são **a mesma pessoa** — o servidor
> vincula o apelido à identidade que o reivindicou primeiro. No mesmo PC, para
> ser um segundo pessoa:
> ```powershell
> $env:SEELE_HOME="$HOME\.seele-outro"
> .\target\release\connection.exe --server 127.0.0.1:8383 --nick terceiro
> ```

---

## 6 · Usar

Aperte `?` — a ajuda cabe na tela e é critério de aceite que ela baste.

O resumo: `i` escreve, `Enter` envia, `Esc` cancela, `Tab` troca de painel,
`j`/`k` navegam, `:q` sai. `m` é mudo, `d` é surdo.

**Falar:** barra de espaço no modo Normal. No Terminal do Windows e no
PowerShell, segurar **não** funciona — esses terminais não reportam soltura de
tecla, então a barra vira trava: aperta para abrir, aperta de novo para fechar
(ADR 0016). A barra de telemetria diz qual estado está valendo.

**Use fones nos dois lados.** Não há cancelamento de eco neste produto, e sem
fones haverá realimentação.

Antes de testar voz, prove o texto: se uma mensagem não atravessa, o áudio
também não vai.

---

## 7 · Deixar que o Mac empacote aqui, por SSH

Esta seção não é para usar o SEELE: é para **produzir** o instalador do Windows
sem o GitHub Actions. O Tauri não cross-compila — o empacotador NSIS e o WebView2
precisam de Windows de verdade —, então o `empacotar/publicar.sh` do Mac monta o
macOS e o Linux lá e vem buscar o Windows aqui.

Quem faz isso é o **OpenSSH Server**, que o Windows 10 e o 11 já trazem como
recurso opcional. Ele vem **desligado de fábrica**; sem ligá-lo, o script do Mac
não tem como chegar aqui e para no começo dizendo isso.

### 7.1 Ligar o servidor

PowerShell **como administrador**:

```powershell
Add-WindowsCapability -Online -Name OpenSSH.Daemon~~~~0.0.1.0
Set-Service -Name sshd -StartupType Automatic
Start-Service sshd
```

A primeira linha baixa o recurso da Microsoft; as outras duas o põem no ar agora
e nas próximas vezes que a máquina ligar. O instalador cria sozinho a regra de
firewall para a porta 22 — confira, porque numa máquina com firewall de terceiro
ela não aparece:

```powershell
Get-NetFirewallRule -Name *ssh* | Format-Table Name, Enabled, Direction
```

### 7.2 Deixar a chave do Mac entrar

No **Mac**, se ainda não houver uma:

```sh
ssh-keygen -t ed25519          # se ~/.ssh/id_ed25519.pub não existir
cat ~/.ssh/id_ed25519.pub      # é esta linha que vai para o Windows
```

**Aqui está a pegadinha que custa mais tempo nesta página.** O OpenSSH do Windows
lê a chave de dois lugares diferentes conforme a conta, e para uma conta de
administrador **não é** a pasta do usuário:

| a conta é… | o arquivo é |
|---|---|
| comum | `C:\Users\<você>\.ssh\authorized_keys` |
| **administrador** | `C:\ProgramData\ssh\administrators_authorized_keys` |

Quase toda conta pessoal de Windows é administradora, então quase sempre é o
segundo. E ele exige permissões estreitas — com permissão folgada o `sshd`
**ignora o arquivo em silêncio**, que é o pior modo de falhar que existe:

```powershell
$arquivo = "$env:ProgramData\ssh\administrators_authorized_keys"
Add-Content $arquivo "cole-aqui-a-linha-do-Mac"
icacls $arquivo /inheritance:r /grant "*S-1-5-32-544:F" /grant "*S-1-5-18:F"
```

Os dois identificadores são o grupo de administradores (`S-1-5-32-544`) e o
SISTEMA (`S-1-5-18`), e eles valem em qualquer Windows do mundo. **Não use os
nomes** `Administrators` e `SYSTEM`: eles só existem no Windows em inglês, e num
Windows em português a mesma linha responde «não foi feito mapeamento entre os
nomes de conta e as identificações de segurança» — que é o erro certo dado de um
jeito que não parece ter nada a ver com idioma. Foi assim que ele apareceu.

Do Mac, confira antes de qualquer outra coisa:

```sh
ssh usuario@maquina-windows powershell -Command "echo ok"
```

Se isso imprimir `ok`, o resto funciona.

### 7.3 O shell padrão é o `cmd`, e isso importa

O OpenSSH do Windows entrega uma sessão de **`cmd`**, não de PowerShell. Quem
esquece disso manda uma linha de PowerShell por SSH e recebe erros que não fazem
sentido, porque as aspas foram comidas por outro interpretador no caminho.

O `empacotar/publicar.sh` não depende de o shell padrão ser um ou outro: ele
manda o PowerShell em `-EncodedCommand`, que é base64 de UTF-16LE — sem aspas,
sem acento, sem `&`, sem nada que o `cmd` possa interpretar. Você não precisa
trocar o shell padrão da máquina, e é melhor não trocar: mudar isso muda o
comportamento de todo mundo que entra por SSH aqui.

### 7.4 O repositório, e o que fica desta máquina

O script espera um clone em `C:\SEELE` (ou onde `--repo-windows` disser),
**no mesmo commit** do Mac — ele confere antes de compilar, porque três pacotes
de códigos diferentes são três releases com o mesmo número:

```powershell
git -C C:\SEELE fetch --all
git -C C:\SEELE checkout <o commit que o Mac disser>
```

E a chave de assinatura do projeto **não fica aqui**. Ela vive no Mac e atravessa
pela entrada padrão do SSH a cada empacotamento, dentro do canal cifrado: não é
gravada em disco nesta máquina e não aparece em linha de comando nenhuma — no
Windows a linha de comando de um processo é legível por outros processos, e o
`sshd` com `LogLevel VERBOSE` a escreveria no log de eventos. Uma chave que
existe em dois discos é uma chave com duas chances de vazar, e esta é a que deixa
assinar atualização para todo SEELE instalado.

---

## Quando der errado

| sintoma | causa mais provável |
|---|---|
| `link.exe not found` | Build Tools sem "Desktop com C++" (passo 1.1) |
| SSH pede senha e a chave está no lugar | conta de administrador lê `C:\ProgramData\ssh\administrators_authorized_keys`, não a pasta do usuário (passo 7.2) |
| SSH entra e o `cargo` "não existe" | o Rust foi instalado noutra conta: ele precisa estar no PATH de quem atende o SSH (passo 7.4) |
| Erro no build do `shiguredo_opus` | `curl`/`tar` ausentes, ou sem saída para a internet |
| `llvm-nm` não encontrado | `rustup component add llvm-tools` |
| Cliente fica em PADRÃO: LARANJA e não passa | firewall — a regra é **UDP** |
| `nickname belongs to a different identity` | apelido já pertence a outra identidade; troque de apelido ou de `$SEELE_HOME` |
| Voz com eco horrível | alguém está sem fones |
| A chave mudou e você não mudou nada | apagou o `seele.db`? O servidor gera certificado novo. Apague `%USERPROFILE%\.config\seele\pins` |

Terminal recomendado: **Windows Terminal** (na Microsoft Store). O `conhost`
antigo desenha mal caracteres de caixa e kanji.

---

## Depois que funcionar

`docs/teste-duas-maquinas.md` é o roteiro de validação de verdade: soak de dez
minutos, perda induzida de 5%, latência boca-a-ouvido, e o checklist de
plataforma que fecha M1.15. É o que este projeto ainda não tem e só você pode
produzir.
