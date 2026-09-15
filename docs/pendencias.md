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
pessoa vê, e sem um número em lugar nenhum.

Medido em `crates/seele-server/tests/par_lento.rs`, que **reprovava antes do
conserto**: 969 de 1160 mensagens chegam, 191 somem, o pessoa segue conectado e
nenhum dos dois lados fica sabendo. As 1160 estão gravadas em PERSISTENCE — o que se
perde é a entrega, não a mensagem.

**O conserto** é encerrar a sessão com `DisconnectReason::FellBehind`, contando
em `Server::atrasos` quantos eventos morreram. Não é castigo: o buraco não tem
remendo no lugar, porque evento não tem endereço — o servidor não sabe dizer quais
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
2. **A fila da tarefa que grava em lote — não descarta.** `Server::post` é canal
   limitado com `send().await`: cheio, ele faz contrapressão até a sessão, e a
   contrapressão volta pelo QUIC. Falha de transação já era registrada em log.
   Nenhum caminho ali perde calado.
3. **A tarefa leitora do cliente morrendo em silêncio — meia verdade.** A tarefa
   registra o erro e fecha o canal, e o `next_event` seguinte falha, então a
   morte é observável. O que engolia eram os **testes**: dois `if let Ok(Ok(_))`
   no m4 e no m5 trocavam um enlace caído por um prazo esgotado com a frase
   errada. Consertados.

### O que ficou instrumentado

`Server::atrasos` (eventos e sessões), `Client::flow_control` (os quadros
`*_BLOCKED` dos dois sentidos, lidos do quinn), um aviso no fim de cada sessão
com quantas vezes o controle de fluxo prendeu a escrita para aquele cliente, e
`Daemon::quantas_mensagens` / `Daemon::mensagens_da_linha`, que abrem a pergunta
"o que o servidor gravou" sem tornar público o `Persistence::connection()` — que era o
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
um `connection --hospedar` de verdade parar de crescer; o que foi medido está abaixo.

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

**Medido**, com `:sync` num `connection --hospedar` sem ninguém do outro lado:

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

## 3 · O instalador do Windows não põe `connection` no `PATH`

O `.exe` do NSIS instala o app e os dois programas de terminal em
`%LOCALAPPDATA%\Programs\SEELE`, e **não acrescenta essa pasta ao `PATH`**.
Quem instalou pelo app tem o `connection`, mas precisa do caminho inteiro para
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
- **Degrau 2 — IPv6.** A escuta era `0.0.0.0`, que atende **só IPv4**: o servidor
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
era enviado**. Um convidado legítimo podia inundar o servidor de mensagens, e
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
um servidor de verdade, e o mecanismo tem teste próprio com o tempo entrando por
parâmetro — nenhum `sleep`, nada dependendo de a máquina estar desocupada. Cada
teste foi visto reprovar com o código sabotado antes de ser dado por bom: o
balde que nunca esvazia, a portaria que sempre deixa passar, o vigia que só
passa, o que nunca avisa, o que nunca derruba, e o balde reescrito de volta
como janela fixa.

**O que continua de fora.** O balde é consultado depois de o QUIC ter feito o
aperto de mão TLS: quem só abre conexões e não fala ainda gasta uma assinatura
por tentativa. Fechar isso é `Incoming::refuse()` do quinn, antes de qualquer
cripto, e tem o custo de recusar sem conseguir dizer por quê. Fica anotado no
ADR 0025 como o degrau seguinte, para o dia em que um servidor for de fato
inundado.

## 6 · Apelido é validado só por tamanho

Trinta e dois bytes, e nada sobre o conteúdo. O terminal está protegido — o
ratatui filtra todo caractere de controle, verificado — e o app usa
`textContent`. Sobra a possibilidade de sósia: caracteres de direção invertida
ou parecidos com os de outra pessoa no roster.

Baixo impacto num Server de amigos, real num aberto.

## 7 · Alargada em 2026-08-25 · A matriz de três SOs nunca foi verde por inteiro

**O que esta entrada dizia, e por que estava pequena demais.** Ela dizia que
Linux e Windows compilam no CI e que ninguém tinha rodado o cliente neles fora
disso — como se faltasse apenas exercitar o produto à mão. Faltava mais que
isso.

**Um crate de produto não compilava no Windows, e atravessou um release.**
`crates/seele-audio/src/device.rs` usava `winreg::enums::HKEY`, e o tipo mora na
raiz do crate — `enums` só carrega as constantes. O bloco inteiro é
`cfg(windows)`, então nenhum compilador de macOS jamais o viu. Entrou em
2026-08-24 com a detecção de consentimento de microfone e ficou assim até alguém
mandar compilar num Windows de verdade, em 2026-08-25.

Junto veio um segundo, mais antigo: `xtask/tests/empacotamento.rs` usava
`std::os::unix::fs::PermissionsExt` sem `cfg` nenhum, derrubando a compilação do
`xtask` no Windows com «cannot find `unix` in `os`».

**O que isso diz sobre o CI, e é a parte que importa.**
`.github/workflows/ci.yml` **tem** o job `windows-2022` rodando
`cargo test --workspace`, com `fail-fast: false`. Ele existia o tempo todo e
teria pegado os dois. Um guarda que existe, roda e é ignorado é pior que guarda
nenhum: ele produz a sensação de cobertura sem a cobertura. **Antes de tratar
esta pendência como técnica, é preciso olhar a aba Actions e descobrir se aquele
job está vermelho, se não está rodando, ou se não é obrigatório para integrar.**

**O que ficou no lugar.** `xtask/tests/plataforma.rs` reprova import que só
existe numa plataforma sem `#[cfg(...)]` que o autorize, e roda em qualquer
sistema — o defeito do `xtask` passa a aparecer na máquina de quem escreve.
Ele **não** cobre o caso do `winreg`: o import estava gateado corretamente e o
caminho é que estava errado, e só um compilador de verdade sabe se um caminho
existe. Para essa classe não há substituto para compilar nos três.

**Medido em 2026-08-25, numa máquina Windows real, por SSH.** Com os dois
consertos: `cargo check --workspace --all-targets` fecha, 174 testes passam,
`cargo build --release --bin seeled` produz o executável. Falha um binário —
`seele-app --test permissoes`, o único que constrói um app Tauri com webview —
com `STATUS_ENTRYPOINT_NOT_FOUND`. É ambiental daquela máquina e **anterior a
tudo isto**: um worktree em `d613fdc` com o mesmo conserto mínimo falha com o
mesmo código de saída, e o mesmo teste passa no macOS.

`docs/teste-duas-maquinas.md` continua sendo o roteiro para a parte que exige
gente.

## 8 · Sem troca de chaves pós-quântica

Ao tirar o `aws-lc-rs` da árvore (para não exigir CMake e NASM no Windows)
perdeu-se o `prefer-post-quantum` do rustls. Nada protege contra gravar hoje e
decifrar depois. Aceitável para v1 — o modelo é TOFU sobre TLS 1.3 e E2EE de
mídia já é pós-v1 — mas é perda real.

## 9 · `:conectar` não reconecta em execução

O comando existe e avisa que não faz. **`:ejetar` agora resolve o caso comum**:
volta à tela de seleção, com a conexão e o áudio derrubados de verdade, e de lá
se escolhe outro Server. O que continua faltando é trocar de destino num comando
só, sem passar pela tela.

O que o laço externo mostrou é que o teardown fecha —
`crates/seele-conformance/tests/ejetar.rs` conecta, solta e conecta de novo no
mesmo processo. O que a pendência recusava era outra coisa: trocar a conexão por
baixo de uma sessão viva, com roster e áudio de pé.

## 10 · O esquema `seele://` não é clicável

Não está registrado no sistema operacional. Quando for, o cliente **precisa
perguntar antes de conectar**: um link que inicia conexão sozinho é superfície
nova. Ver ADR 0006.

## 11 · Reconectar rápido pode esvaziar o roster da sala de voz

**Sintoma esperado.** Alguém dá `:ejetar` e entra de novo em seguida. A sessão
nova sobe, fala e ouve normalmente, e a sala de voz aparece **vazio** — sem nem a
própria pessoa — até o movimento de alguém redesenhar a lista.

**O que se sabe.** É uma corrida entre a sessão que morre e a que nasce, e as
duas mexem na lotação pela mesma chave. `Occupancy::seat` começa apagando o
pessoa de toda parte antes de sentá-lo (`server.rs:171-174`), e o desmonte da
sessão antiga chama `occupancy.vacate(voice_room, person)` (`session.rs:845`). Como
`vacate` filtra só por `PersonId` (`server.rs:177-181`), ele não distingue a
cadeira da sessão velha da cadeira da sessão nova: se o desmonte da primeira
chegar **depois** do `seat` da segunda, apaga a segunda. A ordem depende de
quando a conexão QUIC antiga é dada por morta, o que ninguém controla.

Só atinge a mesma identidade voltando — dois pessoas diferentes não colidem,
porque as chaves diferem. E o cliente não tem como serializar isso do lado dele:
`Drop for Enlace` é um `abort()`, que é assíncrono.

**Encontrado lendo, não observado.** Saiu da revisão do
`crates/seele-conformance/tests/ejetar.rs`, ao perguntar por que os dois lados
do teste usavam a mesma semente. **Não foi reproduzido em uso**, e fica
registrado como defeito de leitura, e não como relato de campo.

**A janela tem dois tamanhos, e o segundo não é estreito.** Com a rede
entregando, o `CONNECTION_CLOSE` da conexão antiga chega e o servidor desmonta
aquela sessão em milissegundos — aí a corrida exige que o desmonte caia depois
de um handshake inteiro, e é de fato improvável. Mas o `CONNECTION_CLOSE` é um
pacote só e não é retransmitido: se ele se perder, o servidor não fica sabendo de
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

**Por que não foi resolvido.** O conserto é no servidor, não no cliente: `vacate`
precisa saber de qual sessão veio o pedido — carregar o `SessionId` no
`Occupant` e só desocupar se for o mesmo —, e isso mexe em `seat`, `vacate`,
`vacate_everywhere` e nos avisos de roster. É tarefa própria, com revisão
própria, e não um remendo no fim de uma tarefa de teste.

**Quando dói.** `:ejetar` seguido de reconexão imediata no mesmo VoiceRoom, que é
exatamente o que a tela de seleção convida a fazer. Some assim que qualquer
pessoa entra ou sai, porque aí o roster é reconstruído.

## 12 · Fechada em 2026-08-13 · A conferência da impressão digital do convite

**O que era.** O app lia a impressão digital de um `seele://` e não a conferia:
colar um link com impressão conectava como se não houvesse impressão nenhuma.
O `connection` conferia — ou parecia conferir: comparava a impressão esperada com ela
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
um servidor já fixado, um convite que discorda **avisa** e não derruba: o TOFU já
provou que é o servidor de ontem, e trancar alguém para fora por causa de um
link velho seria o erro oposto. As duas cascas leem o mesmo veredito; o `connection`
não compara mais nada por conta própria.

**A segunda ponta, do mesmo fio, também fechou.** O `Session::convite` morre
com a sessão que ele abriu e é descartado quando o endereço no campo não é o do
convite. Enquanto nada era conferido isso era inerte; deixou de ser no mesmo
dia em que a conferência passou a existir.

**Como se sabe que não é enfeite.** `crates/seele-conformance/tests/convite.rs`
prova os três desfechos contra um servidor de verdade — a impressão certa
verificando, a errada recusando e desfixando, e o link velho avisando sobre uma
sessão que continua falando. Cada um foi visto ficar vermelho com a política
desligada antes de ser dado por bom.

**O que sobrou, e é de outra entrada.** A faixa de veredito da janela nunca foi
desenhada para um humano — é a mesma ausência da pendência 13, e é lá que ela
está contada.

## 13 · As três telas novas do app nunca foram vistas por ninguém

**Sintoma.** Não há sintoma relatado, e é justamente esse o problema. Três
regiões da janela — a lista de Servers visitados na tela de entrada, a faixa de
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

**Sintoma.** Duas máquinas na mesma rede, o Mac hospedando o servidor e o Windows
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
saída do servidor — o `voice_room.rs` conta `drops.subscriber_lagging` e **`drops()`
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

**Como fechou.** A tela existe: quinta seção do Terminal Server, `ATUALIZAÇÃO`.
Procurar não baixa, instalar instala o que a última procura mostrou, e nenhuma
das duas roda sozinha — o ADR 0026 pede as três coisas. O aviso que esta
pendência exigia está escrito antes do ato, com a parte que mais importa: se
houver um servidor hospedado naquela janela, quem estiver dentro cai junto.

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
desenhar tem um aviso a escrever antes — e, se houver um servidor hospedado naquela
janela, dizer que quem estiver dentro dele cai junto.

**Quando dói.** Em toda versão nova, em toda máquina. E dói em silêncio: quem não
souber que saiu versão nova simplesmente continua na antiga.

## 18 · Fechada em 2026-08-17 · Anexos estão desenhados e não construídos

**Como fechou.** O ADR 0027 passou a aceito e foi construído inteiro, na ordem
em que ele próprio se justifica — o teto antes de qualquer byte trafegar, e a
tela por último.

**O teto.** `crates/seele-server/src/persistence/attachments.rs`. A conferência
acontece contra o tamanho **declarado**, e o descarte também: receber e arrumar
depois deixaria o disco acima do teto pelo tempo da transferência, que é a
propriedade inteira que se perde. O que uma transferência em voo reserva conta
como se já estivesse em disco — sem isso duas subidas simultâneas olham para o
mesmo espaço livre e cada uma se acha cabível, que é o instante entre aceitar e
despejar. O teto por arquivo é derivado (um dezesseis avos do total) e não
configurado, para os dois números não poderem ser postos num par absurdo.

Quem hospeda escolhe com `seeled anexos 2G`, gravado na tabela `configuracao`.
Ausência da chave significa o padrão de 1 GiB, e nada é gravado: um servidor que já
existia sobe com o teto sem que nenhuma migração escreva por ele.

**O caminho.** Um fluxo QUIC unidirecional por transferência, nos dois sentidos,
com o controle acima de toda transferência dentro da mesma conexão. Nenhum lado
segura o arquivo em memória. A resposta volta pelo controle como razão
enumerada, e são dez.

**A permissão é nova.** `Permission::AttachFile`, no fim da enumeração, com
migração 3 trazendo os papéis semeados para a frente — Comandante, Operador e
Pessoa ganham, o Observador é **negado explicitamente**.

**O texto sobrevive ao arquivo.** Expirar apaga os bytes e mantém a linha, e a
página de histórico carrega o nome, o tamanho e um estado enumerado. É por isso
que uma mensagem cujo anexo saiu diz «este arquivo expirou» em vez de aparecer
vazia. A consequência está escrita no ADR e é aceita: a tabela de anexos nunca
perde linha.

**A primeira coisa que o ADR descrevia e não estava construída era a prévia
embutida de imagem, e ela caiu em 2026-08-18.** O relato de campo: *preview de
imagem/documento anexo*. O que faltava era a conferência que o próprio ADR
exige — os **bytes** contra o tipo alegado, antes de escolher decodificador — e
ela existe agora em `crates/seele-core/src/preview.rs`.

Quatro formatos, com o motivo de cada um escrito: PNG e JPEG porque sem eles o
recurso não existe, GIF porque é o que se cola numa conversa, WebP porque é o
que um navegador salva hoje. Fora ficaram SVG (marcação, não imagem), PDF
(documento com um interpretador atrás), HEIC e AVIF (marca dentro de uma caixa
que o `mp4` divide, e suporte desigual nos três alvos) e BMP/ICO/TIFF
(assinatura de dois bytes, e ninguém manda).

**As duas metades têm de concordar.** Farejar sozinho faria o nome virar
enfeite; a alegação sozinha seria confiar em quem mandou. Quando elas discordam
— um JPEG chamado `foto.png`, um executável chamado `gatinho.png` — não se
desenha, **e não se desenha como o que o arquivo por acaso é**, que seria o
mesmo erro pelo avesso. A caixa do anexo escreve o que ele disse ser e o que os
bytes dele são, diz que ele chegou inteiro e que o hash fechou, e deixa o
arquivo ali para salvar: não desenhar é diferente de esconder. É a separação que
as `NOTAS-DE-RELEASE` fazem entre «chegou inteiro» e «é o que diz ser», e a
segunda pergunta passa a ter resposta para estes quatro formatos.

**A busca acontece ao apertar, nunca ao rolar.** O anexo mora no servidor: ver é
baixar, e uma Linha que buscasse toda imagem ao rolar transformaria o teto de
disco de quem hospeda em banda de todo mundo. O que voltou fica guardado por
anexo, inclusive quando é recusa.

**O limite da prévia é 4 MiB, decidido separado do teto por arquivo**, que no
padrão é 64 MiB: aquele protege o disco de quem hospeda, este a memória de quem
lê. Conferido contra o tamanho declarado no cabeçalho, antes de um byte do
corpo, com o fluxo cortado em vez de drenado.

**Prever não é abrir e não é salvar**: os bytes vão para a memória e param ali,
nada toca o sistema de arquivos, e continua não existindo `abrir_anexo`. A CSP
não mexeu — `default-src 'self'` com `img-src 'self' data:` já bastava, e um
guarda reprova se ela ganhar `blob:` ou um `unsafe-`. Nenhuma dependência nova:
quem decodifica é o motor do WebView, e o base64 são vinte linhas conferidas
contra a RFC 4648 em vez de um crate.

**A segunda era o seletor de arquivos nativo, e ela caiu no mesmo dia.** O
relato de campo: o dono arrastou um arquivo e não aconteceu nada, e clicou no
botão ARQUIVO e não abriu nada. Eram dois defeitos sem causa comum. O arrastar
estava morto porque este app não tinha arquivo de capacidade nenhum e `listen()`
é chamada ao plugin `event`, que a ACL da Tauri v2 recusava — **todo** ouvinte
do frontend estava morto, e só este apareceu porque os outros têm um laço de
500 ms ao lado que redesenha a tela de qualquer jeito. O botão não abria nada
porque o ADR tinha decidido que escolher era arrastar. Arrastar não se descobre
sozinho: o seletor entrou, custou três crates contados nos três alvos, e a
última seção do ADR 0027 registra a reversão, o custo e a saída sem dependência
que foi procurada e não existe.

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
**ADR 0027**, que está **proposto** e não aceito: o servidor guarda os anexos com
teto total fixo — 1 GiB por padrão, escolhido por quem hospeda — e ao encher
descarta o mais antigo, com a mensagem passando a dizer que o arquivo expirou. O
motivo da escolha é que um servidor doméstico roda no notebook de alguém, e o pior
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
manda documentar), e um servidor doméstico não varre vírus e não vai varrer.

**Por que não foi resolvido.** Falta a decisão humana sobre um ADR proposto, e
faltam quatro coisas que o próprio ADR nomeia como sem saída boa: justiça sob
teto global — uma pessoa com a permissão esvazia o histórico de anexos de todo
mundo sem estourar disco nenhum —, retomada de transferência caída, concorrência
entre conexões, e o fato de que quem hospeda lê tudo.

**Quando dói.** Nos primeiros cinco minutos de quem chega. É a lacuna que uma
pessoa nota sem ninguém apontar.

## 19 · Fechada em 2026-08-17 · A chave de idempotência reinicia, e a identidade não

**Sintoma.** Depois de reconectar, as mensagens de um pessoa **não são
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
teste caiu, e o que ele caiu provando é que **um pessoa que reconecta não
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
   recebia só a porta. Naquela máquina a pilha dupla falhou e o servidor recuou
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

**O que ficou de fora, e é pequeno.** Um servidor hospedado numa máquina com IPv4
público direto — uma VPS, ou uma casa sem NAT — e sem UPnP continua caindo em
`SoRedeLocal`, cuja frase diz «só funciona na sua rede» quando na verdade
funciona de qualquer lugar. É um erro na direção segura (promete menos do que
entrega), não mudou nesta rodada, e a classificação de endereços que entrou
agora deixa o conserto barato: falta só o degrau que lê «este endereço é global e
está numa placa de rede».

## 21 · Fechada em 2026-08-18 · O degrau 4 está construído e o ponto de encontro padrão não está no ar

**Como fechou.** O ponto de encontro do projeto está no ar, numa VPS, e o
`PONTO_PADRAO` aponta para `encontro.seele.app.br`. A partir daqui **ninguém
precisa de variável de ambiente em máquina nenhuma**: quem hospeda já vem com o
endereço compilado, e quem entra o recebe dentro do próprio convite, no `enc=`.

Um nome e não um endereço, porque a constante viaja dentro de cada executável do
mundo — com um nome, trocar de VPS é um registro de DNS; com um IP, seria versão
nova e todo mundo reinstalando. Com uma rede de endereços embaixo, para o degrau
não sumir por um DNS ruim num dado dia; e o recuo vale **só** para o endereço
padrão, porque cair no nosso quando o de outra pessoa não resolve mandaria o
metadado dela para nós sem que ela tivesse pedido.

**Dois defeitos apareceram ao pôr no ar, e os dois eram nossos.** O serviço subia
servindo **só IPv4** — ele abre uma escuta por família, e sem marcar o socket
IPv6 como exclusivo o Linux o fazia de pilha dupla, colidindo com o IPv4 que já
estava ligado. Ficava `active (running)` com metade do trabalho feito, e o log
dizia exatamente isso desde o primeiro segundo. Foi achado por uma sonda nova
(`cargo run -p seele-encontro --example sondar`), que fala o protocolo em vez de
perguntar se o processo existe — e ela nasceu porque o documento mandava subir o
seu e não oferecia forma nenhuma de conferir.

**O que continua valendo:** NAT simétrico dos dois lados não fura, por decisão do
ADR 0022, e a frase do degrau 4 diz «deve funcionar» e não «funciona».

---

## 21 (registro anterior) · O degrau 4 está construído e o ponto de encontro padrão não está no ar

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
motivo. O `connection` fica com a palheta congelada em v1. Um MOD **não acompanha um
Server**: a sala pode recomendar, e instalar continua sendo ato de quem instala.

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

## 23 · Estreitada em 2026-08-18 · A portaria decide, e o aviso só alcança quem está com a janela aberta

**O que existe.** A portaria do **ADR 0030** está construída inteira: a camada no
servidor (`crates/seele-server/src/portaria.rs`, migração 4), as duas razões de
protocolo, os sete comandos e a tela (`apps/seele-app/ui/camada-portaria.js`).
Quem hospeda pelo botão HOSPEDAR AQUI fecha o servidor, gera convite e decide quem
entra sem abrir terminal, que era o buraco relatado.

**O que foi feito depois, em 2026-08-18, e o que sobrou.**

Um teste de verdade entre duas casas encontrou o buraco pelos dois lados de uma
vez: o amigo bateu, recebeu a frase certa, e ficou esperando; quem hospeda
olhava uma tela onde nada indicava nada. Dos dois lados parecia que o produto
não funcionava. Os itens 1 e 2 abaixo tratavam de metades disso, e as duas
metades foram construídas.

**1 · O toque no ombro — feito dentro da janela, e não fora dela.** Um pedido
pendente é uma linha em SQLite e sobrevive à janela minimizada, ao app fechado e
à máquina reiniciada; nada se perde por ninguém olhar. O que faltava era
qualquer coisa que *chamasse*.

O chip PORTA já contava os pendentes a cada cinco segundos, e ele **mora dentro
de `#tela-sessao`**: entrar numa jaula ou abrir o Terminal Server esconde a
`<section>` inteira e leva o número junto, que é justamente quando quem hospeda
está ocupado com outra coisa. Agora há uma faixa (`#portaria-batendo`, em
`camada-portaria.*`) fora de todas as telas, como a região viva e a varredura,
e ela sobrevive a toda troca de tela.

Ela é o mínimo que ainda é ver: sem `role="alert"`, sem modal, sem `focus()` e
sem desabilitar nada — quem hospeda pode estar falando numa jaula, e o
push-to-talk morre no instante em que o foco cai num campo de texto. Quem fala
por ela é o `#anuncio` que já existe, **uma vez por aparição** e não a cada
leitura. DEPOIS cala estas batidas, e a faixa volta quando o número sobe.

**O que continua faltando é o toque no ombro com a janela fechada**, que é uma
notificação do sistema, do `tauri-plugin-notification` — que **não está nas
dependências**. Vale antes de o produto ser usado por quem hospeda o dia inteiro
com a janela minimizada; enquanto ela estiver aberta, em qualquer tela, a faixa
resolve.

**2 · Quem espera tenta de novo à mão — feito, e não na tela de fim.** O desenho
recusa segurar a conexão de propósito — um prazo fabricaria a resposta «ninguém
atendeu», que quem a recebe não sabe o que fazer com ela —, e por isso
`AdmissionPending` derruba na hora. Faltava o cliente **oferecendo** tentar de
novo.

Esta entrada previa um botão na tela de fim. Ele foi para a tela de entrada em
Server Central (`#tela-auth`, `data-modo="espera"`), e o motivo é que a tela de
fim é sobre uma sessão que houve: aqui não houve nenhuma, e o que a pessoa
precisa não é de um botão solto mas de uma tela que diga *o que aconteceu*, *o
que fazer agora* e *o que não adianta fazer* — que o pedido não vence, que nada
está esperando desta ponta, que a aprovação não puxa ninguém de volta, e que
bater sem parar é o que o balde do ADR 0025 freia. A mesma tela perdeu, na
mesma passagem, os quatro campos que eram travessão em toda entrada: a contagem
de operadores, a rota, o codec e a chave local.

Repetição automática foi recusada em `seele-tui/src/text.rs` (`worth_retrying`)
e continua recusada: seria uma bateria batendo na porta de outra pessoa por
tempo indeterminado. `nothing_on_the_waiting_screen_knocks_again_by_itself` é o
guarda que impede alguém de reintroduzi-la achando que ajuda.

**O que falta, em ordem de quanto atrapalha.**

**3 · Pedido não vence, e a tabela não é varrida.** `portaria` guarda uma linha
por impressão digital, para sempre, inclusive de quem nunca entrou e nunca vai
entrar. Num Server exposto à internet, com portaria ligada, isso é uma linha por
chave que bater — contida pelo balde por endereço do ADR 0025, que responde antes
de o `Hello` ser lido, mas contida não é limitada. `convites` tem prazo e uma
varredura; `portaria` não tem nenhum dos dois, e não há tela que apague em lote.

**4 · Não administra o servidor de outra pessoa.** Os sete comandos falam direto com
o PERSISTENCE da máquina que hospeda, e isso é decisão do ADR 0030, não descuido: a
alternativa era expor à internet a decisão sobre quem entra pela internet. O
efeito é que um Comandante remoto continua sem fechar a porta da casa alheia —
que é onde o ADR 0021 já tinha deixado a administração de verdade, na
alternativa 3, e continua lá.

**O que este trabalho encostou e não consertou.** `Permissions::unban` existe
(`crates/seele-server/src/permissions.rs:541`) e **não tem verbo de protocolo**: um
banimento só se desfaz por quem tem o arquivo do servidor, à mão, e é isso que a
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

**Quando dói.** O item 1 doía no primeiro uso real entre duas casas, e foi ali
que ele apareceu: quem bate esperava sem que ninguém soubesse que ele bateu. O
que sobrou dele só dói com a janela minimizada. Os itens 3 e 4 não doem em nada
que exista hoje.


## 24 · Várias sessões estão desenhadas e não construídas

**Sintoma.** Não dá para estar em dois Servers ao mesmo tempo. O `+` da trilha
existe na tela, desabilitado, com a limitação escrita no `title`
(`apps/seele-app/ui/index.html:906`): «este produto mantém um Connection por vez: para
trocar de Server, use DESCONECTAR». `Session` guarda um
`connection: Mutex<Option<Arc<Connection>>>` (`apps/seele-app/src/main.rs:49`) e `connect`
recusa com `AlreadyConnected` (`main.rs:168-170`). Quem quer ler a Linha de outro
Server desconecta deste.

**O que se sabe.** Tudo o que dá para saber sem escrever código está no
**ADR 0031**, que está **proposto** e não aceito. Em resumo: várias sessões, um
caminho de voz. A sessão é do servidor — VoiceRooms, Linhas, histórico, roster,
telemetria, apelido, permissões. O microfone, a saída, o modo de voz, o A.T.
Field, o isolamento total, a tecla, a chave, o atualizador e o MOD são desta
máquina, e não se multiplicam.

**A resposta sobre voz é não**, e é decisão e não limitação: não dá para estar em
jaula de dois Servers ao mesmo tempo. O microfone é um e a pessoa é uma; a barra
de espaço deixaria de ter referente (`ui/tela-sessao.js:1714-1723` é um `keydown`
de janela sem alvo); e o orçamento do ADR 0009 é de um caminho, com 21 ms já
gastos pelo ADR 0028. Entrar num VoiceRoom de outro Server **ejeta** o anterior, e o
VoiceRoom que se deixou é nomeado.

**Três achados no código mudaram o desenho**, e valem mesmo sem o ADR:

1. **O áudio abre na conexão, não na entrada na sala de voz**
   (`crates/seele-ffi/src/lib.rs:1507-1531`; `enter_voice_room` não toca áudio,
   `lib.rs:565-567` e `1787-1795`). Três conexões abririam três `AudioIo` antes
   de alguém falar. O ADR move a abertura para a entrada no primeiro VoiceRoom, o que
   é melhoria de privacidade por si só.
2. **A troca de sessão embaixo de um caminho de voz vivo já está construída.**
   `Voice::reopen(media, ssrc)` (`lib.rs:1587-1626`) foi feito para reconexão e
   carrega A.T. Field, isolamento, modo, tecla e ganhos (`voice.rs:603-625`).
   Faltava o dono do caminho, não o mecanismo.
3. **Nada no `Event` diz de qual sessão ele é.** O canal é um
   (`main.rs:36`) e a `Bridge` emite o `Event` cru (`main.rs:110-116`). Com duas
   sessões isso não quebra: desenha a mensagem do servidor B na Linha do servidor A,
   calado. É o defeito mais barato de introduzir e mais caro de achar.

**O tamanho, sem estimar para baixo.** O `Snapshot` **não** vira plural — nasce
uma camada acima dele, para preservar `messages_revision` e porque as 28 funções
de desenho continuam desenhando um servidor cada. Ele **perde sete campos** de áudio
para um bloco de máquina. O `main.rs` tem **51 `#[tauri::command]`**, dos quais
**23 resolvem `session.connection()?`** e mais quatro alcançam o `Connection` de outra forma
(`set_talking`, `escolher_microfone`, `escolher_saida` e o `connect`, que é onde a
regra de uma sessão é aplicada). `apps/seele-app/ui/` são ~13 mil linhas, das
quais `tela-sessao.js` sozinho são 1771; a janela lê **23 dos 24 campos** do
`Snapshot`. Há ainda **23 variáveis de módulo no JavaScript que já são estado de
sessão** sem nunca terem sido chamadas assim, e o **endereço do servidor não está
no `Snapshot`** — ele sobrevive só no global `alvoDoServer`
(`ui/tela-auth.js:74`). Não há roteador: seis telas trocadas por `hidden`, e
nenhum conceito de sessão corrente em que rotear. **Nada no protocolo, no
servidor nem no banco muda** — é o que torna o custo grande em vez de perigoso.

**Uma suposição que estava errada, conferida antes de virar decisão.** O
`crates/seele-tui` **não consome `Snapshot`**: zero menções, e ele nem depende do
`seele-ffi` — só de `seele-core` e `seele-server`, pela regra do ADR 0002. Mudar
o `Snapshot` não custa nada ao terminal, ao contrário do que a primeira versão do
ADR dizia. O terminal fica para trás de outro jeito: ele continua com um servidor
por processo, e a resposta dele são dois terminais com dois `$SEELE_HOME` — que
dá duas identidades de brinde e não compartilha nem microfone, nem lista de
visitados, nem trilha.

**O teste que reprova no primeiro minuto tem nome:**
`the_add_server_button_promises_nothing_this_product_can_do`
(`apps/seele-app/tests/frontend.rs:2150-2183`), que exige `disabled`, `title`,
`aria-label` e que **nenhum script mencione `$("trilha-adicionar")`**. É a moldura
sendo cobrada como moldura; quando o `+` funcionar, ele vira o teste do
contrário.

**Um operador de outro Server pode inserir o seu connection.**
`crates/seele-server/src/session.rs:1220-1230`: `MovePerson` transmite
`PersonMoved` sem exigir que o pessoa já esteja num VoiceRoom. A regra do ADR 0031 é
que o servidor decide quem está na sala e **esta máquina decide para onde o
microfone dela vai**: um connection inserido por outra pessoa entra no roster e não
reivindica o caminho de voz.

**Por que não foi resolvido.** Falta a decisão humana sobre um ADR proposto. E
cinco coisas o próprio ADR nomeia como sem saída boa — uma chave para todos os
Servers, uma pessoa só fala num lugar de cada vez, a placa em segundo plano conta
que aconteceu e não o quê, o terminal não ganha nada pela segunda vez seguida, e
não há número para quantos Servers cabem.

**Quando dói.** Dói em uso, e não só em pedido: hoje ler a Linha de outro Server
custa a conversa em que a pessoa está.

## 25 · Personalização de um servidor está desenhada e não construída, e o nome é o pedaço barato

**Sintoma.** Um servidor não tem nada além do nome, e nem o nome se escolhe pela
janela: `hospedar` passa a string literal `"Casa"`
(`apps/seele-app/src/main.rs:372-376`), então **todo Server hospedado pelo botão
HOSPEDAR AQUI se chama Casa**, e é assim que ele aparece no cabeçalho de quem
entra. `ServerConfig::name` é campo de struct montada no `main.rs`
(`crates/seele-server/src/lib.rs:57-58`).

**O que se sabe.** Tudo o que dá para saber sem escrever código está no
**ADR 0032**, que está **proposto** e não aceito. Ele divide o pedido em três,
porque as três não são a mesma coisa.

**O nome é o caso fácil, e vale dizer que é fácil.** O campo já viaja
(`ServerMessage::Session.server`, `crates/seele-proto/src/control.rs:909`, com
teto de 64 em `MAX_CLIENT_NAME_LEN`), já chega ao `Snapshot`
(`crates/seele-ffi/src/types.rs:557`) e já é desenhado. Falta quem escreve: um
valor na tabela `configuracao`, pelo mesmo critério que o ADR 0027 usou para o
teto de anexos, e um acessor no `Hospedagem` falando direto com o PERSISTENCE local
como o ADR 0030 fez com a portaria — **sem verbo novo de protocolo**. Renomear
com o servidor no ar precisa de um evento, ou o nome novo só vale na próxima
conexão e a tela tem de dizer isso.

**A cor: um servidor não repinta a janela de quem entra.** É a mesma resposta do
ADR 0029 sobre MODs, e mais firme aqui, porque lá havia pelo menos o ato de
instalar em que pendurar o consentimento. O que um servidor ganha é **a placa dele
na trilha, 56 px, e mais nada** — aplicada por CSSOM num nó, nunca num token,
nunca numa folha, nunca num seletor. E ele declara **um nome de uma lista
fechada, nunca um valor**, que é o 0029 apertado em um grau: a lista é curta
(`laranja-nerv`, `fosforo`, `padrao-azul`, `osso` — quatro), o vermelho não está
nela, e a preferência local de quem olha vence, numa coluna a mais no
`conhecidos`. Quatro é pouco e está escrito; o que torna isso custo estético e
não funcional é a sigla na placa e a regra de `specs/06-clientes-gui.md:143`.

**O ícone é recusado em v1**, com três razões: a prévia embutida de imagem de um
anexo que alguém **pediu** ainda não está construída (ADR 0027, no alto dele); a
CSP não freia isto, porque `img-src 'self' data:` já aceita imagem embutida
(`apps/seele-app/tauri.conf.json:22`); e o quadro de aperto de mão tem 16 KiB
(`MAX_FRAME_LEN`) e já carrega VoiceRooms, Linhas, papéis e permissões. A saída está
escrita: fora do aperto de mão, endereçada por conteúdo, 128×128 e 16 KiB, lista
curta de tipos com os bytes concordando com a alegação, sem animação, e nunca
fora da placa.

**A ordem, porque um depende do outro e o outro não.** O **nome** não depende de
nada e pode ser construído sozinho hoje. A **cor** depende do ADR 0031: sem a
trilha não existe superfície pequena o bastante para uma cor escolhida por outra
pessoa, e a pergunta «um servidor pode repintar a janela alheia» só tem resposta
útil quando existe uma placa de 56 px para responder «pode pintar a dele». O
**ícone** depende da conferência de bytes que o ADR 0027 deixou anotada. Os três
não devem ser construídos juntos: juntá-los esconde o mais barato atrás do mais
caro.

**Por que não foi resolvido.** Falta a decisão humana sobre um ADR proposto. E
quatro coisas o próprio ADR nomeia como sem saída boa — quatro cores é pouco e
não há mais, a sigla é derivada e derivação erra, a cor não protege de imitação
(só a impressão digital protege), e um nome escolhido por outra pessoa aparece na
tela de quem entra sem moderação nenhuma.

**Quando dói.** O nome dói hoje, em uso: todo Server hospedado pelo app tem o
mesmo. A cor e o ícone doem em pedido, e a cor só passa a fazer sentido depois da
pendência 24.

## 26 · O degrau 2 é um palpite, e o PCP transformaria em fato

**Sintoma, medido em 2026-08-24.** Uma casa com IPv6 global publica os
endereços IPv6 dela no `seele://` e ninguém de fora entra por eles. Não é
defeito do endereço: o firewall do roteador vem fechado para entrada não
solicitada, que é o padrão de fábrica de todo roteador doméstico, e o
anfitrião **não tem como saber disso** — está escrito no próprio
`Degrau::Ipv6Direto`: «se o firewall do roteador deixar entrar, e isso não dá
para saber daqui».

**A prova, com controle.** Três pacotes UDP de uma VPS para o IPv6 do PC, na
porta 8384, com regra de firewall do Windows liberando: nenhum chegou. Os
mesmos três, do Mac na mesma casa, para o mesmo endereço e a mesma porta:
chegaram. A escuta funciona, o Windows deixa passar, o endereço está certo —
quem bloqueia é o roteador.

**O que isso custa a quem entra.** Os dois IPv6 ficam na frente da lista e
custam quatro segundos cada. Medido de um 5G contra um servidor real: 9,6 s
queimados em três candidatos sem chance, e o quarto respondeu em 358 ms. A
ordem já foi consertada (`4c9429c`) e a espera também (`3b5510f`), mas as duas
tratam o sintoma: o endereço continua sendo anunciado e continua não
funcionando.

### Fechada no código em 2026-08-24 · aberta na prova

**O PCP existe, pede, e confere.** O commit `002ccef` entregou
`crates/seele-server/src/alcance/pcp.rs`, ligado à escada em
`alcance.rs::abrir_firewall` e coberto por catorze testes. `crab_nat` fala a RFC
6887, `netdev` descobre o gateway — que o PCP não descobre sozinho —, e o pedido
leva o **nosso IPv6 global** como cliente, que é o que o transforma em abertura
de firewall em vez de mapeamento.

**Esta seção dizia «o que fecharia» e listava o que já estava feito.** Ela não foi
atualizada depois daquele commit, e o custo disso foi medido em 2026-08-31: uma
sessão inteira começou a reconstruir as 933 linhas de `pcp.rs` antes de conferir
se elas existiam. Uma pendência que descreve como futuro um trabalho entregue é
pior que uma pendência que não existe — a primeira manda alguém trabalhar de
novo, a segunda só não ajuda.

As duas armadilhas que esta entrada nomeava foram fechadas por escrito:

- **o recuo do `crab_nat` para NAT-PMP** devolveria um mapeamento IPv4 como se
  fosse abertura de firewall IPv6 — sucesso mentiroso, da mesma família do CGNAT
  que o degrau 3 já vigia. O caminho passou a ser `pcp::port_mapping` direto, e o
  recuo virou a falha `SoFalaNatPmp`;
- **o par externo é conferido contra o interno.** Se o roteador traduziu em vez
  de abrir, é `NaoFoiBuracoNoFirewall`, e o mapeamento é desfeito na hora.

E a disciplina que esta entrada mais cobrava foi respeitada: **o `Ok` não nomeia
degrau nenhum.** Ele produz `Tipo::GlobalLiberado`, que passa na frente do
`Global` cru e fica **abaixo** do `Refletido` — a ordem de `4c9429c` veio de
medição de campo, e um palpite melhor não vira uma observação.

### O que continua aberto, e é só isto

**Nenhum roteador disse sim ainda.** Medido na casa do commit: o roteador não
respondeu ao PCP nem ao NAT-PMP, em nenhuma das duas famílias. O degrau 2
continua palpite ali — a diferença é que agora ele **diz** que é.

A prova que falta é a que esta entrada sempre descreveu, e ela não é código: um
pacote entrando de fora, com controle. A VPS do ponto de encontro serve de sonda,
e o método é o do parágrafo «A prova, com controle» acima — três pacotes UDP da
VPS para o IPv6 da máquina, na porta da escuta, com o firewall do sistema já
liberando; e os mesmos três de outra máquina da mesma casa, como controle.

O critério de sucesso é estreito de propósito: **não basta o PCP responder `Ok`**.
O que se quer ver é o pacote de fora chegando **depois** do `Ok` e não chegando
antes dele, na mesma casa e na mesma sessão. Sem o «não chegando antes», o teste
não distingue um firewall aberto pelo pedido de um firewall que já estava aberto.

**Onde procurar um roteador que responda.** O PCP é comum em roteador de
operadora com IPv6 nativo e raro em roteador de varejo. Vale tentar em mais de
uma casa antes de concluir que o degrau não serve: uma amostra de um é o que esta
entrada tem hoje.

**Quando dói.** Hoje, em toda casa com CGNAT e IPv6 — que é a combinação mais
comum no Brasil de 2026. Quem cai nela depende do degrau 4, e o link do degrau
4 morre quando o app fecha (ver o aviso acrescentado em `d229074`).

### Estreitada em 2026-08-31 · o custo da série saiu; o firewall continua

Os 9,6 s medidos acima eram **custo de série**, e não custo de firewall. O
[ADR 0037](adr/0037-candidatos-do-convite-em-paralelo.md) trocou a série por uma
corrida escalonada: os quatro candidatos partem dentro de 750 ms em vez de um
depois do outro, e o bom fecha em ~1,1 s. Medido no
`um_convite_de_enderecos_mortos_termina_em_segundos_e_nao_em_dezenas`, que é a
versão de laboratório deste cenário: **4,02 s antes, 1,77 s depois**. O número
de campo é maior nos dois lados — lá os candidatos são públicos e gastam o prazo
inteiro de quatro segundos — e a razão entre eles é a que interessa.

**O que continua aberto é o firewall, e é o que esta entrada sempre foi.** Os
endereços IPv6 seguem sem responder; a corrida só faz com que não responder pare
de custar tempo de quem espera. Com PCP eles passam a responder, e aí os dois
consertos se somam em vez de um substituir o outro.

Uma frase da primeira redação do ADR 0037 estava errada e vale corrigir aqui,
porque ela contradizia esta entrada: dizia que «furar NAT não abre firewall»,
logo IPv6 não precisaria do aviso do ponto de encontro. Firewall IPv6 doméstico é
**stateful** — o pacote de saída que o aviso provoca abre o buraco de volta igual
ao NAT. O aviso serve para IPv6, e o PCP continua sendo o conserto para quem não
tem ponto de encontro.

## 27 · Os números do bitrate adaptativo são ponto de partida, não medida

**O que existe.** O [ADR 0036](adr/0036-bitrate-adaptativo-em-faixas.md) construiu
a malha que `specs/03-audio.md` pedia desde a primeira redação e que nunca tinha
sido escrita: três faixas — 48, 32 e 16 kbps — comandadas por perda de subida
medida no servidor a partir de lacunas de `seq`.

**O que não existe.** Nenhum dos cinco números foi medido contra rede de verdade:

| | valor | de onde veio |
|---|---|---|
| Faixas | 48 / 32 / 16 kbps | os extremos são da spec; o meio é o ponto médio |
| Limiar de descida | perda > 5% | `specs/03-audio.md` linha 55, textual |
| Limiar de subida | perda < 2% | histerese de três pontos, escolhida por argumento |
| Permanência | 10 s | duas janelas: uma para medir, outra para confirmar |
| Janela de medida | 5 s | ~250 pacotes a 50/s, para 5% não ser decidido por dois deles |

Só o limiar de descida vem da spec. Os outros quatro vêm de aritmética
defensável, que não é a mesma coisa que medida.

**Qual deles dói mais, se estiver errado.** A **janela**. Curta demais e a malha
persegue ruído, trocando de faixa por acaso — e cada troca reconstrói o encoder,
porque o binding do Opus não tem setter em tempo de execução. Longa demais e ela
reage depois de a conversa já ter picotado. Os outros quatro erram devagar; este
erra rápido e nos dois sentidos.

**Com o que se confirma.** Os perfis de `crates/seele-audio/src/netsim.rs`, que
já existem e já foram usados para o M1.7 — `lan`, `acceptance 5%`, `wifi`,
`mobile_poor`. O que se quer ver é a faixa acompanhando o regime sem trocar
quando o regime não muda. O número a olhar é quantas trocas por minuto cada
perfil produz: num perfil estável, zero.

**Quando dói.** Numa conexão que oscila em torno de 5% — que é justamente o
`acceptance 5%` do M1.7, o perfil que a spec escolheu como critério de aceite.
Não aparece em LAN, onde a perda é zero e a faixa nunca sai do teto.

## 28 · A defasagem da corrida vem do RFC, e não desta rede

**O que existe.** O [ADR 0037](adr/0037-candidatos-do-convite-em-paralelo.md) põe
os candidatos do convite para correr com `DEFASAGEM_ENTRE_CANDIDATOS` = 250 ms,
que é o número do RFC 8305.

**O que não existe.** Nenhuma medição em rede de verdade que diga que 250 ms é o
valor certo **aqui**. O que há é uma medição de laboratório da razão — 4,02 s
para 1,77 s no teste de endereços mortos — e a medição de campo da série, de
9,6 s, que veio de antes.

**O que confirma.** Refazer a medição da pendência nº 26 com a corrida no lugar:
cliente num 5G, quatro candidatos, três sem chance. O que se quer saber é o tempo
até o aperto de mão fechar, e se ele mudaria com defasagem de 150 ms.

**Um segundo número que ninguém mediu, e que importa mais.** O perfil de furos
deixou de gotejar e virou rajada: um candidato público custa três avisos, então
quatro correndo custam até **doze furos em ~750 ms**, contra doze espalhados por
dezesseis segundos. Cabe nos `FUROS_POR_JANELA` = 60 do anfitrião, que são por
dez segundos — a conta fecha. O que não foi medido é o que doze furos quase
simultâneos fazem com um roteador doméstico ruim, e essa é uma pergunta sobre o
aparelho, não sobre este código.

**Quando dói.** Numa casa cujo roteador tenha tabela de NAT pequena ou limite de
criação de mapeamento por segundo. Não aparece em LAN, onde nenhum aviso sai.

## 29 · A conformidade reprova sob a carga da própria suíte

**Sintoma, observado em 2026-08-31.** `cargo test --workspace` reprova um teste
de conformidade por rodada, **sempre um diferente**, e sempre estourando um prazo
de ~20 s. Vistos numa única sessão: `uma_rajada_de_mensagens_grandes_chega_inteira`,
`two_shells_hold_a_conversation`, `a_muted_mic_is_visible_to_everybody_else`,
`a_restarted_server_keeps_its_accounts`, e um do `acceptance_m5`.

**A prova de que é carga e não regressão.** Cada um deles passa sozinho em menos
de um segundo — `a_restarted_server_keeps_its_accounts` roda em **0,25 s** — e
estoura em **20,1 s** quando a suíte inteira corre junto. O `cargo` roda os
binários de teste em paralelo, esta máquina tem quinze núcleos, e cada teste de
conformidade levanta um servidor QUIC de verdade com aperto de mão, TLS e banco.

**Por que dói mais do que parece.** Não é o tempo perdido: é que uma suíte que
reprova aleatoriamente **deixa de ser evidência**. Numa sessão de trabalho isto
apareceu cinco vezes, e em cada uma foi preciso rodar de novo para distinguir «o
que eu acabei de escrever quebrou» de «a máquina estava cheia». Uma suíte assim
treina quem a lê a reexecutar em vez de investigar — e o dia em que a falha for
de verdade, ela vai ser reexecutada também.

**O que ainda não se sabe.** Se o prazo é curto demais para uma máquina carregada
ou se há contenção de verdade — porta, disco, ou o `Daemon::bind` esperando algo
que a saturação atrasa. As duas têm conserto diferente: a primeira é o prazo, a
segunda é o teste.

### Respondida em 2026-08-31 · é carga, e a prova é de uma linha

`cargo test -p seele-conformance --test acceptance_m5 -- --test-threads=1`:
**15 de 15 passam, em 23,02 s.** Em paralelo o mesmo arquivo termina sempre em
~20,01 s e reprova um teste em cerca de duas de cada três rodadas — nem sempre
o mesmo.

Os 23 s em série contra os 20 em paralelo dizem o resto: **o paralelismo não
está comprando quase nada** neste crate. Cada teste levanta um servidor QUIC de
verdade, com aperto de mão, TLS e banco; eles competem por porta, disco e CPU, e
o que se ganha em sobreposição se perde em contenção.

Então a escolha que este registro deixava em aberto está decidida pelos
números: **serializar**, e não alargar prazo. Três segundos a mais por rodada
compram uma suíte que volta a ser evidência.

### Piorou em 2026-08-31, e agora atrapalha o trabalho

Com dois testes novos no `acceptance_m5`, a mesma rodada de `--workspace`
passou a reprovar **três** de uma vez — e o arquivo continua passando 15 de 15
quando roda sozinho, em 20,03 s.

O efeito prático mudou de natureza: deixou de ser ruído e virou **atrito por
passo**. Numa sequência de commits, cada verificação exige rodar duas vezes
para saber se a falha é do que se acabou de escrever. Isso é exatamente o que o
registro acima previa — «uma suíte que reprova aleatoriamente deixa de ser
evidência» —, agora com número.

**O protocolo enquanto ela não é consertada**, e ele está aqui para não ser
reinventado a cada vez: verificar com `cargo test --workspace` **e**, quando o
`seele-conformance` reprovar, repetir só ele. Passando sozinho, a falha é
carga; reprovando sozinho, é regressão. Duas rodadas, e a segunda é barata.

**O que falta é só o como.** Não há chave de `test-threads` no `Cargo.toml` de
um crate — o caminho é um semáforo no `start()` da conformidade, limitando
quantos servidores existem ao mesmo tempo dentro do binário de teste. Contido,
e não mexe no resto do workspace.

**Por onde começar (registro anterior).** Rodar a conformidade com `--test-threads=1` e ver se some.
Se sumir, é carga, e a escolha é entre prazo maior e serializar aquele crate
(`test-threads` no `Cargo.toml` do `seele-conformance`, que não afeta o resto do
workspace). Se não sumir com uma linha só de execução, é contenção de recurso e o
alvo é outro.

**Quando dói.** Em toda rodada de `cargo test --workspace`, que é o comando que
este projeto usa para dizer que está verde.

## 30 · Fechada em 2026-08-31 · O `seeled` não sabia dizer que versão é

**Sintoma.** `seeled --version` não existe. O argumento cai no ramo que lê
endereço de escuta e o processo morre com `could not parse the listen address` —
uma mensagem sobre outra coisa, para uma pergunta razoável.

O que existe é `--ajuda` / `--help` / `-h`, e os subcomandos `convite`, `senha`
e `anexos`.

**Por que dói mais do que parece.** Quem hospeda roda o `seeled` num VPS e o
esquece. Quando algo der errado — e a pendência 26 e a 29 são exemplos de coisas
que dão —, a primeira pergunta de qualquer suporte é «qual versão está rodando»,
e hoje a resposta exige olhar o arquivo, a data, ou o histórico do shell.

É pior com o pipeline de release quebrado desde 2026-08-22: um binário que não
diz a versão dele, entregue por um pipeline que pode ter montado o commit errado,
não tem como ser conferido a não ser pela soma — que quem hospeda não guardou.

**Onde dói junto.** A fórmula de Homebrew (`empacotar/homebrew.sh`) queria
conferir a versão no `brew test`, que é exatamente o que aquele teste existe para
fazer, e teve de se contentar com `--ajuda`. Ele prova que o binário executa
nesta arquitetura e não prova qual build é.

**O conserto.** Um ramo a mais no `match` de `crates/seele-server/src/main.rs`,
imprimindo `env!("CARGO_PKG_VERSION")`. O que exige um pouco de cuidado é que a
versão do workspace é `0.0.0` — ela vem do `Cargo.toml` da raiz e é substituída no
empacotamento —, então imprimir a constante crua diria `0.0.0` em todo lugar. O
número de verdade é o que o `publicar.sh` carimba, e é dele que a linha tem de
sair.

### Fechada em 2026-08-31

`seeled --versao` (e `--version`, e `-V`) responde. O número entra **no binário**
por `SEELE_VERSAO`, exportada por `empacotar/macos.sh` e pelas duas etapas de
build do workflow de release — e não pelo `Cargo.toml`, cuja versão é `0.0.0` de
propósito e continua sendo.

**O fallback é o que merece atenção, e tem teste.** Um build sem carimbo — todo
build de desenvolvimento — responde `local (sem versão carimbada)`, e não um
número. `0.0.0` seria a versão do workspace e qualquer outro seria invenção; os
dois mentem para quem está tentando descobrir o que está rodando, e um número
mente de forma convincente. `a_versao_sem_carimbo_diz_que_nao_foi_carimbada`
cobra isso nos dois sentidos.

Também sai da fórmula de Homebrew a ressalva sobre isto: o `brew test` pode
voltar a conferir a versão, que é o que aquele teste existe para fazer.

## 31 · Trocar de fone ou microfone no Windows exige reiniciar o aplicativo

**Sintoma, relatado por quem usa em 2026-08-31**, num teste em LAN entre Mac e
Windows: *«no Windows a troca de fone e microfone não aconteceu em tempo real,
precisou reiniciar o aplicativo para aplicar corretamente.»*

**Por que isto está aberto e não foi investigado.** Foi relatado junto de outros
quatro pontos, e os outros quatro foram feitos. Este não, e não houve decisão
nenhuma por trás disso — ele simplesmente não virou tarefa. Está escrito aqui
porque um defeito relatado e não registrado é um defeito que volta pela mesma
porta: **esta mesma sessão perdeu uma versão inteira em campo** por um
comentário que envelheceu sem que ninguém notasse.

**O que se sabe do desenho, e é o que torna o sintoma estranho.** O caminho
existe e promete o contrário. `escolher_microfone` grava e chama
`set_capture_device`, cuja documentação diz «*takes effect now*» e abre o
caminho novo antes de soltar o velho. `crates/seele-audio/src/supervisor.rs`
existe inteiro para «manter o áudio vivo através de trocas de aparelho», com
máquina de estados coberta por teste.

Ou seja: ou aquele caminho não é o que roda no Windows, ou ele roda e não
alcança o que o WASAPI precisa que se faça.

**As três suspeitas — as três caíram em 2026-08-31.** Ficam escritas com o que
as derrubou, para ninguém refazer o caminho:

1. ~~**A troca acontece e a interface não conta.**~~ **Falsa.** O
   `marcarUmaLista` já desenha as duas coisas separadas — `ESCOLHIDO`, que sai
   do disco, e `EM USO`, que sai de `snapshot.capture`, o que a máquina
   conseguiu abrir. O comentário da função diz que ela «existe para mostrar a
   divergência» entre as duas. A tela não esconde nada.
2. ~~**O `cpal` no WASAPI não reabre com o fluxo velho vivo.**~~ **Falsa, e
   medida na máquina do defeito.** Uma sonda abriu dois dispositivos de saída ao
   mesmo tempo, nas duas ordens — que é exatamente a ordem do produto, porque
   `switch_to` abre o novo antes de largar o velho. Os quatro abriram e tocaram.
   O WASAPI não impede.
3. ~~**O supervisor trata a troca voluntária como queda.**~~ **Improvável.** O
   `seele-audio::supervisor` não é referenciado de `voice.rs`; a troca não passa
   por ele.

**E o caminho de erro é visível.** Se `start_on` falhasse, a FFI devolveria
`DispositivoSumiu` e a tela escreveria «ESSE MICROFONE NÃO ESTÁ MAIS AQUI» —
`start_on` é o irmão que **não** cai para o padrão da máquina, ao contrário de
`start_preferring`, que é o do disco. Quem relatou não mencionou erro nenhum.

**O que sobrou, e é uma pergunta de dez segundos.** Ao trocar de aparelho com a
conversa aberta, para onde vai a marca `EM USO` na lista?

- **Vai para o aparelho novo** → o áudio trocou de verdade, e o problema é
  depois disto: ou o outro lado não recebe, ou o que se ouve não mudou por
  outra razão.
- **Fica no velho, sem mensagem de erro** → `set_capture_device` devolveu
  sucesso sem trocar nada, e o alvo passa a ser a camada da sessão.
- **Fica no velho, com a mensagem de erro** → `start_on` recusou, e aí falta só
  o motivo que o `cpal` deu.

As três apontam para lugares diferentes, e nenhuma investigação a mais vale a
pena antes de saber qual é.

**Por que não foi consertada.** Nenhuma das três causas prováveis sobreviveu à
prova, e consertar sem causa seria escolher uma no escuro. Por SSH a máquina
enumera **um** microfone só — a sessão gráfica de quem usa vê mais —, então a
troca de entrada não é reproduzível de fora. A de saída é, e foi: passou.

**Quando dói.** Em toda troca de fone durante uma conversa, que é exatamente o
momento em que a pessoa não consegue ouvir e não pode receber a instrução «saia
do servidor e volte» — o argumento que a própria `set_playback_device` escreve
para justificar valer na hora.

**A quarta causa, achada em 2026-09-13 — e é outra pergunta.** A auditoria da
camada de áudio mostrou que a troca feita **no sistema operacional** — o seletor
da bandeja do Windows, as Configurações do Mac, o fone puxado da tomada — nunca
era seguida. O `cpal` avisa das três pelo retorno de erro do fluxo; o produto
contava o aviso e jogava fora o **tipo** dele, e o `supervisor.rs` inteiro era
código morto. Sem tipo, sobravam duas saídas igualmente erradas: reabrir por
qualquer estalo, ou nunca reabrir. O produto fazia a segunda, e reiniciar era o
único momento em que o padrão do sistema voltava a ser resolvido.

Isso foi consertado: o tipo do erro é preservado, `DeviceChanged` e
`DeviceNotAvailable` viram reabertura, o resto continua sendo só contado, e o
laço reabre no padrão de agora nos dois lados. A interface passou a ser avisada
com uma frase própria — «TROCANDO DE APARELHO», «SEM APARELHO DE ÁUDIO», «ÁUDIO
AGORA EM …» — em vez do aviso de falha local, que apaga sozinho. O teste de
conformidade `troca_de_aparelho` exercita os casos por comportamento: a troca do
padrão do sistema, o fone puxado da tomada, o estalo que **não** pode trocar
aparelho nenhum, o aparelho que nunca volta e é dito perdido, e três trocas
seguidas na mesma sessão.

**A segunda troca da sessão, que quase ficou de fora.** Cada abertura em
`device::open` cria contadores novos e zerados, e o laço troca o `AudioIo`
inteiro pelo que a reabertura entregou — passando a ler *esses* contadores. O
ciclo guardava o número já visto e nunca o reiniciava, então o segundo
`DeviceChanged` da sessão levava o contador novo a 1, que não vencia o 1 antigo,
e nada acontecia: quem trocasse o fone duas vezes ficava preso ao aparelho da
primeira troca. O conserto é `Reabertura::aviso_de`: ao reabrir, o ciclo adota
como ponto de comparação o que o aparelho recém-aberto diz, e não o que o
anterior dizia.

**A mesma armadilha, no aviso de falha local.** O contador que reinicia a cada
reabertura tinha um segundo leitor: o `FalhaLocal` de `telemetry.rs`, que decide
se «ÁUDIO LOCAL FALHANDO» acende. Ele guardava uma marca de máximo que só subia,
então depois da primeira troca da sessão o total menor do aparelho novo nunca
«crescia» e o aviso ficava cego — um indicador que já existia apagado
justamente pelo evento que esta parte do produto passou a seguir. Agora, quando
o total observado **encolhe**, o detector adota a régua nova em vez de ficar
preso ao máximo anterior; reiniciar não é falha. O guarda é
`o_aviso_nao_fica_cego_depois_de_trocar_de_aparelho`, e ele reprova com *«o
aparelho novo tropeçou e o aviso não acendeu — detector cego depois da troca»*
quando o ramo do encolhimento é removido.

**E a mesma reabertura tinha um resto de defeito, apontado pela revisão de
2026-09-14 e consertado.** O ramo do encolhimento reposicionava a régua mas não
contava como amostra quieta, e só amostras quietas apagam o aviso. O efeito era
pequeno e na hora errada: um aviso já aceso sobrevivia **uma olhada além do
conserto** — o produto seguia dizendo «ÁUDIO LOCAL FALHANDO» logo depois de a
troca ter resolvido a falha. A reabertura é uma olhada sem crescimento como
qualquer outra — o aparelho novo não tropeçou nada —, então agora ela anda a
folga junto com as demais, por `FalhaLocal::contar_quieta`.

**Prova de reversão, feita em 2026-09-14.** Escrita antes do conserto, como as
outras: `a_reabertura_conta_como_amostra_quieta` reprova com *«a reabertura não
entrou na folga: o aviso sobrevive uma olhada além do conserto»* enquanto o ramo
do encolhimento não chama `contar_quieta` — `5 passed; 1 failed` no módulo
`falha_local`. Com a chamada no lugar, a unidade de `seele-audio` passa inteira:
**220 testes**.

**O buraco que sobrou no próprio seam, achado pela revisão e fechado.** O
conserto tinha guarda em toda parte menos no ponto exato do defeito: `classificar`
estava coberta, o laço estava coberto, e a **ligação** entre o retorno de erro
que o `cpal` chama e a classificação não tinha nenhuma. Medido, não suposto:
trocando o corpo das duas closures por `record_stream_error(Transitoria)` —
que é literalmente o defeito original — `cargo test -p seele-audio` passava
inteiro. Construir um fluxo precisa de placa de som, mas *chamar o retorno* não
precisa de nada, e era só isso que faltava. O retorno virou uma função nomeada,
`device::retorno_de_erro`, que os dois lados entregam ao `cpal` sem corpo
próprio, e três guardas a exercitam como o `cpal` a exercitaria:
`retorno_de_erro_do_cpal::a_troca_feita_no_sistema_chega_ao_laco_como_troca`,
`::o_aparelho_arrancado_chega_ao_laco_como_sumico` e
`::um_estalo_nao_vira_aviso_de_aparelho_nenhum` — o terceiro é o que impede o
conserto exagerado de reabrir a cada clique.

**Prova de reversão, feita em 2026-09-13.** Com a mesma troca por
`FalhaDeAparelho::Transitoria` no corpo do retorno, `cargo test -p seele-audio
--lib` reprova em dois: `a_troca_feita_no_sistema_chega_ao_laco_como_troca` com
`left: AvisoDeAparelho { trocas: 0, sumicos: 0 }` contra `right: { trocas: 1,
sumicos: 0 }`, e `o_aparelho_arrancado_chega_ao_laco_como_sumico` com `sumicos:
0` contra `2`. Remedido em 2026-09-14, com a suíte já crescida: `217
passed; 2 failed`. Com o código de volta, os 219 passam.

**Reexecutada em 2026-09-13, agora alcançando a conformidade.** A mesma
trivialização — `classificar` devolvendo `Transitoria` para qualquer erro —
derruba também `cargo test -p seele-conformance --test troca_de_aparelho`: 5 dos
6 que o arquivo tinha naquele dia reprovam
(`trocar_o_aparelho_padrao_no_sistema_reabre_a_voz_no_novo`,
`tirar_o_fone_da_tomada_leva_a_voz_para_o_aparelho_que_sobrou`,
`trocar_de_aparelho_duas_vezes_na_mesma_sessao_e_seguido_das_duas_vezes`,
`um_aparelho_que_nunca_volta_acaba_dito_perdido` e
`um_panico_noutra_parte_do_programa_nao_congela_a_tela_no_aparelho_antigo`, este
com *«a sessão nunca desistiu, e um estado de "trocando" eterno é a mesma mentira
do silêncio calado»*). O único que continua passando é
`um_estalo_no_fluxo_nao_troca_o_aparelho_de_ninguem` — e passar é o certo: o
estalo já era transitório antes e depois, então a reversão não podia mudá-lo. É
a prova de que a suíte distingue as três famílias de erro em vez de reagir a
qualquer falha. **Refeita do zero em 2026-09-13, numa sessão que não escreveu o
conserto**, para que a prova não fosse só a palavra de quem a escreveu: os
mesmos 5 reprovaram, com as mesmas mensagens, e o do estalo passou — depois o
arquivo voltou ao que era e os 6 passaram de novo. (O sétimo teste nasceu depois
desta medida; a recontagem contra a suíte de sete está mais abaixo.) Na unidade a mesma reversão reprova 4 (`215 passed; 4 failed`),
somando os dois guardas de `classificacao_do_erro_do_cpal` aos dois de
`retorno_de_erro_do_cpal`. Com o arquivo restaurado, 219 e 7 passam de novo.

**A distinção que faltava, apontada por revisão independente e fechada em
2026-09-13.** As duas reversões acima não eram a mesma coisa, e só uma delas
alcançava a conformidade. Trivializar `classificar` derrubava os 6 de 7 descritos
acima; trivializar o **corpo do retorno de erro** — que é onde o erro era jogado
fora, e portanto o defeito original — deixava os 7 verdes. A causa era o próprio
teste: `o_cpal_avisa` chamava `classificar` diretamente e assim saltava justamente
o ponto sob suspeita. Corrigido: `retorno_de_erro` passou a ser pública e o teste
de conformidade entra por ela, pelo mesmo fechamento que o `cpal` recebe ao montar
o fluxo. Refeita a reversão do corpo do retorno (`move |_error| errors
.record_stream_error(FalhaDeAparelho::Transitoria)`), a conformidade agora reprova
6 de 7, com as mesmas mensagens — entre elas *«a tela ficou congelada no aparelho
anterior… a voz saiu pelo aparelho novo e a pessoa leu o nome errado»* (`left:
Some("fone-usb")` contra `right: Some("caixas-da-mesa")`). Arquivo restaurado byte
a byte, conferido por sha256, e os 7 voltam a passar. A cobertura do seam deixou de
existir só na unidade.

**Recontadas em 2026-09-14, porque a suíte cresceu depois da conta.** O arquivo
de conformidade passou a ter **sete** testes — o sétimo é
`o_fone_religado_depois_de_a_sessao_desistir_volta_a_ter_som`, escrito depois dos
números acima — e as duas reversões foram refeitas contra essa suíte maior. As
duas reprovam **6 de 7** (`1 passed; 6 failed`), e o único que continua verde nas
duas é `um_estalo_no_fluxo_nao_troca_o_aparelho_de_ninguem`, pelo mesmo motivo de
sempre: o estalo era transitório antes e depois. Na unidade, trivializar
`classificar` dá `215 passed; 4 failed` e trivializar o corpo do retorno dá `217
passed; 2 failed`. `device.rs` foi restaurado e conferido por sha256 depois de
cada uma, e os 219 da unidade mais os 7 da conformidade voltaram a passar. A
suíte inteira do workspace: **1.732 testes, saída 0**.

**Uma validação relatou `cargo test` reprovando, e a reexecução não reproduziu.**
Refeito `cargo test` no workspace inteiro depois da correção de contagem acima:
**1.732 passados, 0 reprovados, 4 ignorados, saída 0**, com `cargo fmt --all --
--check` limpo e `cargo clippy --workspace --all-targets` sem aviso nem erro. O
relato anterior vinha com a saída truncada e sem nenhum nome de teste reprovado
junto — ou seja, não havia falha a consertar, e registrar isso é melhor do que
deixar no ar a suspeita de uma reprovação que ninguém consegue ver de novo.

**Refeito uma terceira vez em 2026-09-14**, já com o guarda novo do detector de
falha local dentro: `cargo test --workspace` deu **1.733 passados, 0 reprovados,
4 ignorados, saída 0** — um teste a mais que a medida anterior, que é exatamente
o guarda acrescentado. `cargo fmt --all -- --check` limpo e `cargo clippy` com
`-D warnings` sobre `seele-audio`, `seele-core`, `seele-ffi` e
`seele-conformance` sem um aviso sequer. A reprovação relatada continua sem se
reproduzir.

**Refeito uma quarta vez em 2026-09-14**, depois de a revisão independente
aprovar o trabalho e de a validação daquela rodada voltar com reprovação. A
bateria inteira do workspace correu duas vezes seguidas nesta árvore e chegou
ao fim nas duas; a segunda foi gravada por inteiro e somada linha a linha:
**1.733 passados, 0 reprovados, 4 ignorados, saída 0**, sem um `FAILED` sequer
no relatório. `cargo fmt --all -- --check` e `cargo clippy --workspace
--all-targets --all-features -- -D warnings` saíram com zero. A reprovação
relatada continua sendo a saturação descrita na pendência 29 — máquinas
diferentes reprovando testes diferentes, nenhum deles em caminho de áudio — e
não se reproduziu em nenhuma das quatro medições feitas aqui.

**E na quinta vez ela reproduziu — e tinha causa, não azar. 2026-09-14.** As
quatro medições acima disseram «não reproduz», e isso estava certo sobre o que
elas viram e errado sobre a conclusão que se tirou: intermitente não é
inexistente. Nesta rodada `cargo test` reprovou em
`seele-server/tests/tipo_de_fluxo.rs`, no caso
`um_fluxo_que_diz_ser_anexo_e_lido_como_anexo`, com *«sending stopped by peer:
error 0»* — e esse código zero é a assinatura que resolve o caso.

A causa é do teste, e o produto está certo. Sem diretório de anexos, o servidor
lê **só o cabeçalho** (é dele que sai a quem responder), manda `Unavailable` e
larga o fluxo. Largar um fluxo de entrada no QUIC manda `STOP_SENDING` de volta,
com código zero — e o cliente do teste ainda estava escrevendo os quatro bytes do
corpo. Quem ganha a corrida depende de escalonamento, e é por isso que o teste
passava sozinho e caía sob a suíte inteira. O servidor não gastar leitura com
byte que não vai guardar é a economia certa; o teste é que tratava a economia
como erro.

*Medido, e não deduzido:* isolado, 15 de 15 passaram; doze cópias simultâneas do
binário, três vezes, 36 de 36 passaram — a carga leve não alcança a corrida.
Então ela foi **forçada**: com um atraso de 300 ms entre o cabeçalho e o corpo,
o teste reprovou 3 vezes em 3, sempre com a mesma mensagem da validação. Com o
conserto e o mesmo atraso no lugar, passou 3 em 3. O atraso saiu depois; ele
existiu só para tornar a corrida determinística nas duas direções.

O conserto é `escrever_mesmo_que_parem`, no próprio arquivo de teste: as
escritas de bytes que o servidor deliberadamente não lê toleram
`WriteError::Stopped` e **só ela** — qualquer outro erro de escrita continua
reprovando, e nenhuma asserção foi afrouxada. O que cada teste prova continua
inteiro: a resposta só existe se o cabeçalho tiver sido lido do começo.

**O que sobrou, e é honesto dizer que sobrou.** Depois desse conserto a bateria
inteira correu cinco vezes: quatro terminaram com **1.733 passados, 0
reprovados, 4 ignorados, saída 0**, e uma reprovou em
`seele-conformance/tests/acceptance_m2.rs`
(`three_clients_in_one_voice_room_hear_each_other`) com
`ConnectError::SemResposta` — prazo de conexão queimado sob carga. Isolado, esse
teste passou 10 de 10. Não é desta tarefa, e dá para mostrar por quê em vez de
alegar: `SemResposta` nasce em `seele-core/src/enlace.rs` e
`seele-core/src/client.rs`, e nenhum dos dois está no diff — em `seele-core` só
mudaram `voice.rs` (áudio) e quatro linhas de reexport em `lib.rs`. Do mesmo
modo, `seele-server` não depende de `seele-core` nem de `seele-audio` (ADR
0002), o que é o que descarta o caso anterior como regressão daqui. O conserto
certo para esse prazo é outra tarefa; ele vai continuar derrubando validação
enquanto ninguém o fizer, e fingir que é fantasma foi justamente o erro das
quatro medições anteriores.

**Por que `crates/seele-proto/tests/vetores_de_hash.rs` aparece neste diff.** Ele
não tem relação com aparelho de som, e a única mudança nele é formatação mais um
`allow` de lints de teste. Está aqui porque medi: com o arquivo como veio do
commit anterior, `cargo fmt --check -p seele-proto` reprova e `cargo clippy -p
seele-proto --all-targets` acusa sete avisos — isto é, sem esse retoque não dá
para afirmar árvore limpa. Nenhum vetor, nenhum hash e nenhuma asserção mudaram.

**A camada de tela ganhou os dois guardas que ela podia ter, em 2026-09-13.**
Não há motor de JavaScript nesta árvore e não há `node` na máquina, então
executar `desenharAparelho` num teste não é possível sem uma dependência nova —
e o ADR 0019 já escolheu não ter uma. O que dava para fechar, e foi fechado, são
as duas juntas que falham **caladas** e que o resto do arquivo não cobria:

- `todo_estado_do_aparelho_que_a_ponte_manda_chega_dito_na_tela` serializa o
  `EstadoDoAparelho` de verdade e exige frase para cada estado que não seja o
  normal. Renomear a variante em Rust quebra o guarda em vez de deixar um ramo
  morto no script. A lista de estados é conferida por um `match` sem coringa, de
  modo que uma variante nova não compila até entrar nela.
- `a_tela_le_do_instantaneo_os_campos_que_o_rust_realmente_manda` amarra os dois
  lados: uma função no próprio teste nomeia `Snapshot::aparelho` e
  `Snapshot::trocas_de_aparelho` em Rust — renomear qualquer um impede o arquivo
  de compilar — e o guarda exige que o script leia esses mesmos nomes.

**Prova de reversão, feita em 2026-09-13.** Apagando o ramo de `perdido` do
script, o primeiro reprova com *«o estado «perdido» não tem frase nenhuma na
tela, então ele chega como silêncio»*; trocando a leitura para
`snapshot.trocasDeAparelho`, o segundo reprova com *«a tela não lê
`trocas_de_aparelho`, que é o que o instantâneo manda»*. O que continua sem
guarda, e fica dito: a notícia que some em oito segundos e o reposicionamento do
contador entre sessões são lógica de tempo dentro do script, e ela só seria
exercitada por um motor de JavaScript que esta árvore não tem.

**O que a camada de tela não tem, e é honesto dizer.** A faixa que a pessoa
lê — `desenharAparelho`, em `apps/seele-app/ui/tela-sessao.js` — não tem teste
nenhum: este repositório não tem infraestrutura de teste de JavaScript, e
inventar uma por causa desta tarefa seria outro trabalho. Tudo que decide *o
que* dizer mora em Rust e está sob teste (`Snapshot.aparelho` e
`trocas_de_aparelho`); o que ficou só sob leitura é a apresentação — a contagem
que reinicia quando o total encolhe e a janela de oito segundos da frase.

**Onde a conferência ainda é de texto-fonte.** O guarda de
`voz_na_reconexao.rs` continua lendo `include_str!` atrás de `reopen` no braço
da reconexão; ele não foi substituído, foi **complementado** por
`controles_na_reabertura`, que exercita `carregar_controles` por comportamento
(mudo, isolamento, modo, ganhos, salto do relógio). Fica registrado para que a
substituição não seja dada como feita.

O que aquele guarda **não** alcançava era a casca: que o braço da reconexão
reabra a voz viva em vez de construir uma nova. Isso agora tem comportamento
próprio. A decisão saiu de dentro do braço de `Aviso::Reconectado` para
`reabrir_voz_na_reconexao` (`seele-ffi/src/lib.rs`), que é genérica sobre a voz
— justamente para rodar numa máquina sem placa de som — e guarda as três
decisões que já foram defeito: reabrir **a partir** da voz viva (é de onde os
controles vêm), não abrir voz para quem estava em texto puro, e não guardar a
voz que não reabriu. Os guardas são
`a_voz_quando_o_enlace_volta::a_voz_viva_reabre_carregando_os_controles`,
`::quem_estava_em_texto_puro_continua_em_texto_puro` e
`::a_voz_que_nao_reabre_da_lugar_ao_texto`.

**Os dois guardas de texto que ficam, e por quê.** Revisão independente de
2026-09-13 apontou que registrar isso só aqui não basta: quem abre o arquivo não
lê este documento. Os dois agora dizem de si mesmos, no próprio fonte, por que
são de texto e onde mora o complemento comportamental.
`voz_na_reconexao.rs` ganhou a seção *«Este guarda não está mais sozinho»*, e
`as_duas_trocas_conferem_o_que_pediram` (`seele-ffi/src/lib.rs`) ganhou a
justificativa acima dele. O critério é o mesmo nos dois: o texto guarda a casca
**chamar**, o comportamento guarda a chamada **decidir**. Nenhum dos dois pega o
defeito do outro, e as funções que eles cercam abrem aparelho de verdade — a
conformidade roda sem placa de som, então não há terceira opção.

**Prova de reversão, feita em 2026-09-13.** Devolvendo `Err(erro) => Err(erro)`
no lugar do ramo que zera o slot, `a_voz_que_nao_reabre_da_lugar_ao_texto`
reprova com *«a reconexão guardou a voz que não reabriu: ela sai por uma conexão
morta, com `ssrc` velho, e `audio_available` mente»*. Trocando
`Ok(nova) => { *slot = Some(nova); ... }` por `Ok(_nova) => Ok(())`,
`a_voz_viva_reabre_carregando_os_controles` reprova com *«a reconexão devolveu
uma voz nova sem os controles de agora: o roster continua mostrando quem estava
mudo como mudo, e o microfone volta aberto»*. Com o código de volta, os três
passam. O terceiro guarda não tem reversão possível e isso é a favor dele: sem
voz viva não há `&V` para entregar ao fechamento, então o tipo recusa a versão
errada antes de qualquer teste.

O que continua sem guarda, e por isso fica escrito: o corpo do fechamento — que
chama `reopen` e não `start_on` — ainda é afirmado só pelo guarda de
texto-fonte. Ele precisa de uma `Voice` de verdade, e conformidade roda sem
placa de som.

**Prova de reversão, feita em 2026-09-13.** Removendo as três linhas de
`CicloDoAparelho::passo` (`supervisor.rs`) que adotam `quem.aviso_de(&aberto)`
como novo ponto de comparação, e deixando todo o resto no lugar:

- `cargo test -p seele-audio --lib supervisor::tests::a_segunda_troca_da_sessao_tambem_e_seguida`
  reprova com *«a troca para «fone-usb» nunca foi seguida: o laço ficou no
  aparelho anterior»*;
- `cargo test -p seele-conformance --test troca_de_aparelho` reprova em
  `trocar_de_aparelho_duas_vezes_na_mesma_sessao_e_seguido_das_duas_vezes` com
  *«a troca para «fone-usb» não foi seguida; a voz ficou no aparelho anterior»*,
  `left: Some("caixas-da-mesa")`, `right: Some("fone-usb")` — os outros quatro
  testes do arquivo continuam passando, que é o que mostra que este caso tinha
  guarda própria e não vinha de carona.

Com as linhas de volta, os dois passam.

**O que um teste ainda não alcança, e o que se fez para encolher isso.** Dentro
de `pipeline()` não há guarda possível: o laço tem `cpal` de um lado e um socket
QUIC do outro, e `AudioIo` guarda fluxos vivos que não se constroem numa máquina
sem placa de som. A decisão de *quando* reabrir saiu de lá para `Acompanhamento`,
e agora as **medidas** do aparelho também saíram, para `Dimensoes`: reamostrador
de entrada, reamostrador de saída, capacidade do anel e malha de ritmo, numa peça
só, porque as quatro têm de ser refeitas juntas — reabrir no fone certo e seguir
com as medidas do anterior é trocar «não sai som» por «a voz ficou estranha».
Guardas: `as_medidas_seguem_o_aparelho_de_agora_e_nao_o_de_antes`,
`uma_taxa_que_o_reamostrador_recusa_nao_vira_laco` e
`o_que_estava_a_caminho_do_aparelho_antigo_nao_toca_no_novo`. Reversão feita em
2026-09-13: construindo o reamostrador de saída em 48 kHz fixo em vez da taxa do
aparelho, o primeiro reprova com *«o reamostrador de saída ficou na taxa do
aparelho anterior; a voz sai acelerada enquanto a sessão durar»*, `left: 48000`,
`right: 44100`. O que resta sem guarda de comportamento são as linhas de atribuição no laço —
trocar o `AudioIo` e adotar as medidas novas —, e elas só ficam honestas com
placa de som na mão. A chamada de `acompanhamento.passo` deixou de estar nesse
grupo: ela ganhou guarda de texto-fonte, descrito abaixo.

**Um limite de produto que existiu até a revisão de 2026-09-13, e foi fechado.**
Quando as tentativas de reabertura se esgotavam, o aparelho era dado por perdido
e ali acabava: a interface dizia «SEM APARELHO DE ÁUDIO» — honesto — mas um
aparelho que voltasse depois não era seguido, e sair de `Perdido` só acontecia
pela escolha de alguém na tela. Quem tirasse o fone e o religasse meio minuto
depois ficava sem som com um fone funcionando na mão, e a saída era a mesma que
esta tarefa existe para acabar: fechar e abrir o aplicativo.

O conserto não é insistir mais. O ritmo é que muda: esgotadas as oito tentativas,
o acompanhamento passa a uma **ronda lenta** de cinco segundos
(`RONDA_DE_PERDIDO_MS`) — uma reenumeração a cada cinco segundos é o mesmo custo
de alguém abrir a lista de aparelhos, e não o processo girando no fundo do laptop
que a versão anterior deste teste temia. Enquanto nada aparece, o estado continua
`Perdido` e a tela continua dizendo que não há áudio; quando um aparelho abre, o
estado vai para `Funcionando`, a reabertura é contada e a interface vê a mudança
pelo contador de transições, como vê qualquer outra.

Dois testes antigos codificavam a desistência como definitiva e foram reescritos
em vez de contornados: `it_gives_up_after_a_bounded_number_of_attempts` passou a
afirmar o que de fato acaba — o *ritmo* da recuperação — e
`a_late_success_after_giving_up_is_ignored` virou
`uma_abertura_depois_da_desistencia_nao_acontece_escondida`, porque a metade certa
do medo antigo era a interface, não o áudio: jogar fora uma abertura que já
aconteceu é deixar a pessoa sem som com o aparelho na mão, e a interface é
atendida pela transição, que ela enxerga.

**Prova de reversão, feita em 2026-09-13.** Escrita antes do conserto, a dupla de
testes de unidade reprovou em `poll` com *«o aparelho reapareceu e ninguém foi
ver»*, `left: Wait`, `right: Reopen`. Com o conserto no lugar, devolvendo `Lost`
ao `return Action::Wait` de antes, o teste de conformidade
`o_fone_religado_depois_de_a_sessao_desistir_volta_a_ter_som` reprova com *«o fone
voltou à tomada e a sessão continuou dizendo que não há aparelho: só reiniciar o
aplicativo resolveria»*, `left: Perdido`, `right: Funcionando`.

**Duas pontas fechadas na revisão de 2026-09-13.** A revisão independente
apontou dois casos em que o produto sabia e não contava — a falha desta casa:

- *O aparelho que abre e mesmo assim não serve.* Se a reabertura entregava um
  aparelho cuja taxa o reamostrador recusa, o painel já tinha sido escrito como
  «funcionando» com o nome novo e o laço encerrava logo depois: a pessoa ficava
  sem som nenhum lendo normalidade na tela. As duas coisas passaram a ser uma
  função só, `dimensoes_ou_dizer_que_nao_ha`, usada nas duas aberturas (a
  primeira e a de cada troca): sem laço possível, o estado vira «perdido» antes
  de o laço sair. Guardas:
  `o_aparelho_cuja_taxa_e_recusada_nao_fica_na_tela_como_funcionando` e
  `o_aparelho_que_o_laco_aceita_nao_e_anunciado_como_perdido`. Reversão feita:
  removendo a marcação do painel, o primeiro reprova com *«o laço vai encerrar e
  a tela continua dizendo que está tudo funcionando; a pessoa fica sem som nenhum
  sem nada que explique»*, e o segundo continua passando.
- *As taxas que envelheciam.* `Voice::rates()` respondia as taxas da **primeira**
  abertura para sempre; depois de uma troca, um fone de 44,1 kHz era descrito com
  os 48 kHz da placa anterior. Hoje as taxas moram no mesmo painel compartilhado
  que o nome e o estado, e são reescritas pela mesma função que anota a
  reabertura — nome, estado e taxas são do mesmo instante ou de nenhum. Reversão
  feita: sem a linha que adota as taxas do aparelho recém-aberto,
  `trocar_o_aparelho_padrao_no_sistema_reabre_a_voz_no_novo` reprova com *«as
  taxas ficaram nas do aparelho de antes»*, `left: 44100`, `right: 48000`.

**Uma terceira, fechada na revisão seguinte.** *O painel que congelava calado.*
As escritas do acompanhamento no painel usavam `if let Ok(mut painel) =
painel.lock()`: com o cadeado envenenado por um pânico noutra parte do programa
— que não derruba o processo, ele segue tocando —, a interface parava no último
estado que tinha visto, sem dizer nada, enquanto o aparelho trocava por baixo. A
mesma falha desta casa, na sua forma pequena. Hoje as quatro leituras e escritas
do painel passam por `painel_mesmo_envenenado`, que atravessa o veneno: o painel
é um punhado de campos que cada escrita substitui inteiros, não há metade de
estrutura a proteger. Guarda de comportamento:
`um_panico_noutra_parte_do_programa_nao_congela_a_tela_no_aparelho_antigo`, em
`crates/seele-conformance/tests/troca_de_aparelho.rs`, que envenena o cadeado de
verdade e depois faz a troca de aparelho. Reversão feita: devolvendo as duas
escritas de `Acompanhamento::passo` ao `if let Ok`, o teste reprova com *«a tela
ficou congelada no aparelho anterior porque o cadeado estava envenenado; a voz
saiu pelo aparelho novo e a pessoa leu o nome errado»*, `left: Some("fone-usb")`,
`right: Some("caixas-da-mesa")`; os outros cinco continuam passando.

**Um arquivo alheio que reprovava a validação, e por que ele acabou entrando.**
`crates/seele-proto/tests/vetores_de_hash.rs` entrou no repositório sem passar
pelo formatador nem pelo clippy. Por duas rodadas ele foi mantido fora do diff,
por não ser desta tarefa; o que mudou a decisão é que ele não reprovava só o
formatador. `cargo clippy --workspace --all-targets` **falhava de verdade**
(saída 101) nele, por `expect_used` negado no workspace, e sob `-D warnings`
somavam-se `write_with_newline` e `type_complexity`. Enquanto ele estivesse
assim, nenhuma análise estática do workspace inteiro podia passar — nem a desta
tarefa. `seele-proto` não depende de ninguém (`specs/01-arquitetura.md`), então
a falha é comprovadamente herdada da base e não deste trabalho.

O conserto é o mínimo e segue o que os outros testes do repositório já fazem:
`#![allow(clippy::expect_used, clippy::indexing_slicing, reason = "num teste, o
pânico é o relatório")]` no topo, como em `candidatos.rs`, `sincronia.rs` e mais
sete; `write!` terminado em `\n` virou `writeln!`; e o tipo do vetor de casos
ganhou o alias `Caso`. **Nada do que o teste prova mudou**, e há prova disso: o
teste reescreve `vetores-de-hash.json` sempre que a saída difere do comitado, e
o arquivo comitado continua idêntico depois da mudança — se `writeln!` tivesse
alterado um byte, o teste teria reescrito o arquivo e reprovado. Com isso,
`cargo fmt --all -- --check` e `cargo clippy --workspace --all-targets
--all-features -- -D warnings` passam limpos no workspace inteiro.

**O guarda do laço passou a se ancorar no laço, e não no nome de quem o contém.**
A revisão independente notou que ele procurava `async fn pipeline(` e o fim do
corpo por `\n}\n`: renomear a função ou mudar a forma de fechá-la faria o guarda
**estourar** em vez de proteger, e com uma mensagem que não diz a causa. Agora
ele corta o fonte no primeiro `#[cfg(test)]` — o que sobra é programa, não
bateria — procura a volta sobre `controls.stop` e exige a chamada depois dela.
O nome da função deixou de importar. E se um dia o laço deixar de ser escrito
assim, a mensagem diz com todas as letras que o guarda precisa ser reapontado,
em vez de morrer num `expect` cru.

*Reversão refeita com o guarda novo, em 2026-09-14:* apagando do laço o bloco
inteiro que chama `passo`, `cargo test -p seele-core --lib
o_laco_de_audio_conduz` reprova com *«o laço de áudio deixou de conduzir o
acompanhamento do aparelho»* — a mesma frase de antes, agora por um caminho que
não depende do nome `pipeline`. A árvore foi restaurada em seguida.

**Medição desta rodada, gravada por inteiro e somada linha a linha.**
`cargo test` no workspace: **1.733 passados, 0 reprovados, 4 ignorados, saída do
cargo 0** — e desta vez a saída medida é a do próprio `cargo`, não a do `tail`
que estava no fim do cano na rodada anterior, que é como uma reprovação podia
passar por aprovação. `cargo fmt --all -- --check` limpo, `cargo clippy
--workspace --all-targets --all-features -- -D warnings` sem um aviso, e
`seele-core` sozinho com 263 passados.

**Os dois arquivos deste diff que não são desta tarefa, ditos de uma vez.** A
revisão independente pediu que eles fossem declarados à parte para ninguém os
ler como parte da troca de aparelho, e a declaração é esta:

- `crates/seele-server/tests/tipo_de_fluxo.rs` — tolerar `STOP_SENDING` numa
  corrida legítima do servidor, descrito acima. É conserto de teste instável,
  herdado da base: `seele-server` não depende de `seele-core` nem de
  `seele-audio` (ADR 0002), então nada do áudio o alcança.
- `crates/seele-proto/tests/vetores_de_hash.rs` — formatação e um `allow` de
  lints de teste, descrito acima. `seele-proto` não depende de ninguém
  (`specs/01-arquitetura.md`).

Os dois são só de teste, nenhuma asserção foi afrouxada e nenhum vetor mudou.
Entraram porque sem eles não dá para afirmar árvore limpa nem validação verde —
quer dizer, para poder medir esta tarefa. **Quando isto virar commit, eles vão
num commit próprio**, com escopo `test`, antes do commit da troca de aparelho.

**Uma quarta, vinda da revisão independente: a pausa da reabertura mentia no
instrumento.** Reabrir um endpoint custa centenas de milissegundos, e o laço de
voz para de propósito durante essa abertura. O `PlayoutClock` não sabia disso:
na volta seguinte ele via meio segundo de atraso, contava um `resyncs` e gravava
`worst_lateness_ms` de ~520 ms. Esses dois números existem para responder *é a
rede ou é esta máquina?* — e, depois desta tarefa, cada troca de fone passaria a
respondê-la com «é esta máquina» para quem não tinha máquina nenhuma de errado.
O defeito não seria o áudio, que o próprio relógio repõe: seria o **diagnóstico
seguinte**, tomado por um número que mentiu. `PlayoutClock::reacertar` reparte de
`agora` sem contar a pausa como atraso, e o laço o chama junto com o resto do que
é refeito na troca. A pausa continua contada onde ela é o que é: o aviso da
reabertura e o contador de trocas que a tela mostra. Guarda de comportamento:
`a_pausa_de_reabrir_o_aparelho_nao_vira_atraso_da_maquina`, com o par
`sem_reacertar_a_mesma_pausa_apareceria_como_maquina_travada` provando que a
pausa realmente apareceria sem ele. Reversão feita: esvaziando o corpo de
`reacertar`, o teste reprova com *«o compasso recomeça em um quadro, e não num
despejo de reposição»*, `left: 4`, `right: 1`. O que estes dois guardas **não**
cobrem, e é honesto dizer: a linha que chama `reacertar` dentro do laço de voz.
Apagá-la não reprova teste nenhum, porque o laço só roda com placa de som — é a
mesma fronteira que fez `Acompanhamento` nascer como peça à parte. Reproduzir a
chamada num laço de mentira seria testar o teste, e não o produto.

**O teste de moderação que reprovou numa validação não é desta tarefa, e há
medida.** `expulsar_acaba_com_a_sessao_e_deixa_voltar`, em
`crates/seele-conformance/tests/moderacao.rs`, estourou o prazo de 10 s do
auxiliar `ate` numa rodada com as quatro crates em paralelo. Medido nesta
sessão: **6 de 6 rodadas do arquivo sozinho passam, e as seis terminam em
~0,45 s** — o prazo é vinte vezes o tempo real. O arquivo não é tocado por este
diff, e nada do caminho de moderação passa pelo aparelho de áudio. Isto é a
pendência **29** deste mesmo documento, registrada em 2026-08-31 e ainda aberta,
com conserto próprio já escolhido ali (semáforo no `start()` da conformidade) e
com o protocolo que ela mesma manda seguir: reprovando em conjunto e passando
sozinho, é carga. Consertá-la aqui seria fazer a tarefa 29 dentro desta.

**Medido de novo em 2026-09-13, porque uma validação voltou a reprovar sem dizer
o quê.** O relato trazia só o código de saída e um registro cortado no meio de
uma linha, sem o bloco de falhas — não dava para nomear o teste acusado. Em vez
de supor, contei: **seis execuções da suíte inteira do workspace no worktree,
todas com saída 0 e 1.726 testes passando**, mais `cargo fmt --check` e
`cargo clippy --workspace --all-targets --all-features -D warnings` limpos. Seis
de seis não prova que a instabilidade não existe — prova que ela não é
reproduzível daqui, e que o dedo apontado para este diff não se sustenta. A
suspeita que sobra é a mesma pendência 29, ou disputa de lock de compilação
entre execuções concorrentes; as duas têm dono fora desta tarefa.

**A ligação que faltava, apontada pela revisão de 2026-09-13 e fechada.** A
revisão mediu o que ninguém havia medido: apagando do laço real o bloco que chama
`acompanhamento.passo`, a suíte inteira continuava verde. Todo o ciclo de
reabertura seguia existindo, seguia correto e seguia provado — e nunca correria.
O defeito de origem voltaria inteiro, e nada acusaria o dia em que a chamada
sumisse numa refatoração, porque o teste de conformidade refaz a volta do laço à
mão em vez de exercitá-la.

Não há como fechar isso por comportamento: `pipeline()` tem `cpal` de um lado e
um socket QUIC do outro, que é a mesma fronteira que fez `Acompanhamento` nascer
como peça à parte. O que dá para fazer é o que este repositório já faz para ordem
que nenhum tipo expressa — o guarda de `o_ganho_do_microfone_corre_depois_do_portao`
—, e agora está feito:
`o_laco_de_audio_conduz_o_acompanhamento_do_aparelho` (`crates/seele-core/src/voice.rs`)
recorta o corpo de `pipeline` do próprio fonte e exige a chamada lá dentro. Ele
não prova que a chamada funciona; prova que alguém a conduz, que é exatamente o
buraco apontado.

**Prova de reversão, feita em 2026-09-13.** Apagando do laço o bloco inteiro de
`if let Some(novo) = acompanhamento.passo(...)`, o guarda reprova com *«o laço de
áudio deixou de conduzir o acompanhamento do aparelho. O supervisor volta a ser
código morto e a troca de microfone ou de fone feita no sistema operacional volta
a só valer depois de reiniciar o aplicativo.»* Com o bloco de volta, passa.

**Refeita na retomada de 2026-09-14, por quem não a escreveu.** A revisão
independente aprovou o trabalho e deixou esta como a única observação viva: o
teste de conformidade refaz a volta do laço à mão, então quem *conduz* o ciclo
em produção fica coberto por leitura de fonte. Apaguei de novo as 31 linhas do
bloco — o bloco de verdade, não a chamada renomeada, que só provaria que texto
casa com texto — e o guarda reprovou com a mesma mensagem, `0 passed; 1 failed`.
O arquivo voltou ao original conferido por hash.

O buraco que sobra é de **uma linha**: o teste de conformidade chama
`Acompanhamento::passo` de verdade, com os contadores que o retorno de erro do
`cpal` de verdade preencheu, de modo que o ciclo inteiro é comportamento. O que
nenhum teste alcança é o laço chamar aquela função, porque alcançá-lo pede
aparelho de áudio de verdade na máquina que roda a suíte. Estreitar mais isso
sem hardware não é possível; por isso o guarda de fonte fica, complementar e não
único.

**O que isto não responde, e por que a pendência fica aberta.** A pergunta de
dez segundos acima continua de pé: se a troca feita *na tela do SEELE* também
falha no Windows. A leitura mostra aquele caminho completo e correto, e falsificá-lo
exige a máquina de quem relatou.

**A terceira validação que reprovou fora desta tarefa, medida em 2026-09-14.** O
relato veio com saída 101 e um registro cortado; a revisão independente nomeou
`convite.rs` estourando ~20 s com `SemResposta`, e mediu o mesmo numa cópia do
commit base — 1 de 3 rodadas reprovando com a mesma assinatura, em `anexos.rs`.
Repeti aqui a suíte inteira do workspace **quatro vezes**: três com saída 0 e 69
conjuntos verdes; a que reprovou acusou
`um_server_com_portaria_nao_admite_ninguem_por_um_caminho_lateral`
(`crates/seele-conformance/tests/acceptance_seguranca.rs`) com *«esperava uma
recusa enumerada do servidor, veio Some(SemResposta)»*. Sozinho, esse arquivo
passa **3 de 3, em 7,5 s, 7,7 s e 6,9 s**.

É o protocolo da pendência **29** aplicado à letra: reprovando em conjunto e
passando sozinho, é carga. Quatro sinais apontam para lá e nenhum para cá — o
arquivo acusado muda a cada rodada e nenhum deles é tocado por este diff; a
assinatura é sempre a mesma (`SemResposta`, ~20 s); o base reprova na mesma
proporção; e, durante estas medidas, **outra sessão rodava a suíte inteira na
mesma máquina**, que é exatamente a saturação descrita em 29. Nada do caminho de
portaria, convite ou anexo passa pelo aparelho de som. Consertar isso é a
pendência 29 — semáforo no `start()` da conformidade —, e fazê-la aqui seria
trocar de tarefa.

**Medido pela quinta vez em 2026-09-14, na retomada.** A validação independente
voltou com saída 101 e o mesmo acusado —
`um_server_com_portaria_nao_admite_ninguem_por_um_caminho_lateral`, com *«esperava
uma recusa enumerada do servidor, veio Some(SemResposta)»*, e o conjunto levando
**26,7 s** onde sozinho leva **8**. Sozinho ele passou **3 de 3** (9,2 s, 8,2 s,
7,9 s) e a suíte inteira do workspace, refeita com `--no-fail-fast`, deu
**1.733 passados, 0 reprovados, saída 0**, sem um `FAILED` sequer. O prazo que
estoura é `PRAZO_POR_CANDIDATO` (`crates/seele-core/src/enlace.rs:571`), quatro
segundos por candidato, num arquivo que este diff não toca — e o caminho de
portaria não chega perto de placa de som nenhuma. Continua sendo a pendência 29.

**Medido pela sexta vez em 2026-09-14, e desta vez com os dois resultados na
mesma sessão.** `cargo test` no workspace saiu **0**; logo em seguida
`cargo test --no-fail-fast`, na mesma árvore e sem nenhuma edição entre um e
outro, reprovou **1 de 1.733** em
`seele-conformance/tests/moderacao.rs::um_pessoa_comum_e_recusado_pelo_server_e_nao_pela_casca`
com `Error: SemResposta`. É a assinatura de sempre: o arquivo levou **20,1 s** no
conjunto e **0,43 s** sozinho, e sozinho passou **3 de 3**. Vinte segundos é o
prazo de conexão queimando, não um comportamento diferente — e o acusado muda de
rodada para rodada (`convite`, `acceptance_m2`, `ocupacao`, agora `moderacao`),
que é o que distingue saturação de regressão. Nenhum deles toca aparelho de som,
e `seele-server` não depende de `seele-core` nem de `seele-audio` (ADR 0002).
Continua sendo a pendência 29, e o remédio é o semáforo no `start()` da
conformidade.

**As três provas de reversão, refeitas na retomada de 2026-09-14.** A revisão
independente aprovou o trabalho lendo as reversões registradas acima, mas não pôde
executá-las — a instrução dela proibia tocar em arquivo. Ficaria só a palavra de
quem escreveu, e é justamente o que esta casa não aceita. Refeitas as três, uma de
cada vez, com o arquivo restaurado e a bateria reexecutada depois de cada uma:

- **O seam do erro** (`retorno_de_erro`, trocando o corpo por
  `move |_error| errors.record_stream_error(FalhaDeAparelho::Transitoria)`):
  `cargo test -p seele-conformance --test troca_de_aparelho` reprova **6 de 7**,
  com as mensagens de sempre — entre elas *«a tela ficou congelada no aparelho
  anterior… a voz saiu pelo aparelho novo e a pessoa leu o nome errado»*
  (`left: Some("fone-usb")`, `right: Some("caixas-da-mesa")`) e *«a sessão nunca
  desistiu, e um estado de «trocando» eterno é a mesma mentira do silêncio
  calado»*. O único verde continua sendo o do estalo, pelo motivo de sempre.
- **O achado G** (fazendo `poll` apagar `next_attempt_at_ms` em vez de rearmá-lo):
  `cargo test -p seele-audio` dá `219 passed; 1 failed`, e o que cai é
  `quem_nao_relata_a_tentativa_nao_fica_sem_cronometro` — o guarda escrito
  exatamente contra travar em `Recovering` sem cronômetro.
- **O achado F** (devolvendo `UserSelected` para `Running` e
  `(Running, ReopenFailed)` para `Action::Wait`): `cargo test -p seele-audio` dá
  `216 passed; 4 failed`, e caem os quatro que cercam o par —
  `a_escolha_do_usuario_nao_declara_sucesso_antes_de_reabrir`,
  `a_escolha_do_usuario_que_nao_abre_continua_sendo_tentada`,
  `uma_falha_de_reabertura_sobre_um_estado_sao_nao_e_engolida` e
  `the_user_can_always_recover_a_lost_device`.

Com os arquivos de volta: `seele-audio` **220 + 4 + 8**, conformidade
`troca_de_aparelho` **7 de 7**, e `cargo test` no workspace inteiro **saiu 0**
nesta rodada, sem nenhum `FAILED`. (As contagens de unidade citadas mais acima
são de antes do teste acrescentado depois; por isso 220 aqui e 219 lá.)

**O retoque de formatação em `vetores_de_hash.rs`, conferido de novo.** Antes de
mantê-lo no diff, medi outra vez: devolvendo o arquivo ao que veio do commit
anterior, `cargo fmt --check -p seele-proto` reprova em três pontos. Sem ele não
dá para afirmar árvore limpa, e é só por isso que ele continua aqui.

**Medido pela sétima vez em 2026-09-14, na retomada seguinte.** A validação
independente voltou outra vez com saída 101 e a revisão nomeou
`moderacao.rs::expulsar_acaba_com_a_sessao_e_deixa_voltar` — mais um nome novo na
lista, mais um arquivo que este diff não toca. Refiz a bateria inteira aqui:
`cargo test --workspace` saiu **0**, com **1.733 passados, 0 reprovados, 4
ignorados**, e nenhum `FAILED` no registro. `cargo fmt --all --check` e
`cargo clippy --workspace --all-targets -- -D warnings` saíram **0** os dois. E o
crate acusado, rodado sozinho logo depois, deu **122 passados, 0 reprovados**. É a
sétima vez que o acusado muda e a sétima vez que o conjunto passa quando se repete
— o padrão que a pendência **29** descreve, e o remédio dela continua sendo o
semáforo no `start()` da conformidade, que é outra tarefa.

**O teste novo não engorda a carga que causa a pendência 29.** Vale dizer porque
ele mora justamente no crate saturado: `troca_de_aparelho.rs` termina os sete
testes em **0,00 s**. Ele não levanta servidor, não abre porta e não espera prazo
nenhum — injeta o erro do `cpal` no fechamento por onde o `cpal` chamaria e lê o
estado. O crate está pesado pelos que levantam QUIC de verdade; este não é um
deles.

**Medido pela oitava vez em 2026-09-14.** Desta vez a reprovação da validação
**reproduziu aqui**, e foi consertada: era `par_lento.rs` perguntando o contador
de gravação antes de o lote ter vez (a entrada logo abaixo conta o caso e a
medida). Depois dela, `cargo fmt --all --check` e
`cargo clippy --workspace --all-targets -- -D warnings` saíram **0** os dois, e as
partes desta tarefa passaram inteiras: `seele-audio` **232 passados, 0
reprovados** e `seele-conformance` inteira **122 passados, 0 reprovados** —
incluindo os sete testes novos de troca de aparelho.

O que sobrou é a pendência **29**, e agora com o dedo num arquivo só: duas
rodadas seguidas de `cargo test --workspace` reprovaram em
`crates/seele-conformance/tests/moderacao.rs`, uma em
`mover_leva_a_pessoa_e_a_conta_na_sala_nova` e a seguinte em
`expulsar_acaba_com_a_sessao_e_deixa_voltar`, **as duas estourando exatamente o
`PRAZO` de 10 s** do auxiliar `ate`. O mesmo arquivo passa **6 de 6 em 0,42 s**
quando roda sozinho, três vezes seguidas. É espera que não coube na máquina
cheia, num arquivo que este diff **não toca** — e alargar o prazo já foi medido e
recusado nesta mesma pendência: o remédio decidido é serializar a conformidade,
que é tarefa própria. Repetida a rodada logo em seguida, sem tocar em nada,
`cargo test --workspace` saiu **0** com **1.733 passados, 0 reprovados, 4
ignorados** — o mesmo «passa quando se repete» que a pendência 29 descreve.

**Medido pela nona vez em 2026-09-14, na retomada que trata a revisão.** Antes de
mexer em nada, `cargo test --workspace` na árvore como estava saiu **0** com
**1.733 passados, 0 reprovados** — a reprovação relatada pela validação
independente **não reproduziu aqui**, mais uma vez com acusado diferente do da
rodada anterior, que é a assinatura da pendência 29. Depois dos dois guardas
novos desta retomada, a bateria inteira saiu **0** outra vez, com **1.735
passados, 0 reprovados** — os dois a mais são exatamente eles. `cargo fmt --all
--check` e `cargo clippy --workspace --all-targets -- -D warnings` saíram **0**
os dois, sem um aviso sequer. O conflito de integração em `vetores_de_hash.rs`
não existe mais: o arquivo aqui é idêntico ao da `main`, e a simulação de junção
não acusa conflito nenhum.

### A ordem dos três passos da troca, que só existia dentro do laço

A revisão independente apontou, sem bloquear, que o teste de conformidade
`troca_de_aparelho.rs` exercita o ciclo do aparelho e o painel da sessão, mas não
o ramo de `pipeline()` que **refaz as dimensões, esvazia o que era do aparelho
antigo e reacerta o relógio de reprodução**. Estava certo: cada um dos três
passos tinha o seu guarda de unidade, e a ordem entre eles — que é justamente o
que a pessoa ouve no instante da troca — não tinha nenhum, porque morava solta
dentro de um laço de tempo real que nenhum teste alcança.

O ramo virou uma função com nome, `recomecar_no_aparelho_novo`, pelo mesmo
motivo que `dimensoes_ou_dizer_que_nao_ha` já era uma: as coisas que não podem se
separar ficam juntas onde um teste consegue chamá-las. Dois guardas novos em
`seele-core`:

- `a_troca_redimensiona_esvazia_e_reacerta_o_relogio_na_mesma_volta` — uma troca
  de fone de 48 kHz por um de 44,1 kHz, com meio segundo de reabertura, e as três
  consequências conferidas na mesma chamada.
- `o_aparelho_novo_que_o_reamostrador_recusa_encerra_a_volta_sem_mexer_em_nada` —
  a taxa recusada encerra o laço **e** a tela deixa de dizer «funcionando».

**Provas de reversão, medidas.** Tirando `esvaziar_o_que_era_do_antigo` e
`playout.reacertar` do corpo da função, `cargo test -p seele-core --lib
dimensoes_do_aparelho` dá **6 passados, 1 reprovado**, e o que cai é o guarda
novo com *«restou amostra da taxa de antes para tocar no aparelho novo»*.
Tirando **só** o `reacertar`, o mesmo guarda cai na outra ponta: *«o compasso
recomeçou num despejo de reposição em vez de um quadro»*, `left: 4`,
`right: 1` — os quatro quadros de reposição que meio segundo de reabertura
deixaria para trás. Com o arquivo de volta, `seele-core` dá **265 passados, 0
reprovados** (eram 263 antes destes dois).

### O conjunto de vagas é da máquina, e não da árvore de trabalho

A mesma revisão notou que as vagas moram na pasta temporária do sistema, então
duas árvores de trabalho rodando a bateria ao mesmo tempo disputam o mesmo
conjunto e se serializam uma contra a outra. É verdade, e é de propósito: o que
satura é a máquina — núcleos e portas efêmeras —, e um conjunto por árvore daria
a cada uma o direito de levantar o seu tanto de servidores QUIC, que somados são
exatamente a saturação que o portão existe para evitar. O escopo estava certo e o
comentário não dizia; agora diz, junto com a saída para quem quiser as árvores
independentes (`SEELE_VAGAS` reparte o teto, `SEELE_VAGAS=0` desliga o portão de
uma delas).

### A vaga mora na thread do teste, e é por isso que ela basta

A revisão independente levantou, sem bloquear, que `seele_conformance::vaga()` é
chamada de dentro de `async fn` em testes `#[tokio::test(flavor =
"multi_thread")]`: se a chamada caísse numa thread de trabalho, um teste que
levanta dois servidores tomaria duas vagas, e a espera seria bloqueante dentro do
executor. Conferido nos **21 pontos de chamada** do crate, não é o que acontece:
toda chamada está no preparo do servidor, antes do primeiro `tokio::spawn`, e é
aguardada do corpo do teste — e o `block_on` do sabor multi-thread conduz o futuro
na própria thread do `libtest`, não numa de trabalho. Logo é **uma vaga por
teste**, devolvida quando a thread do teste acaba, e a espera não tem tarefa
alguma para atrasar porque nada foi disparado ainda. O que faltava era isso estar
escrito onde se erra: a documentação de `vaga()` passou a dizer para chamar do
corpo do teste e nunca de dentro de uma tarefa disparada, com o motivo. Continua
valendo que o portão **mitiga** a saturação da pendência 29 e não cura a
sensibilidade ao relógio de parede, que é tarefa própria.

**Validação desta rodada, depois dessa nota.** `cargo test` do workspace saiu
**0** com **1.733 passados, 0 reprovados**, sem um `FAILED` sequer;
`cargo test -p seele-audio` deu **232 passados** somando os quatro alvos do crate;
`troca_de_aparelho` deu **7 de 7**; `cargo fmt --all --check` e
`cargo clippy --workspace --all-targets -- -D warnings` saíram limpos. A
reprovação de validação relatada pelo coordenador **não se reproduziu** nesta
rodada — e ela nunca foi o áudio: as duas vezes em que foi possível ler o teste
reprovado, era conformidade ou servidor estourando prazo de relógio de parede sob
carga, o retrato da pendência 29.

### Os três arquivos deste diff que não são do aparelho de som

A revisão independente apontou, com razão, que testes fora do caminho de áudio
aparecem no diff. Nenhum deles é melhoria de passagem: os três são o preço de
conseguir afirmar «árvore limpa» nesta tarefa, e ficam registrados aqui para que
ninguém precise adivinhar depois.

- **`crates/seele-proto/tests/vetores_de_hash.rs`.** O arquivo chegou quebrado do
  commit anterior desta mesma branch. Devolvendo-o ao que veio de lá,
  `cargo fmt --all --check` reprova em três pontos **e**
  `cargo clippy -p seele-proto --all-targets -- -D warnings` sai **101** com cinco
  erros: tipo complexo demais, `write!` terminando em nova linha,
  `indexing_slicing` duas vezes e `expect()` sobre `Result` — este último sendo
  `deny` do workspace (`specs/10-convencoes.md`). O conserto é o `#![allow(...)]`
  de teste que os arquivos irmãos já usam, mais o que o `rustfmt` pede. Nenhuma
  asserção mudou.

  **E em 2026-09-14 ele saiu do diff, porque quem opera já tinha o conserto.** A
  tentativa de integrar travou num conflito neste arquivo, e a regra desta casa é
  conferir o que está publicado antes de explicar o defeito: a `main` já traz o
  mesmo `allow` e a mesma formatação, pelo commit `74056b8`. A única diferença que
  sobrava era uma linha de comentário do alias `type Caso`. Adotada a versão da
  `main`, o arquivo fica **idêntico** ao publicado — o conflito deixa de existir,
  o diff desta tarefa deixa de carregar um arquivo alheio, e o que sustentava
  mantê-lo continua valendo: `cargo fmt --all --check` e
  `cargo clippy -p seele-proto --all-targets -- -D warnings` saem **0** os dois.
  Restam dois arquivos alheios no diff, não três.
- **`crates/seele-server/tests/tipo_de_fluxo.rs`.** Este entrou por ter sido a
  falha de validação de uma rodada anterior desta tarefa, com saída 101. A causa
  está no comentário da função `escrever_mesmo_que_parem`: o servidor larga o
  fluxo assim que decide que não vai guardar aquele corpo, largar um fluxo de
  entrada no QUIC manda `STOP_SENDING`, e a escrita seguinte do cliente falha com
  `Stopped`. Quem errava era o teste, que tratava a corrida como erro e reprovava
  sob carga com «sending stopped by peer: error 0». O que cada teste prova está
  nas asserções depois da escrita, e nenhuma delas depende de o corpo ter chegado;
  qualquer erro de escrita que não seja `Stopped` continua reprovando.
  A revisão independente apontou que o `finish` logo abaixo era descartado com
  `let _`, o que é mais largo do que a justificativa escrita acima dele: engoliria
  **qualquer** erro futuro, e não só a corrida tolerada. Agora o encerramento
  passa por `encerrar_mesmo_que_parem`, que anota o tipo do retorno como
  `Result<(), quinn::ClosedStream>` antes de tratá-lo. `ClosedStream` é a única
  falha que `finish` sabe devolver — o caso `Stopped` ele próprio já converte em
  sucesso —, e a anotação é o guarda: se o quinn passar a devolver um erro mais
  largo, o arquivo deixa de compilar em vez de calar a diferença.
  A prova é de compilação, e não de execução — trocando a anotação para
  `quinn::WriteError`, o crate reprova com «mismatched types: expected
  `Result<_, WriteError>`, found `Result<_, ClosedStream>`», e o arquivo foi
  restaurado com conferência de hash. Reverter para `let _` **não** reprova
  nenhum teste: é exatamente esse silêncio que a anotação passa a quebrar.
- **`crates/seele-server/tests/par_lento.rs`.** Este foi a reprovação da rodada de
  validação de 2026-09-14 mais recente, com saída 101 e a mensagem «o servidor
  perdeu mensagem antes de gravar»: **1.155 de 1.160** gravadas. O teste manda as
  1.160 sem esperar confirmação, dorme três segundos e pergunta o contador de uma
  vez só — mas a gravação sai em lote, uma volta a cada 200 ms. Numa máquina
  carregada, perguntar ali é perguntar **antes de o último lote ter vez**: falta
  de tempo, não de mensagem. Agora o teste repergunta por até cinco segundos e só
  então afirma. **A asserção é a mesma e continua reprovando se faltar mensagem de
  verdade**, e isso está medido: pedindo 1.161 num teste que diz 1.160, ele
  reprova ao fim do prazo, em 8,21 s, com «left: 1160, right: 1161». O arquivo
  passou 3 de 3 sozinho, 3 de 3 com vinte processos comendo CPU e 6 de 6 em
  paralelo consigo mesmo — a reprovação só apareceu sob a suíte inteira, que é o
  retrato da pendência **29** noutro crate.
- **`crates/seele-conformance/tests/anexos.rs`.** Esta foi a reprovação da rodada
  de validação seguinte, também com saída 101, e é a que menos parece ter a ver
  com som: `Error: SemResposta` em
  `o_server_enche_sem_passar_do_teto_e_a_mensagem_diz_que_o_arquivo_expirou`.
  `SemResposta` não é servidor calado — nasce de `PRAZO_POR_CANDIDATO`, os quatro
  segundos por candidato de `seele-core/src/enlace.rs`, que são **relógio de
  parede**. Numa máquina saturada, um `connect` em loopback queima a janela sem
  que nada esteja quebrado: o arquivo sozinho roda em 1,2 s, e a execução que
  reprovou levou 20,13 s. Agora `entrar` tenta de novo, até três vezes e **só
  nesse erro**; qualquer outra recusa sobe na hora, porque é resposta do servidor
  e não falta de tempo, e um servidor mesmo mudo continua reprovando o teste ao
  fim das tentativas.

  **O que não está provado, e é preciso dizer.** A reprovação foi reproduzida uma
  vez — seis cópias do arquivo em paralelo, uma delas em 20,13 s. Depois do
  conserto ela **não voltou a ser reproduzível sob encomenda**, e sem
  reprodutibilidade não há prova de reversão honesta aqui: 18 execuções em
  paralelo, 16 cópias simultâneas com o dobro de núcleos comendo CPU, e 8 cópias
  de cada lado (com e sem o conserto) enquanto a suíte inteira rodava junto — em
  nenhuma delas o binário **sem** o conserto reprovou, e o pico ficou em 3,17 s,
  longe dos 20 s. Quer dizer: o conserto ataca o mecanismo que a falha capturada
  nomeou, mas o que sustenta essa ligação é a mensagem de erro, e não um A/B.
  O que **está** medido é que ele não afrouxa o teste — o número de tentativas é
  três, e só `SemResposta` as consome.

**Medido pela nona vez em 2026-09-14, na retomada do conflito de integração.** A
validação voltou de novo com saída 101, e de novo sem o teste reprovado no
registro — só o rabo da compilação. Refeita a bateria inteira aqui, com a saída
completa guardada em vez de truncada: `cargo test` do workspace saiu **0**, com
**1.733 passados, 0 reprovados, 4 ignorados**, e nenhum `FAILED` em parte alguma
do registro. `cargo fmt --all --check` saiu **0**. É a nona vez que a suíte passa
inteira aqui depois de uma reprovação relatada que não traz o nome do teste, e
continua sendo o retrato da pendência **29** — cujo remédio, serializar a
conformidade, é tarefa própria e não desta.

**Medido pela décima vez em 2026-09-14, respondendo à revisão que pediu o
resultado inteiro.** `cargo test --workspace --all-targets` saiu **0** nas duas
execuções seguidas, com o registro guardado por completo desta vez: **1.734
passados, 0 reprovados**, 73 baterias, nenhum `FAILED` em parte alguma. O que a
revisão disse faltar no registro anterior está aqui pelo nome: `seele-audio`
unidade **220 passados**, e `troca_de_aparelho` **7 de 7** — os sete nomeados, da
troca do padrão ao fone religado depois da desistência. `cargo fmt --all --check`
e `cargo clippy --workspace --all-targets` saíram **0** e sem um aviso.

**As provas de reversão, desta vez executadas por quem não as escreveu.** A
revisão independente registrou, com razão, que as reversões eram relato: ela não
podia alterar arquivo nenhum. Foram refeitas à mão, uma de cada vez, com o
arquivo restaurado byte a byte depois de cada uma:

- **O seam do erro.** Trocando o corpo de `device::classificar` por
  `FalhaDeAparelho::Transitoria` — que é literalmente o defeito de origem —
  reprovam 4 unidades de `seele-audio` (`a_troca_feita_no_sistema_chega_ao_laco_como_troca`
  com `trocas: 0` onde se esperava `1`, `o_aparelho_arrancado_chega_ao_laco_como_sumico`
  com `sumicos: 0` onde se esperava `2`, e as duas de `classificacao_do_erro_do_cpal`)
  e **6 dos 7** testes de `troca_de_aparelho`. O sétimo que continua passando é
  `um_estalo_no_fluxo_nao_troca_o_aparelho_de_ninguem` — o controle negativo,
  que é exatamente quem **não** deveria cair. É a medida mais forte do conjunto:
  a conformidade da troca depende do seam, e depende dele pelo motivo certo.
- **Achado G.** Voltando `self.next_attempt_at_ms = None` no ramo de `poll` que
  entrega `Reopen`, `quem_nao_relata_a_tentativa_nao_fica_sem_cronometro` reprova
  com *«no attempt ever became due»*.
- **Achado F.** Voltando `UserSelected` a entrar em `Running` e o par
  `(Running, ReopenFailed)` a devolver `Wait`, reprovam quatro:
  *«a escolha ainda não abriu nada; o estado não pode ser Running»*,
  *«a falha da reabertura foi engolida: Running»*, *«estado depois da falha:
  Running»*, e `the_user_can_always_recover_a_lost_device`.
- **A ordem da volta da troca.** Tirando `esvaziar_o_que_era_do_antigo`,
  `a_troca_redimensiona_esvazia_e_reacerta_o_relogio_na_mesma_volta` reprova com
  *«restou amostra da taxa de antes para tocar no aparelho novo»*; tirando
  `playout.reacertar`, reprova com *«o compasso recomeçou num despejo de
  reposição em vez de um quadro»*, `left: 4, right: 1`.

**O arquivo alheio que sobrou virou dois.** `crates/seele-proto/tests/vetores_de_hash.rs`
não difere mais da `main`: o conflito de integração foi resolvido adotando o lado
de lá, e `git diff main` sobre ele não mostra nada. Restam os dois de
`seele-server`, com a justificativa de cada um logo acima.

**O braço coringa de `classificar`, e até onde dá para fechá-lo.** A revisão
observou que uma variante futura do `cpal` que signifique «o aparelho sumiu»
entraria como tropeço e não pediria reabertura. `cpal::ErrorKind` é
`#[non_exhaustive]`: nenhuma anotação obriga a lista a crescer junto com a
dependência, então **não existe** conserto que faça o compilador reprovar por
isso. O que dá para fazer é prender por comportamento a superfície inteira de
hoje, e foi feito: o teste dos tropeços passou a cobrir as dez variantes que o
`cpal` 0.18 tem fora das duas famílias que pedem reabertura — antes eram cinco,
e `InvalidInput`, `PermissionDenied`, `ResourceExhausted`, `UnsupportedConfig` e
`UnsupportedOperation` estavam classificadas sem prova nenhuma. Com isso, no dia
em que o `cpal` subir, a variante nova é a única fora da lista, e decidir onde
ela cai vira revisão de quem sobe a dependência em vez de omissão que passa
calada. A revisão seguinte voltou ao mesmo ponto, e o que faltava era pequeno e
vale: as dez variantes estavam presas no teste, mas escondidas atrás do `_` no
próprio `match`. Agora elas estão escritas uma a uma em `classificar`, com o `_`
atrás delas como rede e não como decisão — o comportamento é o mesmo, e quem
subir o `cpal` vê a superfície de hoje no lugar onde vai mexer, em vez de
precisar procurar o teste para descobrir o que já foi decidido. As outras
duas ressalvas da revisão — o conjunto de vagas ser da máquina
inteira e a tolerância a `WriteError::Stopped` — já estão justificadas acima, nos
seus próprios blocos, e a revisão as classificou como efeito fora do escopo desta
tarefa.

### Os guardas de texto-fonte que ficaram, e o que sustenta cada um

O aceite pede que guarda de `include_str!` seja substituído **ou complementado**
por comportamento. Três leituras de texto sobreviveram, e nenhuma delas está
sozinha — cada uma cobre uma ordem ou uma ligação que tipo nenhum expressa:

- `crates/seele-core/src/voice.rs` exige a chamada de `acompanhamento.passo(`
  dentro do corpo de `pipeline`. Prova que **alguém conduz** o supervisor, que era
  o buraco do achado H; que a condução funciona é o que os sete testes de
  `troca_de_aparelho.rs` provam.
- `crates/seele-ffi/src/lib.rs` (`mod conferir_a_troca`) confere a chamada da
  conferência. O comportamento por trás dele são os testes do próprio crate, que
  passam pelo `Snapshot` de verdade.
- `apps/seele-app/tests/frontend.rs` lê o corpo de `desenharAparelho`, mas
  amarrado à serialização real do enum e aos nomes de campo em Rust: o texto
  procurado vem do tipo, não de uma constante repetida na mão. Se o campo mudar de
  nome, o guarda cai junto.

A distinção que importa: nenhum deles é o guarda **principal** de nada. O que
prova a troca é comportamento, e a prova disso são as três reversões registradas
acima.

### A bateria que o coordenador viu vermelha, e o que ela era

A validação automática voltou com `signal: 15`. Isso é **SIGTERM**: a rodada foi
encerrada por tempo antes de terminar, não por teste reprovado — a saída cortada
mostra suites ainda *sendo executadas*, nenhuma delas com falha. Refeita crate a
crate, a bateria inteira passa: `seele-audio` 232 (220 de unidade + 8 + 4 de
integração), `seele-core` 265, `seele-ffi` 91, `seele-conformance` completa com
`--test-threads=1` (27 suites, incluindo os 7 de `troca_de_aparelho`) e o
restante do espaço de trabalho 1025, com `EXIT=0` em todas as quatro rodadas.
`cargo clippy --workspace --all-targets` sai sem um aviso e `cargo fmt --all
--check` sai limpo.

A serialização da conformidade não é máscara: está documentada no item 29 deste
mesmo arquivo — as vagas de porta efêmera são da máquina inteira, e rodadas
concorrentes as esgotam. É a mesma razão pela qual o CI já serializa essa crate.


## 32 · O botão ENVIAR sai da barra de compor

**Pedido de quem desenha o produto em 2026-08-31**, para a 0.9.0: *«era um
ajuste de UI para remover o botão»*.

Enter sempre enviou. A nota ao lado do campo já diz «Enter também envia», e o
botão ao lado dela é a segunda maneira de fazer a mesma coisa ocupando o lugar
mais valioso da barra.

**A armadilha, e ela já está desarmada.** Sem o botão `type="submit"`, o envio
implícito do navegador passa a se apoiar só na outra pré-condição — a de haver
**exatamente um** campo de texto no formulário. Um campo a mais acrescentado
depois apagaria o Enter sem tocar na frase que o promete.

Por isso o ouvinte explícito de Enter entrou antes, em `tela-sessao.js`, com o
guarda `a_nota_que_promete_o_enter_tem_quem_a_cumpra` prendendo a frase e o
código juntos. **Quem remover o botão não precisa fazer mais nada além de
remover o botão** — e o `ENVIAR` do `index.html` sai com o CSS
`.compor-enviar`.

**Quando dói.** Não dói: é ajuste de desenho, e está aqui para não se perder
entre a conversa em que foi pedido e o pacote em que entra.

## 33 · O compartilhamento de tela parou entre duas máquinas Windows

**Sintoma, relatado em 2026-08-31 e estreitado em 2026-09-01.** Entre duas
máquinas Windows a tela não aparece. **É regressão:** quem relatou confirma que
funcionava — *«era bem pixelado e feio, mas pelo menos mostrava que tava
compartilhando»* — e parou entre a **0.8.4** e a **0.8.5**.

**O que a 0.8.5 levou de vídeo, e o que cada um deles já custou de prova:**

| mudança | estado |
|---|---|
| CABAC (`8c6661e`) | **eliminado.** A consequência dele era o perfil errado no decodificador, e o `ffd2025` a consertou na 0.8.6 — que não resolveu. |
| eixo nitidez/movimento (`1acbdb3`) | **eliminado.** Só escolhe entre números que já existiam. |
| reescritor de SPS (`60de331`) | **eliminado em 2026-09-01**, e esta era a hipótese forte: ele mexe bit a bit no cabeçalho, e só tinha sido provado contra o OpenH264 do macOS. Rodado no do Windows: SPS íntegro nas três resoluções, cor presente, 9 de 10 quadros de volta. |
| reescrita da conversão (`bf1162d`) | **eliminado em 2026-09-01.** As duas bibliotecas novas têm caminhos SIMD por processador — NEON aqui, AVX2 lá —, e um defeito num não aparece no outro. As 135 combinações de formato passam **nos dois**. |

**Por que o exemplo `cor.rs` não tinha provado nada disso antes.** Ele procurava
o módulo do codec num caminho terminado em `.dylib`: **nunca teve como rodar no
Windows.** A prova de que o SPS reescrito sobrevive ao codificador foi feita
contra o OpenH264 do Mac, e o defeito é do Windows. Corrigido para aceitar
`SEELE_OPENH264`, e é assim que os dois testes acima puderam ser feitos.

**O que sobrou, e por que daqui não se alcança.** Os quatro passam em teste de
unidade nas duas plataformas. O que os testes não tocam é o caminho de captura
**de verdade** — a textura do D3D11, o `row_pitch` que o driver escolhe, o
`on_frame_arrived` — e ele exige sessão gráfica. Por SSH a máquina não a tem.

**As duas perguntas que separam o que restou**, e as duas são de campo:

1. **O que aparece escrito no palco?** Desde `6410606` toda falha do lado de
   quem recebe escreve no palco em vez de só no console — sete causas, sete
   frases. A frase é o diagnóstico.
2. **Mac → Windows funciona?** Se sim, o problema é o Windows **enviando**; se
   não, é o Windows **recebendo**, e o alvo muda inteiro.

**Quando dói.** Sempre que duas máquinas Windows tentam. É o caso da casa de
quem relatou.
