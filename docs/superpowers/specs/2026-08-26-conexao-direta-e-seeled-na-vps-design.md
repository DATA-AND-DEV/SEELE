# Conexão direta: a escada sai, e o `seeled` vira produto

**Data:** 2026-08-26
**Estado:** aguardando revisão

Uma mudança de escopo, e não um conserto. O pedido, na palavra de quem pediu:

> *«A VPS vai sair do nosso escopo. Ele vai ser um serviço que o usuário vai
> precisar hospedar, ou usar uma VPN a la Tailscale para usar. Nosso objetivo é
> focar agora 100% na melhoria na comunicação dos dados, seja via VPS particular
> com relay, ou LAN.»*

Este documento é a limpeza que esse foco exige. Ele não melhora a comunicação de
dados — ele remove o que está no caminho dela.

## 0 · O que este documento revoga

**O ADR 0022 inteiro.** A escada de alcançabilidade — IPv6 direto, UPnP/NAT-PMP,
furo de NAT com ponto de encontro — deixa de existir. Não vira caminho
secundário, não fica atrás de uma bandeira: sai.

**E revoga um spec desta casa, de seis dias atrás.**
`2026-08-20-conectividade-p2p-design.md` diagnosticou corretamente que *«o
degrau 4 do ADR 0022 está construído, declara sucesso, e não conecta»*, e
consertou o relógio entre o aviso e o aperto de mão. O conserto funcionou. O que
mudou não é a qualidade dele: é que o degrau que ele conserta deixou de ser
necessário.

Vale registrar isso sem eufemismo, porque é trabalho recente sendo apagado. A
razão é econômica e está no relatório de campo do dia 25: numa VPS o anfitrião
tem IP público e escuta direto, e **toda** a escada existe para o caso em que ele
não tem. Trocar "o projeto opera um ponto de encontro para todo mundo" por "cada
pessoa hospeda o seu servidor, ou usa a própria VPN" apaga o problema em vez de
resolvê-lo melhor.

**O que este documento não revoga:** o ADR 0003 (TOFU), o ADR 0006 (esquema de
URI — é emendado, não revogado), o ADR 0002 (regra de dependência), e o
`docs/teste-duas-maquinas.md`, que continua sendo a validação que separa promessa
de produto.

## 1 · As decisões, e o que cada uma custa

Cinco, tomadas na conversa de 26/08:

| # | Decisão | Consequência |
|---|---|---|
| 1 | A escada sai inteira — só endereço direto | ~3.200 linhas em `alcance/`, 3 dependências |
| 2 | IPv4 puro, em todo lugar | `Pilha` e pilha dupla somem; VPS só-IPv6 deixa de servir |
| 3 | Hospedar no app fica, mas escolhe **um** endereço | a tela ganha uma lista de interfaces |
| 4 | O convite leva um endereço e uma impressão digital | `alt=` e `enc=` morrem; a corrida de candidatos morre |
| 5 | Administração no fio fica para o próximo spec | o ADR do dono é escrito aqui, não implementado |

A decisão 4 nasceu de uma pergunta que derrubou a 3 como ela tinha sido
aprovada: *«se o plano é fazer virar produto de hospedagem própria, não precisa
só do IP ou URL amigável?»* Precisa. A decisão 3 foi revista no mesmo instante.

## 2 · A poda, inventariada

O que sai, com tamanho, para que ninguém descubra o custo no meio:

| Alvo | Tamanho | Por quê |
|---|---|---|
| crate `seele-encontro` | binário + testes + exemplo | é o ponto de encontro |
| `seele-proto/src/encontro.rs` | vocabulário de fio | ninguém mais fala isso |
| `seele-server/src/alcance/encontro.rs` | 1.603 linhas | degrau 4 |
| `seele-server/src/alcance/pcp.rs` | 933 linhas | degrau 3 (NAT-PMP) |
| `seele-server/src/alcance/porta.rs` | 666 linhas | degrau 3 (UPnP) |
| `seele-core/src/encontro.rs` | 618 linhas | degrau 4, lado do cliente |
| `seele-tui/src/rede.rs` | diagnóstico `--rede` | construído sobre o ponto de encontro |
| `Bilhete` / `enc=` em `uri.rs` | 46 ocorrências | degrau 4 no link |
| corrida de candidatos em `enlace.rs` | ~180 linhas + 5 constantes | não há mais lista |
| `alt=` em `uri.rs` | `LIMITE_DE_ALVOS` e o que o cerca | idem |

Dependências que saem do `seele-server/Cargo.toml`: **`crab_nat`** (PCP),
**`igd-next`** (UPnP), **`netdev`** (achar o roteador padrão). O `socket2`
perde a razão declarada no comentário dele — `IPV6_V6ONLY` — e fica só se sobrar
outro uso; conferir na fase, não presumir.

O que **fica inteiro e não é tocado**:

- `alcance/interfaces.rs` (479 linhas). A enumeração e a classificação
  Física/Túnel/Virtual são boas, são a base do que sobra, e não têm nada a ver
  com a escada.
- `alcance/firewall.rs` (247 linhas). Detectar o firewall do Windows continua
  valendo — quem hospeda em LAN num Windows é exatamente o caso de teste que
  motivou esta conversa.

## 3 · O que fica no lugar da escada

### A descoberta que torna isto barato

O enum `Tipo` (`alcance.rs:440`) classifica candidatos em sete variantes. Elas
já têm a forma do escopo novo:

| Variante | O que é | Destino |
|---|---|---|
| `Local` | IPv4 da rede de casa | **fica** — é a LAN |
| `Global` | IPv4 público na placa | **fica** — é a VPS |
| `Tunel` | interface ponto-a-ponto | **fica** — é o Tailscale |
| `Ponte` | adaptador virtual | **sai** — ver abaixo |
| `GlobalLiberado` | degrau 2 | sai |
| `PortaNoRoteador` | degrau 3 | sai |
| `Refletido` | degrau 4 | sai |

Local, Global e Túnel **são** LAN, VPS e Tailscale, literalmente. As três que
saem são as três que dependiam de pedir alguma coisa a alguém. Isto muda a
natureza do trabalho: é poda, não reescrita.

### `Ponte` sai junto, e não estava no pedido

É o adaptador virtual: WSL, Hyper-V, Docker, VirtualBox. Num Windows típico são
dois ou três. Eles entram no convite hoje, ocupam vaga de um limite de quatro
(`uri.rs:79`), e são endereços que **nunca** respondem de outra máquina.

Não é hipótese ociosa: no relato de campo de 25/08 — Windows hospedando, Mac
entrando, tempo esgotado nas primeiras tentativas — um PC com WSL pode ter
gasto duas das quatro vagas com `172.x` que não levam a lugar nenhum. Com a
decisão 4 o limite vira um, e um `Ponte` escolhido por acidente seria o link
inteiro.

### A forma nova

```
Escada + Alcance + Escuta + Pilha + Degrau(6)   →   Enderecos + Degrau(3)
~2.500 linhas, async, 3 deps de rede                ~250 linhas, síncrono, 0 deps
```

`Escada::subir` é assíncrona porque fala com roteador e com ponto de encontro.
Sem os dois, ela deixa de ser uma escalada e vira uma pergunta síncrona: *quais
IPv4 desta máquina servem para receber gente, e em que ordem oferecê-los?*

`Degrau` colapsa de seis frases para três, pelo critério que ele já usa — **o
que a pessoa faz a respeito é diferente em cada uma**:

- `EnderecoDireto` — há IPv4 global na placa. Não há nada a fazer.
- `RedeLocalOuVpn` — o que sai daqui é de um túnel. A resposta é pôr os dois
  lados na mesma VPN.
- `SoRedeLocal` — só quem estiver na mesma rede. A resposta é hospedar noutro
  lugar.

## 4 · O convite: um endereço e uma impressão digital

`seele://alvo[:porta]?fp=…[&token=…][&voice_room=…]`

**`alvo` aceita nome, e isso já funciona hoje.** `seele-ffi/src/lib.rs:2603`
resolve com `to_socket_addrs()` e, quando o alvo é um nome e não um IP, usa-o
também como `server_name` do TLS. `seele://seele.meudominio.com.br:8383` conecta
sem mudança nenhuma. O que falta é documentar e recomendar.

**`fp=` fica, e é o campo que mais importa.** Não é endereçamento: é a impressão
digital do certificado (ADR 0003), e é a única coisa que faz o primeiro contato
não ser cego. Sem ela o cliente aceita **qualquer** certificado que atender
naquele endereço e o fixa para sempre — quem chegar primeiro vira o servidor.

E ela fica *mais* necessária com nome amigável, não menos: um domínio é
resolvido por DNS, e DNS é justamente o que um atacante consegue dobrar. Um IP
ao menos falha fechado quando não é alcançado. Trocar `fp=` por "confie no nome"
só seria seguro com certificado de autoridade — o mesmo Let's Encrypt e o mesmo
domínio obrigatório que este projeto recusa por decisão.

**`alt=` e `enc=` saem.** O primeiro só existia para quem hospeda em casa com
várias interfaces; a decisão 3 resolve isso na tela, não no link. O segundo é o
degrau 4.

## 5 · O caminho de conexão do cliente

`conectar_entre_com_bilhete` → `conectar_entre` → `tentar_entre` colapsam em
`Enlace::conectar`, que já existe e nunca saiu de lá.

Somem, com as razões que os justificavam: `PRAZO_POR_CANDIDATO`,
`PRAZO_DA_PRIMEIRA_VOLTA`, `PRAZO_DE_CANDIDATO_DISTANTE`, `ESPERA_DO_FURO`,
`AVISOS_POR_CANDIDATO`, `INTERVALO_DO_AVISO`, `merece_segunda`,
`e_de_outra_casa`, `avisar_pelo_candidato`, `Batida`.

O único prazo visível passa a ser `HANDSHAKE_TIMEOUT` (`transport.rs:45`, 10 s).

**Isto é o que o ciclo compra.** Hoje "tempo esgotado" é o resultado agregado de
uma corrida com quatro candidatos, duas voltas, três prazos diferentes, um
datagrama sem confirmação, um serviço de terceiro e uma resolução de DNS com 600
ms de teto. Depois disto, "não conectou" tem uma causa, e o log diz qual.

### `chegada.rs` emagrece, não morre

A trilha continua valendo — foi ela que deu diagnóstico no relato de 25/08. O
que muda são as etapas, que deixam de contar candidatos e passam a contar o que
de fato acontece com um endereço:

`Resolvendo` → `Conectando` → `Conferindo a chave` → `Dentro` / `Desistiu`

### Um defeito que a decisão do nome amigável torna urgente

`resolve()` (`seele-ffi/src/lib.rs:2603`) usa `to_socket_addrs()`, que é
**síncrono e sem prazo**, dentro de contexto async. Com IP isso nunca doeu. Se
este spec vai empurrar as pessoas para `seele://meudominio.com.br`, um DNS lento
passa a travar uma thread do tokio sem limite.

Vira `tokio::net::lookup_host` com prazo e com erro próprio. É a etapa
`Resolvendo` acima ter algo a dizer.

*(Efeito colateral bom da decisão 2: hoje o `.next()` desse resolvedor pega o
primeiro endereço que o DNS devolveu, que pode ser um IPv6. Com IPv4 puro, vira
determinístico.)*

## 6 · IPv4 puro

`abrir_escuta` (`alcance.rs:193`) liga em `0.0.0.0`. `Pilha`, `Escuta::serve` e
toda a lógica de pilha dupla somem — 130 ocorrências no servidor, 30 no
core/proto/ffi.

Um endereço IPv6 digitado é **recusado na entrada**, com frase própria, e não
silenciosamente ignorado. Um convite antigo que traga um IPv6 é recusado com a
mesma frase.

**A baixa, dita em voz alta:** uma VPS só com IPv6 deixa de servir. Isso é uma
escolha, não um esquecimento. Tailscale não é afetado — ele entrega IPv4
`100.x` junto do IPv6.

## 7 · `seeled` como artefato de VPS

Nada aqui toca código de rede.

### O que já existe, e por que não parecia

`.github/workflows/release.yml:488-493` já publica
`seele-cli-{versão}-linux.tar.gz`, e dentro dele está o `seeled`. O artefato
existe. Quatro coisas o impedem de ser útil numa VPS:

**1. O nome mente.** "cli" não diz a ninguém que ali dentro está o servidor.
Passa a ser `seeled-{versão}-linux-x86_64.tar.gz`.

**2. O piso de glibc.** Compilado em `ubuntu-24.04` → glibc 2.39. Não roda em
Debian 12 (2.36) nem Ubuntu 22.04 (2.35), que é metade das VPS baratas.

Alvo novo: `x86_64-unknown-linux-musl`. Binário estático, sem piso, roda em
qualquer lugar — é o que faz "arquivo isolado" ser verdade em vez de aspiração.

**O risco está aqui e é nomeado:** `rusqlite` está com `bundled` (compila SQLite
em C) e o `ring` tem assembly. Os dois constroem em musl, mas exigem toolchain C
cruzado no runner. É trabalho de CI, e é onde esta fase pode escorregar.

*Recuo, se o musl custar mais de um dia:* trocar o runner de `ubuntu-24.04` para
`ubuntu-22.04`. Uma linha na matriz, baixa o piso para glibc 2.35, cobre Debian
12 e Ubuntu 22.04+. Cobre a maioria das VPS sem cobrir todas. **O recuo é
decisão de quem implementa, e tem de ser registrado no commit** — um binário
que se anuncia estático e não é seria pior que o de hoje.

**3. Só x86_64.** `install.sh:57-59` recusa explicitamente outras arquiteturas.
Entra `aarch64-unknown-linux-musl`: VPS ARM (Ampere, Graviton, Hetzner) é comum e
barata. `install.sh` acompanha.

**4. Sem supervisão e sem doc.** Entram um `seele.service` dentro do tarball e um
`docs/vps.md`: porta, banco em `/var/lib/seele`, usuário de sistema, backup,
como conferir a impressão digital.

**Não** entra um subcomando `seeled instalar`. Isso é operação, e operação é o
próximo spec (§10).

### A guarda do servidor aberto

Hoje o `seeled` sobe com admissão aberta e **avisa**. Numa VPS isso é um servidor
aberto na internet — e somos nós que estamos mandando a pessoa colocá-lo lá.

**Proposta: o `seeled` recusa subir aberto quando o endereço de escuta não é
privado**, e a mensagem de recusa traz o comando que resolve (`seeled senha`).
Escutar em `0.0.0.0` conta como não-privado.

Isto está marcado como **reversível**: foi proposto por quem escreve, não pedido
por quem decidiu o escopo, e é a única coisa neste documento nessa condição. Se
for julgado escopo de operação, sai daqui sem prejuízo para o resto.

## 8 · Compatibilidade

**Links antigos.** O parser aceita e **ignora** `alt=` e `enc=`, pelo princípio
que já está escrito em `uri.rs:79`: *«recusar link novo é o que faz cliente velho
virar parede»*. A recíproca vale: um cliente antigo lendo um link novo vê um
`alvo` e um `fp=`, que é tudo que ele precisa.

**A exceção, e ela é deliberada:** um link cujo `alvo` seja IPv6 é **recusado**,
não ignorado. Não há como aceitar em silêncio um endereço em que ninguém mais
atende.

**Pins.** O pin é arquivado pelo endereço como foi digitado
(`seele-ffi/src/lib.rs`, `pin_key`). Um servidor alcançado por nome e por IP
arquiva sob duas entradas. Isto já é verdade hoje e este spec não o conserta —
fica registrado como pendência conhecida, porque a recomendação de usar nome
amigável vai fazê-lo aparecer mais.

## 9 · Testes e guardas

**Saem:** `crates/seele-conformance/tests/furo.rs` inteiro (é o degrau 4),
`candidatos.rs` inteiro (é a corrida). `convite.rs` encolhe.
`crates/seele-encontro/tests/apresentacao.rs` vai com o crate.

**Entram**, no estilo de guarda-contra-regressão que esta casa já usa:

1. Nenhum endereço IPv6 consegue entrar num `Convite` — nem por construção, nem
   por parse.
2. Nenhum convite gerado carrega `alt=` ou `enc=`.
3. Um `Tipo::Ponte` nunca vira alvo anunciado.
4. Um link antigo com `alt=` e `enc=` conecta pelo `alvo` e não reclama.
5. Um link com `alvo` IPv6 é recusado com a frase própria.

**O portão, e ele não é negociável.** Workspace inteiro verde — não a crate
afetada. Esta regra foi aprendida em 25/08, quando três portões estreitos deram
falsa confiança no mesmo dia.

**E depois disso, `docs/teste-duas-maquinas.md` rodado de verdade.** Nenhuma
suíte deste repositório prova que duas máquinas reais se acham. Este spec reduz
drasticamente o número de coisas que podem dar errado nesse teste; ele não o
substitui.

## 10 · ADRs e documentação

**ADRs novos:**

- **0036 — a conexão é direta.** Supersede o 0022 inteiro. Registra as decisões
  1 e 2, e registra o que se perde: VPS só-IPv6, e quem hospeda de casa atrás de
  CGNAT sem VPN.
- **0037 — o convite leva um endereço.** Emenda o 0006. Registra a decisão 4 e
  por que `fp=` sobrevive à poda.
- **0038 — o dono de um servidor.** **Escrito, não implementado.** É o ADR que
  destrava o próximo ciclo, e ele é escrito agora porque a decisão que ele
  registra — administração pelo fio, autenticada por chave Ed25519, e **não** um
  painel web — foi tomada nesta conversa e não deve ser retomada do zero.

  A razão da escolha, resumida: um painel HTTP numa VPS exige certificado de
  autoridade, que exige domínio e Let's Encrypt, o que obriga todo
  auto-hospedeiro a ter um domínio e contradiz o ADR 0003. `AdministerServer` já
  existe em `permissions.rs` sem ninguém que a exerça pelo fio; pelo ADR 0002 as
  duas cascas ganham de graça. **Não** criar um `seele-admin`.

**Documentação:**

- Morrem: `docs/alcance-pela-internet.md`, `docs/ponto-de-encontro.md`.
- Nasce: `docs/vps.md`.
- `README.md` está atrasado em relação aos ADRs 0033–0035 (ainda usa o
  vocabulário de Evangelion que aqueles ADRs removeram). Não é escopo deste
  spec, mas está registrado aqui porque foi encontrado durante ele.

## 11 · O que este spec não faz

- **Não melhora a comunicação de dados.** Bitrate adaptativo, codec, a `Subida`
  fechada em 25/08 — nada disso é tocado. Este spec é a limpeza que abre espaço
  para esse ciclo, e o ciclo é outro.
- **Não constrói administração no fio.** Convite, senha, portaria, banimento,
  retenção continuam sendo subcomandos de quem tem shell na máquina. O ADR 0038
  registra o desenho; a implementação é o próximo spec.
- **Não constrói `seeled instalar`, backup, nem atualização do servidor.**
  Camadas 1 e 4 da avaliação de 26/08. Próximo spec.
- **Não conserta a chave de pin duplicada** entre nome e IP (§8).
- **Não substitui o teste de duas máquinas.**

## 12 · Fases

Cada fase é um commit revisável e roda o workspace inteiro antes da seguinte. A
ordem não é preferência: é a única em que o código continua compilando entre uma
fase e a outra.

**Fase 1 — o degrau 4 sai.** Crate `seele-encontro`, `proto::encontro`,
`alcance/encontro.rs`, `core/encontro.rs`, `Bilhete`/`enc=`, `Batida` e o aviso
por candidato em `enlace.rs`, `tui/rede.rs`. Morre `furo.rs`.

**Fase 2 — o degrau 3 sai.** `alcance/porta.rs`, `alcance/pcp.rs`. Saem
`crab_nat`, `igd-next`, `netdev`.

**Fase 3 — IPv4 puro.** `Pilha` e pilha dupla somem; `abrir_escuta` liga
`0.0.0.0`; IPv6 é recusado na entrada com frase própria.

**Fase 4 — um endereço, e o que sobra da escada.** `alt=` sai da URI; a corrida
de candidatos sai de `enlace.rs`; `chegada.rs` emagrece e ganha `Resolvendo`;
`resolve()` vira assíncrono com prazo; `Escada`/`Alcance`/`Escuta` viram
`Enderecos`; `Degrau` cai para três; `Ponte` deixa de ser anunciado; a tela de
hospedar ganha a lista de interfaces. Morre `candidatos.rs`.

**Fase 5 — o `seeled` vira artefato.** Alvos musl, aarch64, nome novo,
`install.sh`, `seele.service`, `docs/vps.md`, e a guarda do servidor aberto se
ela sobreviver à revisão.

**Fase 6 — os ADRs e a documentação.** 0036, 0037, 0038. Mortes e nascimento
do §10.

As fases 1 a 4 tocam rede e têm de ser bissectáveis uma a uma. A fase 5 é
independente das outras e pode ser feita em paralelo por outra pessoa — ela não
compartilha um arquivo com nenhuma delas.
