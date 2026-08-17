## O que baixar

**Um arquivo por sistema.** Ele traz as duas metades do SEELE: o cliente
gráfico e as duas ferramentas de terminal.

| sistema | baixe |
|---|---|
| **Windows** | `SEELE_<versão>_x64-setup.exe` |
| **macOS** | `SEELE_<versão>_universal.dmg` — Intel e Apple Silicon |
| **Linux** | `SEELE_<versão>_amd64.deb` — Debian, Ubuntu e derivados |

Dentro de cada um vão três programas:

- **`SEELE`** — o cliente gráfico. Tem um botão **HOSPEDAR AQUI**, e com ele
  você nunca precisa abrir um terminal.
- **`plug`** — o cliente de terminal, que é o produto principal.
- **`seeled`** — o servidor, para quem quer um Dogma no ar o tempo todo. Só uma
  das máquinas precisa dele.

### Onde ficam o `plug` e o `seeled`

No **Linux** o `.deb` põe os dois em `/usr/bin`: é só digitar `plug`.

No **macOS** eles vão dentro do app. Para tê-los no `PATH`:

```sh
sudo ln -sf /Applications/SEELE.app/Contents/MacOS/plug   /usr/local/bin/plug
sudo ln -sf /Applications/SEELE.app/Contents/MacOS/seeled /usr/local/bin/seeled
```

No **Windows** ficam na pasta do programa, em
`%LOCALAPPDATA%\Programs\SEELE`. O instalador ainda não acrescenta essa pasta
ao `PATH` — está anotado como pendência.

### E os `seele-cli-*`?

São o mesmo `plug` e `seeled` num arquivo solto, e existem para o instalador de
uma linha:

```sh
curl -fsSL https://raw.githubusercontent.com/DATA-AND-DEV/SEELE/main/install.sh | sh
```

Se você baixou o instalador do seu sistema, **não precisa deles**.

### E o `latest.json`, os `.sig` e o `.app.tar.gz`?

Também não são para você. São o que o **próprio SEELE** procura para se
atualizar: o `latest.json` diz qual é a versão nova, e cada `.sig` é a assinatura
que o app confere antes de instalar qualquer coisa. O `.app.tar.gz` é o mesmo app
do `.dmg`, num formato que o atualizador sabe trocar de lugar sozinho.

Se estiverem nesta página, esta versão atualiza sozinha a partir da próxima. Se
não estiverem, ela não atualiza — e continua tudo funcionando como antes, com o
instalador baixado à mão.

---

## Como conferir que o arquivo é o que diz ser

Duas perguntas diferentes, e cada uma tem a sua resposta.

**"O arquivo chegou inteiro?"** — o `SHA256SUMS` desta página responde.

```sh
sha256sum -c SHA256SUMS --ignore-missing        # Linux
shasum -a 256 -c SHA256SUMS --ignore-missing    # macOS
```

```powershell
Get-FileHash .\SEELE_*-setup.exe -Algorithm SHA256   # Windows, e compare à mão
```

**"E como sei que este arquivo veio daquele código?"** — essa a soma **não**
responde. Quem confere só o hash está confiando em quem escreveu a página que
publicou o hash. Para essa há atestado de procedência, assinado pelo GitHub, que
amarra cada arquivo ao commit e à execução do workflow que o produziu:

```sh
gh attestation verify SEELE_1.2.3_x64-setup.exe --repo DATA-AND-DEV/SEELE
```

Ele imprime o commit, o workflow e o repositório de origem. Se alguém trocar o
arquivo em qualquer ponto entre a compilação e o seu disco, isto acusa — e
acusa sem depender de nada que esta página diga.

O que o atestado **não** faz é dizer que o software é bom. Ele diz de onde veio.
As duas coisas são frequentemente confundidas, inclusive pelos avisos do sistema
operacional na seção abaixo.

---

## Se o sistema reclamar ao abrir

**Este é o aviso mais importante desta página.** Assinar exige certificado pago —
da Apple num lado, de uma autoridade Windows no outro —, e enquanto não houver um
o sistema operacional vai reclamar. A reclamação não significa que o arquivo
esteja corrompido ou infectado: significa que ninguém pagou para garantir quem o
produziu.

Se o aviso não aparecer, esta versão já saiu assinada, e você pode pular esta
seção inteira. Ela continua aqui porque a assinatura entra por sistema e por
versão, e o silêncio de um não é o silêncio do outro.

**macOS.** É a reclamação mais dura das três, e a mais assustadora: o sistema
diz que **não consegue verificar se o app contém malware**, e o botão que
aparece é "Mover para o Lixo". Não é detecção de nada — é a ausência de
notarização, que exige uma conta paga de desenvolvedor da Apple.

Depois de arrastar o app para Aplicativos, uma linha resolve:

```sh
xattr -dr com.apple.quarantine /Applications/SEELE.app
```

Sem terminal: **clique com o botão direito** no app → **Abrir** → **Abrir** de
novo no alerta. O caminho pelo botão direito é o único que oferece "Abrir";
o duplo clique não oferece, e é por isso que parece que não há saída.

Se aparecer de novo depois de uma atualização, é a mesma linha.

Para os binários de linha de comando, o mesmo na pasta onde você os
descompactou:

```sh
xattr -dr com.apple.quarantine ./plug ./seeled
chmod +x ./plug ./seeled
```

**Windows.** O SmartScreen vai mostrar "O Windows protegeu o computador".
Clique em **Mais informações** → **Executar assim mesmo**.

**Linux.** Nada reclama. Para o AppImage, `chmod +x` antes de executar.

Se isso te incomoda — e é razoável que incomode —, a alternativa é compilar do
código-fonte, que é a coisa que este produto foi feito para permitir. Veja
`docs/windows.md` no repositório.

---

## Como testar rápido

Numa máquina:

```sh
./seeled 0.0.0.0:8383
```

Ele imprime o endereço a usar na outra máquina e a impressão digital do
certificado. **Anote a impressão digital.**

Na outra:

```sh
./plug --server <endereço>:8383 --nick seunome
```

Na primeira conexão o cliente mostra `PRIMEIRO CONTATO — CHAVE FIXADA` com uma
impressão digital. Confira contra a que o servidor imprimiu: se conferir, é
aquele servidor mesmo.

Dentro do cliente, aperte `?`.

Duas coisas que economizam tempo:

- A porta 8383 é **UDP**, não TCP. É o erro de firewall mais comum aqui, porque
  a regra que se escreve de cabeça é sempre TCP.
- **Use fones dos dois lados.** Não há cancelamento de eco, e sem fones haverá
  realimentação.

---

## O que ainda não foi validado

Este projeto nunca foi usado por duas pessoas reais em duas máquinas reais. O
que está testado é o que roda sem hardware: 457 testes automáticos, incluindo
áudio simulado sob perda de pacote e um soak de dez minutos em tempo simulado.

Não testado: voz por microfone de verdade, latência boca-a-ouvido, e o
comportamento em Windows e Linux além do que a integração contínua compila.

`docs/teste-duas-maquinas.md` é o roteiro para produzir essas medições.
