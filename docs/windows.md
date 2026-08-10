# SEELE no Windows

> **O caminho curto:** se houver um release publicado, baixe
> `seele-cli-<versão>-windows-x86_64.zip` na aba **Releases**, descompacte e
> pule direto para o passo 3. Ele traz `seeled.exe` e `plug.exe` prontos, e
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
o SQLite embutido do CASPER e partes do QUIC.

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

### 1.3 Git

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

Nos dois PCs:

```powershell
git clone <url-do-repo> SEELE
cd SEELE
cargo build --release --bin seeled --bin plug
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

## 4 · Subir o Dogma

No **PC A**:

```powershell
.\target\release\seeled.exe 0.0.0.0:8383
```

Ele imprime, entre outras coisas:

```
na outra máquina:
  plug --server 192.168.x.x:8383

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
.\target\release\plug.exe --server 127.0.0.1:8383 --nick alexandre
```

No **PC B**:

```powershell
.\target\release\plug.exe --server 192.168.x.x:8383 --nick outro
```

Use o endereço que o `seeled` imprimiu, e **apelidos diferentes**.

Na primeira conexão o cliente mostra `PRIMEIRO CONTATO — CHAVE FIXADA` com uma
impressão digital. Confira contra a do passo 4. Se conferir, é aquele servidor.

> Dois clientes com o mesmo `$SEELE_HOME` são **a mesma pessoa** — o servidor
> vincula o apelido à identidade que o reivindicou primeiro. No mesmo PC, para
> ser um segundo piloto:
> ```powershell
> $env:SEELE_HOME="$HOME\.seele-outro"
> .\target\release\plug.exe --server 127.0.0.1:8383 --nick terceiro
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

## Quando der errado

| sintoma | causa mais provável |
|---|---|
| `link.exe not found` | Build Tools sem "Desktop com C++" (passo 1.1) |
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
