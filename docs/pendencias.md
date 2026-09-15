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

## 11 · Fechada em 2026-09-13 · A sessão velha apagava a nova

**O que era.** Todo o desmonte de uma sessão era chaveado por `PersonId` e nunca
comparava a sessão que está morrendo com a que está viva. Quatro operações
soltas no fim da conexão — sair da tarefa da sala, encerrar a tela, desocupar o
assento, sair dos presentes — mais uma quinta cópia parcial em `LeaveVoiceRoom` e
uma sexta em `VoiceRoomDeleted`. Com duas conexões da mesma pessoa vivas ao mesmo
tempo, a que morria levava a que tinha acabado de chegar: fora do roster, fora
dos presentes, fora da tarefa de mídia e fora da tela, de uma vez.

**Relato de campo, e não defeito de leitura.** «Como anfitrião, eu não vejo um
amigo que se vê dentro da sala, e não ouço nem vejo esse amigo.» A auditoria
1881a3cb mapeou o caminho inteiro por arquivo e linha.

**O que foi consertado.** Existe uma saída simétrica de `assentar`:
`desassentar`, em `session.rs`, e as seis cópias passaram a chamá-la. Os quatro
estados que uma saída mexe — a lotação, a lista de presentes, o membro da tarefa
da sala e a transmissão de tela — passaram a **carregar o identificador da
sessão**, e cada um confere no próprio registro de qual conexão ele é. Uma sessão
só apaga o que ela mesma escreveu, e `PersonLeft` e `PersonGone` saem apenas
quando houve de verdade o que desocupar. A reserva de assento da carência ganhou
a pergunta correlata — «esta conexão ainda é a desta pessoa?», respondida só por
registro de presença que exista e bata —, de modo que uma sessão que morre depois
de a pessoa já ter voltado não guarda mais uma reserva de cinco minutos que
re-sentaria alguém que saiu de propósito.

**Selo no registro, e não uma pergunta central.** A primeira versão deste
conserto fazia uma pergunta só, no começo de `desassentar`, e voltava sem fazer
nada quando a resposta era «esta já não é a conexão vigente». Isso **custava o
contrário** do que queria comprar, e o teste de expulsão mostrou: quem é expulso
reconecta em milissegundos, a sessão expulsa morre depois, e a pergunta central a
impedia de limpar o que era dela — quem foi expulso ficava desenhado na sala para
quem ficou. Conferir no registro não depende de ordem nenhuma e não tem como
virar vazamento.

**Onde o registro anterior estava errado, e importa.**

1. **«Some assim que qualquer pessoa entra ou sai, porque aí o roster é
   reconstruído.»** Não some. `translate`, em `session.rs`, difunde apenas o
   evento de quem se mexeu, e o cliente funde `seats` de maneira **aditiva**
   (`state.rs`). Não existe evento que reconstrua roster nenhum. A divergência
   ficava de pé pelo resto da sessão de quem ficou — e é por isso que o relato de
   campo diz «com frequência» em vez de «pisca e passa».
2. **O escopo não era `:ejetar` e reentrada.** A aritmética das constantes diz o
   resto: o cliente entra na bateria interna com três `Ping` perdidos a 5 s cada
   — perto de 15 s — e o servidor só desiste da conexão muda aos 20 s de
   `IDLE_TIMEOUT`. Numa queda silenciosa a sessão nova sobe **antes** de a velha
   morrer, com cerca de cinco segundos de folga. Não é uma corrida com
   probabilidade: é a ordem esperada de qualquer queda de rede.
3. **O guarda projetado só sobre `vacate` era insuficiente.** Sem cobrir a saída
   da tarefa da sala e o encerramento da tela, a metade do relato que diz «não
   ouço nem vejo esse amigo» ficava sem explicação e sem conserto.

**Como está provado.** Um teste de conformidade novo
(`crates/seele-conformance/tests/desassentar.rs`) derruba a conexão do visitante
**sem** `CONNECTION_CLOSE` — o objeto é esquecido e o runtime dele é desligado,
de modo que ninguém responde ao keepalive — reconecta na hora e, passado o tempo
ocioso inteiro mais margem, afirma três coisas: o anfitrião continua com o
visitante no roster, o visitante continua nos presentes do servidor, e um
datagrama dele ainda é encaminhado. O teste reprovava antes do conserto e a
reversão do guarda o faz reprovar de novo com a mesma mensagem. Quatro testes de
unidade cobrem os guardas um a um, e também reprovam quando revertidos.

**E o teste afirma que o guarda disparou, não que nada aconteceu.** Achado de
revisão, e o mais importante dos três: as três asserções acima passariam também
num mundo em que a sessão velha simplesmente não tivesse sido desmontada dentro
do prazo medido — passariam por ausência de evento, e não por defesa, que é o
contrário do que o arquivo existe para provar. O conserto, funcionando, não
deixa rastro nenhum: ele acerta calando-se. Então há agora um contador do
processo inteiro, `Server::desassentamentos`
(`crates/seele-server/src/server.rs`, no formato do `Atrasos` que já existia),
que anda uma unidade toda vez que uma conexão chega ao fim e encontra a pessoa
dela **já presente por outra conexão** — exatamente a queda silenciosa desta
pendência. O teste afirma esse número antes de qualquer outra coisa, e a mesma
linha sai por `tracing` para quem opera: zero é o normal numa rede que não cai, e
um número que cresce durante uma chamada é a rede de alguém piscando.

A prova de reversão foi refeita com a asserção nova em vigor, e ela separa as
duas coisas como deveria: revertendo o guarda da tarefa da sala, a asserção do
contador do processo **passa** — o desmonte da sessão velha de facto rodou — e o
teste reprova adiante, em «a voz do visitante deixou de ser encaminhada». Antes,
essa mesma reprovação não distinguia defesa de inércia.

> **Este parágrafo é a rodada de então, e a ordem da reprovação já mudou.** Ele
> ficou porque é o registro de por que o contador do processo existe, não porque
> descreva o que se vê hoje. Depois dele entrou um segundo contador, o da própria
> tarefa da sala, e uma asserção sobre ele **antes** das de estado: revertendo o
> mesmo guarda hoje, o teste reprova primeiro ali — «a tarefa da sala não
> registrou nenhuma saída de sessão velha barrada» —, e só com essa asserção
> neutralizada é que ele chega à voz que deixou de ser encaminhada. A rodada
> corrente está descrita adiante, na décima revisão.

**O que a revisão cobrou, e foi fechado.** Três coisas, todas da família «existir
não é funcionar». A primeira: a recusa de uma saída de sessão velha era só um
contador que ninguém lê fora dos testes — agora ela também sai por `tracing`, e
pode sair por evento sem virar enxurrada porque acontece no máximo uma vez por
conexão desmontada, e não por quadro (é justamente por isso que o resto de
`DropCounts` não pode fazer o mesmo). A segunda: o guarda da reserva de assento
morava solto dentro de uma função que só roda com servidor, QUIC e relógio de
verdade, e portanto nenhum teste o alcançava; virou
`reservar_o_assento_da_carencia`, em `server.rs`, com teste próprio — e a
reversão foi provada, reprovando com «a conexão velha guardou um assento para uma
pessoa que já voltou por outra». A terceira: a saída por pessoa sem guarda de
sessão numa sala só (`Occupancy::vacate`) tinha ficado pública e sem nenhum
chamador de produção, ou seja, pronta para ser a sétima cópia do defeito; foi
removida, e o teste que a usava passou a exercitar a versão com guarda e a
afirmar que a segunda saída não inventa uma despedida.

**O que a segunda revisão cobrou, e foi fechado.** Duas coisas, e as duas da
mesma família. A primeira: sobrara um desmonte chaveado só por pessoa — o fim de
uma transmissão decidido pelo servidor, em `receber_tela`. Aquela tarefa vive
fora do laço da sessão e pode chegar ao fim depois de a conexão nova da mesma
pessoa já ter reaberto a tela; era a sétima cópia da mesma contabilidade e a
única sem o selo. Agora a sessão é levada até lá e o encerramento confere de qual
conexão a transmissão é. Não ganhou teste próprio: o caminho só existe com fluxo
QUIC de verdade, e o que se pode afirmar é a simetria com as outras seis — está
dito aqui em vez de ficar implícito.

A segunda: entrar de novo na tarefa da sala trocava o membro da pessoa mas não
retirava o **número de voz** (`ssrc`) da conexão substituída. Antes do guarda,
quem apagava esse resto era a saída da sessão velha; com o guarda, ela volta cedo
e não apaga mais nada. O efeito não era só memória crescendo por reconexão: o
número de uma conexão morta continuava valendo, e um datagrama com ele seria
encaminhado em nome da pessoa. A entrada agora retira o número anterior, e o
teste `o_ssrc_da_conexao_velha_para_de_valer_quando_a_pessoa_entra_de_novo`
prova os dois lados — revertendo a linha, ele reprova com «o ssrc da conexão
morta continuou entregando voz em nome de quem reconectou».

**O que a terceira revisão cobrou, e foi fechado.** Três achados, nenhum
bloqueante, e dois deles viraram código.

O primeiro era um resíduo de verdade na reserva de assento. A pergunta que
`reservar_o_assento_da_carencia` faz respondia **«sim» quando não havia registro
de presença nenhum**, com o argumento de que uma sessão morta antes de se
anunciar presente não deve deixar assento de fantasma para trás. O argumento
estava certo e a conclusão, invertida: quem nunca se anunciou presente também
nunca entrou em sala, e portanto nunca chega a essa linha — só se chega lá com
uma sala na mão. O ramo permissivo, então, só disparava no caso oposto, o de o
registro **já ter sido tirado por uma sessão posterior**: a sessão A cai calada,
B reconecta, B sai da sala de propósito e se desconecta, e só então A morre, até
vinte segundos depois. Sem registro a que se comparar, A gravava uma reserva de
cinco minutos com a sala e o `ssrc` velhos, e o `handshake` seguinte re-sentava
na sala quem tinha saído por vontade própria — o mesmo sintoma que esta pendência
diz ter fechado. A ausência de registro passou a ser **recusa**, e o teste
`a_sessao_velha_nao_reserva_assento_depois_de_a_nova_ter_saido_de_proposito`
percorre o encadeamento inteiro. Reversão provada: voltando a ausência a ser
permissão, ele reprova com «a conexão velha guardou assento para quem tinha
acabado de sair de propósito», junto com
`sem_registro_de_presenca_ninguem_e_a_vigente`.

O segundo era um estreitamento que o próprio conserto tinha introduzido: em
`LeaveVoiceRoom`, o desmonte passou a rodar só dentro de um `if let Some(id) =
current_voice_room.take()`. Antes, a saída da tarefa da sala e o encerramento da
tela eram incondicionais, e por isso **corrigiam** qualquer divergência entre o
que a conexão lembra e o que o servidor tem. Não há hoje caminho conhecido em que
a pessoa seja membro da sala com a conexão não lembrando de nenhuma — não é
defeito reproduzível —, mas trocar uma saída corretiva por uma que só limpa
quando já está tudo certo é exatamente o tipo de perda silenciosa que esta
pendência existe para não repetir. `Saida::Sala` passou a carregar
`Option<VoiceRoomId>`: a mídia e a tela morrem sempre, e só o assento e o
`PersonLeft` dependem de haver sala conhecida. A expulsão, que tinha a mesma
forma, mudou junto. (Este último pedaço — «só o assento depende de haver sala
conhecida» — foi corrigido na quarta revisão, logo abaixo.)

O terceiro é cobertura, e fica registrado sem conserto: o guarda de sessão do fim
de tela decidido pelo servidor (`receber_tela`) continua sem teste próprio. Vale
dizer por que, em vez de deixar implícito. Alcançá-lo de fora exigiria que a
conexão velha ainda tivesse um fluxo de tela vivo **e** que a conexão nova da
mesma pessoa tivesse reaberto a transmissão; mas a tarefa da sala recusa a
segunda abertura da mesma pessoa enquanto a primeira está registrada, de modo que
não há caminho de conformidade que monte esse estado. O que o guarda faz continua
provado onde a decisão mora — `Telas::encerrar_de` com sessão, com teste e
reversão —; o que não está provado é o **local da chamada** passar a sessão em
vez de `None`. É defesa em profundidade, e é assim que deve ser lido. (Este
parágrafo caducou na sétima revisão, abaixo: o local da chamada passou a ter
guarda próprio, e o que ele diz de «sem teste próprio» já não vale.)

Fora do escopo desta pendência, e **desfeito depois da última revisão**:
`crates/seele-proto/tests/vetores_de_hash.rs`. Este ramo nasceu num commit em que
o portão do repositório
(`cargo clippy --workspace --all-targets --all-features -- -D warnings`) estava
vermelho nesse arquivo — `expect_used` negado no workspace, `indexing_slicing`
virando erro, e formatação fora do padrão —, e a correção foi feita aqui para o
portão passar. Enquanto isso a linha principal consertou o mesmo arquivo por
conta própria, em `74056b8`, e melhor: o `type_complexity` lá saiu por um
`type Caso`, não por mais uma dispensa. Manter a nossa cópia só criaria conflito
num arquivo que nada tem a ver com sessão, então o arquivo passou a ser **byte a
byte o da linha principal**. Voltar à versão da base foi medido e descartado no
caminho: sem nenhuma das duas correções, `cargo fmt --check` reprova em três
pontos desse arquivo e o portão de clippy do workspace volta a ficar vermelho — o
defeito é herdado, mas deixá-lo aberto seria trocar ruído no diff por portão
quebrado. Adotar a versão de lá resolve os dois: o portão passa, o diff desta
tarefa contra a linha principal não mostra mais nenhuma alteração em
`seele-proto`, e a reaplicação não terá conflito nesse arquivo.

**O que a quarta revisão cobrou.** Três achados, nenhum bloqueante; um virou
código e dois já estavam registrados aqui.

O que virou código foi o ramo vazio da saída sem sala lembrada
(`Saida::Sala(None)`). Ele desmontava mídia e tela e **não varria lotação
nenhuma** — a única das saídas a deixar assento para trás. A justificativa
anterior, escrita dois parágrafos acima e agora corrigida, era que anunciar a
saída de uma sala que não se sabe qual é seria adivinhar. Não era preciso
adivinhar: `vacate_everywhere_da_sessao` devolve exatamente as salas em que
**esta sessão** tinha assento, e se não tinha nenhum a varredura não faz nada e
não anuncia nada. O ramo passou a fazer o mesmo que a saída de conexão faz, que
é o análogo seguro. Não há defeito de campo atrás disto — hoje o caminho quase
não se alcança, porque o assentamento sempre grava a sala na conexão —, e é
exatamente por isso que ele estava errado sem ninguém notar: uma conexão que
perdeu a conta de onde estava é a que mais provavelmente deixou assento atrás.

Os outros dois já estão ditos acima e continuam valendo como estão: o guarda do
fim de tela decidido pelo servidor (`receber_tela`) segue sem teste próprio, pelo
motivo já explicado, e o teste de expulsão em `moderacao.rs` mede agora depois do
fechamento do cliente — e por isso deixou de afirmar o efeito **imediato** da
expulsão sobre a sala. A causa dessa perda é anterior a este trabalho
(`EnterVoiceRoom` não confere `Permission::EnterVoiceRoom`, achado aberto da
auditoria), e consertá-la aqui seria outro escopo; o que o teste afirma hoje é
verdadeiro e está declarado no próprio arquivo.

**Um vizinho que apareceu, e está medido.** O teste de expulsão
(`moderacao.rs`) passava por causa deste mesmo defeito: ele afirmava que a sala
esvazia para quem ficou, e o que a esvaziava era a sessão expulsa morrendo depois
e anunciando uma saída que já não era dela. O cliente expulso reconecta na hora e
**reentra na sala**, porque `EnterVoiceRoom` não confere
`Permission::EnterVoiceRoom` (achado próprio da auditoria, aberto). O teste agora
fecha o cliente antes de medir — e aí vem a segunda medida: fechar o cliente
depois de uma expulsão **não se despede** do servidor, porque o `disconnect` da
ponte chega a um motor que já parou. O servidor só dá aquela conexão por morta no
tempo ocioso do QUIC: medido em 2026-09-13, entre 20 s e 22 s. É um defeito de
cliente, fica registrado aqui e não foi consertado nesta pendência.

**A quinta revisão não pediu código, pediu medida.** Dos três achados dela, dois
já estão escritos acima e continuam valendo — a expulsão que não segura ninguém
fora da sala (pendência #45) e o guarda do fim de tela sem teste próprio. O
terceiro era sobre a **prova**, e não sobre o conserto: a saída de validação
anexada junto com a entrega era de outra bateria, a de empacotamento, e portanto
não continha um único teste desta pendência; e a prova de reversão estava
registrada aqui sem ter sido refeita por quem revisou. As duas coisas foram
medidas de novo em 2026-09-13, nesta ordem:

- `seele-server` e `seele-proto`: 400 e 200 testes, mais os de integração — tudo
  passou, sem falha nenhuma.
- Conformidade inteira, serializada: 27 binários, 116 testes, zero falhas,
  incluindo o teste desta pendência.
- Portão do repositório (`clippy --workspace --all-targets --all-features
  -D warnings`): passou sem um aviso.
- Reversão refeita à mão: desarmando o guarda da saída da sala, o teste reprova
  em 31 s com exatamente a mensagem registrada — «a voz do visitante deixou de
  ser encaminhada: a sessão velha o tirou da tarefa da sala ao morrer». Com o
  guarda de volta, ele passa em 28 s. A prova deixou de ser um parágrafo sobre
  uma medida antiga.

**A sexta revisão fechou as duas últimas cópias por pessoa, e trancou a porta.**
Três achados, todos não bloqueantes, todos viraram código.

O primeiro eram `Telas::parar` e `Telas::de`, que continuavam chaveados só por
pessoa. Fora do caminho da queda silenciosa — a sessão velha está muda —, mas da
mesma família, e cada uma com um efeito próprio: um `StopScreenShare` atrasado da
conexão velha derrubava a transmissão que a nova acabou de abrir, e quem transmite
não recebe aviso nenhum, então segue mandando quadros para uma sala que já não os
encaminha; e `de`, que é a pergunta que a tarefa dos fluxos faz para decidir se
aceita um fluxo de tela, casava um fluxo da conexão velha com a transmissão
registrada pela nova — duas fontes escrevendo o mesmo `ScreenId`. As duas passaram
a receber `SessionId` e a conferir. Reversão provada nas **duas metades
separadamente**, porque a primeira asserção a reprovar esconde a segunda:
desarmando só `parar`, o teste
`a_sessao_velha_nao_para_nem_assina_a_tela_que_a_nova_abriu` reprova em «a conexão
velha parou a transmissão da nova»; desarmando só `de`, reprova em «um fluxo da
conexão velha seria aceito como o da nova». Com os dois no lugar, passa.

O segundo era a porta que sobrou: `Occupancy::vacate_everywhere`, a versão sem
guarda, continuava **pública**. Ela não era chamada por nenhum caminho de
desmonte, e é justamente essa a forma do defeito que esta pendência mediu seis
vezes — uma remoção por pessoa disponível é uma remoção por pessoa que o próximo
caminho vai usar. Ela passou a ser privada. O único chamador legítimo é
`Occupancy::seat`, porque sentar tem de derrubar o assento anterior de qual sessão
for, e `seat` passou a **devolver** as salas que esvaziaram — que era a única razão
pela qual `assentar` a chamava por fora. Quem desmonta uma conexão agora não tem
escolha: as versões com sessão são as únicas alcançáveis de fora do módulo. A
prova aqui é de compilação, e não de teste: o portão do workspace inteiro passa, e
nenhum caminho fora de `server.rs` consegue mais nomeá-la.

O terceiro era a perda de cobertura no teste de expulsão, já registrada acima.
(Também caducou na sétima revisão: a cobertura voltou, por outro caminho.) Não
mudou de estado na sexta — enquanto a pendência #45 estiver aberta a reentrada é legítima e
não há o que afirmar sobre a sala com o cliente expulso ainda conectado —, mas
deixou de estar registrada só aqui: o próprio `moderacao.rs` agora diz, no lugar
onde a asserção mudou, **o que deixou de ser afirmado**, por que medir essa janela
seria medir o relógio, e o que continua provado sobre a expulsão e onde. Um teste
que perdeu alcance e não conta é a mesma falha de «o produto sabe e não conta»,
uma altura acima.

**A sétima revisão aprovou, e os dois achados de cobertura viraram código.**
Ambos eram da mesma forma: uma propriedade que o repositório sabia ser verdadeira
e não afirmava em lugar nenhum.

O primeiro era o teste de expulsão. Duas revisões seguidas registraram que ele
tinha deixado de provar o esvaziamento **imediato** da sala, e as duas concluíram
que não havia o que fazer enquanto a pendência #45 estivesse aberta. A conclusão
estava errada, e o erro era de enquadramento: o que atrapalhava a medida não era a
permissão que falta, era a **bateria interna do cliente expulso**, que reconecta em
milissegundos e reentra na sala no meio da janela. Parar o cliente **antes** da
expulsão tira a bateria do caminho e a janela volta a ser mensurável, sem depender
da #45 em nada. É o que o teste faz agora, numa segunda visita calada de
propósito: ela senta, o cliente dela para, o anfitrião a expulsa, e o assento tem
de sumir dentro de 10 s. O prazo é o discriminante — se o assento sumisse por
tempo ocioso do transporte, sumiria em 20 s —, e o próprio teste confere essa
relação numa asserção antes de medir, para que encurtar o tempo ocioso um dia não
transforme a medida em outra coisa calada. Reversão provada: retirando a chamada
de expulsão, reprova em 10,3 s com «a expulsão não tirou a pessoa da sala de voz
dentro de 10s»; com ela, os 6 testes do arquivo passam em 21 s. A asserção lenta
anterior continua onde estava, provando a outra metade — que quem foi expulso e
fechou o cliente some de vez.

O segundo era o guarda de sessão no fim de tela decidido pelo servidor, em
`receber_tela`, registrado duas vezes como «defesa em profundidade sem teste
próprio». Continua sendo verdade que nenhum teste de conformidade alcança aquele
estado, pela razão já escrita acima. Mas o que faltava provar não era o
comportamento: era **o local da chamada passar `Some(sessao)` e não `None`** — e
isso se lê no próprio código. `todo_fim_de_tela_declara_de_qual_sessao_e`, ao lado
do guarda de plano de mídia que já usava essa forma, percorre `session.rs` e exige
que cada uma das três chamadas de `encerrar_telas_de` case com o que sua função
declara: `desassentar` encerra só o desta conexão, `assentar` encerra a pessoa
inteira porque andar de sala tem de levar toda transmissão anterior embora, e
`receber_tela` encerra só o desta conexão. Uma chamada nova em função não listada
reprova pedindo classificação, em vez de herdar um padrão calado. Reversão
provada: trocando `Some(sessao)` por `None` em `receber_tela`, o teste reprova
nomeando a linha, a função, o que esperava e o que encontrou.

**A oitava revisão aprovou, e o único achado de código virou a asserção que
faltava.** Os dois apontamentos eram não bloqueantes e de prova, não de
comportamento.

O primeiro era que a prova de reversão estava registrada mas não tinha sido
reexecutada por quem revisou — a revisão não altera arquivos, e desarmar um guarda
exige alterar um. Foi reexecutada aqui, e nas **duas metades**, porque cada uma
reprova numa asserção diferente e a primeira esconde a segunda. Desarmando só o
guarda da sala de voz, o teste reprova em «a voz do visitante deixou de ser
encaminhada», 31,0 s — é a metade que o usuário relatou como «não ouço nem vejo
esse amigo». Desarmando só o filtro por sessão de `vacate_everywhere_da_sessao`, ele
reprova em «a sessão velha, ao morrer, apagou a nova do roster do anfitrião:
["anfitriao"]», 28,0 s, e a mensagem carrega junto o que o servidor tinha
(`["anfitriao/sessão 1"]`) e que o enlace do anfitrião piscou zero vezes — quer
dizer, a reprovação é do servidor e não da máquina. Com os dois guardas no lugar,
passa em 28,0 s.

O segundo era um contador que ninguém lia de ponta a ponta.
`DropCounts::saida_de_sessao_velha` anda dentro da tarefa da sala toda vez que uma
conexão velha morre tentando tirar da sala a conexão nova da mesma pessoa — é o
defeito se defendendo —, e até aqui só tinha cobertura de unidade. O teste de
conformidade provava a metade de mídia **pelo efeito**: o datagrama chega. O efeito
sozinho não distingue «a sala barrou a saída velha» de «a saída velha nunca chegou
à sala», e essa é exatamente a diferença entre um guarda que funciona e um teste
que passa por inércia — a armadilha do «existir não é funcionar» que este
repositório já pagou caro. A sala é uma tarefa e seus contadores são um campo dela,
então ler de fora pedia um caminho: `VoiceRoomCommand::Contadores` responde por
`oneshot` pela mesma fila, o que também **ordena** a resposta depois de todo comando
já enfileirado; `VoiceRooms::contadores` pergunta e devolve `None` para sala que
não existe, de propósito sem fazê-la nascer, porque uma leitura que cria a coisa
medida mede outra; e `Daemon::voice_rooms` abre a porta para o teste. Nada disso
toca o protocolo: `VoiceRoomCommand` é interno ao servidor. A asserção nova entrou
antes das três de estado, ao lado da que já exigia `conexoes_velhas() >= 1`.
Reversão provada: desarmando o guarda da sala, o teste agora reprova **primeiro**
nela — «a tarefa da sala não registrou nenhuma saída de sessão velha barrada»,
despejando os contadores inteiros — em vez de reprovar só lá adiante no datagrama.

**A nona revisão aprovou sem achado de código, e a falha de validação relatada
junto não reproduz.** Os três apontamentos dela são não bloqueantes e já estão
escritos acima, cada um no lugar onde a decisão foi tomada: a cobertura da janela
curta da expulsão, que voltou pela segunda visita calada e cujo resto está atado à
pendência #45, ainda aberta; o custo de relógio do teste novo, que é o tempo ocioso
do transporte e não folga de implementação — medir a reconexão silenciosa custa
esperar o servidor desistir da conexão velha; e a formatação mais o `allow` de
clippy em `crates/seele-proto/tests/vetores_de_hash.rs`, fora do tema desta
pendência mas necessários para o portão de qualidade passar, e sem efeito de
comportamento.

A falha de validação que veio anexada foi procurada antes de ser explicada, e não
apareceu. Aqui, em 2026-09-13: `cargo test` do workspace inteiro verde (69
binários, nenhuma falha, `desassentar` incluído e passando), `cargo fmt --check`
limpo, `cargo clippy --workspace --all-targets --all-features -- -D warnings`
limpo, e a suíte de conformidade repetida mais duas vezes em seguida, verde nas
duas. A saída relatada na validação estava truncada no começo e tudo o que ela
mostra passa; não há falha reproduzível para consertar, e inventar um conserto
para uma falha não observada seria o erro que este arquivo já registra três vezes.
Se ela voltar, o suspeito conhecido é o tempo: as asserções deste arquivo e de
`moderacao.rs` esperam relógio de transporte, e uma máquina carregada é o caminho
mais curto para uma reprovação por prazo.

**A décima revisão aprovou, e o achado que ela deixou virou teste de
comportamento.** Ela apontou que o fim de tela por sessão estava provado lendo o
próprio texto do arquivo — o guarda casava argumentos e contava chamadas — e que
o efeito continuava provado só de lado, pelo registro das transmissões e pelo
teste de conformidade. Era a queixa certa: um guarda textual protege contra a
regressão que ele imagina, e é justamente disso que o repositório desconfia.

Duas coisas mudaram. A primeira é de forma: o fim de tela deixou de receber «de
qual sessão» como um valor que podia ser vazio e virou **duas funções nomeadas** —
uma que encerra a transmissão desta conexão e outra que encerra a da pessoa
inteira, esta com um chamador só, a troca de sala, que precisa mesmo levar tudo
embora. A troca de um caractere que devolvia o defeito inteiro deixou de compilar.
A segunda é de prova: há agora teste que exercita o servidor de verdade — a mesma
pessoa transmitindo por duas conexões, a velha morrendo depois de a nova ter
reaberto — e afirma as duas metades, que a transmissão nova continua encaminhável
e que **nenhum** anúncio de fim de tela sai, porque é ele que apaga a imagem da
tela de quem assiste. Reversão provada: fazendo o fim de tela ignorar a sessão, o
teste novo reprova dizendo que a conexão velha apagou a transmissão que a nova
tinha acabado de abrir — e o guarda textual, sozinho, continuava verde, que é
exatamente o que a revisão previu.

O guarda textual ficou, com o escopo encolhido e dito em voz alta no próprio
comentário: ele decide qual das duas funções cada lugar chama, que é uma escolha
que nenhum tipo impede, e não afirma mais nada sobre efeito.

O segundo apontamento era sobre este arquivo: o parágrafo da rodada de reversão
anterior descrevia uma ordem de reprovação que já não é a de hoje. Ele foi mantido
como registro, com um aviso em cima dizendo que é histórico e apontando para a
rodada corrente.

**A décima primeira revisão aprovou, e o único achado de código virou a asserção
que faltava pela letra.** Ela observou que o teste provava o encaminhamento
recebendo o datagrama no anfitrião, mas não afirmava em lugar nenhum que
`DropCounts::not_a_member` tinha ficado parado — o aceite pede as duas coisas, e
só uma estava escrita. A diferença não é cerimônia: o encaminhamento pode
sobreviver a um descarte parcial — um segundo ouvinte barrado, por exemplo — e aí
a asserção do datagrama passaria com a sala ainda esquecendo alguém.

A asserção (d) entrou no fim do teste, lendo os contadores da sala pelo caminho
que a rodada anterior abriu. Ela compara com a leitura tirada **antes** do envio,
e não com zero cru, para falar do datagrama que este teste mandou e não herdar
ruído de nada que tenha acontecido antes. A ordem está garantida pela fila: mídia
e contadores viajam pelo mesmo `mpsc` da tarefa da sala, então a resposta vem
depois do datagrama já aplicado.

Prova de que ela não é decorativa, feita por falsificação em vez de por reversão —
desarmar o guarda faria o teste reprovar na asserção anterior, e (d) nunca chegaria
a correr. Trocando a linha de base para `+ 1`, o teste reprova em 28,0 s com
`left: 0, right: 1` e despeja os contadores inteiros, entre eles
`not_a_member: 0` e `saida_de_sessao_velha: 1`: quer dizer que a asserção lê estado
vivo da sala, que o número real é zero, e que o guarda de fato agiu na medida.
Restaurada a linha de base, o teste passa em 28,0 s.

A falha de validação anexada a esta rodada também não reproduz: `cargo test` do
workspace inteiro terminou com código de saída 0 aqui, sem nenhuma falha. Como na
nona rodada, a saída relatada vinha truncada no começo e tudo o que ela mostra
passa.

**A décima segunda revisão aprovou, e as duas notas que ela deixou são de
registro.** A primeira era sobre a validação anexada à rodada: a saída de
`cargo test` que veio com o pedido é a da bateria das ferramentas de publicação e
não passa por nenhuma das crates tocadas aqui. Ela não mostra falha nenhuma, e
também não mostra o que interessa. A medida certa foi refeita nesta rodada e está
adiante, na seção da validação.

A segunda é um limite conhecido, e fica dito em voz alta em vez de ficar
implícito: se a conexão velha morrer **antes** de a conexão nova se anunciar
presente, o registro de presença ainda é o da velha, o guarda casa por direito e
`PersonGone` sai; a nova reanuncia `PersonPresent` logo em seguida e a lista
converge. Quem assiste vê a lista piscar, e não vê ninguém sumir. O assento não é
afetado, porque a lotação confere a sessão no próprio registro dela e não depende
da lista de presentes. Não há conserto sem inventar uma ordem entre duas conexões
que a rede não garante, e o preço de errar essa ordem para o outro lado seria uma
presença fantasma permanente — que é o defeito desta pendência ao contrário.

**A décima terceira revisão aprovou, e os dois achados de código dela foram
consertados.** Nenhum dos dois era regressão: os dois eram buraco que o guarda
novo deixou à mostra ao arrumar o resto.

O primeiro é uma janela estreita na reserva do assento da carência. O guarda
pergunta a `Presentes` quem é a sessão vigente, e a conexão nova só entra em
`Presentes` depois do `handshake`: uma conexão velha que morra exatamente entre
uma coisa e outra ainda é «a vigente», a pergunta responde «sim» e a reserva
obsoleta é gravada assim mesmo, com a sala e o `ssrc` velhos, valendo cinco
minutos — que é o defeito de re-sentar quem saiu de propósito, de volta pela porta
dos fundos. Fechada pelo outro lado da mesma pergunta, e sem cronômetro nenhum:
quando uma conexão se declara presente, o `handshake` dela **já resgatou** o que
houvesse para resgatar, de modo que qualquer reserva que exista naquele instante
foi escrita depois disso — quer dizer, por uma conexão que já não é a desta
pessoa. `descartar_a_reserva_de_quem_ja_voltou` joga-a fora e diz no registro que
jogou. Não há caso legítimo a perder: o descarte só alcança o que foi escrito
depois do resgate.

O segundo é a vaga de tela, que era o único estado do desmonte cuja identidade de
sessão era **sobrescrita** em vez de conferida. Quem já transmite e pede de novo
troca a própria tela — é o `StartScreenShare` depois de reconectar —, e a troca
acontecia calada: o cliente funde aditivamente, e só um `ScreenShareStopped`
apaga um cabeçalho de transmissão, de modo que o `ScreenId` anterior ficava
desenhado para sempre em quem assiste, prometendo um fluxo que já não tem de onde
vir. `Telas::comecar` passou a devolver a tela que substituiu, e quem chama
anuncia o fim dela antes do começo da nova. É a família «o produto sabe e não
conta», e o conserto é dizer.

Reversão provada para os dois, e com a mensagem esperada: tornando o descarte um
`false` constante, `a_reserva_escrita_depois_da_volta_e_descartada_por_quem_voltou`
reprova em «a reserva obsoleta da conexão velha sobreviveu à volta da pessoa»;
fazendo `comecar` devolver `None` na troca,
`quem_troca_a_propria_tela_recebe_de_volta_a_que_saiu` reprova dizendo que a
conexão nova tomou a vaga da velha em silêncio. Restaurado o arquivo, os dois
passam. Os dois testes afirmam também o outro lado — sem reserva não se mente
dizendo que descartou, a reserva de outra pessoa fica onde está, e a primeira
transmissão não substitui transmissão nenhuma —, para que não passem por fazer
sempre a mesma coisa.

A terceira nota da revisão era sobre a validação anexada ao pedido, que de novo
era a bateria das ferramentas de publicação e não toca nenhuma crate desta
pendência. A medida certa está adiante.

**A décima quarta revisão aprovou, e o achado que ela deixou fechou a última porta
por pessoa desta família.** Ela observou que `VoiceRoomCommand::TelaFechou` — o
fim do fluxo de tela no plano de dados — continuava chaveado só por `PersonId`, e
que o mapa de transmissões da sala não guardava de qual conexão cada uma saiu. O
argumento de que o risco era estreito está certo e não basta: numa queda
silenciosa a tarefa da tela morre no `?` da leitura e nunca chega a mandar o
comando, mas num fim **limpo** do fluxo velho — que acontece, por exemplo, quando
a conexão velha fecha ordenadamente enquanto a nova já transmite — a sala apagava
a transmissão da conexão nova, e é a mesma imagem parada que esta pendência
inteira existe para não deixar acontecer.

A transmissão passou a carregar a sessão, copiada do `Member` no instante da
abertura e não recebida do comando: a sala já sabe qual é a conexão vigente de
cada pessoa, e o que ela sabe não precisa ser jurado por quem fala com ela. O
`TelaFechou` declara de qual conexão é, e só encerra quando bate.

O conserto descobriu a metade que faltava do próprio guarda de saída. Com a saída
da sessão velha voltando cedo, a transmissão **dela** — que já não tem de onde
receber bytes — ficava no mapa para sempre, e trancava a vaga de tela da sala
contra a conexão nova da mesma pessoa, que abriria e receberia `tela_ja_tomada`
por causa de uma tela morta. O guarda passou a ser preciso em vez de grosso: a
saída da conexão velha não mexe no que é da nova, e leva embora o que ainda é
dela.

Reversão provada nas duas metades, cada uma reprovando com a sua mensagem.
Fazendo o fim de tela ignorar a sessão,
`o_fim_da_tela_da_conexao_velha_nao_apaga_a_transmissao_da_nova` reprova em «o fim
do fluxo da conexão velha apagou a transmissão da nova», com `left: None` e
`right: Some(ScreenId(90))`. Tirando o encerramento da transmissão da conexão
morta do ramo do guarda, o mesmo teste reprova antes, em «a transmissão da conexão
morta ficou no mapa trancando a vaga de tela da sala contra a conexão nova da mesma
pessoa». Restaurado o arquivo, passa. O teste afirma também o outro lado — a
conexão que abriu a transmissão continua encerrando-a, e quem assiste continua
recebendo quadros da transmissão que ficou no ar —, para que não passe por não
encerrar nunca.

**A décima quinta revisão aprovou, e o achado dela fechou a escrita que sobrava.**
Ela observou que `Telas::comecar` era a única escrita desta família ainda chaveada
só por `PersonId`, e que ela **sobrescreve** a sessão dona da vaga em vez de
conferi-la. A sobrescrita é de propósito — é assim que um `StartScreenShare`
depois de reconectar toma a vaga que a conexão anterior tinha, e é o caso comum —,
mas a mesma porta serve ao contrário: um `StartScreenShare` **atrasado** da
conexão velha toma a vaga da nova e, de lambuja, manda `ScreenShareStopped` da
tela que a nova acabou de abrir. A revisão classificou o achado como não
bloqueante, e com razão: pela queda silenciosa não se chega lá, porque a conexão
velha está muda. Com as duas conexões vivas ao mesmo tempo — que é o que a janela
de cinco segundos permite — chega-se.

O guarda tem nome próprio e mora fora do laço da sessão, pela mesma razão de
`reservar_o_assento_da_carencia`: `comecar_a_tela_da_conexao_vigente` confere em
`Presentes` quem é a conexão vigente antes de deixar escrever, e devolve
`AberturaDeTela::DeConexaoVelha` em vez de um `Option` que não distinguiria «a vaga
é sua, e não substituiu nada» de «esta conexão já não é a desta pessoa». Nada é
escrito, nada é anunciado e nada é recusado a quem pediu — a recusa iria para uma
ponta que ninguém está lendo —, e o caso soma no mesmo contador de conexões velhas
em que o resto do guarda se conta, com uma linha de registro dizendo o que
aconteceu.

Reversão provada, com a mensagem esperada: tirando a conferência da sessão,
`a_tela_pedida_pela_conexao_velha_nao_toma_a_vaga_da_nova` reprova em «a conexão
velha tomou a vaga de tela da nova, e quem chama vai anunciar o fim da transmissão
que está no ar», com `left: Aberta { substituida: Some(ScreenId(2)) }` e
`right: DeConexaoVelha`. Restaurado o arquivo, passa. O teste afirma também o
outro lado — a conexão vigente abre a primeira transmissão e troca a própria tela
recebendo de volta a que saiu —, para que não passe por recusar sempre.

A segunda nota da revisão, o custo de relógio que a asserção «Três» de
`moderacao.rs` acrescentou, é a mesma da nona rodada e continua sendo o tempo
ocioso do transporte, não folga de implementação: medir a reconexão silenciosa
custa esperar o servidor desistir da conexão velha. A cobertura da janela curta
ficou na asserção «Dois», e o resto segue atado à pendência #45, aberta.

**Validação desta rodada.** `cargo test` das três crates tocadas — `seele-server`,
`seele-proto` e `seele-conformance` — terminou com código de saída 0: 740 testes
passando, nenhuma falha e um ignorado (o de plataforma, como sempre), com
`desassentar.rs` entre eles em 28,03 s — um a mais que a rodada anterior, que é o
guarda da vaga de tela contra o pedido atrasado da conexão velha. `cargo clippy
--all-targets -- -D warnings` das crates tocadas, limpo, e `cargo fmt --check`
limpo.

A «falha de validação» anexada a esta rodada é, pela terceira vez, a saída da
bateria das ferramentas de publicação: ela não toca nenhuma crate desta pendência
e, no que mostra, não reprova nada. A medida que interessa é a de cima.

**A décima primeira revisão aprovou sem achado de código, e o único apontamento
que sobrava era sobre prova, não sobre comportamento**: as reversões estavam
registradas aqui mas a revisão não podia reexecutá-las, porque revisar não altera
fonte. Reexecutada à mão nesta rodada, a do guarda da sala: trocando a pergunta
«esta saída é da sessão que ainda é membro?» por um `false`, `desassentar` reprova
em 28,03 s com exatamente a mensagem registrada acima — «a tarefa da sala não
registrou nenhuma saída de sessão velha barrada» — e despeja os contadores com
`saida_de_sessao_velha: 0`. Restaurado o arquivo byte a byte, passa em 28,04 s. A
prova, portanto, deixou de ser só escrita.

**Uma reprovação nova apareceu e não é desta pendência.**
`bateria_interna.rs::o_server_cai_e_a_sessao_entra_na_bateria_em_vez_de_acabar`
reprovou uma vez em «não deu para ligar em 127.0.0.1:56025 · Address already in
use», e passou sozinho e passou de novo na bateria inteira logo em seguida. O
arquivo não é tocado por esta pendência e a causa é do desenho dele: ele pede uma
porta ao sistema, derruba o servidor, espera cerca de vinte segundos a sessão
entrar na bateria e só então religa **a mesma** porta — e a porta que o sistema deu
está na faixa efêmera, de onde outro binário de teste rodando em paralelo pode
tomá-la durante esses vinte segundos. Escolher um número fixo trocaria esta
corrida por outra, e o repositório já decidiu o contrário em
`ejetar.rs:310`. Fica anotado como defeito de teste, não de servidor; consertá-lo
é mexer num arquivo fora do escopo desta tarefa.

**A décima segunda revisão aprovou e deixou dois apontamentos, ambos de texto ou
de escopo, nenhum de comportamento.**

O primeiro era uma frase que prometia mais do que o código garante: a mensagem da
asserção «Dois» de `moderacao.rs` dizia que o tempo ocioso do transporte «é mais
que o dobro» do prazo medido, quando a conferência logo acima pede `PRAZO * 2 <=
IDLE_TIMEOUT` e hoje isso é igualdade exata — dez segundos contra vinte. A
asserção continua correta (a janela medida começa na queda do cliente calado e
termina dez segundos antes de o tempo ocioso poder desocupar o assento), mas o
texto descrevia uma folga inexistente e enganaria quem encurtasse o tempo ocioso.
A frase passou a dizer «é pelo menos o dobro deste prazo, como a conferência logo
acima garante»: agora texto e guarda afirmam a mesma coisa, e é a conferência —
não a prosa — que sustenta a afirmação.

O segundo é a última porta desta família e **fica registrada, não remendada**:
`EnterVoiceRoom` (`session.rs`, no `assentar`) continua sendo caminho de escrita
sem a pergunta `e_a_vigente`. Uma conexão velha ainda viva que mande
`EnterVoiceRoom` reassenta a pessoa pela sessão velha e derruba o membro da
conexão nova. Não está no aceite desta tarefa — o aceite nomeia as seis cópias de
saída e a reserva de assento, todas cobertas — e não é a mesma coisa que os
guardas de saída consertam: aqui não é a morte da sessão velha que apaga a nova, é
um **comando** da sessão velha. O conserto certo é o da pendência #45, que já tem
de decidir o que `EnterVoiceRoom` confere na entrada (a permissão, hoje não lida) e
como o servidor responde em vez de «confirmar por silêncio»; acrescentar a
pergunta da sessão vigente ali, sem essas duas decisões, seria mexer no caminho de
entrada com meia resposta. Fica atado a #45 como terceira metade dela.

**Validação desta rodada.** `cargo test` das três crates tocadas — `seele-server`,
`seele-proto` e `seele-conformance` — e `cargo fmt --check`; o resultado real está
no relato da rodada, e não na saída da bateria das ferramentas de publicação, que
voltou anexada pela quarta vez e não toca nenhuma crate desta pendência.

**A junção com a linha principal é tarefa própria, e a tentativa mediu por quê.**
Este trabalho nasceu sobre uma base que a linha principal deixou 88 commits para
trás, e que reescreveu os mesmos arquivos. Tentada a reaplicação sobre a linha
principal, com o trabalho já comitado e uma referência de segurança, o resultado
foi medido e depois desfeito sem perda: cinco arquivos em conflito, dos quais dois
são triviais (um `allow` de clippy e o bloco de testes, que só concatena) e três
não são. O que os torna não-triviais não é o tamanho:

1. **A linha principal consertou parte do mesmo defeito por outro caminho.**
   `Presentes::saiu` lá já confere o `ssrc` da conexão, com um relato de campo de
   07/09 idêntico ao desta pendência. É a mesma família de defeito resolvida com
   outra chave: `ssrc` distingue conexões, `SessionId` também, e o guarda daqui é
   o mais amplo — cobre as seis saídas e as escritas, e não só os presentes.
   Escolher uma das duas chaves e retirar a outra é decisão de desenho, não
   resolução de conflito.
2. **A linha principal trouxe um subsistema que não existia aqui.** O empréstimo
   de subida entre pares entrou junto do desmonte de conexão: onde esta pendência
   chama `desassentar`, lá se chama `soltar_telas_e_pares_de`, que encerra telas
   **e** devolve ao servidor quem assistia por um par que saiu. Fundir as duas é
   decidir se as operações de pares também passam a conferir a sessão vigente — e
   a resposta provável é que sim, pelo mesmo argumento desta pendência, o que faz
   disso trabalho novo sobre código novo, com diagnóstico próprio.

Resolver isso às cegas seria mexer no subsistema de pares sem a investigação que
ele merece. Fica registrado como a próxima tarefa, com a medida já feita; o
trabalho desta pendência segue comitado no ramo, inteiro e validado sobre a base
em que foi escrito.

**A revisão seguinte aprovou, e o único achado acionável era deriva de escopo.**
Dos três apontamentos, dois já estão escritos acima e continuam valendo sem
mudança de código: a expulsão que deixou de ser encoberta pelo `PersonLeft`
indevido e por isso passou a mostrar o defeito da pendência #45 — é ganho de
verdade, e o custo é que quem modera passa a ver o tamanho real do problema —; e a
impossibilidade de a revisão reexecutar as reversões, que já foi respondida
reexecutando uma à mão na rodada anterior e que o próprio teste compensa exigindo
os dois contadores antes das asserções de estado.

O terceiro virou código, no sentido de tirar código: a formatação e o `allow` de
clippy em `crates/seele-proto/tests/vetores_de_hash.rs` saíram do diff, pelo
motivo registrado lá em cima — a linha principal já conserta esse arquivo em
`74056b8`, e melhor. O arquivo ficou igual ao de lá, e o diff desta tarefa contra
a linha principal voltou a tocar só `seele-server`, `seele-conformance` e este
documento.

**O que ficou de fora.** Os outros achados da auditoria 1881a3cb — a entrada em sala
confirmada «por silêncio», a senha de sala que o cliente nunca manda, o `adopt`
que não limpa assentos, o limite de sala que não barra — seguem abertos e não são
desta pendência.

**A revisão seguinte aprovou de novo, e o que sobrou dela era prova — desta vez
refeita, e em dobro.** Dos apontamentos, quatro são confirmação do que já está
escrito acima e dois pediam medida: que as reversões não tinham sido reexecutadas
por quem revisa (revisar não altera fonte) e que a saída de teste anexada pelo
coordenador continuava sendo a da bateria das ferramentas de publicação, que não
toca nenhuma crate desta pendência — quinta vez.

As duas foram respondidas medindo, e as reversões foram feitas em **dois pontos
diferentes de propósito**, para que a prova não dependa de um único guarda:

1. **O guarda da saída na tarefa da sala.** Trocada a pergunta «esta saída é da
   sessão que ainda é membro?» por `false`, `desassentar` reprova em 28,04 s com a
   mensagem já registrada — «a tarefa da sala não registrou nenhuma saída de
   sessão velha barrada» — e despeja `saida_de_sessao_velha: 0`. Esta reversão
   derruba a asserção **do contador**: prova que o guarda existe e dispara.
2. **O guarda da desocupação por sessão.** Devolvido `vacate_everywhere_da_sessao`
   ao filtro só por pessoa — que é literalmente o código de antes do conserto —,
   `desassentar` reprova em 28,03 s numa asserção **de estado**, e com o sintoma
   que o usuário relatou: «a sessão velha, ao morrer, apagou a nova do roster do
   anfitrião: ["anfitriao"]», com a sala do servidor mostrando só
   `anfitriao/sessão 1`. A mensagem ainda informa que o enlace do anfitrião piscou
   zero vez durante a medida, o que separa reprovação de servidor de reprovação de
   máquina.

Que as duas reversões derrubem asserções **diferentes** é o que fecha o buraco que
o próprio teste poderia ter: o contador impede que ele passe por inércia, e a
asserção de roster impede que ele passe por um contador que anda sem o estado
acompanhar. Os dois arquivos foram restaurados byte a byte depois de cada medida
(a árvore volta a zero arquivo modificado) e `desassentar` passa em 28,04 s.

**Validação desta rodada, com o número real.** `cargo test -p seele-server -p
seele-conformance -p seele-proto` terminou com código de saída 0: **740 testes
passando, nenhuma falha, um ignorado** — o de duas máquinas, que é de plataforma.
A repartição, para que ninguém precise repetir a soma: 422 na crate do servidor
(408 de unidade, 3 do binário e 11 nos testes de integração dela), 116 na de
conformidade, 202 na de protocolo (200 de unidade, 1 de vetores e 1 de
documentação). `desassentar.rs` entre eles.

**A bateria repetida, e o que a repetição mostrou que uma rodada só esconde.** A
rodada de validação seguinte repetiu `cargo test` das três crates várias vezes, e
aí apareceu o que uma execução única não mostra: de vez em quando um teste de
conformidade reprova por espera estourada — `anexos`, `acceptance_seguranca`,
`acceptance_m5`, `ejetar` —, sempre numa suíte que sob carga leva ~20 s, sempre
com a espera voltando vazia, e nunca o mesmo teste duas vezes. `desassentar`
nunca esteve entre eles.

A suspeita óbvia era esta pendência: o teste novo segura 28 s de relógio esperando
o tempo ocioso do transporte e, rodando junto com as outras suítes, poderia estar
apertando a máquina a ponto de estourar as esperas alheias. **Foi medido em vez de
suposto**, em dois passos. Primeiro, pulando `desassentar` da bateria: a
instabilidade continuou, 1 reprovação em 6 rodadas. Depois, no marco **anterior ao
conserto** — uma cópia de trabalho descartável em `a695fe5`, que não tem guarda
nenhum destes —, a mesma bateria repetida seis vezes reprovou uma vez, em
`a_senha_do_voice_room_e_conferida`, com a mesma assinatura de 20 s e espera vazia.

Mesma taxa, mesma assinatura, antes e depois: a instabilidade é **anterior a este
conserto** e é da bateria de conformidade sob paralelismo, não do guarda. Em
isolamento essas suítes passam — `anexos` passou 10/10 em seis rodadas seguidas, a
1,2 s cada, contra os 20 s que leva quando a máquina está cheia. Não foi tocada
aqui, por ser de outra família e fora do escopo desta pendência; fica registrada
para quem for encarar o tempo das esperas dos testes de conformidade. O que vale
dizer de uma vez: **a bateria das três crates passa integralmente quando a máquina
não está saturada** — 740 passando, nenhuma falha, um ignorado —, e nenhuma das
reprovações observadas foi de asserção de comportamento; todas foram de espera.

**O achado da revisão, e o que ele custou.** A revisão independente aprovou o
conserto e deixou uma aresta: o teste novo não tinha prazo nenhum nos passos que
falam com a rede — o aperto de mão do anfitrião, a entrada dele na sala, a volta
do visitante e a reentrada dela. Numa máquina saturada isso não reprova: **pendura**.
O binário chegou a passar minutos acima dos ~28 s esperados sem dizer uma palavra,
e um teste pendurado não diz nada a quem espera por ele. Cada um desses quatro
passos passou a correr com prazo próprio de 20 s, e o estouro reprova nomeando
**qual** passo não voltou. Vinte segundos é folga larga contra `127.0.0.1`; o
número não é o ponto, o ponto é existir. A subida da conexão emudecível já tinha
o seu, e é o mesmo.

**As duas reversões, refeitas nesta rodada e não só citadas.** A revisão observou,
com razão, que a prova de reversão que ela leu era evidência documental — quem
revisa não altera fonte. As duas foram reexecutadas aqui, sobre o código atual e
já com os prazos novos, com o mesmo resultado: desarmando o guarda da saída na
tarefa da sala, `desassentar` reprova em 28,04 s na asserção do contador
(`saida_de_sessao_velha: 0`); devolvendo `vacate_everywhere_da_sessao` ao filtro
só por pessoa, reprova em 28,05 s na asserção de roster, com o sintoma de campo —
«apagou a nova do roster do anfitrião: ["anfitriao"]» — e o enlace do anfitrião
piscando zero vez, o que exclui a máquina. Os dois arquivos foram restaurados
byte a byte depois de cada medida.

**E a bateria, desta vez das crates certas.** A validação anexada à revisão
anterior era a das ferramentas de publicação — 66 testes que não tocam nenhuma
crate desta pendência. A lacuna foi suprida: `cargo test -p seele-server -p
seele-proto` passa integralmente (408 de unidade do servidor e 200 do protocolo
entre os demais) e `cargo test -p seele-conformance` passou **limpo numa rodada
inteira**: 116 passando, nenhuma falha, um ignorado, `desassentar` entre eles em
28,06 s. As mesmas 740 de antes. `cargo fmt` e `clippy -D warnings` limpos na
crate tocada.

**O último guarda que não conferia sessão.** A revisão seguinte aprovou o conserto
e deixou uma observação que era justa: `descartar_a_reserva_de_quem_ja_voltou`
apagava a reserva de carência da pessoa **sem comparar sessão nenhuma**, apoiado
num argumento de ordem — «o aperto de mão desta conexão já resgatou o que havia,
logo o que existir agora foi escrito por outra». O argumento se sustenta no fluxo
de hoje, mas era o único desta família que dependia de uma invariante que nenhum
tipo confere, e uma ordem garantida só pelo texto é uma ordem que a próxima
mudança quebra em silêncio. A reserva passou a guardar a sessão que a escreveu
(`ReservedSlot::sessao`), e o descarte pergunta **quem escreveu** em vez de
**quando isto aconteceu**: só some a reserva de quem já não é a conexão vigente.
No fluxo atual o comportamento observável é o mesmo — nenhuma reserva da conexão
vigente pode existir naquele instante —, e é exatamente por isso que o guarda
precisava de teste próprio, senão ninguém saberia quando ele deixasse de valer.
`a_reserva_da_propria_sessao_vigente_nao_e_descartada` cobre o lado novo, e a
reversão foi provada: tornando o descarte incondicional de novo, ele reprova com
«o descarte comeu a reserva da própria conexão vigente, que é o assento que ela
espera resgatar quando cair». A bateria de unidade do servidor passou de 408 para
409 e continua inteira: `cargo test -p seele-server -p seele-proto` passa por
completo (409 e 200 de unidade entre os demais) e `cargo test -p seele-conformance`
fechou uma rodada limpa em 116 passando, nenhuma falha, um ignorado, com
`desassentar` entre eles. `fmt` e `clippy -D warnings` limpos nas três crates.
Duas rodadas anteriores da conformidade reprovaram um teste cada — `recusa` numa,
`convite` na outra —, sempre estourando 20 s e sempre passando em 0,03 s e 0,23 s
quando rodados sozinhos: é a instabilidade sob carga paralela já medida nesta
pendência, em testes que não tocam reserva, sessão nem sala.

**O único ponto desta família que ainda se apoia em ordem, e por que fica assim.**
A revisão seguinte aprovou o conserto e deixou uma observação de cobertura:
`Presentes::chegou` sobrescreve o registro da pessoa sem comparar sessão nenhuma —
a última gravação vence. No fluxo de hoje isso é inalcançável, porque o aperto de
mão da conexão velha terminou antes de a nova sequer existir: nunca há uma chegada
fora de ordem para sobrescrever a vigente. Trocar a escrita por uma comparação de
ordem aqui seria inventar uma regra de precedência entre sessões que o resto do
código não tem — e um guarda sem teste que o prove é exatamente o que esta
pendência passou o dia desmontando. Ficou o registro no próprio tipo: quem mexer
no aperto de mão — reaproveitando, repetindo ou adiando — lê no lugar certo que
esta linha volta a ser o buraco que o resto fechou.

**A bateria desta rodada, medida com a máquina cheia.** `cargo test` do projeto
inteiro fechou sem uma reprovação sequer, e `cargo test -p seele-server -p
seele-proto -p seele-conformance` somou **741 passando, nenhuma falha, um
ignorado** — o ignorado é o de plataforma —, com `desassentar` entre eles em
28,05 s. As 741 são as 740 de antes mais o teste novo da reserva. Vale o registro
de que a medida saiu com a máquina a *load average* 75, várias baterias de outras
worktrees rodando junto: é exatamente a condição sob a qual esta pendência mediu
instabilidade antes, e desta vez ela não apareceu. `cargo fmt --check` e
`clippy --all-targets -D warnings` limpos na crate tocada.

**A reprovação que veio de fora, medida em dez rodadas.** A validação seguinte
anexou uma reprovação de `cargo test` do projeto inteiro:
`acceptance_m2::two_servers_on_one_machine_do_not_share_a_pin`, `SemResposta` aos
20,22 s. Nenhuma asserção de comportamento — é o prazo por candidato de
`Enlace::conectar` (`enlace.rs`) queimando inteiro sem resposta. Sozinho, esse
teste passou 5 de 5 vezes em 0,95 s cada.

Como a suspeita natural era o teste novo desta pendência, que segura 28 s de
relógio, **foi medido em vez de suposto**, alternando rodadas para que a carga da
máquina não decidisse sozinha: cinco rodadas do workspace inteiro com o teste novo
e cinco com ele pulado (`-- --skip quem_reconecta_antes_de_o_servidor_desistir`),
intercaladas. Com ele: duas reprovações, em `acceptance_m2` e depois em
`acceptance_m3::a_message_reaches_everybody_on_the_line`, 20,22 s e 20,14 s, nunca
o mesmo teste, nunca uma asserção. Sem ele: cinco rodadas limpas. Dois em cinco
contra zero em cinco não separa nada com essa amostra — e a medida do marco
**anterior ao conserto**, registrada acima, já tinha reprovado uma vez em seis
com a mesma assinatura, sem que existisse teste novo nenhum para culpar. A
máquina esteve o tempo todo entre *load average* 60 e 93, com baterias de outras
worktrees rodando junto.

O que se conclui, e o que não: a instabilidade é a mesma já medida aqui — prazos
de rede dos testes de conformidade sob saturação —, é anterior a este conserto e
não é dele. Alargar o prazo por candidato do cliente, ou os prazos das suítes de
aceitação, é mudança de produto em código que esta pendência não toca; fica
registrada para quem encarar o tempo das esperas da conformidade. As crates
tocadas passam integralmente: `cargo test -p seele-server -p seele-conformance`
fechou verde (409 de unidade do servidor, 116 de conformidade, `desassentar` entre
eles em 28,07 s). `cargo fmt --all --check` e `clippy --all-targets -D warnings`
nas duas crates, limpos.

**A revisão que aprovou, e a medida que ela pediu de novo.** A revisão
independente seguinte aprovou o conserto e voltou a apontar a mesma lacuna de
evidência: a saída de `cargo test` anexada pelo coordenador era outra vez a da
bateria das ferramentas de publicação — 66 testes que não tocam crate nenhuma
desta pendência. A lacuna é de anexo, não de código, e foi suprida medindo aqui:
`cargo test -p seele-server -p seele-proto` fechou verde com **409 de unidade do
servidor, 200 de `seele-proto`** e os binários de teste do servidor junto, nenhuma
falha; `cargo test -p seele-conformance` fechou a bateria inteira com **116
passando, nenhuma falha e um ignorado** (o de plataforma), com `desassentar`
entre eles em 28,06 s. `cargo fmt --check` e `clippy --all-targets -D warnings`
nas três crates, limpos. As outras duas observações da revisão já estavam
respondidas acima: a prova de reversão, nas duas metades com as mensagens
esperadas, e `Presentes::chegou`, que segue sem guarda por ser inalcançável no
aperto de mão de hoje e está registrado no próprio tipo.

**A invariante de que todos os guardas dependem, agora presa por teste.** A
revisão seguinte aprovou de novo e deixou a observação mais útil da série: o
registro em `Presentes::chegou` decide **quem é a conexão vigente** de uma pessoa,
e nenhum teste prendia isso. Trocar a escrita por uma comparação de precedência
entre sessões continua fora de escopo — seria inventar uma ordem que o resto do
código não tem —, mas a lacuna real não era a comparação: era que, se a chegada
deixasse de sobrescrever, todos os guardas desta pendência continuariam
«passando» enquanto apontam para a conexão errada, e a conexão velha voltaria a
ser a vigente sem uma única reprovação. Os testes de presença que existiam só
olhavam `saiu`. `quem_reconecta_passa_a_ser_a_vigente_e_a_anterior_perde_a_autoridade`
prende a invariante pelos dois lados: depois da reconexão, a sessão nova é a
vigente e a velha deixou de ser. A reversão foi provada aqui: fazendo `chegou`
não sobrescrever (só inserir quando ainda não havia registro), ele reprova com «a
conexão que acabou de reconectar não é a vigente desta pessoa, e tudo o que ela
fizer daqui para a frente será recusado como se fosse de uma sessão morta».

**As duas reversões do teste de conformidade, reexecutadas nesta rodada.** A
revisão registrou, com razão, que não podia executá-las — revisor não altera
fonte —, e que a evidência delas era documental. Foram refeitas aqui, cada uma
isolada e com a árvore restaurada em seguida. Primeira: devolvendo a desocupação
ao filtro só por pessoa (nos dois pontos de `Occupancy`), `desassentar` reprova
com «a sessão velha, ao morrer, apagou a nova do roster do anfitrião:
["anfitriao"]», e o diagnóstico ainda informa que o enlace do anfitrião não
piscou nenhuma vez — ou seja, a reprovação é do servidor e não da máquina.
Segunda: desarmando o guarda de sessão na saída da sala de voz, ele reprova antes
disso, com «a tarefa da sala não registrou nenhuma saída de sessão velha
barrada», e o despejo dos contadores junto. Duas reprovações em pontos diferentes
e com mensagens diferentes: o teste não passa por inércia.

**A bateria desta rodada.** `cargo test -p seele-server -p seele-proto` verde com
**410 de unidade do servidor** — as 409 mais o teste novo da vigência — e **200
de `seele-proto`**, mais os binários de teste do servidor, nenhuma falha.
`cargo test -p seele-conformance` fechou a bateria inteira com **116 passando,
nenhuma falha e um ignorado** (o de plataforma), com `desassentar` em 28,04 s.
`cargo fmt --all --check` limpo e `clippy --all-targets -D warnings` limpo nas
três crates. A validação anexada pelo coordenador voltou a ser a das ferramentas
de publicação (66 testes, todos passando, nenhuma crate desta pendência): não há
reprovação a consertar ali, há a lacuna de anexo que esta medida supre.

**A revisão seguinte aprovou sem nenhum achado de código, e os dois
apontamentos eram de prova e de escopo.** Nenhum dos dois pedia mudar
comportamento, e nenhum dos dois foi deixado só escrito.

O primeiro é o caminho de **entrada**, e continua de fora de propósito: no
`assentar`, `encerrar_telas_da_pessoa` e a varredura por pessoa são chamadas sem
sessão porque andar de sala tem de levar toda transmissão anterior embora. Numa
janela de sobreposição, a conexão velha que ainda respira e reage ao movimento da
mesma pessoa re-senta a si própria e derruba a tela da nova. Isso não é a saída
que esta pendência fechou: é a entrada, já registrada acima e atada à #45, e
mexer nela aqui seria ampliar o escopo depois do aceite atendido. Fica nomeado no
lugar certo em vez de descoberto de novo daqui a três revisões.

O segundo era, pela sexta vez, **a prova não reexecutada por quem revisa** —
revisar não altera fonte, e desarmar um guarda exige alterar uma. Foram refeitas
aqui, nos mesmos dois pontos distintos, e reprovaram nos dois lugares esperados:

1. Trocada por `false` a pergunta «esta saída é da sessão que ainda é membro?» na
   tarefa da sala, `desassentar` reprova em 28,04 s na asserção **do contador** —
   «a tarefa da sala não registrou nenhuma saída de sessão velha barrada» — com
   `saida_de_sessao_velha: 0` no despejo.
2. Devolvida a desocupação ao filtro só por pessoa — o código literal de antes do
   conserto —, ele reprova em 28,04 s numa asserção **de estado**, com o sintoma
   que o usuário relatou: «a sessão velha, ao morrer, apagou a nova do roster do
   anfitrião: ["anfitriao"]», a sala do servidor mostrando só `anfitriao/sessão 1`
   e o enlace do anfitrião piscando zero vez — o que separa reprovação de servidor
   de reprovação de máquina.

Os dois arquivos voltaram byte a byte depois de cada medida (a árvore volta ao
mesmo diff de antes) e `desassentar` passa de novo em 28,05 s.

**A bateria desta rodada, com o número inteiro.**
`cargo test -p seele-server -p seele-proto -p seele-conformance` fechou com
**742 passando, nenhuma falha e um ignorado** — o ignorado é o que precisa de duas
máquinas —, sendo 410 de unidade do servidor, 200 de `seele-proto`, um de
documentação e o resto repartido entre os binários de integração das duas crates,
com `desassentar` entre eles em 28,11 s. `cargo fmt --check` limpo e
`clippy --all-targets -D warnings` limpo nas três crates. A saída anexada pelo
coordenador voltou a ser a das ferramentas de publicação (66 testes, todos
passando, nenhuma crate desta pendência): não há reprovação a consertar ali — há a
lacuna de anexo, e é ela que esta medida supre.

**A rodada seguinte: o espaço inteiro medido de uma vez, e as reversões de novo.**
A revisão aprovou outra vez sem achado de código, e a validação anexada pedia
conserto de uma reprovação — que não existe. Em vez de medir só as três crates
tocadas, esta rodada mediu **o espaço de trabalho inteiro numa execução só**, com
`--no-fail-fast` para que nada parasse na primeira falha: **1706 passando,
nenhuma falha, quatro ignorados**, 69 baterias, e o processo saiu com zero. Não
há reprovação a consertar: a saída anexada continua sendo a das ferramentas de
publicação, cujos 66 testes passam e não tocam crate nenhuma desta pendência.
Vale o registro para a instabilidade sob carga já medida acima: desta vez a
bateria inteira, rodando em paralelo com todas as outras crates, fechou limpa —
o que confirma que aquelas reprovações eram de espera sob saturação e não de
comportamento. `cargo fmt --all --check` e `clippy --workspace --all-targets
-D warnings` limpos no espaço inteiro.

E, pela sétima vez, as duas reversões foram **reexecutadas** em vez de citadas,
porque quem revisa não altera fonte e por isso a prova sempre chega até ela como
papel. Desarmado o guarda de sessão na saída da sala de voz, `desassentar`
reprova em 28,04 s na asserção do contador, com `saida_de_sessao_velha: 0` no
despejo. Devolvida `vacate_everywhere_da_sessao` ao filtro só por pessoa, reprova
em 28,03 s na asserção de estado, com o sintoma de campo — «a sessão velha, ao
morrer, apagou a nova do roster do anfitrião: ["anfitriao"]» — e o enlace do
anfitrião piscando zero vez, o que exclui a máquina. Os dois arquivos voltaram
byte a byte depois de cada medida e `desassentar` passa de novo em 28,04 s.

**A rodada seguinte: a primeira reprovação de carga que dá para nomear.** A
revisão aprovou de novo sem achado de código — as três notas eram as de sempre: o
caminho de **entrada** continua chaveado só por pessoa, de propósito e anotado
como pendência à parte; as reversões estão escritas aqui e quem revisa não pode
reexecutá-las; e a validação anexada era, pela quarta vez, a das ferramentas de
publicação.

Desta vez a bateria das três crates tocadas **reprovou de verdade**, e vale
registrar como a atribuição foi feita em vez de decretada.
`acceptance_seguranca::um_server_com_portaria_nao_admite_ninguem_por_um_caminho_lateral`
parou em «a chave aprovada não entrou: `Some(SemResposta)`» — o transporte
desistiu de esperar, não a portaria recusou —, num arquivo que esta pendência não
toca e num caminho que o diff não encosta: nenhuma linha de admissão foi alterada.
Rodado sozinho, o mesmo binário fecha 8 de 8 **três vezes seguidas** em 2,74 s;
dentro da bateria, os mesmos 8 testes levaram 22,50 s, oito vezes mais. Olhando a
máquina no meio da medida, a causa aparece com nome: **outro worktree deste mesmo
repositório estava com a bateria dele rodando em paralelo**, com processos de
servidor de teste vivos havia mais de dezessete horas. É a pendência #29 outra
vez, e agora com o culpado identificado em vez de suposto.

A bateria inteira, repetida com a máquina menos disputada, fechou limpa:
**742 passando, nenhuma falha, um ignorado** — o de plataforma —, 38 binários, com
`desassentar` em 28,10 s e o `acceptance_seguranca` em 8 de 8 em 2,75 s.
`cargo fmt --all --check` limpo.

E a prova de reversão foi refeita **nesta** rodada, pela oitava vez, para que ela
não chegue à revisão só como papel: devolvendo `vacate_everywhere_da_sessao` ao
filtro só por pessoa, `desassentar` reprova em 28,04 s com exatamente o sintoma de
campo — «a sessão velha, ao morrer, apagou a nova do roster do anfitrião:
["anfitriao"]», com a sala do servidor mostrando `["anfitriao/sessão 1"]` — e com
o enlace do anfitrião piscando **zero** vez durante a medida, o que exclui a
máquina como explicação. Restaurado o arquivo byte a byte — a árvore volta a não
ter diferença nenhuma —, o teste passa em 28,05 s.

**A medida refeita nas crates certas, com a carga da máquina nomeada.** A
validação voltou mais uma vez com a saída das ferramentas de publicação — 66
testes que não encostam em nada disto —, então a bateria foi refeita onde o
trabalho está. `seele-server` e `seele-proto` juntas: **626 passando, nenhuma
falha, nenhum ignorado**. `seele-conformance` inteira: **116 passando, nenhuma
falha, um ignorado** — o de duas máquinas, que é de plataforma e já era ignorado
antes desta pendência —, com `desassentar` verde em 28,02 s. `cargo fmt --check`
e `cargo clippy --all-targets -- -D warnings` limpos nas três crates. Somando,
os mesmos **742** de sempre.

A carga desta rodada também dá para nomear, e desta vez não era outro worktree:
eram **42 processos `yes` órfãos**, sem pai, restos de um gerador de carga de uma
medida anterior desta própria pendência, que ficaram girando por trinta e oito
minutos e seguravam a média de carga em 58 para 15 núcleos. Não foram mortos —
podiam ser de outra sessão medindo —, terminaram sozinhos no meio da bateria, e a
média caiu para 22 ao fim. A bateria fechou limpa nas duas metades, o que é o
dado útil: desta vez a saturação não produziu reprovação nenhuma. Fica a lição
operacional para a #29: antes de atribuir uma reprovação ao código, `ps` na
máquina — o culpado costuma estar lá, com nome e hora de início.

**A reaplicação sobre a linha principal, em 2026-09-14, e o conserto que ela
absorveu.** Este trabalho nasceu sobre uma base anterior e, no caminho até a
integração, a linha principal recebeu um conserto próprio do **mesmo defeito**:
a saída de uma conexão deixou de apagar a ficha de outra da mesma pessoa,
conferindo o número de voz (`ssrc`) em `Presentes::saiu`, em
`Occupancy::vacate_everywhere` e no `Leave` da tarefa da sala. Ele veio de outro
relato de campo — o cliente tenta vários caminhos ao mesmo tempo (ADR 0037),
fica com o primeiro que abre, e os abandonados fecham uns 80 ms depois rodando a
saída inteira.

**Os dois relatos são o mesmo defeito visto de dois relógios**: uma conexão que
não é mais a vigente removendo estado de quem é. Um deles mede 80 ms, o outro
mede vinte segundos. Por isso a conferência de sessão **substituiu** a de
`ssrc`, em vez de conviver com ela: duas perguntas para a mesma coisa seriam a
segunda cópia de contabilidade que produziu esta família inteira, e o
identificador de sessão é o único que todo o desmonte já carrega — inclusive
onde não há `ssrc` à mão, como nas telas e na reserva de assento. O caso do
caminho abandonado continua coberto, e está escrito no doc de `Presentes::saiu`
para que ninguém conclua que ele se perdeu na integração.

**O que a integração ainda teve de costurar.** A linha principal transformou o
fim de tela em «encerra a tela **e solta os pares** que a pessoa sustentava»,
para que a malha do §5.1 não degradasse para a estrela sem rastro. As duas
funções de fim de tela desta pendência passaram a fazer isso também. As
nomeações de par são chaveadas por transmissão e por pessoa, sem sessão, então
as duas direções que são **por pessoa** passaram a correr apenas quando esta
conexão é a vigente da pessoa, ou quando a pessoa não tem mais conexão nenhuma —
a estratégia de escrita já descrita acima. Dar sessão ao registro de pares é
tarefa própria e não desta; sem o guarda, porém, a conexão que morre aos vinte
segundos derrubaria para a estrela a malha que a conexão nova acabou de montar.

**Medido depois da reaplicação, nesta base.** `seele-server` 471 testes,
`seele-proto` 228, e a bateria de conformidade inteira em execução serializada
(31 conjuntos, nenhuma reprovação), com
`quem_reconecta_antes_de_o_servidor_desistir_da_conexao_velha_continua_no_roster_do_host`
verde em 28,07 s. `cargo fmt --all --check` e
`cargo clippy --all-targets --all-features -D warnings` limpos nas crates
tocadas.

**E as duas reversões, refeitas nesta base, não herdadas da anterior.**
Revertendo o guarda da tarefa da sala, o teste reprova na asserção do contador
da sala — «a tarefa da sala não registrou nenhuma saída de sessão velha
barrada», com `saida_de_sessao_velha: 0` impresso junto. Revertendo o guarda de
`Presentes::saiu`, ele reprova antes disso, na asserção (0): a sessão velha
apagou a pessoa dos presentes, de modo que o desmonte não encontrou mais ninguém
a quem defender e o contador do processo não andou. As duas reversões foram
desfeitas e a bateria voltou ao verde com a árvore limpa.

**Remedido em 2026-09-14, na árvore entregue.** A validação anexada à entrega
anterior era a bateria das ferramentas de publicação — 66 testes de empacotamento
e release — e não continha um único teste das crates desta pendência; não servia
de prova. Refeita sobre a árvore atual: `cargo test -p seele-server` termina com
código 0 e 487 asserções somadas nos nove conjuntos, nenhuma reprovação;
`cargo test -p seele-conformance -- --test-threads=1` termina com código 0 e 150
asserções somadas, nenhuma reprovação, com
`quem_reconecta_antes_de_o_servidor_desistir_da_conexao_velha_continua_no_roster_do_host`
verde em 28,06 s — o tempo varia alguns décimos entre execuções, porque o teste
espera o prazo de desistência da conexão velha. `cargo fmt --all --check` limpo e
`cargo clippy -p seele-server -p seele-conformance --all-targets --all-features
-- -D warnings` sem aviso. A contagem de 471 registrada acima veio de outra forma
de somar os conjuntos; o que vale como prova é a saída com código 0 e zero
reprovações, reproduzida agora.

**Sobre interrupções da máquina, não do código.** Duas execuções desta mesma
bateria foram cortadas no meio por SIGTERM enviado de fora ao binário de teste
(uma em `voz_sob_carga`, outra em `acceptance_seguranca`), com o cargo saindo em
101 sem nenhuma reprovação registrada. Rodados sozinhos, os mesmos testes passam,
e a bateria inteira passou em seguida com código 0. Um corte por sinal externo
não é reprovação: quem ler o código 101 precisa conferir se há `signal: 15` na
saída antes de concluir defeito.

**As duas provas de reversão, reexecutadas agora e não só relembradas.** A
revisão anterior observou, com razão, que quem revisa não altera fontes e por
isso não podia reexecutá-las. Foram refeitas nesta passagem, uma de cada vez,
com a árvore restaurada em seguida e conferida limpa:

| metade desarmada | onde o teste reprova | mensagem |
|---|---|---|
| o filtro por sessão ao desocupar o assento | asserção do roster | «a sessão velha, ao morrer, apagou a nova do roster do anfitrião: ["anfitriao"]» |
| a pergunta de sessão na saída pedida à sala | asserção do contador | «a tarefa da sala não registrou nenhuma saída de sessão velha barrada», com `saida_de_sessao_velha: 0` |

Nas duas, o teste levou os mesmos 28,03 s antes de reprovar — ele espera o prazo
de desistência da conexão velha, e reprovar depressa seria sinal de que mediu
outra coisa. Com o guarda de volta, o mesmo teste passa em 28,03 s. Cada metade
sozinha derruba o teste, o que é a prova de que nenhuma das duas é enfeite.

**A reprovação intermitente de `tela_por_um_par` não é desta entrega, e isso foi
medido.** Numa execução em série da bateria de conformidade,
`a_reconexao_ao_servidor_nao_deixa_a_conexao_velha_atrapalhar_o_par_novo`
reprovou em «o servidor apontou um par e não o contou como ocupado». Sozinho, o
teste passa em 3,66 s; o binário inteiro em série passa em 51,6 s. Em vez de
deduzir do diff, a atribuição foi medida como a pendência #41 manda — **rodadas
intercaladas**, uma desta árvore e uma de uma exportação limpa da `main`, para
que as duas pegassem a mesma carga de máquina:

| árvore | reprovações |
|---|---|
| esta entrega | 0 em 10 |
| `main` limpa, sem este conserto | 1 em 10 |

A base reproduz sozinha o que se viu aqui, e esta árvore não reproduziu nenhuma
vez: a intermitência é a já registrada em #41, da frente da malha, e não uma
regressão deste guarda. O diff desta entrega não toca `Pares` fora do fim de
tela, que só corre quando uma conexão acaba — e no trecho que reprova nenhuma
acaba. A execução final, em série e com a árvore restaurada, passou inteira:
487 asserções em `seele-server` e 150 em `seele-conformance`, uma ignorada por
plataforma, zero reprovações.

**O que este guarda de propósito não cobre, e por quê.** Mover alguém de sala por
moderação (`PersonMoved`) chama `assentar`, que tira a pessoa de toda sala
anterior **sem** perguntar de qual sessão é — e tem de ser assim, ou quem anda de
uma sala para outra continuaria ouvindo a sala de onde saiu. A consequência é
que, na mesma janela de queda silenciosa, a conexão velha ainda viva também
recebe o evento e se re-assenta com o ssrc antigo. É comportamento anterior a
esta entrega, não regressão dela, e consertá-lo pede decidir o que «mover» quer
dizer quando a pessoa tem duas conexões — decisão que não cabia neste escopo.
Fica aqui registrado para não voltar como surpresa.

**Provas de reversão reexecutadas ao vivo em 2026-09-15.** As duas metades foram
desarmadas de novo, uma de cada vez, nesta mesma árvore, e reprovaram com as
mensagens da tabela acima, cada uma depois dos mesmos ~28 s de espera pelo prazo
de desistência; a árvore foi restaurada e conferida limpa entre uma e outra. A
revisão independente não podia fazê-lo por não alterar fontes; agora a prova não
é só documental.

**Conferência final das crates tocadas, 2026-09-15.** Uma validação anterior
anexada a esta pendência era a bateria das ferramentas de publicação, que não
compila nenhuma das crates alteradas e por isso não provava nada aqui. A
execução que vale foi refeita nesta árvore: `seele-server` passou com 487
asserções e `seele-conformance`, em série, com 150 — uma ignorada por
plataforma, nenhuma reprovação nas duas. Dentro dela, o teste novo
`quem_reconecta_antes_de_o_servidor_desistir_da_conexao_velha_continua_no_roster_do_host`
passou em 28,04 s, o tempo de esperar o prazo de desistência inteiro antes de
afirmar.

**A validação do coordenador que estourou o tempo, e o que ela era.** Uma
execução automática de `cargo test` sobre esta árvore foi interrompida ao bater
o limite de 900 s, e isso chegou aqui como «validação falhou». Não era falha do
código: a mesma máquina estava compilando e rodando a bateria de outra árvore de
trabalho ao mesmo tempo — o processo concorrente foi observado —, e a bateria
inteira do repositório não cabe nesse limite nessas condições.

Refeita por crate nesta árvore, com a árvore limpa no commit desta tarefa:
`seele-server` fechou com **487 asserções, nenhuma reprovação**, em 30 s.
`seele-conformance`, em série, reprovou **uma** vez, e a mensagem diz o que era:
`a_reconexao_ao_servidor_nao_deixa_a_conexao_velha_atrapalhar_o_par_novo` não
conseguiu abrir o soquete de escuta — «Address already in use» —, que é a
disputa por portas efêmeras da máquina inteira já registrada na pendência #29, e
não uma asserção de comportamento. Reexecutado sozinho, o mesmo teste passa; a
suíte inteira reexecutada em série fechou com **150 passando, nenhuma
reprovação e uma ignorada** (a que exige duas máquinas), com o teste desta
pendência verde em 28,02 s. `cargo fmt --check` das duas crates tocadas está
limpo.

**Repetida uma última vez, com a máquina livre.** Refeitas as duas baterias
depois que a outra sessão soltou a máquina, sem nenhuma reprovação e sem a
disputa de portas: `seele-server` fechou com **487 asserções, nenhuma
reprovação**, e `seele-conformance` em série com **150 passando, nenhuma
reprovação e uma ignorada** (a que exige duas máquinas), com o teste desta
pendência verde em 28,06 s. `cargo fmt --check` das duas crates limpo.

**O limite honesto desta conferência.** Ela vale para as crates tocadas, e não
para a bateria do repositório inteiro num único comando: essa nunca chegou ao
fim dentro do limite do coordenador enquanto outra sessão ocupava a máquina, e
não foi tentada de novo com a máquina livre.

**Revalidada sobre a base nova, depois da subida do protocolo para a v5.** Esta
ponta foi rebaseada sobre a `main` que já traz a v5; o único choque foi de
documentação — este arquivo —, e era de numeração: a `main` passou a ocupar #42,
#43 e #44, e o registro da expulsão que nasceu aqui como #42 virou **#45**, com
as referências internas desta pendência corrigidas junto. Nenhum arquivo de
código conflitou. Refeitas as baterias sobre essa base: `seele-server` fechou com
**488 asserções, nenhuma reprovação**; `seele-conformance`, em série, fechou com
**saída 0, nenhuma reprovação e uma única ignorada** (a que exige duas
máquinas), com o teste desta pendência verde em 28,04 s. `cargo fmt --all
--check` limpo e `cargo clippy` das duas crates tocadas, com `-D warnings`,
limpo.

**A sétima porta, que era o único apontamento vivo: o anúncio de mudança de sala.**
As revisões anteriores vinham repetindo, e este arquivo vinha registrando, que o
caminho de **entrada** ficava de fora de propósito e que mexer nele seria ampliar
o escopo. **A parte "de propósito" continua certa; a conclusão de deixá-lo fora,
não.** `assentar` chama a saída por pessoa sem conferir sessão porque andar de uma
sala para outra tem de tirar todo membro anterior — isso é correto e não mudou.
O que faltava conferir não é o que `assentar` faz, é **quem manda `assentar`
rodar**: `Event::PersonMoved` é difundido, e toda conexão da pessoa movida o
recebe, inclusive a velha da queda silenciosa, que está muda no fio mas com a
tarefa dela viva lendo o barramento. Respondendo ao anúncio, ela desmontava a
mídia e a tela da conexão nova e re-sentava a pessoa com o canal e o `ssrc` dela,
já mortos — o defeito desta pendência inteiro, entrando pela porta dos fundos, com
um moderador movendo alguém como gatilho.

O guarda mora num nome só, `server::a_mudanca_de_sala_e_desta_conexao`, pela mesma
razão dos outros: guarda dentro de laço de sessão é guarda que teste nenhum
alcança. E mora **nessa porta**, e não dentro de `assentar`, porque o resgate do
assento da carência chama `assentar` antes de a conexão se declarar presente —
lá dentro, a pergunta responderia «não é a vigente» para a única conexão que
existe. A conexão velha que recebe o anúncio agora não mexe em nada e soma no
mesmo contador `Desassentamentos` das outras, que é o que faz um guarda que dá
certo deixar rastro.

**Prova de reversão, refeita e não citada.** Trocado o corpo do guarda por `true`
— o comportamento de antes —, o teste de unidade
`a_mudanca_de_sala_anunciada_nao_e_respondida_pela_conexao_velha` reprova com «a
conexão velha respondeu à mudança de sala e vai desmontar a sala da conexão nova
para sentar com o canal dela, que já morreu». Restaurado o arquivo, passa.

**A bateria desta rodada, medida nas crates certas.** A validação anexada voltou,
pela quinta vez, a ser a das ferramentas de publicação (66 testes, nenhuma crate
desta pendência): não há reprovação a consertar ali, há a lacuna de anexo que esta
medida supre. `seele-server`: **489 passando, nenhuma falha** — 488 de antes mais
o teste novo do guarda. `seele-proto`: **232 passando, nenhuma falha**.
`seele-conformance` inteira, serializada em 29 binários: **154 passando, nenhuma
falha, uma ignorada** (a que exige duas máquinas, ignorada desde antes desta
pendência), com `desassentar` verde em 28,03 s. `cargo fmt --all --check` limpo e
`cargo clippy --all-targets -- -D warnings` limpo nas três crates.

**E a medida que faltava: o `cargo test` do repositório inteiro, sem filtro.**
Era esse o comando da validação anexada, e a lacuna era de anexo, não de
reprovação. Rodado aqui sobre esta árvore, com a sétima porta dentro, ele fecha
com saída 0: **74 binários de teste, 1904 asserções passando, nenhuma falha,
quatro ignoradas** — as que exigem duas máquinas. A bateria de conformidade foi
junto, em paralelo e não serializada, e o teste de repasse de tela que a revisão
viu reprovar uma vez sob disputa de máquina passou; é instabilidade de máquina
disputada, a mesma já registrada em #29, e não regressão desta pendência.

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

### Continua viva em 2026-09-14, e custou uma rodada de revisão

Na subida do protocolo para a v5 ela apareceu duas vezes, em testes diferentes e
sem relação com a mudança — que, fora comentários, só toca testes de versão.
Primeiro `sair_encerra_sem_esperar_a_bateria` (`bateria_interna`), depois
`o_comandante_renomeia_e_todo_mundo_ve_o_nome_novo` (`salas`), este último
reprovando a **20,07 s** dentro do `cargo test` inteiro e passando em **3,09 s**
quando o arquivo roda sozinho — o padrão de carga descrito acima, no prazo
descrito acima. Quatro rodadas completas seguidas depois disso ficaram verdes,
e a falha não voltou.

O custo desta vez não foi o minuto de reexecução: foi uma revisão independente
ter de gastar parágrafo distinguindo «carga» de «regressão» antes de poder
aprovar. É o «deixa de ser evidência» do registro de 2026-08-31, agora cobrado
de terceiros.

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

## 34 · A casca não sabe dizer de onde a tela está vindo agora

**Aberta em 2026-09-10**, junto do consentimento de dois lados do caminho entre
pares (§5.1 do desenho de 05/09).

**O que não existe.** Nada no canal de avisos do `Enlace` diz «esta tela está
vindo por um par» ou «voltou a vir do servidor». A casca sabe o que a pessoa
**escolheu** — foi ela que chamou `consentir_no_caminho_entre_pares` — e não
sabe o que está **acontecendo**.

**Onde isso aparece primeiro.** Quando quem assiste retira o próprio
consentimento, o cliente cancela a tarefa que lia do par, e **uma tarefa
cancelada não emite fim de fluxo**: não sai `TelaFechou`, não sai erro, não sai
nada. Medido, e não suposto — a primeira versão de
`retirar_o_consentimento_de_assistir_por_par_devolve_a_tela_ao_servidor`
esperou por `TelaFechou` e esgotou a paciência inteira.

**Por que não é urgente.** Não custa imagem: o servidor reabre o cano ao
receber a declaração nova, e o teste prova trinta quadros seguidos chegando
byte a byte depois da retirada. O que falta é a frase, não a tela.

**O que ela precisaria ser.** Um aviso por transmissão com a origem de agora —
servidor ou par —, emitido quando ela muda. É a mesma informação que o §6 do
desenho quer medir (`Local`, `Furo`, `Falhou`), e a tela que a mostra é do
subprojeto B.

## 35 · O número de espectadores não é reanunciado quando alguém volta ao servidor

**Aberta em 2026-09-10**, medida enquanto se escrevia a prova da retirada de
consentimento.

**O que acontece.** `Event::ScreenViewers` — o **N** que o §5.1 divide para
achar o teto por cópia — sai de `VoiceRoom::anunciar_espectadores`, e ela é
chamada quando uma transmissão abre e quando ela para. **Não** é chamada quando
alguém passa a assistir (`VoiceRoomCommand::TelaAssistir`): a pessoa entra na
fila do próximo quadro-chave e o N anunciado continua o de antes.

**Como apareceu.** A primeira versão dos dois testes de retirada afirmava que o
contador de cópias voltava a 2 depois de o espectador ser devolvido ao
servidor. Ele fica em 1, por horas se for o caso. Medido com sonda: a nomeação
some de `Pares`, o `TelaAssistir` é mandado e aceito, os quadros voltam a
chegar — e o número anunciado não se mexe.

**Por que dói.** O teto de admissão de cada cliente é calculado sobre esse N.
Um N menor que o real devolve um teto **maior** que o real, que é o sentido
errado do erro: a sala aceita mais gente do que a subida carrega.

**É anterior a esta onda.** O mesmo vale para a recuperação por `ParFalhou`,
que está no ar desde o fecho de 09/09. Não foi consertada aqui porque a
instrução era não ampliar o escopo, e porque o conserto é da sala de voz e não
do caminho entre pares.

**Quando dói.** Em sala com malha ligada e espectadores indo e voltando entre
par e servidor. Com a malha desligada o caminho nem corre.

## 36 · `voz_na_reconexao.rs` não é cobertura de reconexão de roster

**O nome convida ao engano.** `crates/seele-conformance/tests/voz_na_reconexao.rs`
não conecta cliente nenhum a servidor nenhum: `braco_da_reconexao` (linhas
55-64) lê `crates/seele-ffi/src/lib.rs` como texto, recorta o trecho entre
`Aviso::Reconectado` e o próximo `Aviso::`, e confere que ele contém `reopen` e
não contém `Voice::start`/`switch_capture`/`switch_playback`. É uma escolha
declarada e justificada no próprio cabeçalho do arquivo (linhas 3-12) — o
defeito que ele guarda é "a casca chamou a função errada" para reabrir os
**controles de áudio** (mudo, Isolamento total, modo, ganhos), e isso não
aparece em tipo nenhum nem quebra asserção de comportamento sem placa de som.

**O que ele não afirma.** Nada sobre o **roster** sobreviver a uma reconexão —
quem continua sentado, quem devia ter saído, o que `Room::adopt` ou
`Room::apply(ServerMessage::Session)` fazem com `seats`, `presentes` ou
`current_voice_room`. Um arquivo chamado `voz_na_reconexao` ao lado de
`ocupacao.rs` e `sincronia.rs` sugere cobertura de reconexão em geral; cobre só
a metade do áudio. Registrado aqui para não ser contado como prova de
comportamento de reconexão em auditorias futuras — é o caso "existir não é
funcionar" do `CLAUDE.md`: o teste existe, passa, e mede outra coisa.

**Relação com a pendência #11.** #11 é a corrida do **servidor**: o desmonte
de uma sessão vieja, chaveado só por `PersonId`, pode apagar o assento da
sessão nova. O fantasma do lado do **cliente** — `Room::adopt` e o braço
`ServerMessage::Session` de `Room::apply` deixavam `seats`, `presentes` e
`current_voice_room` de pé por cima da fotografia de reabertura, que é
puramente aditiva — foi fechado nesta tarefa (Órbita d4350fdb), fundindo as
duas cópias da contabilidade numa só e limpando os três campos antes de
aplicar a fotografia. A corrida do servidor em #11 continua aberta; o teste
novo `crates/seele-conformance/tests/reconexao_fantasma.rs` prova só a metade
do cliente.

**Quando dói.** Sempre que alguém procurar, neste repositório, um teste que já
prove reconexão de roster e citar este arquivo por engano.

## 37 · A entrada numa sala de voz ainda se confirma pelo silêncio

**O sintoma que este número documenta já foi corrigido do lado do cliente**
(a pessoa que via a sala como sua e o anfitrião nunca via nem ouvia): a senha
agora chega ao servidor de verdade (`Client::enter_voice_room`,
`Enlace::entrar_na_voice_room`, `Command::EnterVoiceRoom` na FFI, até o
comando do Tauri e o `prompt` da tela), e uma recusa (`Alert
VoiceRoomEntryRefused`) agora desfaz o assento que o cliente havia tomado
sozinho, em `Room::apply` (`crates/seele-core/src/state.rs`).

**O que continua pendente, e por quê.** O cliente ainda se senta **antes** de
o servidor confirmar — `specs/02-protocolo.md` continua confirmando a entrada
pelo silêncio, e não por uma mensagem própria. O conserto deste número fecha o
sintoma (o assento errado não sobrevive à recusa), mas não fecha a causa: entre
o pedido e a recusa chegar, a tela deste cliente mostra por um instante uma
sala que ainda não é sua, e qualquer novo motivo de recusa que o protocolo
venha a ganhar exigiria lembrar de novo de desfazer o assento à mão, em vez de
o próprio protocolo garantir isso.

**Por que não foi resolvido agora.** O relatório de origem (seção 2.4 de
`Órbita/Relatórios/Desenvolvimento/1881a3cb-e333-48a9-9fd4-185153456c71.md`)
recomenda uma confirmação do servidor no fio como o conserto de raiz — mas
isso é mensagem de protocolo nova, e mexe em `PROTOCOL_VERSION`. A tarefa
d81586e0 já está subindo essa versão para 5; uma segunda mudança de protocolo
concorrente, na mesma janela, é como duas pessoas mexendo no mesmo número ao
mesmo tempo — colide. Esta tarefa teve escopo explícito de **não** tocar
protocolo, e o que ela entrega — repasse de senha e desfazer o assento na
recusa — já resolve o sintoma relatado sem esperar por isso.

**Quando dói.** Sempre que uma sala de voz tem senha, ou ganha algum outro
motivo de recusa no futuro: o intervalo entre pedir e ouvir a recusa continua
existindo, só não sobrevive mais a ele.


## 38 · O workflow de CI existe escrito, e ninguém provou que ele roda

**Numeração.** Este item nasceu como #34 numa ponta que nunca chegou a se
integrar com a main: enquanto este branch escrevia sobre o workflow de CI, a
main recebeu quatro itens próprios com os números 34, 35, 36 e 37 (casca sem
dizer de onde vem a tela, contador de espectadores não reanunciado, a
cobertura enganosa de `voz_na_reconexao.rs` e a entrada em sala de voz que se
confirma pelo silêncio — nenhum deles sobre CI ou sobre `seele-conformance`).
Renumerado para #38 para não colidir na integração; o conteúdo abaixo é o
mesmo que já foi revisado e aprovado.

**Sintoma.** `.github/workflows/ci.yml` foi escrito com um job `windows-2022`
para `clippy` e `test` — justamente para exercitar os blocos `#[cfg(windows)]`
de `crates/seele-audio/src/device.rs` e o arquivo inteiro de
`laco_por_processo.rs` (`#![cfg(windows)]`), que é onde vive o defeito relatado
de troca de fone e microfone (item 31). Mas o arquivo nunca foi enviado ao
GitHub: a autorização desta tarefa não incluía `push` nem `merge`, e sem
`push` nenhum workflow novo dispara — arquivo não versionado no remoto não é
workflow que rodou, é workflow que foi só desenhado.

**O que foi medido, e o que não foi.** Rodado nesta máquina — **macOS**, com
`rustup`, a toolchain fixada e o alvo `x86_64-pc-windows-msvc` já instalados,
mas sem `gh` para inspecionar ou disparar Actions — `cargo fmt --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings` e
`cargo test --workspace` passam. Isso prova que o job `linux`/`ubuntu-24.04`
do workflow tem chance real de passar (a plataforma é próxima o bastante) e
que nada no restante do workspace quebrou. **Isso não prova nada sobre o job
`windows-2022`**: em macOS, `laco_por_processo.rs` compila vazio porque
`#![cfg(windows)]` cobre o arquivo inteiro, e os ramos `#[cfg(windows)]` de
`device.rs` — incluindo o consentimento de microfone — nem chegam a ser
compilados, muito menos linkados ou executados. Tentar cruzar para o alvo
Windows a partir daqui (`cargo check -p seele-audio --target
x86_64-pc-windows-msvc`) nem chega a compilar: a build de `shiguredo_opus`
precisa do gerador do Visual Studio via CMake, e essa máquina não tem Visual
Studio nem o `vcvars` correspondente — a barreira é o linker/gerador nativo do
Windows, não a ausência de `rustup` ou do alvo. Nenhum comando rodado
localmente toca essa área; só um runner Windows de verdade toca.

**Por que não se fecha esta pendência daqui.** Fechar exigiria um dos dois: um
push real que dispare o Actions com o job `windows-2022`, ou uma máquina
Windows local para reproduzir os mesmos comandos. Nenhum dos dois está ao
alcance de quem escreveu este workflow nesta sessão — a tarefa que o criou foi
explicitamente proibida de publicar. Isso não é um defeito do workflow: é uma
lacuna de prova que só quem tem permissão de `push` pode fechar, e este
registro existe para que a primeira pessoa que enviar o branch saiba que ainda
falta olhar o resultado real do job `windows-2022` antes de confiar nele.

**Quando dói.** Toda vez que alguém disser «o CI cobre o Windows agora» antes
de ter visto ao menos uma execução real do job `windows-2022` passar ou
reprovar no GitHub Actions.

### Uma alegação de medição sobre `-j1` que não se reproduziu, e foi revertida

Numa retomada anterior desta tarefa, este registro chegou a afirmar que
`cargo test -p seele-conformance -- --test-threads=1` (sem `-j1`) reprovava
`moderacao::expulsar_acaba_com_a_sessao_e_deixa_voltar` por contenção entre os
24 binários de integração do crate rodando como processos concorrentes, e que
acrescentar `-j1` corrigia isso. Uma revisão independente contestou a
explicação técnica — o `-j` do `cargo test` não governa quantos binários de
teste já compilados rodam ao mesmo tempo — e pediu reprodução.

Rodei o comando sem `-j1` três vezes nesta sessão: as três passaram 100% dos
testes, incluindo o citado como reprovado, sem qualquer serialização extra.
A alegação anterior não se sustentou à medição repetida, então a mudança em
`ci.yml` (o `-j1`) foi revertida e o comando voltou ao que a pendência #29 já
tinha decidido: `cargo test -p seele-conformance -- --test-threads=1`.

**Quando dói.** Toda vez que uma explicação técnica plausível for escrita como
fato a partir de uma única observação não repetida — é exatamente o tipo de
conclusão que este arquivo pede para não fazer.

**O que sobrou, e não é o mesmo problema.** Rodando só `tests/moderacao.rs`
sozinho, sem nenhum outro binário disputando porta — então sem a contenção
entre binários que a explicação revertida descrevia —, uma de quatro
execuções ainda reprovou `expulsar_acaba_com_a_sessao_e_deixa_voltar` num
`assert!` de tempo (a função `ate(...)`, que espera até um prazo). Isso é
instabilidade intrínseca ao teste — provavelmente de tempo, não de recursos
disputados — e continua sem diagnóstico. Esta tarefa é só de infraestrutura
de CI e não mexe em código de teste; o registro fica aqui para quem for
investigar não gastar tempo checando `-j1` ou concorrência entre binários de
novo.

## 39 · Fechada em 2026-09-14 · O anúncio de MODs está pronto e não sai, esperando uma subida de versão

**Como fechou.** `PROTOCOL_VERSION` subiu para 5 na integração conjunta com a
malha, `VERSAO_DO_ANUNCIO` continuou em 5, e o portão ligou sem uma linha a mais
dentro de `o_anuncio_alcanca_alguem` — a comparação que ela sempre fez passou a
dar verdadeiro. Um servidor padrão com MOD habilitado agora anuncia, e quem não
aceita não entra: provado em
`o_anuncio_sai_do_servidor_padrao_sem_ninguem_baixar_limiar`, e os testes da
costura em `aceite_dos_mods.rs` deixaram de baixar o limiar à mão — correm na
configuração padrão.

**E o custo previsto abaixo é pago, não evitado — a tentativa de evitá-lo durou
um dia.** A tabela deste registro diz que subir para 5 tira do ar o cliente
publicado. Uma terceira saída pareceu existir quando a medida foi refeita — **a
v4 nunca foi publicada**, então N−1 protegeria uma fronteira que não existe — e
`COMPATIBILITY_WINDOW` foi para 2. A revisão mostrou que a saída era inerte: a
janela decide só o que esta build **ouve**, e todo quadro sai carimbado com a
versão global, que o build v3 recusa antes de ler o corpo. A janela voltou a 1.
Medido em `seele_proto::control::a_release_publicada_recusa_o_carimbo_desta_build`;
contado por inteiro no ADR 0046, seção «A janela não estica o que parecia», e
aberto como pendência #42.

**O que a subida cobra de quem está em campo, e é preciso dizer inteiro:** quem
está na v3 publicada **perde o servidor** quando a v5 for publicada, com MOD ou
sem MOD. Dentro da janela, um par da v4 entra num servidor sem MOD — provado em
`um_par_da_versao_anterior_continua_entrando` — e é recusado com `Incompatible`
por um servidor **com MOD habilitado**, porque não alcança `VERSAO_DO_ANUNCIO` e
mandar-lhe o anúncio mataria o fluxo de controle dele. Provado em
`um_par_dentro_da_janela_e_recusado_por_um_servidor_que_exige_mod`.

**O que continua aberto e não é esta pendência:** a tela de aceite (os dados
chegam à casca, falta o botão) e o download dos bytes em `mods.seele.app.br`.

---

**Estado de então.** O anúncio, o aceite e a recusa de MODs estão implementados
dos dois lados e cobertos por testes — inclusive por fluxos QUIC de verdade. O
que não acontece é o anúncio **sair**: ele viaja em
`seele_proto::mods::VERSAO_DO_ANUNCIO`, que é 5, e `PROTOCOL_VERSION` continua 4.

**Por que ele não subiu junto.** O postcard indexa variante por posição. O
anúncio de MODs e a entrega da malha acrescentam variantes ao mesmo par de
listas ao mesmo tempo; se cada uma subisse a versão global por conta própria, as
duas chamariam «5» a vocabulários diferentes — que é exatamente o defeito que o
guarda dos ordinais existe para pegar, e que já custou uma tela preta sem
mensagem nenhuma (pendência #33 e o histórico em `version.rs`).

**O que vale enquanto isso.** O portão fica **dormente**, e nada muda para
ninguém: um servidor com MOD habilitado admite quem entra como admitia antes
desta entrega e avisa quem hospeda pelo log de que a exigência ainda não vale no
fio; uma troca de MOD com gente dentro não derruba a sala; e um servidor sem MOD
habilitado, que é a maioria, não troca nenhum quadro novo.

A primeira versão desta entrega recusava todo cliente com `Incompatible` nesse
estado, e isso era uma regressão: enquanto nenhum par pode aceitar, recusar não
protege ninguém — só fecha uma casa que funcionava. Ver
`mods::anuncio::o_anuncio_alcanca_alguem`.

**O que fecha.** Subir `PROTOCOL_VERSION` para 5 uma vez, com as variantes das
duas entregas já na lista, e reconferir os ordinais que o guarda
`o_ultimo_verbo_de_cada_lista_esta_onde_esta_versao_o_deixou` prende. O contrato
inteiro está em `docs/superpowers/specs/2026-09-10-anuncio-e-aceite-de-mods.md`.

**O que a subida custa, medido e não suposto.** O último release publicado é o
`v0.10.5-1`, de 05/09/2026, e o `crates/seele-proto/src/version.rs` daquele
commit fala `PROTOCOL_VERSION = 3`. A janela de compatibilidade é N−1, então:

| versão global | mais antiga aceita | o que acontece com quem já instalou |
|---|---|---|
| 4 (hoje, não publicada) | 3 | o cliente publicado continua entrando |
| 5 (o dia do portão) | 4 | **o cliente publicado deixa de entrar**, com `Incompatible` |

Ou seja, ligar o portão e tirar do ar quem está em campo são o **mesmo ato**,
até que uma versão falando 4 seja publicada e instalada. É por isso que a
decisão não cabe nesta tarefa: ela não é sobre MODs, é sobre quem perde o
servidor na segunda-feira.

**O caminho que evita o custo**, e é o que sugere a ordem: publicar antes uma
versão que fale 4 — a entrega da malha, que já está em `main` e ainda não saiu —,
esperar o campo atualizar, e só então subir para 5. Aí a janela cobre quem
atualizou e o portão liga sem tirar ninguém do ar.

> **O que aconteceu, e o que a tabela acima não mostra.** A linha «5 → mais
> antiga aceita: 4» descreve a janela, e a janela é só o lado que ouve. Esticá-la
> para 2 faria a coluna do meio dizer 3 — e não faria um par v3 conectar, porque
> quem o barra é o carimbo de versão no primeiro byte do quadro que **sai**
> daqui, não a coluna desta tabela. Foi tentado e desfeito em 14/09/2026. A
> ordem sugerida abaixo (publicar antes uma versão falando 4) tampouco resolve
> pelo mesmo motivo: o build v4 recusaria o carimbo 5 igualmente. Ver #42.

**O que não resolve, e por que foi descartado.** Fazer o servidor **recusar**
quem não alcança o limiar, em vez de admitir, faz a frase «quem não aceitou não
entra» virar verdade — mas só porque ninguém entra. Enquanto nenhum par
consegue aceitar, «recusar quem não alcança» e «recusar todo mundo» são a mesma
coisa, e a metade «quem aceita, entra» continua sem existir. Troca-se um
critério não cumprido por outro, e de quebra habilitar um MOD passa a trancar a
sala. O estado dormente é o único que não mente sobre o que o produto faz hoje.

**O que esta entrega fechou desta pendência.** Uma revisão apontou que o portão
dormente, sozinho, deixa quem hospeda sem informação: a tela de habilitar lia
`enabled: true` e não tinha como saber que isso ainda não tranca ninguém na
rede — o "produto sabe e não conta" que o `CLAUDE.md` deste repositório nomeia
como o defeito mais caro daqui, e desta vez cometido contra quem hospeda, não
contra quem entra. `mods_instalados` agora devolve também
`exigencia_vale_na_rede` (`seele_server::mods::anuncio::exigencia_vale_na_rede`,
coberta por teste que falhou no dia em que o portão ligou — e falhou mesmo, que é
como esta pendência foi conferida de propósito em vez de descoberta pelo primeiro
servidor que passou a recusar), então quem monta a
tela de habilitar tem como avisar "exigido, mas ainda não bloqueia ninguém pela
rede" em vez de deixar o interruptor mentir por omissão. **Com o portão ligado o
campo vale sempre verdadeiro, e nenhum arquivo da casca o lê** — ele chega em
`mods_instalados` e para ali; quem hospeda continua sem ver na tela que habilitar
um MOD passou a barrar pares. Essa tela ganhou endereço próprio na #44. Isso não move a data
em que o portão liga de verdade — continua dependendo da subida de
`PROTOCOL_VERSION` descrita abaixo — só impede que o estado dormente seja
tomado por ativo por quem lê a tela.

**O que ainda falta depois disso**, e não é esta pendência: a frase ao lado do
interruptor na tela de quem hospeda, que agora é a #44; e a tela de aceite —
os dados chegam à casca pelo `ConnectionError::ModsNaoAceitos`, o `frases.js` já
desenha a lista, e os três verbos de responder estão registrados como comando da
janela (`aceite_de_mods`, `aceitar_mods`, `esquecer_aceite_de_mods`), declarados
no `AGUARDANDO_TELA` de `apps/seele-app/tests/frontend.rs`; o que não há é a tela
com o botão. E o download dos bytes em `mods.seele.app.br`, que é outra etapa.

### Cobertura da recusa por limiar, aprofundada em 2026-09-14 depois da revisão

Uma revisão independente desta entrega apontou que
`um_par_dentro_da_janela_e_recusado_por_um_servidor_que_exige_mod`, em
`crates/seele-conformance/tests/aceite_dos_mods.rs`, afirmava só `is_err()`: um
aperto de mão quebrado por **qualquer** outro motivo — outra recusa, transporte,
tempo esgotado — passaria verde e o teste continuaria dizendo que mediu o
limiar.

O ajudante `abrir_falando` agora devolve o quadro que veio no lugar de `Session`
(`NaoEntrou`), e o teste cobra a igualdade com
`Disconnecting { reason: Incompatible }`.

**Provado por reversão, e não por leitura.** Trocando em
`crates/seele-server/src/session.rs` o motivo da recusa por limiar de
`Incompatible` para `ProtocolViolation` — mudança que o `is_err()` antigo
engoliria —, o teste reprova dizendo exatamente o que mudou («left:
`Disconnecting { reason: ProtocolViolation }`, right: `Disconnecting { reason:
Incompatible }`»). O motivo foi restaurado em seguida; só a prova ficou.

## 40 · Uma reprovação intermitente, rara, no teste do cliente contra o anúncio

**O que foi visto.** `um_aceite_guardado_de_outro_conjunto_nao_e_reaproveitado`,
em `crates/seele-conformance/tests/aceite_dos_mods.rs`, reprovou **uma vez**: o
`Client::connect` devolveu `SemResposta` — a conexão morreu antes de qualquer
resposta chegar — no lugar da pergunta com a lista de MODs. A execução levou
20,01 s, que é exatamente o `IDLE_TIMEOUT` do transporte, e não os 10 s do
orçamento do aperto de mão. Isso põe a parada **antes** do aperto de mão, na
conexão QUIC em si, contra o servidor de mentira que o próprio teste levanta.

**O que foi medido, e não deu.** Depois da ocorrência: 40 execuções isoladas, 20
com `RUST_LOG` ligado, 20 sob carga de CPU e mais 60 do binário inteiro — **zero
reprovações em cerca de 170 execuções**. Isoladamente o teste leva 0,21 s. Não
reproduzi, e por isso não conserto: um conserto sem reprodução seria uma
hipótese vestida de correção.

**O que mudou mesmo assim.** O teste passou a contar o que o servidor de mentira
fez quando o cliente reprova — `o_que_o_servidor_fez`. Antes, a mensagem
descrevia só o sintoma do cliente e a tarefa do outro lado morria sem ser
recolhida; foi isso que impediu o diagnóstico. Na próxima ocorrência a mensagem
diz se o servidor leu a recusa, parou com erro, ou nem chegou lá.

**O que não é.** Não é a falha de validação que derrubou a bateria e foi
consertada nesta mesma passagem — aquela era a despedida de mudança de MODs
saindo sem `despedir`, reproduzível a ~8%, e está fechada com guarda
determinístico. Esta é outra, mais rara, e num teste que usa servidor de mentira.

**Atualização de 14/09/2026: a reprodução que faltava apareceu.** A mesma
assinatura — `SemResposta` numa execução de pouco mais de 20 s — reproduz na
`acceptance_m3` quando a máquina está com dois processos prontos por núcleo. A
receita e a medida estão na #43, e valem também para esta: o que faltava não era
conserto, era carga.

## 41 · A bateria de tela por um par reprova por espera, e não é desta entrega

**O que foi visto.** `crates/seele-conformance/tests/tela_por_um_par.rs` reprova
de forma intermitente com «a paciência acabou esperando: o servidor contar as
duas cópias que ele mesmo sobe», em pelo menos dois testes diferentes do mesmo
binário — `destruir_o_enlace_encerra_o_caminho_do_par_e_quem_emprestava_volta_a_servir`
e `uma_saida_voluntaria_derruba_o_caminho_do_par_e_a_tela_para`. Medido em **duas
reprovações em doze execuções do binário**; isolado, um dos dois testes não
reprovou em vinte execuções, o que põe a causa na concorrência entre os treze
testes do arquivo e não no teste sozinho. A espera que estoura é o `ate(...)` com a `PACIENCIA` do
arquivo, o que põe a causa na contagem de cópias do servidor não chegar ao valor
esperado dentro do prazo — a mesma família de defeito que a entrega da malha já
documentou uma vez, no contador que congelava sob estouro de fila.

**A atribuição, medida dos dois lados e não deduzida.** O argumento de que «é de
outra frente» valia por leitura do diff; agora vale por medida. Esta árvore e uma
exportação limpa do commit anterior — `ae5b65e`, sem nenhuma das mudanças de MOD
— foram rodadas **intercaladas**, uma rodada de cada por vez, para que as duas
pegassem a mesma carga de máquina:

| árvore | reprovações |
|---|---|
| com as mudanças de MOD | 2 em 20 |
| `ae5b65e` limpo, sem elas | 2 em 20 |

Intercalar não é detalhe: medidas separadas deram 3 em 10 aqui contra 0 em 10 lá,
e a diferença toda era a máquina estar mais ou menos ocupada na hora. Quem
concluísse dali teria culpado a entrega errada.

Ao todo caíram **cinco testes diferentes** do mesmo arquivo entre as duas
árvores, todos pela mesma espera. Não é um teste ruim: é o arquivo inteiro
correndo apertado.

**Por que está registrada aqui e não consertada.** É da frente da malha, e não
do anúncio de MODs. A única linha que esta entrega tocou neste arquivo é um
`aceito: None` num literal de struct, acrescentado porque o campo nasceu agora —
não tem como mexer em tempo. Consertar a espera seria mexer no teste de outra
frente sem a medida que a frente dela já tem, e ampliar o escopo desta tarefa.

**O efeito prático.** `cargo test` no repositório inteiro pode reprovar por esta
causa, sem relação com o anúncio e o aceite de MODs. Quem repetir a bateria vê
o binário `tela_por_um_par` no lugar de `aceite_dos_mods`. A bateria completa
**passou inteira** numa execução desta árvore — 1838 testes em 71 binários, sem
nenhuma reprovação —, o que diz que uma execução verde não prova estabilidade e
uma vermelha não prova regressão: nas duas direções, a atribuição precisa da
medida acima.

**Atualização de 14/09/2026: mais dois testes do mesmo arquivo, outra espera, e
a atribuição refeita contra o commit anterior.** A subida do protocolo para a v5
foi devolvida pela validação com uma reprovação neste mesmo binário, agora em
`a_reconexao_ao_servidor_nao_deixa_a_conexao_velha_atrapalhar_o_par_novo` e em
`a_queda_de_uma_conexao_so_derruba_o_caminho_do_par_com_o_par_ainda_vivo`. A
espera que estoura não é mais a das cópias: é o `SEM_IMAGEM_TOLERAVEL` de um
segundo, e o que se mede lá são 3,04 s — o `PRAZO_DO_PAR` inteiro, isto é, a
discagem gastando o prazo porque a vaga de quem empresta ainda não tinha voltado
quando o pedido novo chegou.

Medido:

| Condição | Reprovações |
| --- | --- |
| o teste sozinho, repetido | 0 em 12, primeiro quadro entre 162 e 173 ms |
| o binário inteiro (18 testes em paralelo), máquina carregada | 2 em 10 |
| **intercalado** com o commit anterior `63f4b80`, sem a subida da v5 | v5: 0 em 14 · anterior: 1 em 14 |

O teste sozinho tem folga de seis vezes contra o próprio limite; o que o derruba
é o arquivo inteiro levantando dezoito servidores QUIC de verdade ao mesmo tempo
numa máquina de quinze núcleos já entregue a outra coisa — a condição da #43. A
linha intercalada é a que fecha a atribuição, e foi feita alternando uma rodada
de cada binário para que os dois pegassem a mesma carga: a reprovação do lado
anterior caiu justamente em
`destruir_o_enlace_encerra_o_caminho_do_par_e_quem_emprestava_volta_a_servir`,
um dos dois nomes com que esta pendência nasceu.

**O que continua sem conserto, e é honesto dizer.** A mensagem do teste descreve
um mecanismo de produto plausível: a limpeza do caminho de par da conexão
substituída correndo atrás da discagem nova. Sob carga, a limpeza chega tarde e a
vaga aparece ocupada. Isso não foi consertado aqui, e não foi mascarado com
folga maior nem com menos paralelismo: aumentar o limite esconderia exatamente o
sintoma que aponta para o mecanismo. É trabalho da frente da malha, com a medida
acima já pronta para quem pegar.

**Numeração.** Estes três itens nasceram como #34, #35 e #36 numa ponta que
ainda não tinha se integrado à main. Enquanto isso, a main recebeu quatro
itens próprios com esses números (34 a 37) e a ponta do CI ocupou o #38.
Renumerados para #39, #40 e #41 na integração; o conteúdo é o mesmo que já foi
revisado e aprovado.

## 42 · A janela de compatibilidade não atravessa o fio: todo quadro sai carimbado com a versão global

**O que foi medido.** `seele_proto::control::encode` põe `PROTOCOL_VERSION` — o
número global desta build, nunca o negociado — no primeiro byte de todo quadro, e
`decode` recusa o byte recebido fora da janela **antes** de ler o corpo. Os dois
juntos dizem o seguinte: a janela de compatibilidade vale só para o lado que
ouve. Um servidor v5 aceita o `Hello` de um par mais velho que esteja na janela
dele, e no quadro seguinte manda a esse par um carimbo que o build dele recusa.

A conta, contra o código publicado e não contra suposição: a última release é a
`v0.10.5-1` de 05/09/2026 (commit `12a6401a6`), cujo `version.rs` tem
`PROTOCOL_VERSION = 3` e `COMPATIBILITY_WINDOW = 1` e cujo `decode` é o mesmo
desta árvore. Ela aceita o primeiro byte 2 ou 3 e recusa o resto com
`PeerTooNew`. Preso em
`seele_proto::control::a_release_publicada_recusa_o_carimbo_desta_build`.

**Por que isso importa agora.** A subida para a v5 alargou a janela para 2 em
nome de «quem já instalou não perde o servidor», e essa promessa não tem como ser
cumprida por nenhum valor desta janela. O número voltou a 1 e os textos foram
corrigidos (`version.rs`, ADR 0046, `specs/02-protocolo.md`,
`specs/10-convencoes.md`, #39). Fica valendo, então, o custo cru: **publicar a v5
tira do ar quem está na v3**, e o mesmo valeu para toda subida de versão desde a
primeira — as frases de «continua entrando» escritas nas subidas anteriores
descrevem só o lado que ouve.

**O que consertaria, e o que cada saída custa.**

1. **O seletor de versão do ADR 0046.** É a decisão já tomada: os dois lados
   sabem a versão antes de falar, e a negociação volta a ser conferência de
   coerência. Aposenta a pendência inteira, e não existe ainda.

2. **Carimbar o quadro com a versão negociada.** Barato de escrever e caro de
   acertar: o servidor passaria a ter de **calar** toda variante fora do
   vocabulário do par — hoje `SirvaTelaPara`, `AssistaTelaPor`, `ModsExigidos` —
   porque o postcard indexa variante por posição e uma variante desconhecida
   desloca a leitura do fluxo do outro lado para sempre. É a «tela preta, sem
   mensagem nenhuma» deste repositório, e foi por causa dela que este carimbo
   nasceu global. Precisa de ADR próprio, e de um teste por variante nova.

3. **Não fazer nada e avisar.** Enquanto o seletor não sai, publicar uma versão
   de protocolo nova é uma atualização obrigatória para todo mundo ao mesmo
   tempo. Isso é dizível no produto e nas notas da release; hoje não é dito em
   lugar nenhum, o que é o «o produto sabe e não conta» do `CLAUDE.md`.

**O que não é.** Não é uma regressão desta entrega: já valia na v4 não publicada
e em todas as subidas anteriores. O que esta entrega mudou foi deixar de afirmar
o contrário.

## 43 · A intermitência da bateria de conformidade reproduz sob máquina saturada, e o mecanismo tem nome

**O que ficou em aberto.** A #40 descreveu a assinatura — `Client::connect`
devolvendo `SemResposta` numa execução de 20,01 s, que é o `IDLE_TIMEOUT` do
transporte e não o orçamento de 10 s do aperto de mão — e fechou dizendo, com
razão, que sem reprodução não há conserto, só hipótese vestida de correção. O
que faltava era a receita. Ela existe, e está medida aqui.

**A receita, medida em 14/09/2026.** O binário `acceptance_m3` de
`seele-conformance`, repetido 40 vezes:

| Condição | Reprovações |
| --- | --- |
| `cargo test` do repositório inteiro, 3 rodadas seguidas | 0 |
| binário isolado, 8 threads, 8 núcleos ocupados | 0 em 40 |
| binário isolado, 15 threads, **15 núcleos ocupados** | 2 em 40, depois 1 em 20 |

A máquina tem 15 núcleos. A terceira linha é a única que reprova, e é a única em
que a suíte disputa cada núcleo com outra carga — dois processos prontos para
cada núcleo disponível. Os testes reprovados foram
`a_restarted_server_keeps_its_history` e
`a_returning_person_reclaims_their_seat_and_their_ssrc`, ambos em 20,1 s, ambos
com `SemResposta`.

**O mecanismo.** `SemResposta` é a tradução de `quinn::ConnectionError::TimedOut`
em `seele-core/src/client.rs` — a conexão inteira expirando, e não o aperto de
mão demorando. Cada `#[tokio::test]` levanta o seu próprio servidor QUIC de
verdade e o seu próprio runtime; com quinze deles em paralelo numa máquina que já
está entregue a outra coisa, o keepalive de 5 s não chega a ser servido dentro
dos 20 s de `IDLE_TIMEOUT` e a conexão morre sozinha. É fome de CPU, e a bateria
está medindo a máquina em vez do produto.

**Por que isso importa para quem valida.** É o mesmo sintoma que vinha derrubando
`cargo test` na validação desta entrega e sendo devolvido três vezes como «não
reproduz». Não reproduzia porque a máquina de quem media estava ociosa. Na
prática, rodar duas baterias ao mesmo tempo — duas worktrees, duas sessões — é
condição suficiente. Uma bateria verde não prova estabilidade e uma vermelha,
sozinha, não prova regressão: a atribuição precisa da carga junto.

**E aconteceu de novo enquanto esta pendência era escrita, com a carga medida
junto.** Um `cargo test` comum desta árvore reprovou em
`a_shell_connects_and_the_snapshot_describes_the_server`, do binário
`acceptance_m5`, em 20,31 s — a mesma assinatura. A carga média da máquina
naquele instante era **77,5 para 15 núcleos**, com outra worktree compilando o
workspace inteiro a partir de um arquivo de baseline e o antivírus do sistema em
74% de um núcleo. Cinco processos prontos para cada núcleo. Isso fecha a
atribuição: não é um teste específico que é frágil — é qualquer teste de rede da
conformidade que peça um aperto de mão enquanto a máquina está entregue a outra
coisa. Três testes diferentes já reprovaram com a mesma assinatura nesta mesma
sessão, o que descarta defeito localizado num deles.

**A leitura prática para quem valida.** Uma bateria reprovada por esta causa não
diz nada sobre a entrega que está sendo validada. Antes de devolver uma
reprovação como bloqueio, vale conferir a carga da máquina no momento: se houver
outra worktree compilando ou testando, a reprovação é desta pendência até prova
em contrário — e a prova é repetir com a máquina livre.

**O que não é, conferido e não suposto.** Não é regressão da subida para a v5. O
commit desta entrega não toca `acceptance_m3.rs`, nem `seele-core`, nem o
transporte; o único toque recente naquele arquivo, vindo da entrega dos MODs, foi
acrescentar um argumento `None` a uma chamada. Os dois testes que reprovam não
passam por MOD nenhum nem por variante de malha.

**O que consertaria, e o que cada saída custa.**

1. **Dar à conformidade um prazo de transporte próprio.** Honesto e barato de
   descrever, caro de acertar: hoje `Client::connect` monta o `TransportConfig`
   por dentro, com as constantes de `seele-proto::transport`, e não há por onde
   um teste pedir outro prazo. Abrir essa costura é mexer em produto para servir
   a teste, e precisa de decisão própria.

2. **Limitar a concorrência da suíte.** Uma linha em `.config/nextest` ou um
   `--test-threads` menor tira a disputa e devolve tempo de parede. Não conserta
   o produto, e é o que de fato está em jogo: a suíte, não o cliente.

3. **Deixar como está e dizer.** É o que esta pendência faz. O custo é que a
   validação continua podendo reprovar por carga, agora com nome e receita em vez
   de mistério.

**O que não fica prometido.** Nenhuma das três foi feita aqui: as duas primeiras
são escolha de outra frente, e fazê-las por conta desta entrega seria ampliar o
escopo de uma subida de versão até dentro do transporte.

**A saída 2 foi medida antes de ser recusada, em 14/09/2026.** A validação
devolveu a entrega mais uma vez, e desta vez a medida foi feita com a máquina na
condição que a pendência descreve, e não ociosa: carga 92 para 15 núcleos, com
outra worktree rodando a própria bateria de conformidade e três `swift-frontend`
compilando ao lado. Nessa condição:

| Condição | Reprovações |
| --- | --- |
| `cargo test` do repositório inteiro, carga 84 a 95 | 0, saída 0 |
| `acceptance_m3` com 15 threads (o padrão), alternado | 0 em 12 |
| `acceptance_m3` com 4 threads (a saída 2), alternado | 0 em 12 |

As duas colunas foram intercaladas rodada a rodada para pegarem a mesma carga. O
resultado é o que decide: **limitar a concorrência não mostrou ganho medível
nesta janela**, então adotá-la seria trocar tempo de parede de todo mundo por uma
melhora que ninguém mediu. Fica a saída 3, agora por medida e não por preferência.

E há um dado novo sobre a reprovação devolvida: ela não reproduziu **nem com a
máquina saturada**. As três devoluções anteriores foram respondidas com «não
reproduz» de máquina ociosa, o que era resposta fraca; esta foi respondida com a
carga junto. Continua valendo que uma bateria verde não prova estabilidade — mas
24 execuções do binário devolvido, mais uma bateria inteira, sem nenhuma
reprovação sob a carga que a #43 aponta como causa dizem que a
reprovação devolvida não está nesta entrega.

**A quarta devolução, em 14/09/2026 às 13h, trouxe um alvo nomeado — e ele
passou.** Desta vez a reprovação veio identificada (`-p seele-conformance --test
acceptance_m3`, saída 101), o que permitiu medir o binário exato em vez de a
suíte inteira. Com a máquina na mesma condição que a pendência descreve — carga
58 a 73 para 15 núcleos, outra worktree compilando ao lado —, `acceptance_m3`
devolveu 8 de 8 e o `cargo test` do repositório inteiro devolveu 1881 testes
passando, 0 reprovados, 4 ignorados, saída 0, com os 73 binários verdes. O alvo
nomeado é o mesmo que a #43 já descreve: um aperto de mão de rede num teste que a
subida para a v5 não toca. Vale a leitura prática acima — a reprovação é desta
pendência, e a prova é repetir.

**A quinta devolução repetiu o mesmo alvo, e a medida foi repetida junto.** Ainda
em 14/09/2026, com carga de 66 a 74 para 15 núcleos, `acceptance_m3` devolveu 8
de 8 em cinco rodadas seguidas e a bateria do repositório inteiro devolveu 73
binários, 1881 testes passando, 0 reprovados, 4 ignorados. São agora duas
devoluções do mesmo alvo nomeado respondidas com a máquina saturada, e nenhuma
reprovou. Isso não promete que a próxima rodada será verde — a #43 diz justamente
que não há essa promessa —, mas mantém a atribuição onde a medida a colocou: a
reprovação é de carga, e não da subida para a v5.

**A sexta devolução foi a primeira que reproduziu, e ela trouxe o controle que
faltava.** Ainda em 14/09/2026, por volta das 14h30, com a máquina em carga 76
para 15 núcleos — dois `clippy-driver` de outras worktrees, um processo de
navegador a 99% de um núcleo e dezenas de shells, todos de fora deste worktree —,
a bateria do repositório inteiro reprovou em quatro de sete rodadas. As cinco
respostas anteriores diziam «não reproduz»; esta reproduz, e por isso vale mais
do que todas elas juntas.

O que ela mostra:

| Condição | Reprovações | Quem reprovou |
| --- | --- | --- |
| `cargo test` padrão, carga 70 a 86 | 4 em 7 | `salas`, `a_client_without_permission_is_refused`, `o_fim_limpo_do_repasse_devolve_quem_assiste_ao_servidor` |
| `cargo test` com `TOKIO_WORKER_THREADS=2` | 2 em 3 | `a_pilha_dupla_recebe_um_pacote_ipv4_de_verdade`, `destruir_o_enlace_encerra_o_caminho_do_par_e_quem_emprestava_volta_a_servir` |
| **`cargo test -- --test-threads=1`**, um teste por vez | 1 em 1 | `um_operador_modera_pessoas_e_nao_o_comandante` |

**A linha serializada é a que decide, e ela fecha duas portas de uma vez.** Com
um teste de cada vez não há concorrência interna nenhuma para culpar: é um
servidor e um cliente em loopback, sozinhos no processo — e o aperto de mão QUIC
ainda queima os 20 s do `IDLE_TIMEOUT` sem resposta. Isso exclui o paralelismo da
bateria como causa, e com ele exclui também o conserto que mais tentava a mão:
baixar `--test-threads` ou fixar `worker_threads` não teria adiantado nada, e
teria custado tempo de parede de todo mundo em troca de uma melhora inexistente.
Confirma, por outro caminho, a saída 3 medida acima.

**E exclui esta entrega.** Cada rodada reprovou num teste *diferente*, em
subsistemas que a subida para a v5 não toca — desde a descoberta de pilha dupla
até a moderação de pessoas. Uma regressão de número de versão não muda de
endereço a cada execução. Além disso, o caminho dos testes que reprovaram é
**byte a byte o mesmo de antes da subida**: com nenhum MOD habilitado,
`exigir_aceite_dos_mods` devolve antes de escrever quadro nenhum, e a única
diferença no fio é o número dentro do `Hello`. A reprovação chega antes de
qualquer quadro, como `quinn::ConnectionError::TimedOut`, que
`classify_connection_error` traduz em `SemResposta`: o `Initial` não obteve
resposta em 20 s numa máquina em que cada processo recebe um quinto de núcleo.

**O que isso deixa em aberto, dito sem enfeite.** Esta pendência não tem conserto
dentro desta entrega, e ela não é o único trabalho que a máquina atrapalha: uma
validação que roda a bateria inteira enquanto outras sessões compilam vai
reprovar de vez em quando, em testes sorteados. O conserto de verdade é de duas
frentes que não são esta — dar à bateria de conformidade um transporte que não
dependa do relógio de parede da máquina, ou dar à validação uma máquina que ela
não divida. Até lá, a leitura prática do começo desta pendência continua sendo a
única honesta: a reprovação é desta pendência, e a prova é repetir.

**A contagem fechada da sexta devolução:** sete baterias completas na
configuração padrão, três verdes e quatro vermelhas, com carga entre 70 e 86 o
tempo todo. Nove testes distintos apareceram nas quatro reprovações, e **nenhum
apareceu em todas** — o mais repetido saiu duas vezes. Nessa mesma janela,
`cargo fmt --all --check` e `cargo clippy --workspace --all-targets
--all-features -- -D warnings` devolveram saída 0 sem um aviso sequer, porque
nenhum dos dois depende do relógio de parede. É a diferença entre o que a carga
atinge e o que ela não atinge, e ela é a própria assinatura.

**A sétima devolução veio com o mesmo alvo nomeado, e o que ela acrescenta é
uma taxa.** Ainda em 14/09/2026, por volta das 15h, a validação devolveu de novo
`-p seele-conformance --test acceptance_m3` com saída 101. Com a máquina na
condição que esta pendência descreve — carga de 59 a 72 para 15 núcleos durante
a medida inteira —, o binário nomeado devolveu 8 de 8 em três rodadas seguidas e
a bateria do repositório inteiro devolveu **quatro rodadas completas, todas
verdes**: 73 binários, 1881 testes passando, 0 reprovados, 4 ignorados, saída 0
em cada uma. `cargo fmt --all --check` e `cargo clippy --workspace
--all-targets --all-features -- -D warnings` devolveram saída 0 sem um aviso.

Isso não contradiz a sexta devolução, e é importante não ler como se
contradissesse: lá a bateria reprovou em quatro de sete rodadas com carga de 70 a
86, aqui passou em quatro de quatro com carga de 59 a 72. As duas medidas juntas
são a mesma curva — a reprovação aparece quando a máquina passa de um ponto e
desaparece quando ela volta. É exatamente o que se espera de uma reprovação por
fome de CPU, e não de uma regressão: uma regressão de número de versão não
some porque a carga caiu doze pontos.

**O que continua sem conserto aqui, dito de novo porque a devolução repetiu.** As
duas saídas que consertariam de verdade — dar à conformidade um prazo de
transporte próprio, ou dar à validação uma máquina que ela não divida — seguem
fora desta entrega, pelos motivos contados acima. Enquanto a validação rodar a
bateria inteira numa máquina compartilhada com outras worktrees, ela vai reprovar
de vez em quando em testes sorteados, e a leitura prática continua sendo a do
começo: conferir a carga antes de tratar a reprovação como bloqueio.

**A oitava devolução trocou o alvo nomeado, e é isso que ela acrescenta.** Ainda
em 14/09/2026, por volta das 16h, a validação devolveu saída 101 em
`-p seele-conformance --test acceptance_seguranca` — um binário diferente dos
dois que as devoluções anteriores nomearam. A reprovação cai em
`acceptance_seguranca.rs`, no auxiliar `razao`, que existe justamente para não
dobrar três recusas numa só: ele entra em pânico quando não vem uma recusa
enumerada, e o que veio no lugar foi `SemResposta` — a assinatura desta
pendência, e não uma recusa errada.

Com a máquina na condição descrita aqui — carga de 65 a 80 para 15 núcleos
durante a medida inteira —, a bateria do repositório inteiro devolveu **duas
rodadas completas, ambas verdes**: 73 binários, 1881 testes passando, 0
reprovados, 4 ignorados, saída 0 em cada uma. `cargo fmt --all --check` e
`cargo clippy --workspace --all-targets --all-features -- -D warnings`
devolveram saída 0 sem um aviso.

**O alvo mudar é o dado, e ele reforça a atribuição em vez de abri-la.** São
agora três binários distintos nomeados em devoluções sucessivas — `acceptance_m3`
duas vezes, `acceptance_seguranca` uma — mais os nove testes sorteados da sexta
devolução. Uma regressão de número de versão não muda de endereço a cada
validação; uma fome de CPU sorteia justamente quem estiver esperando um aperto de
mão no pior instante. O caminho de `acceptance_seguranca` não passa por MOD nem
por variante de malha: são recusas de portaria, e a única diferença no fio
continua sendo o número dentro do `Hello`.

**Nada mudou no conserto, e continua valendo a leitura prática.** As duas saídas
que resolveriam de verdade seguem fora desta entrega. Antes de devolver uma
reprovação como bloqueio, conferir a carga da máquina — e a prova continua sendo
repetir.

**A nona devolução foi a primeira em que a carga tem dono com nome e hora.** Ainda
em 14/09/2026, por volta das 16h40, a validação devolveu saída 101 de novo. Desta
vez a medida começou pela máquina, e não pela suíte: havia **vinte processos `yes`
órfãos** — pai 1, todos iniciados às 16:41:11 — queimando cerca de 30% de um
núcleo cada, o que é seis núcleos dos quinze consumidos por nada. Eles são o
resto de uma medição feita para esta própria pendência: a receita da #43 satura a
máquina de propósito, e desta vez os geradores de carga sobreviveram a quem os
levantou. A carga média ficou entre 76 e 112 para 15 núcleos durante toda a
medida.

Nessa condição:

| Condição | Reprovações | Quem reprovou |
| --- | --- | --- |
| `cargo test` do repositório inteiro | 3 em 3 | `o_fim_limpo_do_repasse_devolve_quem_assiste_ao_servidor`, `a_senha_do_server_fecha_a_porta`, `a_reconexao_ao_servidor_nao_deixa_a_conexao_velha_atrapalhar_o_par_novo` |
| `cargo test -p seele-conformance` | 2 em 4 | `uma_saida_voluntaria_derruba_o_caminho_do_par_e_a_tela_para`, mais `a_reconexao...` e `o_quadro_chega_pelo_par_e_o_servidor_nao_o_subiu` juntos |
| `tela_por_um_par` **sozinho**, mesma carga | 0 em 5 | — |

A terceira linha é a que separa produto de máquina: o binário que reprovou dentro
da bateria passou 18 de 18 cinco vezes seguidas sob a mesma carga, rodando
sozinho. E `cargo fmt --all --check` e `cargo clippy --workspace --all-targets
--all-features -- -D warnings` devolveram saída 0 sem um aviso na mesma janela,
porque nenhum dos dois espera relógio.

**O que isso muda na leitura prática, e é a única novidade desta rodada.** Até
aqui a orientação era conferir a carga antes de tratar a reprovação como
bloqueio. Agora há um item concreto a conferir junto: **`pgrep -x yes`**. Se
houver geradores de carga órfãos vivos, a bateria inteira está medindo eles, e
nenhuma entrega passa na validação enquanto isso durar.

**A décima devolução corrigiu a conclusão da nona, e é ela que fecha o assunto.**
A nona parou em «encerrá-los é decisão de quem opera a máquina, não desta
entrega», supondo que os `yes` viessem de fora. Não vinham. Às 17h06 de
14/09/2026 os mesmos vinte continuavam lá, com a hora de início intacta —
16:41:11, dentro da janela da própria medição da #43 —, e foram encerrados. A
carga não cedeu: caiu de 94 para 95. Foi aí que apareceu o que `pgrep -x yes`
não mostra: **trinta e oito subshells em laço ocupado** (`while :; do :; done`),
levantados pelos scripts de medição desta pendência, todos com o caminho deste
worktree na própria linha de comando, órfãos de pai 1. `pgrep -x yes` não os via
porque o nome do processo é `zsh`. Encerrados esses, a carga caiu de 94 para 9,9
em três minutos.

Com a máquina assim:

| Condição | Reprovações |
| --- | --- |
| `cargo test` do repositório inteiro, carga inicial entre 5,8 e 10,3 | **0 em 4** |
| a mesma bateria, iniciada enquanto a carga ainda descia (média de 5 min em 24) | 1 em 1, em `a_reconexao...`, com `SemResposta` aos 20,22 s |

A segunda linha é a assinatura de sempre: 20,22 s é o tempo ocioso do
transporte, não o orçamento de 10 s do aperto de mão. Ela reprovou porque a
máquina ainda estava devolvendo os núcleos, e não porque o protocolo mudou.

**A lição que sobra, e ela é sobre entulho e não sobre o produto.** Quem levanta
carga de propósito para medir é quem tem de derrubá-la, e um `kill` no fim do
script não basta se o script é interrompido antes de chegar lá. A conferência
correta antes de tratar uma reprovação de conformidade como bloqueio não é
`pgrep -x yes` e sim olhar o topo de `ps -A -o pcpu,pid,command -r`: laço ocupado
em subshell aparece como `zsh`, com nome nenhum que denuncie o que está fazendo.

**A décima primeira devolução repetiu o alvo e confirmou a décima, com a máquina
já limpa.** Às 17h42 de 14/09/2026, sem nenhum gerador de carga vivo (topo de
`ps -A -o pcpu,pid,command -r` mostrando só processos do sistema e do editor) e
com carga inicial de 5,9, `cargo test` do repositório inteiro passou: 73
binários, 1881 testes, 0 reprovações, saída 0. `cargo fmt --all -- --check` e
`cargo clippy --workspace --all-targets --all-features -- -D warnings` também
saíram 0 na mesma bateria. Nada foi mudado no produto entre a devolução e esta
medição — a única alteração desta rodada foi de documentação (três ponteiros de
`docs/adr/README.md`). Isso fecha o assunto do mesmo jeito que a décima já o
fechava: a reprovação relatada é da máquina saturada, não do protocolo.

**Um endereço errado corrigido, e é só isso.** `docs/adr/README.md` ainda
mandava as linhas dos ADRs 0022, 0029 e 0032 para o `0044`, número que hoje
pertence ao ADR do portão da subida. O destino é o `0045` — a página de MODs
nasceu 0044 e foi renumerada na integração, e ela mesma diz isso no cabeçalho.
Endereço não é registro de decisão, e a regra desta pasta manda corrigir
endereço errado onde ele estiver.

**A décima primeira devolução mostrou um segundo mecanismo, e esse tinha
conserto dentro da entrega.** Em 14/09/2026, com a bateria inteira rodando sob
carga forçada, `tela_por_um_par` reprovou em
`a_reconexao_ao_servidor_nao_deixa_a_conexao_velha_atrapalhar_o_par_novo` com
uma mensagem que nenhuma rodada anterior tinha registrado: não foi paciência
acabando, foi `não deu para ligar em 127.0.0.1:50742 · Address already in use`.
O teste derruba o servidor e sobe outro **na mesma porta**, porque é isso que
faz do reinício uma reconexão em vez de um destino novo. Entre a queda e a
volta a porta fica livre por um instante, e outro binário da bateria — que pede
porta efêmera com `bind(0)` — chega a levá-la. O sorteio de portas do sistema
reprovava a entrega.

Isso não é fome de CPU, e por isso teve conserto: `tela_por_um_par.rs` e
`bateria_interna.rs` passaram a subir o servidor de volta com
`ligar_insistindo`, que repete o `bind` por até dez segundos **e só enquanto a
recusa for `AddrInUse`**. Qualquer outro erro sobe na hora, para que insistência
não vire máscara de defeito. É o que um operador faria ao subir de novo um
servidor que acabou de cair.

**O guarda foi provado contra a regressão, e não só escrito.** O teste
`a_porta_tomada_por_um_instante_nao_reprova_a_volta_do_servidor` encena a
disputa: alguém segura a porta, o `bind` direto recusa na hora, o dono solta
meio segundo depois e o servidor sobe no mesmo endereço. Trocando
`ligar_insistindo` de volta por `Daemon::bind`, ele reprova com
`Address already in use` — medido em 14/09/2026. A fome de CPU descrita acima
continua valendo para o resto da bateria; o que mudou é que uma das reprovações
tinha causa própria e deixou de acontecer.

**E a espera passou a ter uma casa só.** Ela nasceu copiada nos dois arquivos,
palavra por palavra, e uma revisão independente apontou o que isso custa: quem
for mexer no teto de dez segundos, ou no motivo que autoriza a espera, tem de
achar as duas cópias e acertar as duas. Desde 14/09/2026 ela mora em
`crates/seele-conformance/tests/porta/mod.rs`, e os dois testes a declaram com
`mod porta;`. A pasta é deliberada: o `cargo` só transforma em binário de teste
os arquivos soltos em `tests/`, então um módulo dentro de uma pasta é
compartilhável sem virar suíte própria. A prova de regressão continua onde
estava, em `tela_por_um_par.rs`, e continua reprovando se a espera voltar a ser
`Daemon::bind`.

**A oitava devolução trocou o alvo, e o alvo novo não passa por esta entrega.**
Ainda em 14/09/2026, por volta das 19h, a validação devolveu
`-p seele-conformance --test acceptance_seguranca` com saída 101 — binário
diferente dos anteriores, mesma assinatura de prazo. Com a máquina em carga 7,2 a
7,4 para 15 núcleos, o binário nomeado devolveu 8 de 8 em seis rodadas seguidas e
a bateria do repositório inteiro devolveu duas rodadas completas verdes, sem uma
reprovação. O arquivo `crates/seele-conformance/tests/acceptance_seguranca.rs`
não é tocado por esta entrega e levanta cada servidor em porta efêmera
(`127.0.0.1:0`), então não é a corrida de porta da #41: é a mesma fome de
transporte descrita aqui. O que a troca de alvo acrescenta é confirmação do que
a sexta devolução já media — **quem reprova é sorteado**, e um alvo que muda a
cada devolução não é regressão de número de versão.

**A nona devolução sorteou um terceiro alvo, e o contador de variantes ganhou um
conserto de verdade.** Às 19h30 de 14/09/2026 a validação devolveu
`-p seele-conformance --test acceptance_m3` com saída 101 — o terceiro binário
diferente em três devoluções. `acceptance_m3.rs` também levanta cada servidor em
porta efêmera (`127.0.0.1:0`), então não é a corrida de porta da #41. Com carga
5,8 a 7,6 para 15 núcleos, o binário nomeado devolveu 8 de 8 em seis rodadas
seguidas e a bateria do repositório inteiro passou inteira: 73 binários, 1883
testes, 0 reprovações. `cargo fmt --all -- --check` e
`cargo clippy --workspace --all-targets --all-features -- -D warnings` saíram
limpos na mesma bateria. Três devoluções, três alvos distintos, nenhum deles
reproduzível: o padrão é o da máquina, não o do protocolo.

**O que mudou de produto nesta rodada foi o contador de variantes, e ele tinha um
buraco real.** A revisão notou que `ultima_variante`, em
`crates/seele-proto/src/control.rs`, contava a lista tentando decodificar um
quadro de 64 zeros em cada ordinal até um falhar. O buraco está exatamente onde o
guarda mais precisa acertar: uma variante nova **acrescentada no fim**, com um
campo que não aceita zeros (um `char`, um `NonZeroU32`), não decodifica; a
contagem para antes dela e devolve o mesmo número de ontem. O teste passaria
verde para a adição que ele existe para cobrar — a versão não sobe, e dois builds
com listas diferentes voltam a se cumprimentar como iguais, que é a tela preta
sem mensagem de novo.

A pergunta certa não passa pela carga: o `derive` do `serde` entrega a lista de
nomes das variantes ao `Deserializer` em `deserialize_enum`, antes de qualquer
campo. `quantas_variantes` implementa um `Deserializer` que só sabe atender a
essa chamada, anota `variants.len()` e desiste. Nenhum campo é lido, então nenhum
tipo de campo muda a resposta.

**E o guarda foi provado contra a regressão, não só escrito.** O teste
`uma_variante_que_nao_aceita_zeros_ainda_e_contada` monta uma lista de três
variantes cuja última carrega um `NonZeroU32`, confere que um quadro de zeros
naquele ordinal de fato **não** decodifica, e exige que a contagem ainda chegue a
2. Trocando `quantas_variantes` de volta pelo laço de cargas de zeros, ele
reprova dizendo 1 em vez de 2 — medido em 14/09/2026. É essa reprovação que diz
que o guarda voltou a ser cego para uma adição no fim.

**A décima segunda devolução tinha o mesmo entulho da décima, vivo de novo, e
desta vez a conferência veio antes da suíte.** Às 21h15 de 14/09/2026 a
validação devolveu `cargo test` com saída 101 em `acceptance_m3`. Antes de rodar
qualquer coisa, o topo de `ps -A -o pcpu,pid,command -r` mostrou **trinta
subshells em laço ocupado** (`while :; do :; done`), órfãos de pai 1, todos com
o caminho deste worktree na linha de comando — o mesmo resto de medição que a
décima devolução já tinha descrito, levantado outra vez por uma medida feita
para esta própria pendência e sobrevivendo a quem a levantou. A carga era 45,6
para 15 núcleos, com nenhum `cargo` rodando: **os trinta eram a carga inteira**.

Encerrados eles, e com a máquina devolvendo os núcleos (carga descendo de 45 para
13):

| Condição | Reprovações |
| --- | --- |
| `cargo test` do repositório inteiro, **duas rodadas**, carga de 13 caindo a 8,2 | **0 em 2**, saída 0 em cada — 73 binários, 1883 testes, 4 ignorados |
| `cargo fmt --all -- --check` | saída 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | saída 0, sem um aviso |

**O que esta rodada acrescenta, e é a única novidade.** A décima já tinha dito
que o entulho de medição é a causa e que `pgrep -x yes` não o enxerga. O que ela
não tinha dito é que o entulho **volta**: qualquer sessão que rode a receita da
#43 e seja interrompida antes do `kill` deixa a máquina assim para a validação
seguinte, e a devolução que chega depois parece regressão da entrega. A
conferência do começo desta pendência ganha, por isso, uma ordem: **olhar o topo
de `ps -A -o pcpu,pid,command -r` antes de rodar a suíte**, e não depois de ela
reprovar. Se o topo for subshell `zsh` com o caminho de um worktree, a bateria
vai medir isso e mais nada.

**A décima terceira devolução veio com a máquina limpa, e é isso que ela
acrescenta.** Às 21h45 de 14/09/2026 a validação devolveu `cargo test` com saída
101 apontando `acceptance_m3` — de novo sem nomear teste algum, com a saída
cortada no meio da lista de binários. Seguindo a ordem que a décima segunda
devolução deixou escrita, a conferência começou pela máquina e não pela suíte: o
topo de `ps -A -o pcpu,pid,command -r` não tinha entulho de medição nenhum —
nenhum `yes`, nenhum subshell em laço —, e a carga era 4,3 a 6,7 para 15
núcleos, com apenas outra worktree rodando a própria bateria ao lado. Nessa
condição:

| Condição | Reprovações |
| --- | --- |
| `cargo test` do repositório inteiro, **duas rodadas** | **0 em 2**, saída 0 em cada — 73 binários, 1883 testes, 4 ignorados |
| `cargo fmt --all --check` | saída 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | saída 0, sem um aviso |

**O que ela muda, dito sem enfeite: nada na atribuição, e uma coisa no que se
pode prometer.** Treze devoluções depois, a leitura do começo desta pendência
continua sendo a única honesta, e ela agora tem um limite reconhecido: repetir a
medida responde a devolução da vez e não impede a próxima. As duas saídas que
consertariam de verdade — dar à conformidade um prazo de transporte que não
dependa do relógio de parede, ou dar à validação uma máquina que ela não divida
— seguem fora desta entrega, e é honesto dizer que enquanto nenhuma das duas for
feita por uma frente própria, a validação vai continuar reprovando de vez em
quando em testes sorteados. Essa frente é o próximo passo real desta pendência,
não mais uma repetição da medida.

**A décima quarta devolução repetiu o alvo, e desta vez a assinatura ganhou
endereço no código.** Ainda em 14/09/2026, por volta das 22h, a validação
devolveu de novo `-p seele-conformance --test acceptance_m3` com saída 101. A
medida foi refeita com outra worktree rodando a própria bateria ao lado, carga
de 3,7 a 6,9 para 15 núcleos:

| Condição | Resultado |
| --- | --- |
| `cargo test` do repositório inteiro, **duas rodadas** | **0 em 2**, saída 0 em cada — 73 binários, 1883 testes, 4 ignorados |
| `cargo fmt --all --check` | saída 0 |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | saída 0, sem um aviso |

Isso, sozinho, não acrescenta nada: é a décima quarta repetição da mesma
resposta, e esta pendência já reconheceu acima que repetir a medida responde a
devolução da vez e não impede a próxima.

**O que esta rodada acrescenta de verdade: por que o relógio marca 20 s e nunca
10.** Desde a #40 esta pendência anota a discrepância — a reprovação chega em
20,0 a 20,3 s, que é o `IDLE_TIMEOUT` do transporte, e não nos 10 s do
`HANDSHAKE_TIMEOUT` que `specs/02-protocolo.md` promete — sem nunca dizer o
motivo. O motivo está em `crates/seele-core/src/client.rs` e é conferível numa
leitura: o `tokio::time::timeout(HANDSHAKE_TIMEOUT, …)` envolve apenas
`handshake(…)`, ou seja, a troca de quadros **depois** de a conexão já estar
aberta. O estabelecimento da conexão QUIC em si —
`endpoint.connect_with(…)?.await` — não tem prazo nenhum por cima, e só desiste
quando o `max_idle_timeout` de 20 s do transporte o derruba. Daí sai
`quinn::ConnectionError::TimedOut`, que `classify_connection_error` traduz em
`SemResposta`.

Ou seja: os 10 s prometidos nunca cobriram o trecho em que a suíte reprova. A
assinatura de 20 s deixa de ser um detalhe estranho e passa a ser consequência
esperada de onde o prazo está.

**Isso não muda a atribuição, e muda duas outras coisas.** Não muda a
atribuição porque o que estoura o prazo continua sendo a máquina: com prazo de
10 s a mesma fome de CPU reprovaria os mesmos testes, mais cedo e com outro
nome. Muda, primeiro, o custo da saída 1 desta pendência — «dar à conformidade
um prazo de transporte próprio» foi descrita como cara por exigir abrir uma
costura de configuração no transporte, e agora se sabe que o ponto em questão é
uma linha de `client.rs`, não uma costura nova.

E muda, segundo, uma coisa que não é sobre a suíte: **quem tenta entrar num
servidor calado espera o dobro do prometido e recebe um motivo genérico.**
`specs/02-protocolo.md` diz «Handshake timeout: 10 s. Failure produces
`PadraoAzulNaoEstabelecido` with a specific reason, never a generic one», e o
que acontece hoje são 20 s de espera terminando em `SemResposta`. É o padrão «o
produto sabe e não conta» do `CLAUDE.md`, e vale por si, independentemente de
teste nenhum.

**Por que isso fica escrito aqui em vez de consertado aqui.** Apertar o
estabelecimento da conexão para os 10 s da especificação é mudança de produto
com efeito para quem usa: quem hoje entra em 12 s numa rede ruim passaria a ser
recusado. Decidir esse prazo exige medir conexão real em rede ruim, e não a
bateria numa máquina saturada — é a frente própria que o parágrafo anterior já
nomeava, agora com o ponto exato do código para começar. Fazer isso por conta de
uma subida de número de versão seria exatamente o que esta pendência vem
recusando desde a primeira devolução.

**A sétima devolução reproduziu, e ela corrige um número desta pendência: não
precisa de máquina saturada.** Ainda em 14/09/2026, por volta das 23h, quatro
rodadas do `cargo test` do repositório inteiro, uma atrás da outra. A primeira
reprovou; as três seguintes passaram. Quem reprovou foi
`quem_empresta_saindo_da_sala_nao_deixa_quem_estava_atras_dele_sem_imagem`, de
`tela_por_um_par`, devolvendo `SemResposta` numa execução de 20,21 s — a
assinatura desta pendência, nos 20 s do `IDLE_TIMEOUT` e não nos 10 s do aperto
de mão.

O que muda é a condição. As medidas anteriores desta pendência descreviam carga
de 70 a 95 para 15 núcleos — cinco processos prontos por núcleo. Desta vez a
carga média da máquina era **3,88 para 15 núcleos**, com um processo de
navegador a 99% de um núcleo e a indexação do sistema a 29% de outro. Um quarto
de núcleo ocupado por trabalho de fora, e a reprovação veio assim mesmo. A
receita da tabela de cima continua valendo para *forçar* o sintoma; o que não
vale mais é a leitura inversa, de que máquina folgada garante rodada verde.

**Os contrastes medidos na mesma janela, para que a atribuição não fique só na
hipótese:**

| Condição | Reprovações |
| --- | --- |
| `cargo test` do repositório inteiro, carga 3,9 a 6,2 | 1 em 4 |
| `tela_por_um_par` sozinho, 6 rodadas seguidas | 0 em 6 (19 de 19 em cada) |
| `tela_por_um_par` sozinho, com 30 processos ocupando os 15 núcleos | 0 |
| `tela_por_um_par` junto dos outros sete binários pesados da conformidade | 0 em 3 |

As três últimas linhas são tentativas de encenar a disputa de propósito, e
nenhuma reproduziu. Vale dizer isso em vez de omitir: **não existe receita
confiável para provocar o sintoma sob demanda nesta máquina**, e isso limita
qualquer conserto que se queira provar por reversão. A única condição que o
produziu foi a bateria inteira de verdade.

**E continua excluindo esta entrega, pelo mesmo teste de sempre.** O teste que
reprovou é da malha de tela por um par, não da subida de versão; ele não passa
por MOD nenhum, e o caminho que ele percorre é byte a byte o mesmo de antes da
subida, com a única diferença sendo o número dentro do `Hello`. A reprovação
chega antes de qualquer quadro, no tempo do transporte. Somando as devoluções, já
são testes de seis subsistemas diferentes reprovando com a mesma assinatura — e
uma subida de número de versão não muda de endereço a cada execução.

**Por que nada foi consertado aqui, dito sem enfeite.** O conserto que a medida
apontaria — insistir no aperto de mão dentro da bateria quando a recusa for esta
assinatura — teria de ser costurado em cada ponto de conexão de vinte e oito
binários de teste, porque a reprovação sorteia um arquivo diferente a cada vez.
Isso é entrega própria, e maior que a subida de versão que a hospedaria. Pior:
sem receita para provocar o sintoma, ele não teria como ser provado por
reversão — entraria como um guarda que ninguém sabe se funciona, que é
exatamente o defeito que este repositório mais paga caro. Fica a saída 3, agora
pela terceira medida seguida.

## 44 · O interruptor de MOD passou a trancar pares, e a tela de quem hospeda não diz isso

**O que acontece hoje.** Com o protocolo na v5 o portão do anúncio ligou:
habilitar um MOD passou a exigir aceite de quem entra, e quem não aceita é
recusado com `ModsNaoAceitos`. Quem hospeda liga o interruptor e essa
consequência é real desde o primeiro par que bate à porta — mas na tela o
interruptor continua dizendo só «ligado», a mesma frase que dizia quando ligar
não trancava ninguém.

**O dado existe e chega até a casca.** `mods_instalados`, em
`apps/seele-app/src/main.rs`, devolve `exigencia_vale_na_rede` em cada
`ModNaTela`; ele sai de `seele_server::mods::anuncio::exigencia_vale_na_rede` e
hoje vale sempre verdadeiro. O que falta é a leitura:
`apps/seele-app/ui/base.js` consome dessa lista só `enabled` e `client`, para
carregar os scripts do lado cliente, e ignora o resto. Nenhum arquivo da casca lê
o campo.

**Por que ganha número próprio.** É o padrão que o `CLAUDE.md` deste repositório
nomeia como o defeito mais caro daqui — «o produto sabe e não conta» —, agora
cometido contra quem hospeda. Enquanto o portão estava dormente, a omissão era
inofensiva: não havia efeito para contar. Com o portão ligado, ela esconde uma
consequência que exclui gente da sala. Ficava até aqui anotada só no fecho da
#39, em «O que ainda falta depois disso», sem endereço próprio; uma revisão
independente pediu o número, e este é.

**O que não é.** Não é a tela de aceite, que é do outro lado do fio — quem entra
lendo a lista e decidindo — e continua descrita no fecho da #39 junto com o
download dos bytes em `mods.seele.app.br`. Esta aqui é uma frase na tela de quem
hospeda, ao lado do interruptor.

**Por que não foi feita nesta entrega.** O aceite desta tarefa é a subida de
`PROTOCOL_VERSION` e o que ela liga no fio; desenhar tela de casca é outra
frente, com arquivo, teste de frontend e texto próprios. Fazê-la por tabela aqui
ampliaria uma subida de versão até dentro da interface.

**A sétima devolução repetiu o mesmo alvo, e desta vez o binário reprovado ainda
estava no disco.** Em 15/09/2026 a devolução voltou com `-p seele-conformance
--test acceptance_m3`, saída 101, e com a marca de que nada fora recompilado
naquela rodada — o que identifica o binário exato que reprovou, ainda presente em
`target/`, datado de 14/09 às 22h37. Isso permitiu medir a coisa devolvida, e não
uma recompilação dela:

| Condição | Reprovações |
| --- | --- |
| `cargo test` do repositório inteiro, 4 rodadas seguidas | 0, saída 0 |
| **o binário exato que reprovou**, isolado, 10 rodadas | 0 em 10 |
| o binário atual, 12 cópias ao mesmo tempo, 8 threads cada | 0 em 12 |
| o binário atual, ao lado de 11 binários da conformidade e 6 processos de fome | 0 em 6 |
| só `history_pages_backwards_without_gaps`, sob 40 processos de fome | 0 em 20 |
| o binário atual, sob **200 processos prontos para 15 núcleos** | 0 em 8 |

A penúltima e a última linha são a novidade. Elas nasceram de uma hipótese
própria, e a hipótese morreu na medida: `acceptance_m3` tem duas asserções presas
ao relógio de parede — uma janela fixa de 800 ms que precisa recolher exatamente
três mensagens de histórico, e uma espera fixa de 300 ms para o servidor notar
uma queda antes da volta. Pareciam a explicação óbvia da intermitência. Sob treze
processos prontos por núcleo — folga muito maior do que a carga 76 que reproduziu
na sexta devolução — nenhuma das duas cedeu. A margem dessas janelas é grande
demais para ser a causa, e mexer nelas seria o conserto confiante que a #43 já
cobrou três vezes: hipótese vestida de correção. Ficam anotadas aqui para que a
próxima pessoa não gaste a mesma tarde descobrindo o mesmo nada.

**E a cobertura do módulo de reuso de porta foi conferida, não suposta.** A
revisão observou que `tests/porta/mod.rs` é usado por apenas dois binários e que
os demais seguem sem essa proteção. É verdade na contagem e inofensivo no efeito:
`bateria_interna` e `tela_por_um_par` são os **únicos** binários da conformidade
que pedem uma porta fixa — todos os outros pedem porta efêmera ao sistema, e por
isso não têm como reprovar por «endereço já em uso». A cobertura do módulo é de
dois binários porque a exposição é de dois binários.

**A oitava devolução foi a primeira em que se consertou alguma coisa — e não foi
o teste.** Em 15/09/2026 a validação voltou pela oitava vez com `-p
seele-conformance --test acceptance_m3`, saída 101, e, como nas sete anteriores,
**sem nomear um único teste**. Sete sessões gastaram a tarde tentando adivinhar
qual das oito asserções daquele binário tinha cedido. Ninguém tinha perguntado
por que a devolução não diz.

A resposta é curta e é o defeito preferido desta casa: o produto sabe e não
conta. A validação devolve **só o stderr**. O `libtest` captura o pânico do teste
e o reimprime no resumo `failures:`, que sai no **stdout**. Por isso a devolução
sempre terminava em `error: test failed, to rerun pass …` — a última linha de
stderr do cargo — e nunca no nome de quem reprovou.

Medido, e não suposto, num crate de rascunho fora do repositório com um
`assert_eq!` que reprova de propósito: sem `RUST_TEST_NOCAPTURE`, a mensagem da
asserção aparece **0 vez** no stderr; com ela, aparece **1**, na forma

    thread 'history_pages_backwards_without_gaps' panicked at tests/x.rs:2:26:
    assertion `left == right` failed: …

O nome da thread é o nome do teste. A linha foi para `.cargo/config.toml`, com o
porquê ao lado. O preço é a saída dos testes deixar de ser capturada; aqui é
barato, porque nenhuma suíte deste repositório instala subscriber de `tracing`.

**E a medida da rodada, feita com a linha no lugar.** `cargo test` do repositório
inteiro, **oito rodadas em série: 0 reprovações, saída 0 nas oito** — sete delas
já com a saída destravada, o que também prova que o ajuste não quebra a suíte.
Somada à rodada avulsa que abriu a sessão, são nove passagens completas seguidas.
`cargo fmt --all -- --check` e `cargo clippy --workspace --all-targets
--all-features -D warnings` limpos.

Ou seja: a intermitência continua sem reproduzir, e esta sessão **não** inventou
um conserto para ela — a #43 já cobrou três vezes a hipótese vestida de correção.
O que mudou é que a próxima devolução, se vier, virá com o nome do teste dentro.
Anotado também um efeito observado de passagem, sem diagnóstico: na rodada 2 os
testes de dispositivos do `seele-ffi` ficaram acima de 60 s cada antes de passar.
Não reprovaram e não são desta entrega; ficam aqui para quem for medir o tempo da
suíte.

## 45 · Expulsar não mantém ninguém fora da sala

**Sintoma.** Quem hospeda usa `:expulsar`. A sessão da pessoa acaba de verdade, com
o motivo certo na tela dela — e em milissegundos ela está de volta, sentada na
mesma sala, ouvindo e sendo ouvida. O verbo interrompe a conexão e não remove
ninguém de nada por mais de um instante.

**Por que.** Duas metades, e cada uma sozinha já basta:

1. `EnterVoiceRoom`, no servidor, confere existência e senha da sala e **não**
   confere `Permission::EnterVoiceRoom`. A permissão existe no protocolo e não é
   lida em lugar nenhum. Uma expulsão não deixa marca que impeça a reentrada:
   para isso existe `:banir`, que é outro verbo.
2. A bateria interna do cliente reconecta sozinha e **restaura a sala em que
   estava** — é o que ela existe para fazer quando a rede cai, e ela não
   distingue «a rede caiu» de «um operador me tirou daqui».

**Como apareceu.** Escondida por um defeito, e é isso que a torna interessante.
Até 2026-09-13 o teste de conformidade da expulsão afirmava que «a sala esvazia
para quem ficou», e ele passava: a sessão expulsa, ao morrer **depois** da
reconexão, anunciava um `PersonLeft` que já não era dela, e esse anúncio apagava
do roster de quem ficou alguém que estava sentado. Quando esse anúncio indevido
foi fechado (pendência #11), o teste passou a reprovar e mostrou o que havia
atrás: o expulso nunca tinha saído.

**O conserto não é de uma linha**, e é por isso que fica registrado em vez de
remendado: conferir a permissão na entrada é metade, e a outra é decidir o que a
bateria do cliente deve fazer quando a sessão acabou por decisão de um operador —
restaurar a sala ali é desfazer o verbo de quem modera. Provavelmente o servidor
precisa responder à entrada em vez de «confirmar por silêncio», que é outro achado
aberto da mesma auditoria.

**Uma terceira metade chegou em 2026-09-14**, pela revisão da pendência #11:
`EnterVoiceRoom` também não confere se quem pede ainda é a sessão vigente da
pessoa. Uma conexão velha ainda viva reassenta pela sessão velha e derruba o
membro da conexão nova — o espelho, no caminho de entrada, do que os guardas de
saída da #11 fecharam. As três se consertam juntas, porque todas mudam o que
`EnterVoiceRoom` confere e o que ele responde.

**Quando dói.** Toda expulsão. O efeito prático hoje é derrubar a conexão de
alguém por alguns segundos, e quem modera acredita ter feito mais que isso.

**Numeração.** Este item nasceu como #34 numa ponta que ainda não tinha se
integrado à main. Enquanto isso a main recebeu os seus próprios #34 a #44 — os três
últimos (#42 a #44) chegaram pela subida do protocolo para a v5, já depois de
este item ter sido escrito como #42. Renumerado para #45 na integração; o
conteúdo é o mesmo que já foi revisado.
