# Roteiro do dono, 23/10 — a medida com tela antes de publicar a 0.16

Fecha as condições L20, L56 e L259 das notas da 0.16 no Mac, e L123, L259 e o
Windows de L20 no PC. Cada passo diz o que se vê e o que se confere no
terminal. **Nada aqui toca os dados reais:** tudo roda com `SEELE_HOME` numa
pasta temporária. O armazenamento do WebView (duas preferências de MOD) é o do
app de verdade, porque o identificador é o mesmo; nada mais é compartilhado.

## Mac (uns 15 minutos)

**Antes, sem o dono:** montar a 0.16 sem publicar, no checkout da publicação
(`./empacotar/macos.sh 0.16.0`; sem a chave ela só não serve de origem de
atualização). O app fica em `target/release/bundle/macos/SEELE.app`. Fechar o
SEELE de verdade (Cmd-Q), para a porta 8383 ficar livre.

```sh
export T="$(mktemp -d)/seele"; mkdir -p "$T"
export NOVA="<checkout>/target/release/bundle/macos/SEELE.app/Contents/MacOS/seele-app"
```

1. **A 0.15.1 de hoje** (a instalada em `/Applications` é a 0.15.1, a origem de
   quem atualiza).
   `SEELE_HOME="$T" /Applications/SEELE.app/Contents/MacOS/seele-app`
   - HOSPEDAR AQUI; escrever uma mensagem no canal; COPIAR O LINK e guardá-lo
     (ele traz `v=0.15.1`). Fechar (Cmd-Q).
   - `shasum "$T/identity.key"` e `find "$T" -name '*.db' -exec shasum {} \;`
     — anotar as somas.
2. **A primeira abertura da 0.16** (o que o atualizador faria: troca o app, a
   pasta fica). `SEELE_HOME="$T" "$NOVA"`
   - **Na tela:** a nota, uma vez, de que as conversas da 0.15 ficaram guardadas
     para o Servidor 0.15.
   - `ls "$T/lancador/dados/servidor-0.15/PRONTA"` existe;
     `ls "$T/lancador/dados/servidor-0.15/identity.key"` **não** existe (D-id-1);
     `shasum "$T/identity.key"` **igual** ao do passo 1 (**L56**);
     os bancos copiados para `lancador/dados/servidor-0.15/` com as somas do passo 1.
   - Em uns 30 s: `cat "$T"/lancador/versoes/0.15.2/INSTALADA` com 4 linhas (a
     v0.15.2 baixada sozinha do release publicado).
3. **Hospedar no Servidor 0.15:** na 0.16, HOSPEDAR AQUI → Servidor 0.15 →
   USAR ESTA → ABRIR O SERVIDOR 0.15 PARA HOSPEDAR.
   - **Na tela:** a v0.15.2 abre hospedando, com a mensagem do passo 1 no canal,
     e você é o **Comandante**. Só a mesma chave mantém esse papel; com outra,
     você entraria como uma conta nova (**L56**, pela tela).
   - Na v0.15.2: a seção VERSÃO E ATUALIZAÇÃO não aparece.
4. **Entrar pelo link `v=0.15.1`:** na 0.16, CONECTAR → colar o link do passo 1.
   - **Na tela:** a pergunta «Este servidor roda o Servidor 0.15. Ele abre
     noutra janela…» → ABRIR → outra janela da v0.15.2 entra no servidor do
     passo 3. Ficam três janelas abertas; anotar o que o macOS perguntar
     (microfone, rede).
5. **O computador volta a dormir (L259):** fechar as duas janelas da v0.15.2.
   Na 0.16, HOSPEDAR AQUI (Servidor 1.0) e entrar numa sala de voz.
   - `pmset -g assertions | grep -i PreventUserIdleSystemSleep` → a do SEELE
     aparece; sair da sala → some.
6. **Limpar:** fechar tudo; `rm -rf "$T"`.

## Windows, no `aleta@192.168.50.30` (uns 20 minutos)

**Antes, sem o dono:** o instalador e o portátil da 0.16 montados no
`C:\SEELE-016-teste` (`empacotar\windows.ps1 0.16.0`), sem publicar. A v0.15.2
instalada no PC, como a de quem atualiza. PowerShell **como administrador**.

```powershell
$env:SEELE_HOME = "$env:TEMP\seele-medida"; New-Item -ItemType Directory -Force $env:SEELE_HOME
```

1. **A v0.15.2 de hoje:** `& "C:\Program Files\SEELE\SEELE.exe"` (na mesma
   janela, para herdar o `SEELE_HOME`). HOSPEDAR AQUI, uma mensagem, COPIAR O
   LINK. Fechar. `Get-FileHash $env:SEELE_HOME\identity.key`.
2. **Atualizar:** rodar o `SEELE_0.16.0_x64-instalador.exe` por cima. Abrir a
   0.16 na mesma janela do PowerShell.
   - `Test-Path $env:SEELE_HOME\dados\servidor-0.15\PRONTA` → True; o
     `identity.key` lá dentro → False; o hash da chave igual ao do passo 1.
   - Em uns 30 s, `Get-Content $env:SEELE_HOME\lancador\versoes\0.15.2\INSTALADA`
     → 4 linhas (o **portátil** baixado).
3. **Hospedar no Servidor 0.15** pela 0.16 (como no Mac). É a primeira vez que o
   portátil roda: anotar se o Windows pergunta sobre o firewall para o
   `SEELE.exe` do depósito (a regra do instalador é pelo caminho de Program
   Files). Você é o Comandante, e a mensagem do passo 1 está lá.
4. **A pergunta do firewall (L123):** fechar tudo;
   `netsh advfirewall firewall delete rule name=SEELE`. Abrir a 0.16 e pôr a
   sala no ar: em SEU PERFIL aparece a frase do firewall e **ABRIR A PORTA
   AGORA** → clicar → UAC → `netsh advfirewall firewall show rule name=SEELE`
   mostra a regra de volta.
5. **O computador volta a dormir (L259):** numa sala de voz,
   `powercfg /requests` mostra o SEELE; fora da sala, não mostra.
6. **Limpar:** `Remove-Item -Recurse $env:SEELE_HOME`.

## Se o Windows não couber hoje, saem das notas

- L123: a frase «No Windows, quando falta a regra de firewall que o instalador
  cria, o SEELE pergunta…».
- L259: a metade do Windows (o `powercfg`); fica «medido no Mac».
- L20: o pacote portátil do Windows e a primeira abertura no Windows; a seção
  fica com o Mac medido.
