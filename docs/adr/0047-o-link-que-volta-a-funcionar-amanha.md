# 0047 — O link que volta a funcionar amanhã: permanência, multiconexão e chamada privada

Status: **rascunho** — desenho, não decisão. Nenhuma linha de produção foi
tocada para escrevê-lo.
Data: 2026-09-15
Sobre o commit `3c59eec7d8f515fb4d01b772d7bb4f5f92e0371c`.

> **Leia primeiro a seção «O que já está construído».** Três das quatro camadas
> que este documento foi encarregado de propor **já existem no código e já foram
> publicadas**. O que falta não é código: é um binário atualizado numa VPS. Uma
> proposta que ignorasse isso mandaria construir de novo o que está pronto —
> exatamente o erro que o `CLAUDE.md` deste repositório existe para não deixar
> repetir.

O pedido, nas palavras de quem o fez: *«hoje, ao hospedar, o link fica salvo no
histórico para convidados voltarem depressa, mas na prática é inútil — depois de
reiniciar o servidor o link antigo não serve mais.»* Com três propostas: (a)
cliente conectado a vários servidores ao mesmo tempo; (b) conexão P2P «sem
server» para conversa privada; (c) link persistente ou amigável. E com o pedido
explícito de que a viabilidade fosse decidida pela análise do código.

---

## 1. Inventário: o que torna um `seele://` volátil, arquivo por linha

### 1.1 As quatro causas levantadas, conferidas uma a uma

| # | hipótese do coordenador | veredito | evidência |
|---|---|---|---|
| 1 | porta fixa 8383 em `seele-proto/src/transport.rs:24` | **confirmada como fato, e ela não é causa — é o contrário** | `DEFAULT_PORT: u16 = 8383` (`crates/seele-proto/src/transport.rs:24`); o app hospeda nela por constante própria (`apps/seele-app/src/main.rs:917` e `:617`), e a escuta é `[::]:porta` (`crates/seele-server/src/hospedagem.rs:87`) |
| 2 | escuta de avisos com bind em porta 0 | **confirmada** | `escuta_de_avisos` liga em `0.0.0.0:0` / `[::]:0` (`crates/seele-server/src/alcance/encontro.rs:633-639`) |
| 3 | recuo do UPnP para «qualquer porta» | **confirmada, e contradiz uma tabela do ADR 0022** | `add_any_port` no braço `PortInUse \| SamePortValuesRequired` (`crates/seele-server/src/alcance/porta.rs:354-366`) |
| 4 | histórico guarda só endereço, descarta `fp`, `alt` e bilhete | **refutada** | `Conhecido` guarda `caminhos` (`:76`), `bilhete` (`:81`) e `impressao` (`:94`) em `crates/seele-core/src/conhecidos.rs`, além de `alvo` (`:43`) |
| 5 | a identidade TLS do servidor é persistente | **confirmada** | `Identity::load_or_create` lê do banco ou gera e grava (`crates/seele-server/src/tls.rs:66-110`) |

**A hipótese do usuário — «a porta do link troca» — está certa no sintoma e
errada na causa.** A porta em que o servidor escuta é fixa e a impressão digital
dele é eterna. O que troca é o **mapeamento de NAT**, que é outra coisa: um par
`IP público:porta` que o roteador inventa quando um pacote sai e reinventa na
abertura seguinte.

### 1.2 O que de fato caduca, em ordem de dano

1. **O bilhete `enc=`.** A segunda metade dele é o endereço público da escuta de
   avisos, e essa escuta pede porta zero ao sistema (`alcance/encontro.rs:633-639`).
   Porta nova a cada execução, mapeamento novo, bilhete morto. **É a causa mais
   direta**, e é deliberada: ler do socket do servidor roubaria pacotes do QUIC
   (ADR 0022, «O furo é uma propriedade do socket»).
2. **A porta externa do UPnP, quando 8383 está tomada no roteador.** O ADR 0022
   afirma, na tabela de «O quarto», que o degrau 3 *«pede 8383 externa e **recusa**
   outra»* e por isso *«sobrevive a fechar e abrir: sim»*. **O código não faz
   isso**: ele aceita qualquer porta (`porta.rs:363`), com o comentário dizendo
   que o `seele://` carrega a porta e por isso está tudo bem. Está tudo bem para
   um link recém-gerado e **não** para um link guardado. A tabela do ADR está
   errada sobre o próprio código; é o achado mais barato de consertar deste
   documento — uma linha de ADR — e o mais fácil de não notar.
3. **O IP público e o prefixo IPv6.** Fora do alcance do produto. Religar o
   modem troca os dois em boa parte das operadoras brasileiras.
4. **O histórico — não.** Ele guarda as quatro colunas que interessam, e há
   testes nomeados para cada uma: `os_outros_caminhos_sobrevivem_a_uma_volta_sem_convite`
   e `a_impressao_sobrevive_a_uma_visita_que_nao_a_traz` (`conhecidos.rs:735` e
   `:812`).

### 1.3 A alavanca central existe e está ligada

`Identity::load_or_create` grava certificado e chave no mesmo SQLite do servidor
(`tls.rs:66-110`), e o app hospeda com `Location::File` (`main.rs:614-618`).
Logo **a impressão digital de um servidor hospedado pelo app é a mesma amanhã**.
É a única coisa do convite que não envelhece, e é dela que sai a marca do
quarto (`marca_do_convite` e `marca_do_server`, `alcance/encontro.rs:359-375`).

---

## 2. O link persistente: três das quatro camadas já estão de pé

### 2.1 O que já está construído, e onde

| camada proposta | estado | onde |
|---|---|---|
| **DDNS / porta encaminhada, sem código** | disponível hoje, sem documento próprio | degrau 1 do ADR 0022; `docs/alcance-pela-internet.md` |
| **histórico guardando o Convite inteiro** | **construído** | `conhecidos.rs:43-94`; lido de volta em `main.rs:243-263` |
| **parar de persistir o `enc=` caduco** | **construído, e melhor que o proposto**: o bilhete guardado é *substituído* pelo endereço de hoje antes de conectar | `main.rs:284-305` |
| **registro de endereço no ponto de encontro** | **construído** — verbos `MORO`/`QUEM`, quarto com prazo de 60 s e teto de 4096 | `seele-proto/src/encontro.rs`; `seele-encontro/src/lib.rs:144,288`; `seele-core/src/encontro.rs:333` (`onde_mora`); `seele-ffi/src/lib.rs:6646` (`onde_mora_hoje`) |

O caminho inteiro, como ele está no commit: quem volta pela lista lê `caminhos`,
`bilhete` e `impressao` do histórico (`main.rs:243-263`); pergunta ao ponto de
encontro do bilhete onde aquele servidor mora **hoje**, por duas marcas em
paralelo — a do socket do servidor e a da escuta de avisos (`main.rs:284-305`,
`ffi:6646-6666`); põe a resposta **na frente** dos endereços guardados; e o ADR
0037 faz os quatro candidatos correrem com 250 ms de defasagem. Quem cola um
link antigo cai no mesmo caminho: a impressão vem do `fp=` em vez do histórico
(`main.rs:284`, o `.or(esperada)`).

**Isto foi medido, não deduzido:**

```
cargo test -p seele-conformance --test quarto
running 5 tests … test result: ok. 5 passed; 0 failed
```

Entre eles, `um_servidor_que_trocou_de_porta_ainda_e_achado_pela_impressao` e
`um_ponto_que_nao_conhece_a_pergunta_apenas_cala`.

### 2.2 Por que, então, o link continua inútil: o ponto de encontro em produção é antigo

Sondado de fora, o ponto de encontro padrão, hoje:

```
ONDE → SEELE-ENC/1 AQUI <marca> <meu ip público>   (responde)
MORO → sem resposta                                 (3 s de prazo, repetido)
QUEM → sem resposta                                 (3 s de prazo, repetido)
```

`encontro.seele.app.br` resolve para `216.128.168.216` e
`2001:19f0:b800:1bf5:5400:6ff:fe97:3c3b` — os dois endereços de `REDE_DO_PADRAO`
(`alcance/encontro.rs:107-110`). O serviço está no ar e fala o protocolo. **Mas
`responder` no código de hoje trata `MORO` exatamente como `ONDE`**
(`seele-proto/src/encontro.rs:353`): um binário atual responderia `AQUI` a um
`MORO`. O binário que está na VPS cala. Ele é **anterior a 03/09/2026**, quando o
quarto nasceu.

E a versão publicada do cliente **já traz o quarto**: a última release é
`v0.10.5-1`, do commit `12a6401a6` (2026-09-05), e `git show 12a6401a6:crates/seele-core/src/encontro.rs`
contém `onde_mora`.

> **Conclusão, e ela muda a prioridade inteira deste documento:** o conserto que
> o usuário pede foi escrito, testado, publicado e entregue à máquina dele há dez
> dias. Ele não funciona porque o serviço do outro lado da pergunta nunca foi
> reimplantado. **O primeiro item de trabalho não é código. É `cargo build
> --release -p seele-encontro` e um `systemctl restart` na VPS.**

Como conferir, depois de reimplantar (de outra máquina, não da VPS):

```sh
cargo run -p seele-encontro --example sondar -- encontro.seele.app.br:8384
```

e a prova de verdade: `MORO` passando a devolver `AQUI`. Um `systemctl status`
responde uma pergunta diferente — foi o que este documento mediu para não supor.

### 2.3 O que continua sem resposta depois de a VPS ser atualizada

Estes são buracos reais, e nenhum deles é o que o usuário relatou:

- **Servidor sem bilhete não tem a quem perguntar.** O quarto só é consultado
  quando há `bilhete_guardado.ponto` (`main.rs:284-287`). Um servidor com IPv4
  público não tenta o degrau 4 de propósito — por metadado — e, se o IP dele
  mudar, o histórico não tem saída. Isso é o degrau 1 e a resposta dele é DDNS.
- **Endereço digitado à mão não tem impressão.** `impressao` é `None`, não há
  marca, não há pergunta. Correto: sem `fp` também não haveria verificação.
- **O prazo é de 500 ms** (`ffi:6629`), pago **antes** de qualquer tentativa,
  inclusive por quem está na mesma casa. É o custo aceito e está documentado.
- **O quarto esvazia num reinício do ponto de encontro**; os anfitriões voltam em
  até 15 s (`REAVIVAR`, `alcance/encontro.rs:145`).

### 2.4 A quarta camada proposta — «registro assinado pela chave estável» — e por que eu a recuso

A proposta era um registro de endereço **assinado** pela chave do servidor. Ela é
uma versão mais forte do quarto que já existe, e a diferença é quem paga:

- **O custo de privacidade, nomeado em voz alta.** O quarto de hoje já entrega ao
  operador do ponto de encontro *«que uma marca está no ar, e em que endereço»* —
  metadado vivo, palavra do próprio ADR 0022. Uma assinatura **não reduz** isso:
  reduz o que um terceiro consegue **escrever**, não o que o operador consegue
  **ler**. O preço de privacidade seria pago de novo sem nada de volta nessa
  moeda.
- **A contradição com «ele não guarda nada», de frente.** `docs/ponto-de-encontro.md`
  ainda diz, hoje, na voz do produto: *«Nada guardado. Ele não tem banco, arquivo,
  nem tabela em memória.»* **Essa frase é falsa desde 03/09**: há uma tabela em
  memória, com prazo e teto, e o ADR 0022 a registrou — o documento do usuário é
  que não foi atualizado junto. Isto é exatamente o defeito que o `CLAUDE.md`
  chama de «o produto sabe e não conta», e **está publicado**. Corrigir esse
  parágrafo é trabalho de meia hora e é dívida de honestidade, não de desenho.
  Um registro assinado tornaria a contradição maior, não menor: chave a conferir,
  relógio a comparar, e um serviço que deixaria de ser uma função sem `self`.
- **O que a assinatura compraria de concreto é pouco.** O ADR 0022 já explica por
  que ninguém toma o lugar de ninguém sem ela: quem chega confere a impressão
  digital de qualquer jeito (ADR 0003), então o teto do estrago é *não entrar* —
  nunca *entrar no lugar errado* —, e quem escreveu primeiro fica enquanto reaviva
  a cada 15 s. Um atacante compra um ataque de recusa de serviço contra um
  servidor que está fora do ar.

**Recomendação: não construir a camada 4 agora.** Ela é a resposta certa para um
problema que ainda não temos (abuso do quarto em escala), e o custo dela é
estrutural. Se um dia for construída, ela precisa do ADR 0006: campo novo no
`seele://` é ignorado por cliente velho, nunca recusado — e um `sig=` que um
cliente velho ignore é um `sig=` que não protege ninguém enquanto houver cliente
velho, o que é uma propriedade a decidir antes e não depois.

### 2.5 «Amigável» é a outra metade do pedido, e ninguém a atacou

O usuário pediu «persistente **e/ou** amigável». Este documento inteiro é sobre
persistência. Amigável — um `seele://casa-do-alexandre` — exige um nome que
alguém resolva, e isso é ou DNS (degrau 1, sem código, documentável hoje) ou um
diretório nosso, que é um serviço com estado e nome próprio, do outro lado da
linha que o ADR 0022 traçou. **Fica como pergunta ao usuário**, não como
proposta: `docs/alcance-pela-internet.md` pode ganhar uma seção «um nome em vez
de um número» com DuckDNS e afins em quinze linhas, e isso resolve o caso de
quem hospeda a sério sem que o produto ganhe infraestrutura.

---

## 3. Multiconexão: o custo real, e ele já estava escrito

**O ADR 0031 já é este inventário**, com a decisão «várias sessões, um caminho de
voz» em estado `proposto` e explicitamente **não construído**. Este documento o
confere contra o commit de hoje em vez de o repetir — o próprio 0031 avisa que é
um retrato de 18/08.

### 3.1 O que continua valendo, reconferido

- `Session.connection: Mutex<Option<Arc<Connection>>>` — **`apps/seele-app/src/main.rs:57-58`**, confirmado.
- A recusa da segunda sessão: `ConnectionError::AlreadyConnected` em `main.rs:191`.
- `hospedagem`, `busca`, `alvo`, `convite` moram no mesmo `Session` e três deles
  são estado de sessão em vaga única (`main.rs:57-102`).
- Canal de eventos único, `EVENT_CHANNEL` (`main.rs:45`), e **nada no payload diz
  de qual sessão o evento veio**. É o defeito mais barato de introduzir e mais
  caro de achar: a mensagem do servidor B desenhada no canal do A, calada.

### 3.2 O que **envelheceu**, e para pior

O 0031 dimensionou o trabalho em 51 comandos Tauri, dos quais 23 resolvem a
conexão, e ~13 mil linhas de casca. Hoje:

| medida | ADR 0031 (18/08) | commit `3c59eec` | fator |
|---|---|---|---|
| `#[tauri::command]` em `main.rs` | 51 | **92** | 1,8× |
| chamadas que resolvem `.connection()` | 23 | **47** | 2,0× |
| menções a `snapshot` em `ui/` | ~240 | **310** | 1,3× |
| `ui/tela-sessao.js` | 1 771 linhas | **3 305** | 1,9× |
| `ui/` total | ~13 000 | **12 210** (só `.js`) | — |

**A superfície da multiconexão dobrou em quatro semanas, e continua dobrando.**
Isso não é argumento para fazer agora nem para não fazer: é argumento para
**decidir agora**, porque o custo de decidir depois é o dobro outra vez.

### 3.3 O recorte «conectado a vários, voz em um» contra o ADR 0009

**O recorte se sustenta, e o ADR 0009 é o argumento mais forte a favor dele.**

O orçamento do 0009 é **de um caminho**: 19,6 ms de dispositivo + 20 ms de quadro
+ 6,5 ms de lookahead + jitter buffer, dando ≈ 67 ms no piso e ≈ 87 ms no padrão,
tudo medido. Dois caminhos de voz simultâneos são dois `cpal::Stream` de entrada,
dois codificadores, dois mixers e duas threads de tempo real — e o 0009 registra
que a **versão do `cpal` vale milissegundos** e que o macOS **não tem alavanca de
buffer** (forçar buffer menor *aumenta* a latência: 512 quadros → 20,6 ms, 64 →
43,1 ms). Não há folga no orçamento para disputar dispositivo com um segundo
caminho, e não há medição que diga o que dois fluxos fazem com o
`kAudioDevicePropertyBufferFrameSize` do mesmo aparelho.

Some-se o que o 0031 já argumentou e que não é sobre latência: a barra de espaço
perde referente (`ui/tela-sessao.js`, `keydown`/`keyup`/`blur`), e transmitir
numa sala por engano é, por `specs/03-audio.md`, a falha mais cara que este
produto tem.

**E há um ganho que cai de graça e que vale por si**, independente de
multiconexão: hoje o microfone abre **na conexão**, não na entrada na sala de
voz. Quem conecta e nunca fala já abre o dispositivo. Três sessões abririam três.

### 3.4 Etapas e dependências, se for adiante

1. **Evento com dono** — campo de sessão em tudo que atravessa `EVENT_CHANNEL`, e
   a `Bridge` deixa de ser repasse. *Sem dependências. Faz sentido sozinho.*
2. **Caminho de voz sai da `Connection` e passa a ser da máquina**, aberto na
   entrada na sala de voz, trocado por `Voice::reopen` — que já existe e já
   carrega mudo, isolamento, modo, tecla e ganhos. *Depende de 1 para o aviso de
   microfone saber de quem é. É o pedaço mais delicado e o de menor volume.*
3. **Camada de sessões** acima do `Snapshot`, que **não** vira plural. *Depende de 1.*
4. **Casca**: 47 resolvedores de conexão, 310 menções a snapshot, as ~23 variáveis
   de módulo que já são estado de sessão sem se chamarem assim. *Depende de 3. É
   a maior parte, e é a que dobrou.*
5. **Trilha** — o menor pedaço, e o único que o pedido descreve.

**Esforço:** grande e de baixo risco técnico, alto risco de regressão de casca.
Nada no `seele-proto`, nada no `seele-server`, nenhum verbo novo, nenhuma
migração de banco. Custo de reverter **alto**, porque é mudança de forma
espalhada — o 0031 já diz isso e continua certo.

---

## 4. Chamada privada: a Hospedagem embutida como sala única, e não um transporte novo

### 4.1 `par.rs` não é o caminho, e o motivo é estrutural

`crates/seele-core/src/par.rs` (2 373 linhas) é um caminho QUIC real entre dois
clientes, com identidade efêmera (`identidade_efemera`, `:140`), verificação por
impressão digital (`ImpressaoNaoBate`, `:64`) e furo de NAT de verdade
(`ligar`, `:585`, sobre uma lista de endereços). Parece exatamente o que uma
chamada privada precisa. **E não serve**, por uma razão que está no próprio
módulo:

- **Ele carrega tela, não voz.** `escrever_tela_para_par` (`:810`) e
  `crate::tela::Recepcao::do_fluxo` são o que atravessa. Não há caminho de áudio,
  nem SSRC, nem jitter buffer, nem mixer no par.
- **E, sobretudo, ele não é «sem servidor».** A impressão digital esperada e a
  lista de endereços do par chegam **pelo servidor** — `ligar(…, enderecos,
  impressao_esperada, …)` recebe os dois de fora, e quem os anuncia é o
  `seele-server`. O par é o plano de dados; **o plano de controle é o servidor**.

Reusar `par.rs` para uma chamada privada seria construir, em cima dele, o plano
de controle que ele não tem: apresentação, identidade durável, salas, presença,
admissão. Isso é o `seele-server` de novo, escrito outra vez e pior.

### 4.2 O que custaria um P2P sem plano de controle central — dito explicitamente

Quatro coisas, e nenhuma é opcional:

1. **Descobrir o próprio endereço público.** Impossível de dentro do NAT (ADR
   0022, e é fato de rede, não do nosso código). Exige um terceiro — que é o
   ponto de encontro, que já existe.
2. **Coordenar o instante do furo.** Os dois lados mandando pacote quase ao mesmo
   tempo. Coordenação exige canal; canal exige um terceiro alcançável pelos dois.
3. **Saber que o outro está no ar, e onde.** É literalmente o quarto (`MORO`/`QUEM`),
   e **ele é estado**. Um P2P «sem nada no meio» que funcione amanhã precisa de
   mais estado no ponto de encontro, não menos.
4. **NAT simétrico dos dois lados não fura**, e a resposta seria retransmissão —
   que o ADR 0022 põe **fora de escopo por decisão, e não por falta de tempo**.

**Então «sem server» é impossível no caso geral, e o usuário já suspeitava disso
ao admitir «pode ser um servidor de quem liga para uma sala isolada». Essa
intuição está certa e é a recomendação deste documento.**

### 4.3 A proposta: sala única sem portaria, sobre a Hospedagem que já existe

Enquadrar a chamada privada como **o servidor de quem liga, com uma sala só e o
convite valendo por uma pessoa**. O que isso reusa, inteiro e sem código novo:
`Hospedagem::iniciar` com a escada do ADR 0022 já embutida
(`hospedagem.rs:113-126`), o degrau 4 e o quarto, o convite de uso único do ADR
0021, o TOFU do 0003, a voz, o texto, a tela. **Nenhum verbo novo de protocolo.**

Três coisas precisam de decisão, e são de produto, não de transporte:

- **Onde ela escuta.** O app hospeda numa constante, `PORTA_PADRAO = 8383`
  (`main.rs:917`), e `Session.hospedagem` é um `Option` de vaga única
  (`main.rs:63`). Uma chamada privada **enquanto** se hospeda «Casa» precisa de
  uma segunda porta e de uma segunda vaga — ou de aceitar que ligar e hospedar
  são exclusivos. *Isto depende da multiconexão do §3 para ser bom, e não para
  funcionar.*
- **A portaria.** O ADR 0030 semeia a portaria **ligada** no botão HOSPEDAR AQUI
  (`main.rs:637`). Numa chamada de uma pessoa só, uma fila de aprovação é
  cerimônia pura: quem liga já escolheu para quem. O convite de uso único do ADR
  0021 é o porteiro certo aqui — ele já é descartável por construção.
- **Quem liga fica sendo o dono do lugar**, com tudo o que o ADR 0030 dá a quem
  hospeda. É assimetria real e precisa aparecer na tela em vez de ser descoberta.

**Esforço:** pequeno a médio, e quase todo de casca e de fraseado. É o item de
melhor relação entre valor e risco deste documento inteiro, depois de atualizar
a VPS.

---

## 5. O corte entre 0.11.0 e 0.12.0

A 0.11.0 está perto do fim e com fechamento planejado. A recomendação é por
risco, e a ordem é a do risco.

### Na 0.11.0

1. **Reimplantar o `seele-encontro` na VPS.** *Não é código.* É o conserto que o
   usuário pediu, já escrito e já publicado no cliente, esperando um binário do
   outro lado. Risco: nenhum sobre o produto — os três verbos antigos respondem
   byte a byte igual (`um_ponto_de_encontro_reiniciado_responde_igual`), e um
   ponto de encontro que não conhece `QUEM` apenas cala, que é o comportamento de
   hoje. **Sem isto, tudo o mais neste documento é secundário.**
2. **Corrigir `docs/ponto-de-encontro.md`.** A frase «Nada guardado. Ele não tem
   banco, arquivo, nem tabela em memória» está publicada e é falsa desde o
   quarto. Risco: nenhum. Custo: meia hora. É dívida de honestidade.
3. **Corrigir a tabela de «O quarto» no ADR 0022**, que afirma que o UPnP recusa
   porta externa diferente de 8383 quando `porta.rs:363` aceita qualquer uma.
   Risco: nenhum.
4. **Preferir a porta externa 8383 no UPnP e dizer quando não deu.** Uma linha de
   comportamento (tentar de novo mais tarde, ou nomear a frase) em `porta.rs`.
   Risco baixo, e vira o degrau 3 numa âncora estável de verdade — que é o que o
   ADR 0022 já achava que ele era.

### Na 0.12.0

5. **Chamada privada como sala única sem portaria.** Depende de decisões de
   produto (§4.3), não de transporte novo. Recorte natural para uma versão menor.
6. **Multiconexão, começando pelo evento com dono e pela voz que sai da
   `Connection`.** Os dois primeiros passos do §3.4 valem por si e não pedem a
   casca inteira. A casca é o corpo de uma 0.12.0 e provavelmente maior que ela.

### Fora de qualquer corte, por ora

7. **Registro assinado no ponto de encontro** (§2.4) — recusado enquanto não
   houver abuso medido.
8. **Link amigável por diretório nosso** — é um serviço com estado, do outro lado
   da linha do ADR 0022. DDNS documentado é a resposta barata (§2.5).

---

## Requisitos confirmados × perguntas que só o usuário decide

**Confirmado por código e por medida:**

- A porta de escuta é fixa e a impressão digital é estável entre reinícios.
- O histórico guarda convite inteiro; o mecanismo de reencontro por impressão
  existe, está ligado no app, está publicado desde `v0.10.5-1`, e passa 5/5 nos
  testes de conformidade contra um ponto de encontro local.
- O ponto de encontro em produção **não** implementa o quarto — medido de fora,
  três vezes, com `MORO` e `QUEM` sem resposta e `ONDE` respondendo.
- Multiconexão não muda protocolo, servidor nem banco; o custo é de casca e
  dobrou desde o ADR 0031.
- Voz simultânea em dois servidores não cabe no orçamento do ADR 0009.
- `par.rs` não substitui um servidor: ele é plano de dados e o plano de controle
  é o `seele-server`.

**Só o usuário decide:**

- Aceitar que o ponto de encontro guarde metadado vivo é uma decisão já tomada
  pelo ADR 0022 em 03/09 — mas **ela nunca foi contada a quem lê
  `docs/ponto-de-encontro.md`**. Confirmar ou rever a decisão sabendo que o
  documento público a contradiz.
- «Amigável» significa DNS de terceiro documentado, ou um diretório nosso?
- A chamada privada pode ser exclusiva com hospedar «Casa», ou precisa conviver?
- Multiconexão entra na 0.12.0 inteira, ou só os dois primeiros passos?

## Custo de reverter

Deste documento, **nenhum**: ele é análise. Do que ele recomenda: reimplantar o
ponto de encontro reverte com um `systemctl` e um binário antigo; as correções de
documentação revertem com `git revert`; o UPnP preferindo 8383 reverte em uma
linha; a multiconexão reverte **alto**, e o ADR 0031 já escreveu por quê.
