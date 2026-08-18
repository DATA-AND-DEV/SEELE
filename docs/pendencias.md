# Pendências conhecidas

O que está quebrado ou frouxo e ainda não foi resolvido. Ordenado por quanto
atrapalha na prática, não por dificuldade.

Uma entrada que fecha **não sai da lista e não é renumerada**: os números são
citados de fora — "pendência #9" aparece em `docs/` e em `specs/` — e renumerar
faria cada citação apontar para outra coisa. Ela fica no lugar, marcada como
fechada, com a data e com o que a substituiu.

## 1 · Estreitada em 2026-08-17 · Rajada perde entrega quando um par para de ler

**Sintoma original.** Dez mensagens de ~3,9 KB enviadas em rajada, sem o
receptor ler no meio: só duas chegam. As mesmas dez, com o receptor drenando
entre lotes, chegam todas. Corpos pequenos chegam todos em qualquer ordem.

**Uma afirmação daqui estava errada.** Esta seção dizia "não é o conserto de
cancelamento: o comportamento é idêntico antes e depois". Aquela comparação foi
feita quando só o **cliente** tinha sido consertado; a sessão do servidor
continuou lendo quadro dentro de um `select!` até o defeito derrubar o
`acceptance_m5` no Linux e ser diagnosticado de verdade. Descartar o
cancelamento com meia correção na mão não valia nada, e a frase saiu.

### O que foi encontrado, medido e consertado

**O caminho que reproduz é o par que para de ler.** A sessão escreve para o par
de dentro do mesmo `select!` em que lê o barramento de eventos. Quando o par
para de ler, a janela do QUIC fecha, a escrita bloqueia, e **enquanto ela está
bloqueada ninguém tira evento do barramento**. O barramento é um `broadcast` de
anel fixo: passado o anel, o mais antigo é descartado e a leitura seguinte
devolve `Lagged(n)`. Um `let Ok(event) = event else { continue }` transformava
isso em nada — a sessão seguia, calada, com um buraco permanente no que aquele
piloto vê, e sem um número em lugar nenhum.

Medido em `crates/seele-server/tests/par_lento.rs`, que **reprovava antes do
conserto**: 969 de 1160 mensagens chegam, 191 somem, o piloto segue conectado e
nenhum dos dois lados fica sabendo. As 1160 estão gravadas em CASPER — o que se
perde é a entrega, não a mensagem.

**O conserto** é encerrar a sessão com `DisconnectReason::FellBehind`, contando
em `Dogma::atrasos` quantos eventos morreram. Não é castigo: o buraco não tem
remendo no lugar, porque evento não tem endereço — o Dogma não sabe dizer quais
faltaram e o cliente não sabe pedir. Reconectar e buscar histórico repõe tudo, é
caminho que já existe, e a bateria interna o percorre sozinha.

### O que **não** reproduziu, e isso importa

**O sintoma original — dez mensagens, duas chegando — não reproduz no macOS.**
Nem com o `Client`, nem com um par cru. Mais do que isso: com as duas tarefas
leitoras dedicadas no lugar, a condição da pendência ("sem o receptor ler no
meio") deixou de ser alcançável pelo `Client` — a tarefa leitora dele drena o
fluxo para um canal sem limite, então um cliente que não chama `next_event`
continua esvaziando a janela do QUIC. Quem ainda para de ler é outro par: um
cliente de terceiro, ou uma casca cuja tarefa travou.

Isso deixa a causa do 10/2 **provável e não provada**: o cancelamento dos dois
lados explica cada observação registrada aqui, inclusive a de corpo pequeno
chegar sempre (quadro que cabe num pacote termina a leitura sem ceder, então
nunca é cancelado no meio). O mecanismo está provado em
`crates/seele-core/src/frame.rs`; o que não está é que fosse ele o autor deste
sintoma.

### As três suspeitas, uma a uma

1. **Janela de controle de fluxo no começo da conexão — morta com medida.** A
   janela por stream do quinn abre em 1,25 MB; dez corpos de 3,9 KB são 39 KB.
   `tests/rajada.rs` afirma que nenhum dos dois lados jamais emitiu
   `STREAM_DATA_BLOCKED` nessa rajada, via `Client::flow_control`.
2. **A fila da tarefa que grava em lote — não descarta.** `Dogma::post` é canal
   limitado com `send().await`: cheio, ele faz contrapressão até a sessão, e a
   contrapressão volta pelo QUIC. Falha de transação já era registrada em log.
   Nenhum caminho ali perde calado.
3. **A tarefa leitora do cliente morrendo em silêncio — meia verdade.** A tarefa
   registra o erro e fecha o canal, e o `next_event` seguinte falha, então a
   morte é observável. O que engolia eram os **testes**: dois `if let Ok(Ok(_))`
   no m4 e no m5 trocavam um enlace caído por um prazo esgotado com a frase
   errada. Consertados.

### O que ficou instrumentado

`Dogma::atrasos` (eventos e sessões), `Client::flow_control` (os quadros
`*_BLOCKED` dos dois sentidos, lidos do quinn), um aviso no fim de cada sessão
com quantas vezes o controle de fluxo prendeu a escrita para aquele cliente, e
`Server::quantas_mensagens` / `Server::mensagens_da_linha`, que abrem a pergunta
"o que o Dogma gravou" sem tornar público o `Casper::connection()` — que era o
obstáculo anotado aqui, e cuja abertura entregaria uma `rusqlite::Connection` e
faria do esquema o contrato.

**O que continua aberto.** Se o 10/2 tinha outra causa, ela reaparece na máquina
onde reproduzia — e agora há com que medir lá. Quem pegar isto: rode
`par_lento.rs` e `rajada.rs` no Linux, e leia `atrasos` e `flow_control`.

**Quando dói.** Colar um texto longo, ou um cliente reconectando e recebendo
histórico em rajada. Não apareceu em uso normal.

## 2 · Estreitada em 2026-08-17 · A reprodução perde amostras devagar, o tempo todo

**O que foi feito.** O anel de reprodução ganhou **alvo**, e uma malha que o
segura ali reamostrando — `crates/seele-audio/src/pacing.rs`, tarefa M1.8. Ver
o ADR 0028 para a decisão e para o que ela custa de latência, e o
`docs/m1-medicoes.md` para os números. O que ainda não foi visto é o `:sync` de
um `plug --hospedar` de verdade parar de crescer; o que foi medido está abaixo.

**Uma conta desta seção estava errada, e o erro importa.** Aqui se lia que
"centenas de amostras por dezena de segundos dão algo da ordem de algumas
centenas de partes por milhão, que é a faixa em que dois relógios independentes
vivem". A aritmética supõe que a perda **é** a deriva, e isso só vale com o anel
encostado no fundo *e* a diferença saindo toda em amostra perdida. Medido neste
Mac com `cargo run --release -p seele-audio --example ritmo`, que dá voltas com a
forma do laço de voz contra o dispositivo de verdade: o cristal da saída está a
**12 ppm** do relógio desta máquina, não a centenas — e a malha, que chega ao
número por outro caminho, pediu 9. A deriva existe, é o que a malha cancela, e
**não era ela que estava produzindo a perda**.

O que produzia era o anel não ter reserva nenhuma. Sem malha, o fundo do anel
mediu **zero em todos os intervalos de dez segundos**, do primeiro ao último: o
anel raspa o fundo o tempo todo, e a perda sai quando o retorno de chamada do
dispositivo calha de cair lá. O bloco dele é de 512 quadros e é servido inteiro
ou o resto é inventado, então basta uma volta do laço atrasar para a próxima
chamada não achar o bloco. Isso explica a tabela abaixo de um jeito que deriva
constante não explica: 0 num intervalo de dez segundos e 128 no seguinte é
perda **por evento**, e não por vazamento uniforme.

E dá uma explicação candidata para a diferença TECLA/ABERTO que ficou sem
nenhuma: em ABERTO o portão está aberto, então o laço codifica e envia cinquenta
quadros por segundo que em TECLA ele não envia. Volta mais longa, vale mais
fundo, mais perda. Não está medido nos dois modos — quem for fechar esta
pendência mede.

**Medido depois**, com o mesmo `ritmo`, dez minutos, a máquina compilando Rust
no meio: `falta` **zero**, `anel cheio` **zero**, fundo entre 494 e 733
amostras, razão estável entre +4 e +24 ppm, nenhum grampo, uma reposição (a do
arranque). Sem a malha, na mesma máquina: 258 amostras perdidas em sessenta
segundos, e fundo zero o tempo todo.

**Sintoma.** "ÁUDIO LOCAL FALHANDO" acende sozinho e volta a acender depois de
apagar, com o áudio audivelmente bom.

**Medido**, com `:sync` num `plug --hospedar` sem ninguém do outro lado:

| | captura | saída |
|---|---|---|
| arranque | 832 | 320 |
| +10 s, modo TECLA | 832 | 320 |
| +10 s, modo ABERTO | 832 | 448 |

Duas coisas separadas, e uma delas eu já sabia errado por outro motivo:

1. **A captura estoura uma vez, no arranque, e nunca mais.** O fluxo começa a
   encher o anel antes de alguém drenar. Inofensivo — e era isto que acendia o
   aviso para sempre, antes de a regra virar derivada.
2. **A reprodução perde amostras continuamente**, algumas centenas por dezena
   de segundos. É pouco para ouvir e é suficiente para o aviso ser verdade.

**O que a medição desmentiu.** A suspeita era que o anel de captura só fosse
drenado ao transmitir, o que explicaria o aviso sumir no modo aberto. **Não é
isso**: os contadores crescem igual nos dois modos. O laço drena a captura
incondicionalmente, a cada 2 ms. A diferença que aparece na interface entre
TECLA e ABERTO ainda não tem explicação.

**A primeira suspeita era certa, e foi consertada.** Ela dizia: "o laço de
reprodução empurra um quadro de 20 ms por tica e recupera atraso somando 20 ms
ao alvo — se uma volta passar do prazo, ele não repõe o que ficou para trás".
Era isso mesmo. O conserto está contado na pendência 15, que é onde o mesmo
defeito ficou grande o bastante para ser ouvido; aqui ele só vazava devagar.

**Duas das outras suspeitas caíram, medidas.** Nesta máquina os dois
dispositivos rodam a 48 kHz e os dois conversores são passagem direta
(`cargo run -p seele-audio --example device_smoke`), então nem conversão de
taxa nem contagem de canais explicam o que se mediu aqui. Continuam de pé para
uma máquina cujo dispositivo **não** rode a 48 kHz — o `:audio` diz a taxa.

**O que sobrou.** Deriva de relógio — o laço produz 48 000 amostras por segundo
de `Instant`, o dispositivo consome no ritmo do cristal dele, e os dois não são o
mesmo. O `drift.rs` já documentava que a correção certa é **reamostrar** —
`RateConverter::adjust_ratio` existe e tem teste — e não descartar. Ligar isso ao
anel de reprodução era a tarefa M1.8.

A parte que faltava a esta leitura, e que só apareceu ao medir, está no alto
desta seção: a deriva aqui é de doze partes por milhão, e sozinha ela não
produzia a perda. O que ela faz é **drenar qualquer reserva** — doze ppm são
0,6 amostra por segundo, e um alvo de 21 ms leva meia hora para secar. Uma
reserva sem malha que a segure é uma reserva que dura o começo da conversa. É por isso que as duas metades do conserto são uma coisa só, e é
por isso que `specs/09-roadmap.md` pede dez minutos e não um.

**O que o `:sync` mostra agora.** Além de `LAÇO volta … · reposição … · anel …`,
a linha `RITMO {ppm} · anel {ms} de {alvo} · grampo … · reposição …`. O `anel`
dizia que o anel estava cheio ou vazio e nunca por quê; as três respostas
possíveis mandam para consertos diferentes — deriva sendo cancelada, razão fora
da faixa em que cristal vive (aí não é deriva: taxa diferente da anunciada,
dispositivo trocado), ou anel raspando o fundo (aí é a volta do laço, que é a
pendência 15).

## 3 · O instalador do Windows não põe `plug` no `PATH`

O `.exe` do NSIS instala o app e os dois programas de terminal em
`%LOCALAPPDATA%\Programs\SEELE`, e **não acrescenta essa pasta ao `PATH`**.
Quem instalou pelo app tem o `plug`, mas precisa do caminho inteiro para
chamá-lo.

O NSIS do Tauri aceita um gancho de pós-instalação que resolveria isso. Não
entrou agora porque não tenho Windows aqui para testá-lo, e um gancho errado
quebra o instalador inteiro — que é pior do que não ter `PATH`.

No macOS o problema é o mesmo e a saída está nas notas de release: dois
`ln -s`. No Linux o `.deb` já instala em `/usr/bin`.

## 4 · Estreitada em 2026-08-17 · Em CGNAT sem IPv6, ainda só na mesma rede

**O que era.** Fora da rede local, só funcionava com o anfitrião alcançável de
fora: VPS, porta encaminhada à mão, ou VPN. Atrás de um roteador doméstico não
conectava — e a mensagem não explicava isso, que era metade do problema.

**O que ficou no lugar.** Os degraus 2 e 3 do ADR 0022, em
`crates/seele-server/src/alcance.rs`. Ao hospedar, o SEELE sobe uma escada e
para no degrau mais alto que funcionar, sem que ninguém configure nada:

- **Degrau 3 — UPnP.** Pede a porta ao próprio roteador do anfitrião. Nenhum
  terceiro em lugar nenhum. Resolve boa parte das casas.
- **Degrau 2 — IPv6.** A escuta era `0.0.0.0`, que atende **só IPv4**: o Dogma
  não estava em IPv6 nem quando as duas pontas tinham. Agora é `[::]` com pilha
  dupla escrita à mão, e o cliente também deixou de ligar só em IPv4.
- **Degrau 1.** Continua sendo a resposta quando os dois de cima não dão.

O endereço que entra no `seele://` passa a ser o do degrau alcançado, e não
mais sempre o da rede local.

**O que ainda não tem saída.** **CGNAT sem IPv6 e sem UPnP.** Nesse caso o
roteador da casa abriria a porta de boa vontade, e o endereço dele também é
privado: não há para onde apontar. É o que o ADR 0022 já dizia que ficaria de
fora antes do degrau 4, e continua verdade. O degrau 4 — ponto de encontro —
não foi feito de propósito: ele custa uma decisão sobre metadado que o ADR quer
tomar em voz alta. O degrau 5, retransmissão, está fora de escopo por decisão.

**O que mudou é que agora isso é dito.** A escada não falha em silêncio: cada
recusa é uma variante nomeada com frase própria, ela aparece junto do link — e
não numa tela de diagnóstico —, e `docs/alcance-pela-internet.md` explica caso
a caso o que fazer. Um link que só funciona na rede de casa e um link que
funciona pela internet são o mesmo texto, e era isso que fazia o anfitrião
mandar o primeiro achando que mandou o segundo.

**Uma coisa que o ADR não previa.** Ele trata CGNAT como um caso em que UPnP
não funciona. Não é: o roteador atende o pedido e **abriria a porta com
sucesso**, na WAN dele, que não sai para a internet. Não é um erro que se possa
mostrar — é um sucesso mentiroso. Por isso o endereço externo é conferido antes
de mapear. Na primeira rede real em que rodou, era exatamente esse o caso.

## 5 · Fechada em 2026-08-15 · Não havia limitação de taxa

**O que era.** `DisconnectReason::RateLimited` existia no protocolo e **nunca
era enviado**. Um convidado legítimo podia inundar o Dogma de mensagens, e
qualquer um podia bater à porta em laço — cada tentativa comprando um Argon2id
inteiro de CPU do anfitrião, que o ADR 0021 escolheu caro de propósito.

**O que ficou no lugar.** Um balde de fichas, em `crates/seele-server/src/taxa.rs`,
consultado em três lugares. ADR 0025 conta as escolhas; em resumo:

- **Antes de autenticar**, por endereço de origem, no primeiro instante de cada
  conexão: trinta apertos de mão de rajada, trinta por minuto de reposição.
  Estourar responde `RateLimited` **com motivo**, antes de o `Hello` ser lido.
- **Depois de autenticar**, por conexão: sessenta quadros de controle de
  rajada, vinte por segundo. O primeiro excedente rende `AlertReason::RateLimited`
  — variante nova, porque derrubar calado é o que faz alguém achar que o
  produto quebrou —, os seguintes são descartados, e ao ducentésimo a conexão
  cai com `RateLimited`.
- **A mídia** já tinha limite, em janela fixa de um segundo; passou a usar o
  mesmo balde, e com isso perdeu a borda que deixava passar o dobro da taxa na
  virada da janela.

**Como se sabe que não é enfeite.**
`crates/seele-conformance/tests/limite_de_taxa.rs` cobra as duas pontas contra
um Dogma de verdade, e o mecanismo tem teste próprio com o tempo entrando por
parâmetro — nenhum `sleep`, nada dependendo de a máquina estar desocupada. Cada
teste foi visto reprovar com o código sabotado antes de ser dado por bom: o
balde que nunca esvazia, a portaria que sempre deixa passar, o vigia que só
passa, o que nunca avisa, o que nunca derruba, e o balde reescrito de volta
como janela fixa.

**O que continua de fora.** O balde é consultado depois de o QUIC ter feito o
aperto de mão TLS: quem só abre conexões e não fala ainda gasta uma assinatura
por tentativa. Fechar isso é `Incoming::refuse()` do quinn, antes de qualquer
cripto, e tem o custo de recusar sem conseguir dizer por quê. Fica anotado no
ADR 0025 como o degrau seguinte, para o dia em que um Dogma for de fato
inundado.

## 6 · Apelido é validado só por tamanho

Trinta e dois bytes, e nada sobre o conteúdo. O terminal está protegido — o
ratatui filtra todo caractere de controle, verificado — e o app usa
`textContent`. Sobra a possibilidade de sósia: caracteres de direção invertida
ou parecidos com os de outra pessoa no roster.

Baixo impacto num Dogma de amigos, real num aberto.

## 7 · A matriz de três SOs nunca foi verde por inteiro

Linux e Windows compilam no CI, mas ninguém rodou o `plug` neles fora disso.
`docs/teste-duas-maquinas.md` é o roteiro.

## 8 · Sem troca de chaves pós-quântica

Ao tirar o `aws-lc-rs` da árvore (para não exigir CMake e NASM no Windows)
perdeu-se o `prefer-post-quantum` do rustls. Nada protege contra gravar hoje e
decifrar depois. Aceitável para v1 — o modelo é TOFU sobre TLS 1.3 e E2EE de
mídia já é pós-v1 — mas é perda real.

## 9 · `:conectar` não reconecta em execução

O comando existe e avisa que não faz. **`:ejetar` agora resolve o caso comum**:
volta à tela de seleção, com a conexão e o áudio derrubados de verdade, e de lá
se escolhe outro Dogma. O que continua faltando é trocar de destino num comando
só, sem passar pela tela.

O que o laço externo mostrou é que o teardown fecha —
`crates/seele-conformance/tests/ejetar.rs` conecta, solta e conecta de novo no
mesmo processo. O que a pendência recusava era outra coisa: trocar a conexão por
baixo de uma sessão viva, com roster e áudio de pé.

## 10 · O esquema `seele://` não é clicável

Não está registrado no sistema operacional. Quando for, o cliente **precisa
perguntar antes de conectar**: um link que inicia conexão sozinho é superfície
nova. Ver ADR 0006.

## 11 · Reconectar rápido pode esvaziar o roster do Cage

**Sintoma esperado.** Alguém dá `:ejetar` e entra de novo em seguida. A sessão
nova sobe, fala e ouve normalmente, e o Cage aparece **vazio** — sem nem a
própria pessoa — até o movimento de alguém redesenhar a lista.

**O que se sabe.** É uma corrida entre a sessão que morre e a que nasce, e as
duas mexem na lotação pela mesma chave. `Occupancy::seat` começa apagando o
piloto de toda parte antes de sentá-lo (`dogma.rs:171-174`), e o desmonte da
sessão antiga chama `occupancy.vacate(cage, pilot)` (`session.rs:845`). Como
`vacate` filtra só por `PilotId` (`dogma.rs:177-181`), ele não distingue a
cadeira da sessão velha da cadeira da sessão nova: se o desmonte da primeira
chegar **depois** do `seat` da segunda, apaga a segunda. A ordem depende de
quando a conexão QUIC antiga é dada por morta, o que ninguém controla.

Só atinge a mesma identidade voltando — dois pilotos diferentes não colidem,
porque as chaves diferem. E o cliente não tem como serializar isso do lado dele:
`Drop for Enlace` é um `abort()`, que é assíncrono.

**Encontrado lendo, não observado.** Saiu da revisão do
`crates/seele-conformance/tests/ejetar.rs`, ao perguntar por que os dois lados
do teste usavam a mesma semente. **Não foi reproduzido em uso**, e fica
registrado como defeito de leitura, e não como relato de campo.

**A janela tem dois tamanhos, e o segundo não é estreito.** Com a rede
entregando, o `CONNECTION_CLOSE` da conexão antiga chega e o Dogma desmonta
aquela sessão em milissegundos — aí a corrida exige que o desmonte caia depois
de um handshake inteiro, e é de fato improvável. Mas o `CONNECTION_CLOSE` é um
pacote só e não é retransmitido: se ele se perder, o Dogma não fica sabendo de
nada e só derruba a sessão pelo tempo ocioso, que é o
`seele_proto::transport::IDLE_TIMEOUT` de **20 s**. Contra um handshake com
orçamento de 10 s, a janela deixa de ser uma corrida e passa a ser a regra —
qualquer volta dentro desses 20 s cai nela. Perder um datagrama numa rede real
não é exótico, e é justamente ao ejetar por causa de uma conexão ruim que se
volta depressa.

**O que ficou tentado.** Nada, de propósito — mas o `ejetar.rs` foi escrito para
não depender disto: o teste que mede lotação usa duas identidades distintas, e o
que faz a mesma pessoa voltar não olha a lotação. Está comentado nos dois
lugares, senão alguém junta os dois "simplificando" e ganha uma reprovação
intermitente no lugar do defeito.

**Por que não foi resolvido.** O conserto é no Dogma, não no cliente: `vacate`
precisa saber de qual sessão veio o pedido — carregar o `SessionId` no
`Occupant` e só desocupar se for o mesmo —, e isso mexe em `seat`, `vacate`,
`vacate_everywhere` e nos avisos de roster. É tarefa própria, com revisão
própria, e não um remendo no fim de uma tarefa de teste.

**Quando dói.** `:ejetar` seguido de reconexão imediata no mesmo Cage, que é
exatamente o que a tela de seleção convida a fazer. Some assim que qualquer
pessoa entra ou sai, porque aí o roster é reconstruído.

## 12 · Fechada em 2026-08-13 · A conferência da impressão digital do convite

**O que era.** O app lia a impressão digital de um `seele://` e não a conferia:
colar um link com impressão conectava como se não houvesse impressão nenhuma.
O `plug` conferia — ou parecia conferir: comparava a impressão esperada com ela
mesma, porque `PinDecision::Matches` não carregava a ofertada, e era um teste
que não tinha como reprovar. Duas cascas, dois comportamentos, nenhum dos dois
o que o ADR 0006 desenhou.

**O que ficou no lugar.** Uma decisão só, em `seele-core`, com cinco desfechos
nomeados (`tofu::Verdict`). A impressão do link atravessa a ponte
(`ConnectConfig::expected_fingerprint` → `Destino::impressao_esperada`) e a
comparação acontece antes de haver sessão. No primeiro contato, um convite que
não confere **recusa**: derruba a conexão e desfaz o pin que o TLS já tinha
escrito — sem essa segunda metade a recusa seria decorativa, porque a visita
seguinte, sem link para conferir, entraria calada no servidor recusado. Contra
um Dogma já fixado, um convite que discorda **avisa** e não derruba: o TOFU já
provou que é o servidor de ontem, e trancar alguém para fora por causa de um
link velho seria o erro oposto. As duas cascas leem o mesmo veredito; o `plug`
não compara mais nada por conta própria.

**A segunda ponta, do mesmo fio, também fechou.** O `Session::convite` morre
com a sessão que ele abriu e é descartado quando o endereço no campo não é o do
convite. Enquanto nada era conferido isso era inerte; deixou de ser no mesmo
dia em que a conferência passou a existir.

**Como se sabe que não é enfeite.** `crates/seele-conformance/tests/convite.rs`
prova os três desfechos contra um Dogma de verdade — a impressão certa
verificando, a errada recusando e desfixando, e o link velho avisando sobre uma
sessão que continua falando. Cada um foi visto ficar vermelho com a política
desligada antes de ser dado por bom.

**O que sobrou, e é de outra entrada.** A faixa de veredito da janela nunca foi
desenhada para um humano — é a mesma ausência da pendência 13, e é lá que ela
está contada.

## 13 · As três telas novas do app nunca foram vistas por ninguém

**Sintoma.** Não há sintoma relatado, e é justamente esse o problema. Três
regiões da janela — a lista de Dogmas visitados na tela de entrada, a faixa de
veredito de identidade que a sessão acende, e a barra de busca com o contador
`[n/m]` — foram escritas, testadas por fora e **nunca desenhadas para um
humano**. O ambiente onde este ramo foi feito não consegue capturar tela, e
`cargo tauri dev` abre uma janela que ninguém está lá para olhar.

**O que se sabe.** O que dá para afirmar sem ver está afirmado, e não é pouco:
`apps/seele-app/tests/frontend.rs` amarra cada `invoke` a um comando registrado
e cada `$("id")` a um id que existe na página, `apps/seele-app/tests/tokens.rs`
recusa cor que não esteja nos tokens, e a aritmética do realce é conferida
sobre os mesmos tipos que atravessam a ponte. Nada disso alcança o que só o
olho alcança: se o contador cabe ao lado do campo em janela estreita, se a
lista de visitados empurra o formulário para fora da tela quando tem vinte
entradas, se o realce corrente se distingue do resto de verdade e não só no
papel, se a faixa de veredito aparece onde alguém vai ler.

**O que ficou tentado.** Nada — e a ausência é o registro. `specs/06-clientes-gui.md`
já descreve as três como prontas, e é essa diferença entre "descrito" e "visto"
que esta entrada existe para não deixar passar calada.

**Por que não foi resolvido.** Precisa de uma máquina com tela e de alguém na
frente dela. Não é tarefa de código, e fingir que um teste de texto substitui
isso seria trocar a verificação pelo seu retrato.

**Quando dói.** Na primeira vez que alguém abrir o app depois de M5 — que é
tarde demais para descobrir que uma das três está torta. Ver
`docs/teste-duas-maquinas.md`, que é o roteiro onde este passo cabe.

## 14 · A janela reemite caractere em casamentos sobrepostos

**Sintoma.** No app (GUI), buscar um termo cujas ocorrências se sobrepõem —
"aa" em "aaa" — não só realça errado: o corpo da mensagem sai reescrito.
`occurrences` devolve `(0,2)` e `(1,3)`; `corpoComRealce`
(`apps/seele-app/ui/tela-sessao.js`) desenha os dois intervalos sem descontar a
sobreposição, e o caractere do índice 1 sai dentro de dois `<mark>` — "aaa"
que a pessoa escreveu aparece como "aaaa" na tela. É texto do usuário saindo
errado, não só a cor do realce.

**O que se sabe.** O terminal já resolve o mesmo caso. `ui.rs` (linhas
546-553) guarda `if start < cursor { continue; }` depois de já ter lido
`ordinal` de `*seen`, então pula só o desenho e mantém a contagem certa.
`corpoComRealce` não tem guarda equivalente nenhuma.

**O que ficou tentado.** Nenhum conserto nesta passada. Um registro anterior
(`.superpowers/sdd/2026-08-10-navegacao-gui-tui/final-fix-report.md`, achado
5) deu como motivo que consertar mexeria na numeração dos ordinais que o
realce corrente (`I1`) acabou de amarrar — **esse motivo está errado**. O
ordinal de `corpoComRealce` vem do índice do `.entries()` sobre `intervalos`,
não de um contador manual que a sobreposição pudesse atrapalhar; um
`continue` cedo, logo depois de ler `ordinal`, deixaria a numeração
exatamente como está — a mesma forma que `ui.rs` já usa do outro lado. O
conserto em si é barato.

**Por que não foi resolvido.** O que falta não é a dificuldade do conserto, é
prová-lo. `apps/seele-app/tests/frontend.rs` só confere o script como texto —
nomes de comando existentes, ids que aparecem na página — porque o projeto
não tem runtime de JavaScript no conjunto de testes: não há como executar
`corpoComRealce` e afirmar sobre o DOM que ela produz sem abrir uma janela.
Trocar o comportamento de um caminho que desenha toda mensagem, sem forma de
provar que o resultado bate, é um risco diferente de escrever o `continue`
em si.

**Quando dói.** Buscar um termo curto que se repete dentro de si mesmo — "aa",
"ll", "oo" — num corpo que o contém sobreposto. Raro, e visível assim que
acontece: a mensagem na tela deixa de ser a mensagem que a pessoa escreveu.

## 15 · Uma máquina ouve picotado e a outra não

**Sintoma.** Duas máquinas na mesma rede, o Mac hospedando o Dogma e o Windows
conectando. O Windows fala e o Mac ouve perfeitamente; o Mac fala e o Windows
ouve picotado. O texto atravessa inteiro nos dois sentidos, o tempo todo.

**O primeiro erro foi de leitura, e custou uma rodada.** A assimetria foi lida
como sendo do **sentido** — servidor → cliente falha, cliente → servidor não —
e daí se concluiu que o trecho suspeito era o servidor reenviando. Só que em
cada metade do teste **uma máquina só está reproduzindo**. "O Mac fala e o
Windows pica" e "a reprodução do Windows pica" produzem exatamente a mesma
observação, e a segunda leitura reabre o caminho de recepção inteiro, que a
primeira tinha descartado. Nada no relato distingue as duas.

**Medido.** Com `cargo run --release -p seele-core --example cadencia`, que dá
voltas com a mesma forma do laço de voz — sem microfone, sem rede, sem outra
máquina:

| | p50 | p99 | pior |
|---|---|---|---|
| volta do laço, neste Mac | 5,65 ms | 5,70 ms | 22,44 ms* |
| a mesma volta sem a soneca | 2,26 ms | 2,30 ms | 7,90 ms |

\* numa corrida com a máquina ocupada; numa ociosa o pior caso foi 5,80 ms.

**O defeito que isso encontrou.** O laço de voz conferia `if agora >= próximo`
e produzia **um** quadro de 20 ms. Isso se sustenta enquanto a volta durar bem
menos que 20 ms: cada volta entrega 20 ms de áudio gastando cinco de relógio, e
qualquer atraso é reposto. Assim que a volta passar de 20 ms, a mesma linha
vira vazamento permanente — 20 ms de áudio por volta, custando mais que 20 ms
de relógio —, e o anel de reprodução esvazia na diferença, para sempre.
Medido em teste: com uma volta de 31 ms saem **322 quadros onde o relógio pedia
499**, 64,5% do áudio, e os outros 35,5% saem como silêncio inventado pelo
retorno de chamada. É picotado, e nada no áudio recebido explica o buraco.

E a volta não dura o mesmo em toda parte. Ela é feita de duas esperas de
temporizador — `timeout(1 ms)` pela mídia e `sleep(2 ms)` no fim —, e cada uma
é arredondada para cima pela granularidade do temporizador do sistema antes de
somar. Onde essa granularidade é fina, 5,65 ms. Onde é grossa, dezenas.

**O que foi eliminado.**

- **Fragmentação de datagrama.** Era a hipótese principal, e ela é bonita: o
  texto vai em fluxo e se adapta ao caminho sozinho, a voz vai em datagrama e
  um datagrama que não cabe é recusado inteiro. A aritmética não fecha. Medido
  em `codec.rs`: o maior datagrama de voz que este build produz, com áudio de
  verdade no teto de bitrate, tem **272 bytes**. A RFC 9000 §14.1 exige 1200
  bytes de carga UDP no pacote Initial, então um caminho que não os entregue
  **não completa o aperto de mão** — não haveria texto atravessando para
  comparar. As duas metades do sintoma não podem ter a mesma causa. Está travado
  em teste, com folga de 3x, para o dia em que alguém aumentar o quadro.
- **Conversão de taxa neste Mac.** Os dois dispositivos rodam a 48 kHz e os dois
  conversores são passagem direta (`device_smoke`). Não foi eliminada do lado
  do Windows: o `:audio` diz a taxa daquela máquina, e ninguém olhou.

**O que ficou no lugar.** O laço passa a perguntar **quantos** quadros venceram
(`seele_audio::playout::PlayoutClock`) e produz todos, com teto de quatro para
não despejar uma hibernação inteira no anel. Isso torna a reprodução correta
independentemente de quanto a volta durar, que é a propriedade que faltava.

**E o que torna a próxima medição conclusiva.** Três coisas, e nenhuma delas
depende de hardware:

- `:sync` ganhou `LAÇO volta … · reposição … · reacerto … · recusa … · anel …`.
  `volta` acima de 20 ms diz, com número, que a máquina não acompanha o
  relógio pela via normal.
- um `tracing::warn!` uma vez por sessão quando a volta passa de um quadro.
- `examples/cadencia` roda em dez segundos, em qualquer máquina, e dá veredito.

**O que falta.** Rodar o `cadencia` no Windows. Se a volta de lá couber dentro
de um quadro, este diagnóstico está errado e a suspeita seguinte é a fila de
saída do servidor — o `cage.rs` conta `drops.subscriber_lagging` e **`drops()`
só é lido em teste**, então nada em produção mostra aquilo. Se a volta não
couber, a alavanca já está medida: tirar a soneca do fim do laço tira uma das
duas esperas de temporizador, e custou 3,39 ms de p50 aqui.

**Quando dói.** Sempre, em qualquer máquina cujo laço de voz não feche uma
volta em 20 ms — e a folga neste Mac era de 3,5x, não das dez que a forma
antiga supunha.

## 16 · A assinatura está pronta e não há credencial para ela

**Sintoma.** O SmartScreen mostra "O Windows protegeu o computador" e o
Gatekeeper diz que não consegue verificar se o app contém malware. Foi essa a
queixa que originou este trabalho: «Windows com erro com o controle inteligente,
precisamos urgentemente assegurar a confiabilidade do sistema».

**O que se sabe.** Não falta código. O `release.yml` já escreve o `signCommand`
do Azure Artifact Signing quando os três segredos existem, já instala a
ferramenta que assina, e já achava a identidade da Apple do mesmo jeito. O que
falta é comprar: uma conta paga da Apple, uma assinatura do Azure, e uma
validação de identidade que a Microsoft faz por gente e demora dias.

**O que ficou tentado.** ADR 0026 e `docs/assinatura-e-atualizacao.md`, que é o
passo a passo inteiro — de criar a conta a nomear cada segredo. Escrito porque
quem vai fazer isso é uma pessoa, uma vez, e não vai lembrar.

**Por que não foi resolvido.** Depende de cartão de crédito e de validação
humana; nenhuma das duas é trabalho de código.

**Quando dói.** Toda instalação. É o primeiro contato de quem baixa, e a frase
do macOS — a que oferece "Mover para o Lixo" — é a mais assustadora das três.

## 17 · Fechada em 2026-08-17 · O botão de atualizar existe em Rust e não tem tela

**Como fechou.** A tela existe: quinta seção do Terminal Dogma, `ATUALIZAÇÃO`.
Procurar não baixa, instalar instala o que a última procura mostrou, e nenhuma
das duas roda sozinha — o ADR 0026 pede as três coisas. O aviso que esta
pendência exigia está escrito antes do ato, com a parte que mais importa: se
houver um Dogma hospedado naquela janela, quem estiver dentro cai junto.

O andamento vem pelo canal `seele://atualizacao`. Quando o pacote traz `total`,
é barra com porcentagem; quando não traz, é travessão com o motivo no `title` —
a mesma resposta que a barra da bateria já dava para a mesma falta, em vez de uma
barra fingindo medir o que ninguém mediu. As seis variantes de `FalhaAoAtualizar`
têm frase própria, e duas delas dizem para **não** tentar de novo.

Os dois nomes saíram de `AGUARDANDO_TELA`, que foi o que o teste daquela lista
existia para cobrar.

**O que segue aberto e não é isto:** a pendência **16** — a assinatura ainda
espera credencial. Esta tela sabe atualizar; o que ela vai buscar só é confiável
depois que houver chave.

---

**Sintoma.** Não há como atualizar sem baixar o instalador de novo. Foi a segunda
queixa: «botão de atualizar para não precisar ficar baixando o exe no github toda
vez», e já custou um teste real — as duas máquinas ficaram em versões diferentes.

**O que se sabe.** A metade em Rust está pronta e testada de compilação:
`procurar_atualizacao` e `instalar_atualizacao`, em `apps/seele-app/src/main.rs`,
com o andamento do download saindo pelo canal `seele://atualizacao`. Os dois
nomes estão em `AGUARDANDO_TELA`, em `apps/seele-app/tests/frontend.rs`, e o
teste que lê essa lista falha no dia em que a tela chamar um deles — que é o
lembrete de tirá-lo de lá.

**O que ficou tentado.** Nada de interface: `apps/seele-app/ui/` estava sendo
reescrito por outro trabalho ao mesmo tempo, e desenhar por cima seria conflito
garantido.

**Por que não foi resolvido.** Falta a tela, e ela não é um botão qualquer:
`instalar_atualizacao` **fecha e reabre o SEELE** nos três sistemas, então quem a
desenhar tem um aviso a escrever antes — e, se houver um Dogma hospedado naquela
janela, dizer que quem estiver dentro dele cai junto.

**Quando dói.** Em toda versão nova, em toda máquina. E dói em silêncio: quem não
souber que saiu versão nova simplesmente continua na antiga.

## 18 · Fechada em 2026-08-17 · Anexos estão desenhados e não construídos

**Como fechou.** O ADR 0027 passou a aceito e foi construído inteiro, na ordem
em que ele próprio se justifica — o teto antes de qualquer byte trafegar, e a
tela por último.

**O teto.** `crates/seele-server/src/casper/attachments.rs`. A conferência
acontece contra o tamanho **declarado**, e o descarte também: receber e arrumar
depois deixaria o disco acima do teto pelo tempo da transferência, que é a
propriedade inteira que se perde. O que uma transferência em voo reserva conta
como se já estivesse em disco — sem isso duas subidas simultâneas olham para o
mesmo espaço livre e cada uma se acha cabível, que é o instante entre aceitar e
despejar. O teto por arquivo é derivado (um dezesseis avos do total) e não
configurado, para os dois números não poderem ser postos num par absurdo.

Quem hospeda escolhe com `seeled anexos 2G`, gravado na tabela `configuracao`.
Ausência da chave significa o padrão de 1 GiB, e nada é gravado: um Dogma que já
existia sobe com o teto sem que nenhuma migração escreva por ele.

**O caminho.** Um fluxo QUIC unidirecional por transferência, nos dois sentidos,
com o controle acima de toda transferência dentro da mesma conexão. Nenhum lado
segura o arquivo em memória. A resposta volta pelo controle como razão
enumerada, e são dez.

**A permissão é nova.** `Permission::AttachFile`, no fim da enumeração, com
migração 3 trazendo os papéis semeados para a frente — Comandante, Operador e
Piloto ganham, o Observador é **negado explicitamente**.

**O texto sobrevive ao arquivo.** Expirar apaga os bytes e mantém a linha, e a
página de histórico carrega o nome, o tamanho e um estado enumerado. É por isso
que uma mensagem cujo anexo saiu diz «este arquivo expirou» em vez de aparecer
vazia. A consequência está escrita no ADR e é aceita: a tabela de anexos nunca
perde linha.

**Duas coisas que o ADR descreve e que não foram construídas** estão anotadas no
alto dele: a prévia embutida de imagem — que exige conferir os bytes contra o
tipo alegado antes de escolher um decodificador, e enquanto não existir todo
anexo é um arquivo com nome e tamanho — e um seletor de arquivos nativo, que
custaria um crate a mais.

**O que segue aberto e não é isto:** as quatro coisas que o próprio ADR nomeia
como sem saída boa. Justiça sob teto global — uma pessoa com a permissão esvazia
o histórico de anexos de todo mundo sem estourar disco nenhum, e o balde de bytes
do `taxa.rs` **atrasa e não impede**; retomada de transferência caída, que
recomeça do zero e agora ao menos diz isso; concorrência entre conexões, que a
prioridade de fluxo não ordena; e o fato de que quem hospeda lê tudo. Nenhuma
delas foi tocada, e nenhuma delas foi fingida como resolvida.

---

**Sintoma.** Não dá para mandar imagem, nem áudio, nem arquivo. Foi o item 6 da
lista que veio do teste em rede local, e é a maior lacuna funcional que sobrou
depois que a limitação de taxa fechou (pendência 5) e a escada de alcance subiu
dois degraus (pendência 4).

**O que se sabe.** Tudo o que dá para saber sem escrever código está no
**ADR 0027**, que está **proposto** e não aceito: o Dogma guarda os anexos com
teto total fixo — 1 GiB por padrão, escolhido por quem hospeda — e ao encher
descarta o mais antigo, com a mensagem passando a dizer que o arquivo expirou. O
motivo da escolha é que um Dogma doméstico roda no notebook de alguém, e o pior
caso de disco tem que ser conhecido no dia um.

O ADR também decide o caminho: fluxo QUIC unidirecional próprio por
transferência, nunca o fluxo de controle — hoje existe **um** fluxo bidirecional
por conexão, e ele carrega aperto de mão, presença, comandos, texto e histórico
juntos. `MAX_DATAGRAM_LEN` não tem nada a ver com isto: aquilo é voz.

**O que ficou tentado.** Nada de código, de propósito. O que existe é o
documento, e ele existe antes do código pelo mesmo motivo que o ADR 0022 existiu
antes do degrau 4: as perguntas caras aqui não são de implementação. Quem
hospeda passa a poder ler toda foto que chega (`specs/08-seguranca.md` já põe
"vazamento de histórico por acesso ao disco do servidor" fora de escopo em v1, e
manda documentar), e um Dogma doméstico não varre vírus e não vai varrer.

**Por que não foi resolvido.** Falta a decisão humana sobre um ADR proposto, e
faltam quatro coisas que o próprio ADR nomeia como sem saída boa: justiça sob
teto global — uma pessoa com a permissão esvazia o histórico de anexos de todo
mundo sem estourar disco nenhum —, retomada de transferência caída, concorrência
entre conexões, e o fato de que quem hospeda lê tudo.

**Quando dói.** Nos primeiros cinco minutos de quem chega. É a lacuna que uma
pessoa nota sem ninguém apontar.

## 19 · Fechada em 2026-08-17 · A chave de idempotência reinicia, e a identidade não

**Sintoma.** Depois de reconectar, as mensagens de um piloto **não são
gravadas**. A primeira mensagem da sessão nova é tratada como reenvio da
primeira mensagem da sessão anterior, a segunda como reenvio da segunda, e assim
por diante. Ninguém é avisado dos dois lados.

**O mecanismo.** `Messages::append_batch` deduplica por `(author_id,
client_message_id)`. As duas metades dessa chave têm tempos de vida diferentes, e
é exatamente aí que ela quebra:

- `author_id` vem da chave Ed25519 **em disco** (ADR 0004) e é a mesma para
  sempre;
- `client_message_id` **recomeça em 1** — em `crates/seele-tui/src/main.rs` a
  cada sessão (`next_message_id: 1`, em dois lugares), e em
  `crates/seele-ffi/src/lib.rs` a cada processo (um `AtomicU64::new(1)` estático).

Então a chave que deveria ser única por mensagem se repete a cada reconexão.

**Como foi encontrada.** Não pelo sintoma: o agente que investigou a pendência 1
esbarrou nela lendo o caminho de escrita. Ela nunca apareceu num teste porque o
teste de idempotência que existia (`a_retried_send_does_not_post_twice`) reenvia
**o mesmo corpo** — e com corpos iguais a troca é invisível.

**Metade já consertada.** O caminho de deduplicação montava a resposta com a
mensagem que **chegou**: id da linha antiga, corpo novo, carimbo novo. Ou seja, o
corpo novo era anunciado ao vivo sob o id de uma linha que no disco guarda o
texto velho — quem estava com a janela aberta lia uma coisa e quem abrisse um
minuto depois lia outra, com o mesmo id, sem nada em lugar nenhum dizendo isso.
Agora a resposta é a linha realmente gravada, e há teste com corpos diferentes.

Isso conserta a **divergência**, e não a perda: a mensagem nova continua não
sendo escrita.

**O que falta decidir, e é por isso que não foi feito junto.** Onde fica a
fronteira da idempotência. `specs/02-protocolo.md` diz «idempotente por
`client_msg_id`», e o propósito é reenvio depois de confirmação perdida — o que
acontece sempre **dentro de uma conexão**. Duas saídas, e nenhuma é óbvia:

1. **A chave passa a ser única de verdade**, sorteada pelo cliente por sessão em
   vez de contada a partir de 1. `rand` já está em `seele-tui`; em `seele-ffi`
   não está.
2. **O servidor limita a busca à sessão corrente**, por exemplo com um
   `created_at >= início da sessão`. Não muda cliente nem esquema — mas junta
   duas sessões simultâneas da mesma identidade, que é raro e não impossível.

**Quando doía.** Em toda reconexão, que é o caminho mais comum deste produto:
cair o wi-fi, fechar o notebook, ser expulso e voltar.

**Como fechou, e o que apareceu no caminho.** Escolhida a saída 1: a chave passa
a ser sorteada. `seele-tui` sorteia por sessão e `seele-ffi` no arranque do
processo — a metade alta é sorteada, a baixa conta, o que deixa quatro bilhões de
mensagens antes de as duas poderem se encontrar. A saída 2 foi recusada por
juntar duas sessões simultâneas da mesma identidade.

Isto deixou de ser risco latente no meio do conserto. `seele-conformance/tests/
ejetar.rs::a_mesma_pessoa_volta_pela_tela_de_selecao` conecta a mesma identidade
duas vezes e fixava `ClientMessageId(1)` nas duas — e **passava por causa do
defeito**: o servidor devolvia o corpo que chegou vestindo o id da linha antiga,
então o eco batia e ninguém via que nada tinha sido escrito. Consertado o eco, o
teste caiu, e o que ele caiu provando é que **um piloto que reconecta não
conseguia falar**. O teste agora usa chave que não se repete, que é o que um
cliente correto faz. Ele também passou a rodar em 0,8 s em vez de 15: antes
esperava o prazo inteiro por um eco que nunca vinha.

Da mesma família da pendência 1 — destruía dado em silêncio —, por mecanismo
diferente.

## 20 · Fechada em 2026-08-17 · O convite anunciava um endereço que ninguém na mesma rede alcança

**Sintoma, de campo e não hipotético.** Um Windows hospedando e um Mac na mesma
casa. O Windows entra no Mac sem esforço; **o Mac não entra no Windows**. O link
saía com cara de certo e com a frase «alcança de qualquer lugar» embaixo dele.

**Três defeitos independentes, com o mesmo sintoma.**

1. **A escada declarava degrau que o socket não servia.** `Escada::subir`
   recebia só a porta. Naquela máquina a pilha dupla falhou e o Dogma recuou
   para `0.0.0.0` — comportamento certo, e medido: `Get-NetUDPEndpoint` mostrou
   `0.0.0.0:8383`. A escada, sem saber disso, achou o IPv6 global da máquina e
   declarou degrau 2. O convite anunciava um endereço IPv6 onde ninguém
   escutava. Independe de VPN: morde qualquer máquina cuja pilha dupla falhe, ou
   seja, Windows e os BSD. O predicado necessário — `Pilha::alcanca_ipv6` — já
   existia e não era perguntado a ninguém.
2. **A descoberta de endereço seguia a rota padrão**, que o Cloudflare WARP
   capturava. `endereco_de_saida_v4` abre um socket UDP e lê o `local_addr`, o
   que responde «qual endereço meu o sistema usaria para sair» — com VPN, o do
   túnel. O `192.168.0.30` da Ethernet não existia para o produto: nem no
   convite, nem no pedido de porta ao roteador, que mandava encaminhar para um
   endereço inexistente naquela rede. E o IPv6 do túnel é um unicast global de
   verdade, então passava por «IPv6 direto» sem nada que o distinguisse.
3. **O convite levava um endereço só**, o do degrau mais alto. Enquanto for um
   só, alguma situação sempre perde: o de fora não serve para quem está dentro —
   a maioria dos roteadores domésticos não faz *hairpin* — e o de dentro não
   serve para quem está fora. Isto tirou o caso que **já funcionava**: até a
   0.4.x o convite levava o endereço da rede local.

**Como fechou.** Os três consertos são diferentes e estão nos commits desta
entrada. `Escada::subir` passou a receber uma `Escuta` (porta e `Pilha`), e todo
endereço que entra num `Alcance` passa por um construtor privado que pergunta ao
socket — não dá para afirmar alcance sem perguntar, porque não há outro caminho
até o endereço. A descoberta passou a enumerar interfaces (`if-addrs`, um crate,
`libc` como única dependência), classificando placa de rede, túnel e ponte
virtual; o degrau 3 escolhe o endereço interno contra a sub-rede do roteador,
que é conta exata e não heurística. E o `seele://` ganhou `alt=`, com a lista
ordenada — rede de casa primeiro — e o cliente tentando um de cada vez.

Um degrau novo apareceu junto, `RedeLocalOuVpn`, para o caso em que o único
endereço que sai da máquina é de uma VPN: o que a pessoa faz a respeito é
desligar a VPN, e não mexer no roteador, e é esse o critério que o projeto usa
para separar variantes.

**A metade que não é código.** O firewall do Windows era a outra metade do
sintoma e foi resolvido à mão pelo dono: perfil da rede para `Private` e regra
de entrada UDP 8383. A caixa «Permitir que este aplicativo se comunique» **nunca
apareceu**, e é o esperado para escuta UDP de programa de console — quem espera
por ela espera para sempre. Está documentado em `docs/alcance-pela-internet.md`,
junto com a VPN.

**O que ficou de fora, e é pequeno.** Um Dogma hospedado numa máquina com IPv4
público direto — uma VPS, ou uma casa sem NAT — e sem UPnP continua caindo em
`SoRedeLocal`, cuja frase diz «só funciona na sua rede» quando na verdade
funciona de qualquer lugar. É um erro na direção segura (promete menos do que
entrega), não mudou nesta rodada, e a classificação de endereços que entrou
agora deixa o conserto barato: falta só o degrau que lê «este endereço é global e
está numa placa de rede».

## 21 · O degrau 4 está construído e o ponto de encontro padrão não está no ar

**O que existe.** O furo de NAT do ADR 0022 foi construído em 2026-08-17: o
serviço (`crates/seele-encontro/`), o lado de quem hospeda
(`crates/seele-server/src/alcance/encontro.rs`), o lado de quem entra
(`crates/seele-core/src/encontro.rs`) e o bilhete no `seele://` (`enc=`). Quem
sobe o próprio ponto de encontro tem o degrau 4 funcionando hoje —
`docs/ponto-de-encontro.md` são dez linhas de comando numa VPS.

**O que falta.** O endereço padrão, `encontro.seele.app`, é um nome **reservado
e ainda não publicado**. Enquanto ele não existir, a resolução falha em
milissegundos, a escada cai para o degrau de baixo, e a frase que a pessoa lê é a
mesma de antes deste degrau existir — ninguém fica esperando e ninguém recebe uma
promessa falsa. Mas o «funciona sem mexer em nada» só vale de verdade para quem
souber apontar o `SEELE_ENCONTRO`.

Publicar o nome é uma tarefa de infraestrutura, não de código: uma VPS pequena, o
binário, um registro DNS. Vale antes do próximo release que anunciar o degrau 4,
ou o anúncio promete o que a instalação padrão não entrega.

**O que fica sem saída mesmo com ele no ar.** NAT simétrico dos dois lados. O
mapeamento muda a cada destino, o endereço que o ponto de encontro viu não é por
onde o outro lado chegaria, e a resposta a isso seria retransmissão — que o ADR
0022 põe fora de escopo por decisão. Nesse caso continuam valendo o
encaminhamento de porta à mão e a VPN de rede, que é o que a frase do degrau 4
diz.

**O que nenhum teste automático cobre**, e onde está escrito o que fazer: o furo
em si, que precisa de duas redes atrás de NATs diferentes. O roteiro está na
seção 7 de `docs/teste-duas-maquinas.md`.
## 22 · MODs estão desenhados e não construídos

**Sintoma.** Não dá para mudar nada da aparência do produto. A tela de
configurações não oferece `TEMA` — e há teste cobrando que ela não ofereça
(`the_settings_screen_omits_what_the_product_lacks_instead_of_drawing_it_dead`,
`apps/seele-app/tests/frontend.rs`) —, porque `apps/seele-app/ui/index.html`
registra que «um segundo tema é uma segunda paleta canônica, e essa é decisão de
ADR, não de tela». `specs/00-visao-geral.md` põe «marketplace de plugins» como
não-objetivo de v1.

**O que se sabe.** Tudo o que dá para saber sem escrever código está no
**ADR 0029**, que está **proposto** e não aceito. Em resumo: um MOD é um arquivo
de valores em JSON, um por vez, no diretório do ADR 0017; ele **nunca escreve um
seletor** e nunca traz código, e é por isso que a CSP não afrouxa, que o conjunto
fechado de arquivos de `ui/` não muda, e que os quatro guardas do vermelho
continuam de pé — todos eles perguntam se uma regra *nomeia* o token, e nenhum
pergunta que cor o token guarda.

A palheta congelada do ADR 0014 vira o **piso**: o produto mede contraste e
distância em CIELAB na instalação, contra os valores que o próprio MOD declara,
com os pisos que cada token já cumpria — e recusa **por token**, mantendo o
nosso, sem interruptor para ignorar. O papel não se move: `vermelho-alerta`
continua sendo a única cor de alerta e de queda, porque quem decide isso são os
seletores.

O esquema é fechado e só cresce; **a versão 1 traz uma capacidade só, `cor`**,
pela regra de que uma capacidade entra quando a tabela e o consumidor dela já
existem — glifo, frase, som, atalho, comando e painel são recusados um a um com o
motivo. O `plug` fica com a palheta congelada em v1. Um MOD **não acompanha um
Dogma**: a sala pode recomendar, e instalar continua sendo ato de quem instala.

**O que ficou tentado.** Nada de código, de propósito — a mesma postura da
pendência 18 e do degrau 4 do ADR 0022: as perguntas caras aqui não são de
implementação. O indexador aprende metadado (quem baixou o quê, e quando), e por
isso é catálogo estático com busca no cliente, sem consulta automática ao abrir,
espelhável, opcional e trocável.

**Por que não foi resolvido.** Falta a decisão humana sobre um ADR proposto. O
ponto mais provável de ser derrubado está isolado de propósito: v1 com uma
capacidade só. E cinco coisas o próprio ADR nomeia como sem saída boa — feio não
se mede, distinção aos pares não é olho, o esquema só cresce, o indexador sabe
quem baixou o quê, e julgar exige ler.

**Quando dói.** Não dói em uso; dói em pedido. É a diferença entre um produto que
as pessoas usam e um que elas fazem seu.

## 23 · A portaria decide, e ninguém avisa quem hospeda que há alguém batendo

**O que existe.** A portaria do **ADR 0030** está construída inteira: a camada no
servidor (`crates/seele-server/src/portaria.rs`, migração 4), as duas razões de
protocolo, os sete comandos e a tela (`apps/seele-app/ui/camada-portaria.js`).
Quem hospeda pelo botão HOSPEDAR AQUI fecha o Dogma, gera convite e decide quem
entra sem abrir terminal, que era o buraco relatado.

**O que falta, em ordem de quanto atrapalha.**

**1 · O toque no ombro.** Um pedido pendente é uma linha em SQLite e sobrevive à
janela minimizada, ao app fechado e à máquina reiniciada — nada se perde por
ninguém olhar. Mas nada *chama*. Enquanto a janela está aberta, o botão da porta
conta os pendentes a cada cinco segundos; com a janela minimizada, quem bateu
espera até quem hospeda lembrar de olhar. O conserto é uma notificação do
sistema, e ela é do `tauri-plugin-notification` — que **não está nas
dependências**, e o `Cargo.toml` do app é território de outro trabalho agora.
Vale antes de o produto ser usado por quem hospeda o dia inteiro com a janela
fechada.

**2 · Quem espera tenta de novo à mão.** O desenho recusa segurar a conexão de
propósito — um prazo fabricaria a resposta «ninguém atendeu», que quem a recebe
não sabe o que fazer com ela — e por isso `AdmissionPending` derruba na hora,
com a frase mandando tentar depois. O que não existe é o cliente **oferecendo**
tentar de novo: hoje a pessoa reabre a entrada e aperta de novo. Um botão TENTAR
DE NOVO na tela de fim, com o endereço já preenchido, é barato e não muda o
protocolo. Repetição automática foi recusada em `seele-tui/src/text.rs`
(`worth_retrying`) e continua recusada: seria uma bateria batendo na porta de
outra pessoa por tempo indeterminado.

**3 · Pedido não vence, e a tabela não é varrida.** `portaria` guarda uma linha
por impressão digital, para sempre, inclusive de quem nunca entrou e nunca vai
entrar. Num Dogma exposto à internet, com portaria ligada, isso é uma linha por
chave que bater — contida pelo balde por endereço do ADR 0025, que responde antes
de o `Hello` ser lido, mas contida não é limitada. `convites` tem prazo e uma
varredura; `portaria` não tem nenhum dos dois, e não há tela que apague em lote.

**4 · Não administra o Dogma de outra pessoa.** Os sete comandos falam direto com
o CASPER da máquina que hospeda, e isso é decisão do ADR 0030, não descuido: a
alternativa era expor à internet a decisão sobre quem entra pela internet. O
efeito é que um Comandante remoto continua sem fechar a porta da casa alheia —
que é onde o ADR 0021 já tinha deixado a administração de verdade, na
alternativa 3, e continua lá.

**O que este trabalho encostou e não consertou.** `Melchior::unban` existe
(`crates/seele-server/src/melchior.rs:541`) e **não tem verbo de protocolo**: um
banimento só se desfaz por quem tem o arquivo do Dogma, à mão, e é isso que a
frase de confirmação do banimento diz. A portaria **não piora** aquilo, porque
não acrescenta verbo nenhum, e mostra a forma da saída: uma decisão que se desfaz
é uma linha que se apaga, e `unban` já é literalmente `DELETE FROM bans`. Falta a
ele só o caminho até a janela — e o caminho é o mesmo que estes sete comandos
percorreram.

**Uma coisa que mudou dentro do ADR 0021, e que vale saber.** O convite passou a
ser gasto por quem **entra**, e não por quem **bate**
(`admissao::Passe`/`gastar`). Era defeito antigo — um handshake que morresse
depois daquela camada queimava o convite de alguém que nunca entrou — e a
portaria o tornaria constante, porque uma batida pendente é o caminho projetado.
A corrida entre dois clientes com o mesmo convite continua sendo perdida no mesmo
`UPDATE ... WHERE usado_em IS NULL`.

**Quando dói.** O item 1 dói no primeiro uso real entre duas casas: quem bate
espera sem que ninguém saiba que ele bateu. Os itens 3 e 4 não doem em nada que
exista hoje.

