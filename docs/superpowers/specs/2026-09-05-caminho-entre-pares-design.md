# O caminho entre pares — desenho

**Data:** 2026-09-05
**Estado:** desenho aprovado, implementação por fazer
**Subprojeto A de três.** B é a árvore, C é o interruptor adaptativo. Cada um
com spec, plano e implementação próprios.

## 0 · De onde isto veio

Pedido de quem desenha o produto, em 05/09/2026, e vale citar porque ele é o
critério de aceite e não uma intenção vaga:

> Numa call de 5 pessoas, 2 querem transmitir a tela. Quero qualidade — sem
> pixelização, sem artefatos —, pouco ou nenhum atraso, e que a quantidade de
> pessoas na call não seja um problema.

A última cláusula é a que decide a arquitetura, e não é um pedido de mais
qualidade: é uma **propriedade**. No formato de hoje — todo mundo puxa do
servidor — a conta é fechada e não tem conserto:

> uma máquina sobe N cópias · logo cada cópia recebe 1/N do cano · logo a
> qualidade cai com N

Nenhuma otimização dentro da estrela entrega isso, porque o que se pediu é
justamente que o N saia da conta. Só uma árvore faz isso: nela cada máquina sobe
um número **fixo** de cópias, e esse número não sabe quantas pessoas existem na
sala. O que cresce no lugar da qualidade é a profundidade, e ela cresce com o
logaritmo de N.

Pela régua do próprio produto — `bits_por_quadro(P1080)` a 30 quadros —, 1080p
custa **6,24 Mbps por cópia**. Numa subida doméstica de 70 Mbps:

| | 1080p30 com duas telas |
|---|---|
| estrela, hoje | call de 4 pessoas |
| árvore leque 2, com folga guardada | call de 15 (profundidade 3) |
| árvore leque 2, profundidade 4 | call de 31 |

## 1 · O problema deste subprojeto, e por que ele não tem desvio

A premissa da malha é que os bytes vão de um espectador a outro **sem passar
pelo servidor**. Se passarem, a subida de quem hospeda paga — que é exatamente a
conta que a malha existe para não pagar.

E aí esbarra numa coisa pequena e absoluta: **alguém tem que atender.** QUIC
exige um lado no papel de servidor, e hoje nenhum cliente sabe atender. Fora de
teste, o `seele-core` só chama `quinn::Endpoint::client`: ele disca e nunca
responde. É a frase com que o §5.1 do desenho de compartilhamento de tela
recusou a alternativa B em 22/08 — *«custa um caminho que este produto nunca
teve»*.

Os desvios óbvios não sobrevivem:

- **quem empresta discar para quem recebe** — só troca quem precisa atender;
- **os dois discarem ao mesmo tempo, sem ninguém atender** — QUIC não faz
  abertura simultânea; um lado tem de estar no papel de servidor;
- **reaproveitar a conexão que já existe** — ela é com o servidor, e usá-la para
  alcançar outro cliente é voltar a passar por ele.

A única alternativa que dispensa tudo isto é **pôr o servidor num VPS de cano
grosso**: um link de 1 Gbps entrega 1080p para umas 96 pessoas na estrela de
hoje, sem arquitetura nova. Ela foi apresentada a quem desenha o produto e
recusada por escolha, não por impossibilidade: o servidor na casa de quem
hospeda é metade da identidade do produto. Fica registrada aqui porque é a saída
honesta caso este subprojeto se mostre caro demais.

## 2 · O que o A1 entrega, e o que ele não entrega

**Entrega:** um espectador recebe um quadro de tela de **outro cliente**, num
teste de integração com dois clientes de verdade contra um servidor de verdade,
com o contador do servidor provando que aquela cópia não saiu dele.

E dois números que hoje são estimativa e não podem virar desenho do B sem
medida: **quanto custa um salto** e **com que frequência o furo de NAT entre dois
clientes domésticos funciona**.

**Não entrega:** árvore, reparo com troca de pai, mais de um par por
transmissão, interface.

**A escolha de quem serve quem existe, e é deliberadamente burra.** O servidor
precisa apontar alguém, então aponta: entre quem declarou que empresta, e não é
quem compartilha, e ainda não está servindo ninguém, o primeiro. É uma linha, e
ela existe para o resto do caminho poder ser exercitado — a escolha **boa**, que
olha subida e topologia, é o subprojeto B inteiro. Chamá-la de «escolha
automática» aqui seria vender como pronto o que é um espaço reservado.

**Entrega o que parece fora de escopo, e por um motivo de custo:** o opt-in de
quem empresta viaja no protocolo desde já, mesmo sem tela para acioná-lo. Ver
§5.

## 3 · Como dois clientes se conectam

### 3.1 · Quem empresta atende, na ponta que já tem

Não é escuta nova, socket novo nem porta nova. O `quinn` tem
`Endpoint::set_server_config`, que recebe `&self`: dá para dizer a uma ponta que
já existe e já está conectada que ela também atende.

Isso é economia e é mais do que economia. Aquela porta **já tem mapeamento de
NAT vivo**, mantido pelo `keep_alive_interval` da conexão com o servidor. E
quando o degrau 4 do ADR 0022 foi usado, `client::local_endpoint` já adotou o
socket por onde o furo saiu — o comentário dele diz isso em voz alta: *«vem
pronto porque o furo de NAT já saiu dele»*.

**Só quando a pessoa optou por emprestar.** Quem não optou nunca chama
`set_server_config`, e a ponta continua só discando, como hoje.

### 3.2 · A identidade não é TOFU — é apresentação

O ADR 0003 vale para o `seele://`, onde não há intermediário e por isso a
primeira vez tem de ser confiada. Aqui há intermediário: os dois clientes já
fixaram o **mesmo servidor** e já se autenticaram nele por chave pública (ADR
0004).

Então quem empresta declara ao servidor a impressão digital do certificado que
vai apresentar, e o servidor a repassa a quem vai receber. **O servidor ocupa o
lugar que o link ocupa no `seele://`.** Sem pino novo em disco, sem TOFU entre
clientes, e nenhum par anônimo alimentando quadro.

O certificado de quem empresta é **efêmero por sessão**, e é a diferença que
justifica não reusar o caminho do servidor: o do servidor é persistido porque o
pino depende dele; este não é pinado por ninguém, porque a impressão digital
chega pelo servidor a cada apresentação.

### 3.2.1 · A conferência é dos dois lados — revisão de 06/09/2026

**Escrito depois de a Task 4 do plano medir o que este documento supunha.** O
§3.3 diz que os dois lados discam, e daí segue uma coisa que a primeira redação
não seguiu até o fim: **qualquer um dos dois pode acabar sendo quem aceita.** E
quem aceita não passa pelo verificador de cima — ele roda em quem disca.

Com `with_no_client_auth()`, metade das ligações não teria conferência nenhuma, e
quem empresta a subida serviria quadro a qualquer um que alcançasse a porta.
Este documento promete o contrário desde a primeira versão.

**O conserto usa um campo que já existia sem uso.** `SirvaTelaPara` leva a quem
empresta a impressão digital de quem vai receber; era para isto, e o desenho não
dizia o que fazer com ela. Quem atende instala um verificador de **certificado
de cliente** contra essa impressão, e exige certificado — um par que não
apresente nenhum é recusado, não aceito.

Duas paredes espelhadas, e as duas necessárias porque as duas direções existem.

**E alguém precisa atender.** Instalar o `ServerConfig` não faz o `quinn`
responder: ele enfileira a chegada e não diz nada até alguém chamar
`Endpoint::accept()`. Medido, não suposto — dois pares que só discassem nunca se
ligariam.

**Onde ele é gerado.** O `tls.rs` que gera certificado mora no `seele-server`, e
o ADR 0002 proíbe o `seele-core` de depender do daemon. A decisão é **duplicar
as poucas linhas de `rcgen` no core**, com o porquê escrito — o precedente que
`crate::tela` abriu e escreveu: *«quarenta linhas repetidas custam menos que um
crate de transporte que os dois dependeriam e nenhum seria dono»*. As duas
saídas alternativas ficam registradas no §9.

### 3.3 · Os dois discam, e o furo sai de graça

Como as duas pontas passam a atender, **os dois discam um para o outro** ao
receber a mensagem do servidor. As tentativas de conexão **são** os pacotes que
abrem o NAT dos dois lados; a primeira que fecha o aperto de mão vence, e a
outra é descartada.

Três coisas boas caem daqui, e nenhuma custa código:

- **o caso assimétrico se resolve sozinho** — se só um lado consegue sair, é a
  conexão dele que vinga;
- **nenhum verbo novo no ponto de encontro**, e nenhuma dependência dele;
- **só a API pública do `quinn`** — sem socket cru, sem pacote inventado.

**O ponto de encontro não participa, e isto é uma revisão do que se pensou
primeiro.** A ideia inicial era reusar o `LEVE` do degrau 4. Ele é o
intermediário errado aqui: existe para apresentar duas máquinas que não têm nada
em comum, e estas têm o servidor — conectado às duas neste instante, e vendo o
endereço público de cada uma como origem da conexão. Pedir a um terceiro que
descubra o que o segundo já sabe é um salto de rede e um serviço a mais no
caminho crítico, por nada.

### 3.4 · O que já é agnóstico, e por isso não entra na conta

O lado que recebe tela **já não sabe de onde o fluxo vem**:
`tela::TelaRecebida::do_fluxo` aceita qualquer `quinn::RecvStream`, e quem envia
escreve num `SendStream` qualquer. O enquadramento, o cabeçalho, o quadro-chave
e o byte de `StreamType::Screen` funcionam entre dois clientes sem uma linha de
mudança.

O que falta é a **conexão**, e só ela. É por isso que este subprojeto é de
transporte e não de mídia.

## 4 · O protocolo, e o que ele custa

Quatro mensagens. **`PROTOCOL_VERSION` sobe de 3 para 4**, com
`COMPATIBILITY_WINDOW` em 1: um cliente v3 conecta, nunca recebe as mensagens
novas, e roda como hoje — servido pelo servidor, que é o comportamento de antes
desta onda existir.

A versão custa, e custa de verdade: **a v3 já saiu** no release `v0.10.5-1`
(commit `12a6401a6`). O ADR 0036 pôde acrescentar quadro sem subir versão porque
a v2 ainda não tinha sido publicada; aqui não dá, e o `postcard` indexa variante
por posição — acrescentar no fim é ilegível para quem não conhece a variante.

**cliente → servidor**

A `impressao` das quatro mensagens é uma `String`: o **SHA-256 do certificado
DER em hexadecimal minúsculo**, exatamente o que
`seele_proto::transport::certificate_fingerprint` devolve, que é o que
`tls::Identity::fingerprint` usa e o que o `fp=` do `seele://` carrega. Um
formato só para a mesma coisa, em vez de um segundo jeito de dizer «este é o
certificado» — e `String` e não `[u8; 32]` porque é assim que o pino do ADR 0003
já viaja e é guardado, e dois formatos para o mesmo hash é o começo de os dois
discordarem.

- `EmprestarSubida { emprestando: bool, impressao: [u8; 32], locais: Vec<SocketAddr> }`
  — «eu empresto, este é o meu certificado, e estes são os meus endereços de rede
  local». O endereço público **não** vai aqui: ele é a origem da conexão que já
  está aberta, e o servidor o tem sem perguntar. Um endereço público que o
  cliente afirma seria um endereço que ele pode mentir.
- `ParFalhou { screen: ScreenId, motivo: MotivoDeFalhaDePar }` — **mandada por
  quem recebe**, e nunca por quem empresta: quem sabe que a imagem parou é quem
  estava esperando por ela, e quem empresta pode ter caído sem chegar a saber de
  nada. Enumerado, como todo motivo deste protocolo
  (`specs/02-protocolo.md`), com cada variante carregando o que a casca precisa
  para escrever a própria frase (ADR 0012). Quatro para começar, e cada uma
  distinguindo um conserto diferente:

  | variante | o que aconteceu | o que o B faz com ela |
  |---|---|---|
  | `NaoAlcancou` | nenhum dos endereços fechou aperto de mão | não tentar aquele par de novo nesta sala |
  | `ImpressaoNaoBate` | alcançou, e o certificado não era o apresentado | nunca mais aquele par, e é evento de segurança |
  | `CaiuNoMeio` | estava servindo e a conexão morreu | pode voltar a ser pai depois |
  | `ParouDeMandar` | conexão viva, quadro nenhum dentro do prazo | pode voltar a ser pai depois |

  **`ImpressaoNaoBate` não é o mesmo que `NaoAlcancou`**, e juntá-las seria o
  defeito que o ADR 0003 nomeia em outro lugar: a diferença entre «não consegui
  falar com ele» e «alguém respondeu no lugar dele» é a informação inteira.

**servidor → cliente**

- `SirvaTelaPara { screen: ScreenId, enderecos: Vec<SocketAddr>, impressao: [u8; 32] }`
  — para quem empresta.
- `AssistaTelaPor { screen: ScreenId, enderecos: Vec<SocketAddr>, impressao: [u8; 32] }`
  — para quem recebe.

As duas são simétricas de propósito: os dois lados fazem a mesma coisa com elas
— discar para os endereços e conferir a impressão digital —, e a assimetria fica
só em quem já tem os bytes.

## 5 · O opt-in entra agora, e a razão é de custo

Quem desenha o produto decidiu em 05/09/2026: **emprestar a subida é escolha de
quem empresta, e quem entra na sala é avisado de que a malha está ligada.**

O opt-in é escolha por duas razões independentes, e vale separá-las porque só a
primeira é óbvia:

1. **privacidade** — numa malha, espectadores passam a conhecer o endereço IP uns
   dos outros. Hoje não conhecem: nada em `control.rs` expõe endereço de membro,
   e todo mundo só fala com o servidor. E um servidor não é necessariamente entre
   amigos: o ADR 0021 deixa a admissão poder ser aberta;
2. **custo real** — a máquina de quem empresta passa a subir cópias para outras
   pessoas. Isso gasta a internet dela, e ninguém deve gastar a internet de
   alguém sem perguntar.

Vale registrar o que **não** piora: quem repassa já é espectador autorizado
daquele fluxo, então ninguém novo enxerga o conteúdo. O que muda é endereço, não
imagem.

**Por que no A1, se a tela dele é do B.** Porque a mensagem
`EmprestarSubida` é protocolo, e protocolo custa versão. Deixá-la para o B
custaria a v5 para carregar uma decisão que já está tomada hoje. Uma decisão
tomada não deve pagar duas vezes.

## 6 · O que se mede, e é metade da razão do A1

Toda a aritmética do §0 depende de dois números **estimados e não medidos**. Se
o furo falhar em boa parte dos pares, a árvore do B não pode supor que qualquer
par se alcança — e vira «árvore entre quem se alcança, estrela para o resto»,
que é um desenho bem diferente. Medir antes de desenhar é a regra da casa, e ela
já custou caro três vezes por ser ignorada.

- **Ida e volta entre pares** — `connection.rtt()` na conexão par a par. É o
  custo de um salto.
- **Como a conexão foi conseguida**, enumerado: `Local` (mesma rede, sem furo),
  `Furo` (endereço público, deu certo), `Falhou { motivo }`.

Os dois viram `tracing` e contador, lidos pelo roteiro de duas máquinas. **Não**
viram telemetria mandada a lugar nenhum — o ADR 0022 nomeia metadado como custo,
e um contador de quem-alcançou-quem enviado a um servidor seria criar
exatamente o registro que este projeto não quer que exista.

Onde não houve medida, `——`, como no resto do produto.

## 7 · A queda, e por que ela não é adiável

Decidido por quem desenha o produto em 05/09/2026: **quando um par que estava
repassando cai, quem estava atrás dele volta a ser servido pelo servidor.** A
malha é sempre alívio, nunca dependência; ninguém perde imagem por causa da
máquina de outra pessoa.

Isso tem uma consequência aritmética que entra no desenho do B e está aqui para
não se perder: **a subida de quem hospeda precisa guardar folga** para absorver a
queda de um par. Com leque `f`, a queda de um nó órfã exatamente `f`
espectadores, então a folga é `f` cópias. Conferido que ela não come o ganho: com
duas telas e leque 2, quem hospeda gasta 25 Mbps servindo e guarda 12,5 de
folga — 37,4 dos 42 disponíveis numa subida de 70 Mbps. **A folga custa um degrau
de leque, e nada mais.**

No A1 não há árvore para reparar, então a queda é o caminho simples: o par
falhou, `ParFalhou` sai, o servidor volta a servir. Mas ela **é** provada aqui,
porque é a propriedade de segurança da decisão, e uma propriedade de segurança
provada depois é uma propriedade que passou um tempo sem existir.

## 8 · Como se prova

**Unidade**

- a escolha do servidor e a montagem do par de mensagens, como função pura;
- a recusa de um par cuja impressão digital não bate com a apresentada;
- a máquina de estados «pede ao par, e se falhar pede ao servidor».

**Integração**, um servidor e dois clientes de verdade

- o quadro que chega ao segundo cliente veio pelo primeiro — provado byte a byte
  **e** pelo contador do servidor, que não subiu aquela cópia. É a mesma prova de
  negativa que `subida_no_arranque.rs` já usa;
- mata-se o par no meio da transmissão, e quem estava atrás continua vendo,
  servido pelo servidor.

**Guarda de regressão.** Com o caminho de par arrancado, os testes falham — e
isso é conferido revertendo o conserto, não suposto.

**Duas máquinas.** `docs/teste-duas-maquinas.md` ganha uma seção, e é ela que
produz os dois números do §6. Nenhum teste automático deste repositório pode
produzi-los: um furo de NAT entre dois roteadores domésticos não acontece em
`127.0.0.1`.

## 9 · Alternativas recusadas

- **Reusar o `LEVE` do ponto de encontro** para apresentar os dois pares.
  Recusada pelo §3.3: o servidor já sabe o que se iria perguntar, e o ponto de
  encontro seria um serviço a mais no caminho crítico por nada. Continua sendo o
  mecanismo certo para o degrau 4, que é outro problema.
- **Subir `Identity` para o `seele-proto`**, que os dois lados já dependem.
  Recusada porque o `proto` é formato de fio e certificado não é formato de fio:
  o crate deixaria de ter uma responsabilidade só.
- **Um crate novo só para identidade TLS.** Recusada por peso: um crate, uma
  entrada no workspace e uma fronteira nova para poucas linhas que nenhum dos
  dois lados disputa.
- **TOFU entre clientes**, com pino por par. Recusada por inventar um armazém de
  pinos e uma decisão de confiança onde já existe um intermediário que os dois
  lados confiam.
- **Escuta em porta própria, separada da conexão com o servidor.** Recusada pelo
  §3.1: perderia o mapeamento de NAT vivo, que é a melhor propriedade do
  desenho, e abriria uma porta a mais na casa de quem empresta.
- **Deixar o opt-in para o B.** Recusada pelo §5: custaria uma segunda versão de
  protocolo para carregar uma decisão já tomada.

## 10 · O que fica para o B e para o C

**B — a árvore.** Quem serve quem, com que leque, e como ela se refaz quando
alguém sai. Depende do que o §6 medir.

**C — o interruptor adaptativo.** Quando malha e quando estrela, e o que a
pessoa vê acontecer. O gatilho natural é o mesmo teto do §5.1 que o portão de
admissão já usa: enquanto a estrela entrega a qualidade pedida, ela basta; a
malha entra quando ela deixa de entregar.

## 11 · O que continua em aberto

- **A subida de quem assiste não é medida.** O servidor mede a própria desde o
  ADR 0044; a de cada cliente, não. A árvore do B precisa dela para escolher quem
  fica no meio, e a decisão de 05/09 foi que a malha se mede a si mesma — quem é
  promovido a repassar passa a encher o próprio cano, e a `caminho::Sonda` que já
  existe no cliente mede de graça. **O primeiro palpite de cada pessoa continua
  sendo um palpite**, e a rede de segurança é o §7.
- **Quantos pares um cliente serve ao mesmo tempo** é decisão do B, e o A1 não
  a antecipa: aqui é sempre um.
- **O aviso a quem entra na sala** — que a malha está ligada e o que ela expõe —
  é interface, e a tela dele é do B. O A1 carrega o dado; não desenha a frase.
