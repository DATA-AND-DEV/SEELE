# API 8 e o MOD de multiplayer — desenho

**Data:** 2026-10-29 · **Estado:** rascunho para aprovação do dono
**Parte de:** v0.18.1 publicada (`ae0f2715`), que roda a API 7 (`APIS_POR_LINHA`
`("2", [7, 6])`, `crates/seele-proto/src/mods.rs`).

Este documento junta dois trabalhos que nascem juntos e se entregam em ordem: a
**API 8**, que dá a um MOD poderes que nenhuma API deu até aqui, e o **primeiro
MOD que os usa**, um gerenciador de servidores de jogos multiplayer. O que foi
decidido pelo dono está marcado como dele; o que é escolha minha está marcado
**[escolha do desenho]**, para ele poder recusar.

## Por que existe

Para jogar em grupo um jogo que pede servidor (Minecraft, por exemplo), um grupo
de amigos hoje escolhe entre três caminhos ruins:

- **uma VPN de jogo**, como o Radmin, que pede instalação, conta e a mesma rede
  virtual para todos;
- **um servidor pago**, na faixa de R$ 40 por mês;
- **um servidor gratuito de terceiro**, com lag e, muitas vezes, sem suporte
  aos mods do jogo.

O grupo que usa o SEELE já resolveu a parte difícil: as máquinas dele já se
alcançam pela internet, pela escada de UPnP, IPv6 e furo de NAT
([`docs/alcance-pela-internet.md`](../../alcance-pela-internet.md)), sem servidor
no meio. O MOD aproveita essa ligação para carregar o tráfego do jogo.

**O sucesso, para o dono:** quem hospeda o SEELE instala um MOD, escolhe o jogo,
escolhe em que máquina ele roda, configura pela tela do SEELE, e os amigos
entram no jogo sem instalar VPN, sem pagar e sem depender de um serviço de fora.
Funciona igual para o jogo puro e para o jogo com mods.

## Objetivos

1. **Um MOD de multiplayer, e não um MOD de Minecraft** (dono). Ele abre com um
   **seletor de jogos**; o Minecraft é o primeiro, e a coleção cresce com o
   tempo e com a comunidade (Hytale, Terraria e outros que pedem servidor).
2. **Mais de um jogo ao mesmo tempo** (dono). Cada servidor de jogo é um programa
   próprio, com a sua porta.
3. **A pessoa escolhe a máquina** em que o servidor de jogo roda — a de quem
   hospeda o SEELE ou a de outro membro (dono).
4. **Ninguém hospeda na máquina de outro sem a permissão dele** (dono). A
   pessoa dona da máquina aceita, na máquina dela, antes de qualquer coisa ser
   baixada ou rodar, e pode desligar a qualquer momento.
5. **Vanilla ou com mods**, escolhido no topo da configuração (dono). Com mods,
   o pack vem do **Modrinth** (`.mrpack`), por link ou por arquivo exportado do
   app do Modrinth (dono aprovou o Modrinth; os dois caminhos de entrada são
   [escolha do desenho]).
6. **A configuração básica do servidor fica na tela do SEELE**, dentro da
   configuração do MOD (dono): máximo de jogadores, nome do servidor e a cor
   dele, dificuldade, modo, PvP, e o que mais for configuração básica do jogo.
7. **Uma máquina reserva** (dono): uma segunda máquina pode assumir o servidor
   de jogo quando a principal sai. Perder os últimos minutos numa queda de
   surpresa é aceitável (dono, 29/10).
8. **O poder é do MOD, e não do SEELE** (dono). A API 8 dá ao MOD os poderes
   novos; o SEELE fornece o mecanismo, as regras e os portões.
9. **Um MOD pode declarar que não roda no navegador** (dono). Quem entra pelo
   navegador nem baixa a metade de cliente desse MOD, e a tela diz que ele só
   funciona no app.

## Fora do escopo

- **Divisão de carga de verdade.** Um servidor de jogo é um programa dono de um
  mundo em disco; duas máquinas não servem o mesmo mundo ao mesmo tempo. O
  objetivo 7 é reserva, e não balanceamento. (Explicado ao dono em 29/10.)
- **Relay.** Quem o SEELE não conecta, o MOD também não conecta. Aceito pelo
  dono (decisão de 26/10 sobre o relay, reafirmada em 29/10).
- **Jogar pelo navegador.** O jogo precisa de uma porta local na máquina de quem
  joga, e o navegador não abre porta. O túnel é só do app de computador.
- **Instalar os mods no Minecraft de quem joga.** O SEELE entrega o mesmo
  `.mrpack` do servidor, e a pessoa o importa no app do Modrinth.
  **[escolha do desenho]**: o dono não respondeu a essa pergunta; é o menor
  passo que já faz os dois lados baterem.
- **Jogos cujo multiplayer não passa por uma porta de rede**, como os que usam
  o P2P da Steam por dentro. Não há o que tunelar. O seletor não os oferece.

## O que não pode acontecer (anti-objetivos)

- **Dois mundos que se separam calados.** Se a principal cai do SEELE mas segue
  rodando, e a reserva liga também, o grupo joga em dois mundos diferentes sem
  saber. Isso é pior que ficar fora do ar. Só uma máquina roda um servidor de
  jogo por vez, e quem decide é o anfitrião do SEELE.
- **Um programa rodando numa máquina sem o dono dela saber.** Nenhum download e
  nenhum processo antes do aceite na própria máquina; o aceite diz o que vai
  rodar, de onde vem e quanto ocupa.
- **«O produto sabe e não conta»** (CLAUDE.md). Download que falhou, hash que não
  bateu, porta ocupada, Java faltando, servidor que caiu, reserva que assumiu,
  perda de minutos: cada um aparece na tela de quem precisa saber, e no log.
- **O mesmo arquivo de identidade alcançável pelo jogo.** A pasta do jogo nunca
  é a pasta onde moram `identity.key`, `pins` e o banco (a regra que o guia de
  MODs já dá para o `dados`).

## O que existe hoje, e que o desenho usa

- **Ligação:** WebRTC desde a 0.17 (ADR 0063), em **estrela**: cada membro liga
  à máquina de quem hospeda o SEELE. Já há DataChannels além da voz
  (`crates/seele-core/src/transporte.rs`).
- **A malha:** o compartilhamento de tela já apresenta dois membros um ao outro
  (`SirvaTelaPara`, `crates/seele-server/src/pares.rs`), cada um confere a
  identidade do outro, e a ligação direta tem a estrela como reserva. A regra
  escrita: *«A malha é alívio, nunca dependência»*. **Hoje ela só nasce do
  compartilhamento de tela**: o túnel de jogo precisa generalizar a apresentação
  (`SirvaTelaPara`) para um segundo motivo, e não apenas reusá-la.
- **MODs:** JavaScript num QuickJS isolado
  ([`docs/como-se-faz-um-mod.md`](../../como-se-faz-um-mod.md)). A metade de
  **cliente** roda em cada máquina que entra, sem rede e sem disco; a metade de
  **servidor** roda na máquina de quem hospeda o SEELE, com `mundo.buscar`
  (teto de 2 s por busca e de 4 MiB por resposta) e uma pasta própria (4 MiB
  por arquivo).
- **Formas de tela:** a API 7 já tem `abas`, `escolha`, `numero`, `interruptor`,
  `cor`, `campo`, `formulario`, `deslizante` e `distintivo`. A tela de
  configuração **não precisa de forma nova**.
- **Permissões de MOD:** a API 7 deixa o MOD declarar ações em
  `manifesto.permissoes`, e os cargos (ADR 0064) decidem quem pode cada uma.
- **Envio de arquivo:** `SeeleUI.enviar` aceita só imagem, até 10 MiB, e não
  existe no navegador.
- **O celular é a casca web** (Safari/PWA, `seele-web`), e não um app nativo:
  para este desenho, celular é navegador.
- **Quem hospeda pode ser o `seeled`**, sem tela (`crates/seele-server`, binário
  `seeled`).

## A API 8

A 8 é um **alargamento da 7**, como a 7 foi da 6: a v7 inteira, mais o que segue.
Ela entra **na linha 2**, que passa a rodar `[8, 7, 6]` (`APIS_POR_LINHA`), sem
linha nova: o ADR 0058 só pede linha nova para mudança que não é acréscimo. Um
app que não roda a 8 recusa o manifesto da 8 como `ApiTooNew`, ou como
`Malformed` se ele traz as chaves novas, do mesmo jeito que a 0.16 recusa a 7.
(Corrigido em 29/10 ao planejar; a primeira versão dizia «linha nova».)

### 1. `plataformas` no manifesto

```json
"plataformas": ["app"]
```

- Valores: `"app"` (o app de computador) e `"navegador"`, que inclui o celular,
  porque o celular é a casca web. Ausente quer dizer as duas, que é o
  comportamento de hoje. Um valor desconhecido é recusado, para um app nativo de
  celular futuro poder ganhar o próprio valor sem ser confundido com estes.
- **A marca viaja no catálogo** que a janela pede ao servidor (o JSON de
  `seele-server/src/mods/pedidos.rs`), e não no `ModAnunciado`: campo novo
  numa mensagem do fio nunca entra (ADR 0058), e campo novo nesse JSON não toca
  no protocolo. O aceite do conjunto não muda.
- Quem entra por uma plataforma fora da lista **não baixa** a metade de cliente.
  A tela de MODs mostra o MOD com a frase «só funciona no app», e o MOD não
  conta como falha. Até as plataformas viajarem antes da entrada (próximo
  plano), isto vale depois de entrar: durante o aceite o navegador pode baixar
  o pacote sem executá-lo, porque o anúncio ainda não as carrega.
- Declarar `poderes` (abaixo) **exige** `"plataformas": ["app"]`. Um manifesto
  com poderes e sem essa restrição é recusado na instalação, com o motivo
  escrito.
- **O que fica com o dono:** saber se os MODs de aparência e de perfis
  funcionam no navegador. Ninguém mediu. Medir antes de marcar qualquer MOD
  oficial.

### 2. `poderes` no manifesto, e a metade de máquina

Hoje um MOD tem duas metades. A 8 acrescenta uma terceira, **a metade de
máquina** **[escolha do desenho: nome e corte]**, porque o servidor de jogo pode
rodar no computador de um membro que não é quem hospeda o SEELE, e nenhuma das
duas metades atuais roda lá com os poderes certos.

```json
"maquina": "maquina/main.js",
"poderes": ["porta", "programa"]
```

- **A metade de máquina só roda numa máquina cujo dono aceitou**, nela, pelo
  portão do produto (seção 4). Sem aceite, ela não existe ali.
- Ela roda no mesmo executor QuickJS das outras duas, com os mesmos limites de
  tempo e de memória. Os poderes chegam como objetos novos no contexto dela, e
  não existem nas outras metades.
- **`porta`** — abrir um túnel de jogo (seção 3).
- **`programa`** — baixar artefatos e rodar processos (seção 5).
- Conversa com a metade de servidor pelo mesmo caminho de pedido e aviso que
  a metade de cliente já usa.

### 3. O túnel de jogo (poder `porta`)

```js
const tunel = await maquina.porta.servir({ nome: "minecraft-1", protocolo: "tcp", porta: 25565 });
```

- **Na máquina do jogo:** o túnel liga a uma porta local, onde o servidor de
  jogo escuta.
- **Em cada máquina de quem joga:** o app abre uma porta em `127.0.0.1` e mostra
  qual é. Na primeira vez ele tenta a mesma porta do jogo (25565); se ela estiver
  ocupada, usa a próxima livre e a tela diz qual. O jogador entra no jogo por
  `localhost:<porta>`.
- **O caminho:** primeiro direto entre a máquina de quem joga e a máquina do
  jogo, pela malha. Se a ligação direta não abre, o tráfego vai pela estrela,
  passando pela máquina de quem hospeda o SEELE. A malha é alívio, nunca
  dependência.
- **Só quem está no servidor SEELE alcança o túnel**, e quem pode é decidido
  pelos cargos (seção 7). A porta local só escuta em `127.0.0.1`, e quem joga a
  abre pela seção 6.
- **TCP e UDP.** O Minecraft Java é TCP; outros jogos usam UDP. O UDP vai num
  DataChannel sem retransmissão e sem ordem, e o TCP num DataChannel confiável e
  ordenado. **[escolha do desenho]**
- **O túnel conta:** latência medida, caminho atual (direto ou pela estrela),
  bytes por segundo e o motivo de cada queda. Isso vai para a tela do MOD e para
  o log.
- Mais de um túnel ao mesmo tempo, um por servidor de jogo (objetivo 2).

### 4. O aceite da máquina

O portão é do produto, e não do MOD, porque é ele quem protege quem não instalou
o MOD.

- A metade de servidor pede «rodar o servidor de jogo X na máquina de Fulano».
- A máquina de Fulano mostra, numa tela do produto e não do MOD:
  - qual MOD pede, e quem no servidor pediu;
  - o que vai ser baixado (nome, origem, tamanho total) e o que vai rodar (o
    programa e os argumentos);
  - quanto disco ocupa, e onde;
  - os termos do jogo que a pessoa precisa aceitar, como o **EULA da Mojang**.
    Quem aceita é a pessoa, e não o SEELE nem o MOD.
- **Aceitar vale para aquele servidor de jogo naquela máquina**, e não para o
  MOD inteiro. Um servidor novo pede aceite novo.
- **O aceite prende o que foi mostrado** **[escolha do desenho]**. O produto
  guarda, com a autorização: a receita e a versão dela, a versão do jogo, o
  pack (pelo hash), a lista de domínios de origem, o comando e os argumentos
  fora do esquema de configuração, e o teto de disco.
  - **Mudar qualquer um desses pede aceite novo**, e até lá o servidor de jogo
    não liga com a mudança.
  - **Mudar um campo do esquema de configuração não pede** (jogadores, nome,
    cor, dificuldade, modo, PvP): esses viram texto num arquivo de configuração,
    e não mudam o que roda.
  - `programa.baixar` recusa URL fora dos domínios aceitos, e `programa.rodar`
    recusa comando fora do aceito. A recusa é contada na tela do MOD.
- **Quem hospeda pelo `seeled`, sem tela, não pode ser máquina de jogo** na
  primeira entrega **[escolha do desenho]**: não há tela onde aceitar. A máquina
  de jogo é sempre alguém com o app de computador.
- **Desligar a qualquer momento**, por um botão do produto, que encerra o
  processo e o túnel e avisa a metade de servidor.
- **Revogar** apaga a autorização. Apagar os arquivos baixados é uma escolha à
  parte, com o tamanho dito.

### 5. Baixar e rodar (poder `programa`)

```js
await maquina.programa.baixar({ url, sha512, tamanho, destino: "server/mods/x.jar" });
const proc = await maquina.programa.rodar({ comando: "java/bin/java", argumentos: [...], pasta: "server" });
```

- **Assíncrono, fora do momento do servidor.** O `mundo.buscar` é síncrono, com
  teto de 2 s e de 4 MiB, e não serve para centenas de MB. O download corre em
  segundo plano e avisa progresso por evento.
- **Todo download do MOD tem hash e tamanho declarados antes.** Valem `sha1`,
  `sha256` e `sha512`, porque é o que as origens publicam (a Mojang publica
  `sha1` do `server.jar`, a Adoptium publica `sha256` do Java, e o `.mrpack`
  traz `sha1` e `sha512`); vale o mais forte que a origem der **[escolha do
  desenho]**. Hash que não bate é apagado e contado.
- **O que o próprio jogo baixa sozinho não passa por aqui.** Os instaladores de
  loader (Fabric, Forge, NeoForge) buscam bibliotecas por conta própria na
  primeira vez. O SEELE não confere esses arquivos. A receita declara esses
  domínios, e o aceite os mostra como «o jogo baixa por conta própria de: …».
- **Tudo mora numa pasta por servidor de jogo, dentro da área do MOD**, longe da
  pasta de identidade. Caminho absoluto e `..` são recusados, como no `dados`.
  Um teto de disco é dito no aceite.
- **O processo roda com o usuário da máquina, e o aceite diz isso** (dono,
  29/10: «avisar e isolar depois»). O aceite diz, com estas palavras ou mais
  claras: o servidor do jogo roda com a sua conta deste computador, e pode ler o
  que ela lê; e o jogo baixa por conta própria de tais domínios, sem o SEELE
  conferir. Isolar o processo no sistema operacional (um usuário próprio ou uma
  sandbox, por sistema) é uma entrega seguinte, e não bloqueia o passo 4.
- O MOD lê a saída do processo (para saber quando o jogo está pronto, e quem
  entrou), manda texto para a entrada dele (comandos do console, como
  `save-all`) e sabe quando ele termina e com que código.
- O produto encerra todo processo do MOD quando o MOD é desligado, a autorização
  é revogada, ou o app fecha.

### 6. O lado de quem joga

Quem joga não tem metade de máquina nem aceite, e precisa de duas coisas que a
metade de cliente hoje não pode **[escolha do desenho]**:

- **`SeeleUI.tunel.entrar(nome)`** — pede ao produto que abra a porta local
  daquele túnel nesta máquina, e resolve com a porta escolhida, para a tela
  mostrar. Só funciona dentro de um evento de clique da pessoa (o mesmo gesto
  que hoje escolhe um arquivo), e só se a pessoa tem a ação `entrar-no-jogo`.
  A porta fecha quando a pessoa sai do servidor, desliga o MOD ou fecha o app.
  O produto mostra, fora da tela do MOD, que há uma porta aberta e para qual
  jogo.
- **`SeeleUI.salvar(arquivo)`** — entrega a quem joga um arquivo que a metade
  de servidor guardou (o `.mrpack`), pelo diálogo de salvar do sistema. O MOD
  nunca escolhe o caminho.

Os dois só existem para um MOD com o poder `porta` e com
`"plataformas": ["app"]`.

### 7. Quem pode o quê

Pelas `permissoes` da API 7 e pelos cargos do ADR 0064. **[escolha do desenho]**
As ações que o MOD declara:

| ação | quem costuma ter |
|---|---|
| `criar-servidor-de-jogo` | administradores |
| `configurar-servidor-de-jogo` | administradores |
| `ligar-desligar-servidor-de-jogo` | administradores e quem o dono quiser |
| `entrar-no-jogo` | `@everyone` (abrir o túnel do seu lado) |

Ser dono da máquina **não** se compra com cargo: o aceite da seção 4 vale acima
de qualquer cargo, incluindo o Administrador e o proprietário do servidor SEELE.

### 8. Enviar o `.mrpack`, e como ele chega à máquina do jogo

**[escolha do desenho]**

- `SeeleUI.enviar` passa a aceitar, na 8, o tipo
  `application/x-modrinth-modpack+zip` (conferido pelos bytes: um zip com
  `modrinth.index.json` na raiz). O guia da 7 diz «imagem, até 10 MiB»; a 8
  escreve a lista de tipos por extenso, cada um com o seu teto.
- **O pack vai para a metade de servidor**, no volume, e não no `dados` (que tem
  teto de 4 MiB por arquivo). Ela é quem conhece o servidor de jogo e quem o
  devolve a quem joga.
- **A máquina do jogo o busca da metade de servidor** pela estrela, pelo mesmo
  fluxo de volume, e confere o hash que ficou no aceite.
- **Teto do pack: 64 MiB** na primeira entrega. Um `.mrpack` só com a lista de
  mods tem poucos KiB; o que pesa são os `overrides`. Um pack acima do teto é
  recusado com o tamanho e o teto, e a tela sugere usar o link do Modrinth.
- Pelo link do Modrinth, nada passa pelo envio: a máquina do jogo baixa o
  `.mrpack` direto do `cdn.modrinth.com`, com o hash que a API do Modrinth dá.

## O MOD de multiplayer

### As peças

- **Metade de cliente:** o seletor de jogos, a configuração, o estado de cada
  servidor de jogo e o botão «entrar» de quem joga.
- **Metade de servidor:** a lista de servidores de jogo, a configuração de cada
  um, quem roda onde, e a regra de uma máquina por vez.
- **Metade de máquina:** baixa, instala, roda e vigia o servidor de jogo, e
  copia o mundo para a reserva.
- **Receitas:** uma por jogo, que dizem como aquele jogo é hospedado.

### As receitas, e a comunidade

Cada jogo suportado é uma **receita**: um arquivo de dados dentro do pacote do
MOD. **[escolha do desenho]**: dados e não código, para uma receita da
comunidade não carregar código novo; e dentro do pacote, para passar pela mesma
avaliação do indexador que o MOD passa. A comunidade contribui por pull request
no repositório do MOD.

Uma receita declara:

- nome, ícone e versões suportadas;
- o que baixar (servidor do jogo e runtime, como o Java), com URL, hash e
  tamanho, por versão; e de quais domínios um pack pode baixar;
- o comando de início, a porta e o protocolo;
- **o esquema da configuração**: cada campo (nome, forma da API 7, valores
  aceitos, padrão) e como ele vira o arquivo de configuração do jogo (no
  Minecraft, o `server.properties`). A tela é montada a partir disso;
- os termos que a pessoa da máquina precisa aceitar;
- **onde fica o mundo salvo e como salvar com segurança** (no Minecraft:
  `save-off`, `save-all flush`, copiar, `save-on`). Obrigatório: é o que torna
  a reserva possível;
- como saber que o servidor está pronto e quem entrou (o padrão de texto na
  saída).

### Minecraft, a primeira receita

- **Vanilla:** a pessoa escolhe a versão; a receita baixa o `server.jar` oficial
  e o Java que aquela versão pede.
- **Com mods:** a pessoa cola o link de um modpack do Modrinth ou escolhe um
  `.mrpack` exportado do app do Modrinth. O MOD lê o `modrinth.index.json` (o
  formato foi conferido na especificação oficial em 29/10):
  - `dependencies` dá a versão do Minecraft e o loader (`fabric-loader`,
    `forge`, `neoforge` ou `quilt-loader`). Chave desconhecida é recusada com o
    nome dela, e não ignorada;
  - de `files`, só entram os que têm `env.server` diferente de `unsupported`,
    cada um conferido pelo `sha512`, e só de `cdn.modrinth.com`, `github.com`,
    `raw.githubusercontent.com` e `gitlab.com`;
  - `path` com `..` ou absoluto é recusado;
  - aplicam-se `overrides` e depois `server-overrides`;
  - instala-se o loader de servidor que o pack pede.
- **Quem joga** recebe pela tela o mesmo `.mrpack`, para importar no app do
  Modrinth, e a porta local para entrar.

### A tela

Desenhada com o dono no navegador (`.superpowers/brainstorm/`, versão
`config-layout-v2`):

1. **O seletor de jogos** vem antes: a lista de receitas, e os servidores de
   jogo que já existem, com o estado de cada um.
2. **A configuração de um servidor de jogo** é uma página com abas (o dono
   escolheu a opção A): **GERAL · MODS · JOGADORES · MÁQUINA**.
   - No topo de GERAL, **Vanilla** e **Com mods** como duas opções grandes, lado
     a lado. Mais espaço entre os blocos que no primeiro rascunho (pedido do
     dono).
   - Abaixo: o modpack, o nome com cor, o máximo de jogadores, a dificuldade, o
     modo, o PvP e «só entra quem está neste servidor SEELE».
   - Em MÁQUINA: a principal, a reserva, e o estado do aceite de cada uma
     («esperando o aceite dele»).
3. **Quem joga** vê um cartão por servidor de jogo: ligado ou não, quantos
   estão dentro, a porta local para entrar e o pack para baixar.

### A reserva

- Cada servidor de jogo tem uma **principal** e, se quiser, uma **reserva**.
  As duas aceitaram (seção 4).
- **A metade de servidor guarda quem é a dona agora.** Só a dona roda o
  processo. A reserva só liga quando a metade de servidor manda.
- **Cópias do mundo:** a principal salva com segurança (pela receita) e manda o
  mundo para a reserva de tempos em tempos, pelo mesmo caminho do túnel. O
  intervalo é configurável, com um padrão de 5 minutos **[escolha do desenho]**.
- **Desligamento avisado:** salvar, mandar a cópia final, a reserva liga, a
  principal desliga. Nada se perde; os jogadores reconectam no mesmo
  `localhost:<porta>`.
- **Queda de surpresa:** quando a principal some do SEELE por mais que um prazo
  curto **[escolha do desenho: 60 s]**, a metade de servidor passa a posse à
  reserva, que liga com a última cópia. A tela diz a todos quanto tempo de jogo
  se perdeu.
- **A principal que volta não liga sozinha.** Ela pergunta à metade de servidor
  quem é a dona. Se não é ela, recebe a cópia atual da reserva antes de voltar a
  ser principal. Se a principal ficou fora do SEELE mas o processo dela seguia
  rodando, ela o encerra assim que reconecta e vê que perdeu a posse. Esse caso
  é contado na tela, nunca resolvido em silêncio.
- **Sem quem hospeda o SEELE, nada muda de dono**: a decisão é dele, e sem ele
  não há quem decida. **A dona atual continua rodando** **[escolha do desenho]**:
  como a reserva só liga por ordem da metade de servidor, ninguém mais pode
  ligar, e a regra de uma dona por vez segue de pé. Quem joga por ligação
  direta (malha) continua jogando; quem passava pela estrela cai, e a tela diz
  por quê. Quando quem hospeda volta, a dona continua sendo a mesma.

## Como provar que funciona

O CLAUDE.md pede: provar cada guarda revertendo o conserto e vendo o teste
falhar.

- **Túnel de ponta a ponta:** um teste de conformidade com duas máquinas
  simuladas e um eco TCP e UDP pelo túnel, pelo caminho direto e pela estrela.
- **O aceite:** sem aceite, nenhum `baixar` e nenhum `rodar` acontece; o teste
  derruba o portão e vê o download acontecer. Mudar a versão ou o pack depois do
  aceite não liga o servidor até o aceite novo; URL fora dos domínios aceitos é
  recusada.
- **A porta de quem joga:** `SeeleUI.tunel.entrar` fora de um clique é
  recusado, e a porta fecha quando a pessoa sai do servidor.
- **Sem quem hospeda:** a dona segue rodando, e a reserva não liga.
- **Hash:** um artefato adulterado é recusado e apagado.
- **Pasta:** `..` e caminho absoluto recusados em `baixar` e em `rodar`.
- **Uma dona por vez:** o teste da principal que volta com o processo ainda
  rodando, e o que mede que só um processo fica de pé.
- **`plataformas`:** quem entra pelo navegador não baixa a metade de cliente do
  MOD com `["app"]`; o manifesto com poderes e sem a restrição é recusado.
- **Minecraft de verdade:** um servidor Vanilla e um com um `.mrpack` pequeno
  (Fabric), um cliente entrando por `localhost` através do túnel, entre duas
  máquinas reais. O que não for medido entre duas casas é dito como não medido.
- Os testes do túnel e do processo **rodam no Mac e no Windows**, e não dentro
  de um `#![cfg(windows)]` que nunca roda aqui.

## A ordem de entrega

1. `plataformas` no manifesto, e a medida de quais MODs oficiais funcionam no
   navegador.
2. O túnel de jogo (poder `porta`) e o lado de quem joga
   (`SeeleUI.tunel.entrar`), provados com um eco, pela malha e pela estrela.
   Inclui generalizar a apresentação da malha para além da tela.
3. A metade de máquina e o aceite da máquina.
4. O poder `programa`: baixar com hash e rodar processo.
5. O MOD de multiplayer com a receita do Minecraft Vanilla.
6. O Minecraft com mods (`.mrpack`), o envio do pack e `SeeleUI.salvar`.
7. A reserva.
8. A segunda receita, para provar que o formato não é o Minecraft com outro
   nome.
9. Depois: isolar o processo do jogo no sistema operacional, e tirar do aceite
   o aviso de que ele roda com a conta da pessoa.

## O que fica com o dono

- **Quais jogos vêm depois do Minecraft.** Hytale e Terraria são candidatos; o
  Risk of Rain 2 usa o P2P da Steam, e não teria o que tunelar.

## O que fica com quem constrói

- O nome dos objetos no JS (`maquina.porta`, `maquina.programa`) e das recusas
  novas, seguindo o idioma de `api/v7.json`.
- O teto de tamanho do `.mrpack`, e o de disco por servidor de jogo.
- Como a cópia do mundo trafega (inteira ou só o que mudou), desde que a
  principal salve com segurança antes.
- O formato exato do arquivo de receita.
