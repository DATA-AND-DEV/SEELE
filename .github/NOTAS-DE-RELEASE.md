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

**Nem toda versão tem atestado, e é preciso dizer isso aqui.** Ele é assinado
pela infraestrutura que compila, e existe só para os pacotes que saíram do
workflow. Quando a cota de Actions acaba — já aconteceu —, os instaladores são
montados nas máquinas de quem publica, e para esses **não há atestado nem há
como haver**: não existe execução de workflow a que amarrá-los. Nessas versões o
comando acima responde que não encontrou atestado, e essa resposta significa
«esta versão não veio do CI», e não «este arquivo foi adulterado».

Como saber em qual das duas você está: **as versões montadas à mão dizem isso no
fim desta página**, numa seção chamada "Como esta versão foi montada". Se ela não
estiver aí, esta versão saiu do workflow e o atestado existe.

O que continua valendo numa versão montada à mão é o que já estava dito acima —
o `SHA256SUMS` responde «o arquivo chegou inteiro» — mais uma coisa: os `.sig`
respondem «veio de quem tem a chave do projeto», que é a assinatura que o próprio
SEELE confere antes de instalar uma atualização. O que se perde é exatamente
«veio daquele código», e não há como fingir que não.

O que o atestado **não** faz, quando existe, é dizer que o software é bom. Ele
diz de onde veio. As duas coisas são frequentemente confundidas, inclusive pelos
avisos do sistema operacional na seção abaixo.

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

**Windows.** Duas defesas diferentes podem aparecer, e elas não são a mesma
coisa.

O **SmartScreen** mostra "O Windows protegeu o computador". Clique em **Mais
informações** → **Executar assim mesmo**.

O **Smart App Control** é outro, e mais duro: ele **não deixa executar**, e não
oferece "executar assim mesmo". Ele é do Windows 11, só liga em instalação
limpa, e por isso duas máquinas parecidas se comportam diferente. Se for o seu
caso, a saída honesta é **esperar uma versão assinada** ou **compilar do
código-fonte** — `docs/windows.md` tem o passo a passo.

Existe a opção de desligar o Smart App Control, e ela não está aqui de
recomendação: **é caminho sem volta.** Uma vez desligado, só reinstalando o
Windows para religá-lo, e isso é um preço grande demais por um programa de
conversar com amigos.

**Linux.** Nada reclama. Para o AppImage, `chmod +x` antes de executar.

Se isso te incomoda — e é razoável que incomode —, a alternativa é compilar do
código-fonte, que é a coisa que este produto foi feito para permitir. Veja
`docs/windows.md` no repositório.

---

## Mandar arquivo, e o que isso significa

Dá para mandar imagem, áudio e arquivo. Duas coisas sobre isso, e as duas
precisam ser lidas antes da primeira foto — não depois.

**Uma foto mandada num Dogma é uma foto no notebook de alguém, e quem hospeda
pode vê-la.** Não é um defeito nem uma brecha: é como este produto funciona. Os
arquivos ficam numa pasta `anexos/` ao lado do banco de dados, em claro, na
máquina de quem hospeda. Em trânsito continua tudo cifrado, e no disco de quem
hospeda continua legível. Quem entra num Dogma tem de saber disso.

**O SEELE não varre vírus, e não vai varrer.** Ele responde a uma pergunta só
sobre um arquivo — *chegou inteiro?* — e é a mesma primeira pergunta desta
página, com a mesma resposta: a soma confere. As outras duas perguntas — de onde
veio, e se é bom — ele não responde sobre um arquivo que alguém mandou, porque
não há workflow nenhum que tenha produzido a foto de outra pessoa. **Não há
lista de extensões proibidas**, de propósito: uma lista é contornada com um
`rename`, quebra mandar a um amigo um build deste próprio projeto, e faz o que
passou parecer conferido — que é pior que não conferir nada.

O que ele faz: nenhuma tela do SEELE abre arquivo, em nenhum sistema. Salvar é
um ato seu, num lugar que você escolheu, e o arquivo é marcado com a quarentena
do próprio sistema ao ser gravado — a mesma marca que o navegador põe, e a que
faz o Gatekeeper e o SmartScreen pararem o arquivo na sua frente. Reentregar um
arquivo entre amigos aqui é como entregar um pendrive na mão, com a mesma
quantidade de conferência.

**Quem hospeda escolhe quanto disco isso pode ocupar.** O padrão é 1 GiB, e o
Dogma nunca passa disso: ao encher, o anexo mais antigo sai e a mensagem passa a
dizer que o arquivo expirou — o texto fica. Para mudar:

```sh
./seeled anexos 2G
```

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

## O que já foi validado, e o que não

**Duas máquinas reais, em rede local, já conversaram** — um Mac hospedando e um
Windows conectando, com voz por microfone de verdade. Isso deixou de ser
hipótese. O que aquele teste encontrou está em `docs/pendencias.md`, com nome e
número, e o que ele **não** cobriu continua valendo como aviso:

- **Fora da rede local, ainda não.** Alcançar um Dogma pela internet é assunto
  do ADR 0022, e não do que está nesta página.
- **Latência boca-a-ouvido nunca foi medida.** «Funcionou» não é medição.
- **Linux só foi compilado, não usado.** A integração contínua garante que ele
  constrói; ninguém falou por ele ainda.

O resto é o que roda sem hardware: a bateria de testes automáticos do
repositório, incluindo áudio simulado sob perda de pacote e um soak de dez
minutos em tempo simulado.

`docs/teste-duas-maquinas.md` é o roteiro para produzir as medições que faltam.
