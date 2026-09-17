# Notas da versão — v0.11.0

> **Esta página é o texto para o corpo do release**, no formato das publicadas.
> Ela fica no repositório porque a v0.11.0 ainda **não foi publicada**: a tag e
> a publicação são passo manual de quem opera, e estão descritos no fim.

_As mudanças de `v0.10.5-1` até `v0.11.0`. São 148 commits._

---

## ⚠️ Esta versão não fala com as anteriores

**Leia antes de atualizar uma máquina só.**

A `v0.10.5-1` publicada fala **protocolo 3**. A v0.11.0 fala **protocolo 6**, e
a janela de compatibilidade alcança uma versão para trás — 6 e 5. A v3 está
fora dela.

Conferido lendo o commit que gerou a release publicada (`12a6401`), e não
suposto: `PROTOCOL_VERSION = 3`, `COMPATIBILITY_WINDOW = 1`.

**Os dois lados se recusam, e é recusa mútua e simétrica:**

| quem tenta | o que acontece |
|---|---|
| cliente v0.10.5-1 → servidor v0.11.0 | recusado: `PeerTooOld` vira `Incompatible` |
| cliente v0.11.0 → servidor v0.10.5-1 | recusado: `PeerTooNew` vira `Incompatible` |

**O que a pessoa vê**, nos dois casos, é a mesma frase:

> VERSÃO INCOMPATÍVEL COM ESTE SERVIDOR.
> Um dos dois lados está desatualizado — atualize o SEELE nas duas máquinas e
> tente de novo.

A frase diz «um dos dois» e não «o seu» de propósito: o motivo chega à tela
como o mesmo `Incompatible` nas duas direções, e mandar *você* atualizar
erraria metade das vezes — justamente com quem já atualizou.

**Consequência prática: atualize todas as máquinas do grupo juntas.** Não há
caminho de convivência, e não é descuido — o `postcard` indexa variante por
posição e não é autodescritivo, então uma mensagem desconhecida não é ignorada:
ela desloca a leitura do fluxo de controle para sempre. Falar duas versões do
protocolo significaria manter dois conjuntos de tipos vivos.

---

## O que mudou

### A tela compartilhada pode vir de outra pessoa, e não só do servidor

O caminho entre pares. Quem assiste passa a poder receber a tela direto de quem
a transmite, com consentimento dos dois lados, e o servidor reassume sozinho
quando o par falha ou sai. Em sala com muita gente, é a subida de quem hospeda
que deixa de ser o teto.

### MODs saem do papel

O servidor passa a **rodar** os MODs habilitados, e não só a conhecê-los. O
anúncio e o aceite atravessam o protocolo: quem entra vê a lista — quem é o
MOD, de onde ele veio e o que ele alcança — antes de qualquer byte ser baixado,
e um MOD obrigatório recusado barra a entrada em vez de deixar a sessão meio
montada.

E entra a **ponte de pedidos**: um MOD pode perguntar ao servidor e receber uma
resposta que vai **só para quem perguntou**, em vez de sair pelo barramento que
todo mundo lê. A identidade de quem pergunta é escrita pelo servidor a partir da
sessão autenticada, nunca pelo pedido — um cliente que a forje não muda nada.

### Trocar de fone ou de microfone vale na hora

Era preciso reiniciar o aplicativo. O produto ignorava o aviso que o sistema já
mandava — ele contava «mais um erro» e jogava fora o tipo dele —, e o laço
seguia falando com um aparelho que já não era de ninguém.

Agora a troca feita **fora** do SEELE vale: a bandeja do Windows, as
Configurações do Mac, o fone puxado da tomada. A interface é avisada enquanto a
troca acontece, em vez de um aviso de falha local que apagava sozinho.

O som da **tela compartilhada** segue junto: ele abria uma vez e ficava preso ao
aparelho de quando a transmissão começou.

### Voltar depois de cair não apaga você

Uma conexão que morria depois de a pessoa já ter voltado por outra apagava o
estado da nova: a pessoa sumia do roster de todo mundo continuando a se ver
dentro da sala. Toda a saída passa a ser guardada por identificador de sessão.

E o fantasma do outro lado saiu junto: reconectar deixava uma cópia velha da
contabilidade de pé por cima da fotografia nova.

### Entrar numa sala com senha funciona, e o teto e a permissão passam a valer

A senha de sala de voz era declarada no protocolo, anunciada ao cliente e
**nunca conferida** — uma fechadura que se anuncia trancada e não está.

Junto dela, dois números que o produto anunciava e não cumpria: o **limite de
pessoas** que quem hospeda escreve passa a barrar de verdade, com o motivo
«sala cheia»; e a **permissão de entrar** passa a ser conferida pelo servidor,
e não só escondida na tela.

### Versões lado a lado

O núcleo do launcher entra ligado ao produto: o app lista as versões instaladas
nesta máquina e abre outra, já hospedando, com os dados dela em separado. É o
pedido de quem hospeda com MODs feitos para uma versão anterior.

**O que ainda não faz:** baixar uma versão que não está instalada. Isso precisa
do catálogo, que vem da rede, e continua sendo trabalho do botão de atualizar.
O que existe funciona offline.

### Um link com nome, em vez de um número que muda

Quem tem um nome apontado para a máquina pode pô-lo no link, e ele passa a
valer enquanto o nome valer. O endereço numérico vai junto, atrás: quem não
resolver o nome entra por ele do mesmo jeito.

A receita de DNS está em `docs/alcance-pela-internet.md`, com a ressalva que
importa — quem já entrou pelo endereço numérico vai ser perguntado de novo
sobre a impressão digital ao entrar pelo nome.

### UPnP: o link guardado volta a servir

Quando a porta externa 8383 estava ocupada por um mapeamento antigo do próprio
SEELE, o servidor desistia dela e aceitava qualquer porta — e o link guardado
por quem hospeda deixava de funcionar. Agora a porta canônica é tentada de novo
antes do recuo, e **quando o recuo acontece quem hospeda é avisado** de que o
link gerado pode não servir depois de reiniciar.

### A suíte de testes volta a ser evidência

A bateria de conformidade reprovava um teste por rodada, sempre um diferente,
sempre por prazo. Não era defeito de produto: cada teste levanta um servidor
QUIC de verdade, e os apertos de mão concorrentes estouravam o prazo do
transporte. A conformidade passa a ser serializada dentro do próprio crate,
sem serializar o resto.

E o job de teste da integração contínua, que nunca rodou um doctest sequer,
passou a rodar; as três conferências de regra do repositório, que existiam e
ninguém executava, passaram a rodar a cada push.

---

## Todos os commits desta versão

São 148, do mais recente ao mais antigo. O resumo curado está acima; esta lista
é para quem precisa achar o commit que mexeu numa coisa específica.

> Gerada com `git log --oneline v0.10.5-1..v0.11.0 --no-merges` na hora de
> publicar. Ela não está copiada aqui de propósito: uma lista copiada à mão é
> uma lista que fica velha entre o texto e a tag.

---

## O que baixar

**Um arquivo por sistema.** Ele traz as duas metades do SEELE: o cliente
gráfico e a ferramenta de terminal.

| sistema | baixe |
|---|---|
| **Windows** | `SEELE_<versão>_x64-setup.exe` |
| **macOS** | `SEELE_<versão>_universal.dmg` — Intel e Apple Silicon |
| **Linux** | `SEELE_<versão>_amd64.deb` — Debian, Ubuntu e derivados |

Dentro de cada um vão **`SEELE`**, o cliente gráfico, que tem um botão
**HOSPEDAR AQUI** e com o qual você nunca precisa abrir um terminal; e
**`seeled`**, o servidor, para quem quer um servidor no ar o tempo todo — só uma
das máquinas precisa dele.

Os outros arquivos desta página não são para instalar. O `SHA256SUMS` serve para
conferir que o download chegou inteiro; os `.sig` e o `latest.json` são como o
próprio SEELE se atualiza sozinho.

---

## O que esta versão **não** resolve

Dito aqui porque a página de release é onde se procura, e porque metade destes
tem relato de campo em aberto.

- **Compartilhar tela entre duas máquinas Windows** continua sem funcionar
  (pendência 33). Os quatro suspeitos foram eliminados por medida; o que sobra
  é o caminho de captura de verdade, que exige sessão gráfica para investigar.
- **O job `windows-2022` da integração contínua nunca rodou.** O workflow existe
  escrito e conferido, e ninguém o viu executar (pendência 38). Adiado por
  decisão explícita nesta rodada.
- **O caminho de Windows do som da tela não é compilado no Mac de quem fechou
  esta versão** (pendência 46). A parte que decide foi tirada de dentro do `cfg`
  e é testada; o que sobra sem compilação é uma chamada e um `match`.
- **O indexador de MODs não está no ar**, então nenhum MOD de terceiro tem como
  chegar a ninguém ainda.
- **Descer de versão não leva as conversas junto.** Cada versão tem o próprio
  diretório de dados, de propósito, e o produto diz isso na tela.

---

## Como esta versão foi montada, e o que ficou por provar

**Nada foi publicado.** Nenhum push, nenhuma tag, nenhum binário enviado. A tag
e a publicação são o passo manual descrito abaixo.

### O que foi exercitado de verdade, nesta máquina

- `./empacotar/macos.sh 0.11.0` — **saída 0**. Saíram
  `SEELE_0.11.0_aarch64.dmg` e `seele-cli-0.11.0-macos.tar.gz`.
- O `seeled` empacotado responde `seeled 0.11.0` a `--versao`, e o binário do
  app contém a cadeia `0.11.0` e **não** contém `connection/local` — que é a
  prova de que o carimbo de versão chegou às duas metades, e não só ao `seeled`.
  Era um defeito desta rodada: `SEELE_VERSAO` não alcançava o build do app.
- O `tauri.conf.json` voltou a `0.0.0` sozinho depois do empacotamento, como o
  `trap` do script promete. A versão é do artefato, nunca do repositório.

### O que esta máquina **não** pôde provar

- **O `.dmg` é só `aarch64`.** Um Mac Intel não o abre. O universal sai do
  workflow, que junta os dois com `lipo`.
- **Windows e Linux não foram empacotados.** Esta é uma máquina só.
- **Nada foi assinado nem notarizado**: a assinatura é ad-hoc (`-`), que faz a
  permissão de microfone grudar e **não** vale para o Gatekeeper. Sem
  `TAURI_SIGNING_PRIVATE_KEY` não saiu pacote de atualização — este `.dmg`
  serve para instalar à mão e não serve como origem de atualização.
- **O job `windows-2022` continua sem nunca ter rodado** (pendência 38).

### O passo manual, para quem for publicar

1. Confira que a árvore está limpa e que a validação está verde — os comandos e
   os números estão no fim desta página.
2. Crie a tag: `git tag -a v0.11.0 -m "v0.11.0"`.
3. Empurre-a: `git push origin v0.11.0`. É a tag que dispara o
   `release.yml`, e é dela que sai o número — `v0.11.0` vira `0.11.0` nos três
   instaladores.
4. Gere a lista de commits para o corpo do release:
   `git log --oneline v0.10.5-1..v0.11.0 --no-merges`.
5. Cole o texto desta página no corpo, e **não apague o aviso do topo sobre a
   incompatibilidade**: ele é a única coisa que separa uma atualização tranquila
   de um grupo inteiro sem conseguir se falar.
