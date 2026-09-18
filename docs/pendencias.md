# Pendências conhecidas

O que está quebrado ou frouxo e ainda não foi resolvido. Ordenado por quanto
atrapalha na prática, não por dificuldade.

Uma entrada que fecha **não sai da lista e não é renumerada**: os números são
citados de fora — "pendência #9" aparece em `docs/` e em `specs/` — e renumerar
faria cada citação apontar para outra coisa. Ela fica no lugar, marcada como
fechada, com a data e com o que a substituiu.

## O que a v0.11.0 fechou, e o que ela não fechou

**Escrito em 2026-09-17**, ao fechar a versão. É um índice, não uma entrada:
cada item aponta para a seção que tem a história.

### Fechadas nesta versão

| # | o quê |
|---|---|
| 11 | a sessão velha apagava a nova ao morrer |
| 22 | MODs estavam desenhados e não construídos |
| 29 | a conformidade reprovava sob a carga da própria suíte |

### Consertadas, e sem confirmação de campo

| # | o quê | o que falta |
|---|---|---|
| 31 | trocar de fone ou microfone exigia reiniciar | alguém trocando o fone num Windows, no meio de uma conversa |

A distinção não é formalidade. Nenhuma máquina de integração contínua tem duas
placas de som, e o que os testes fingem é a abertura do aparelho. Escrever
«fechada» sem a confirmação seria afirmar o que não foi medido — e este
repositório já pagou caro por isso.

### Abertas, e nomeadas de propósito

- **38 — o job `windows-2022` nunca rodou.** O workflow existe escrito e
  conferido, e ninguém o viu executar. A tarefa de fechamento da v0.11.0 registra
  que a execução real foi **adiada por decisão explícita** naquela rodada, e nada
  nesta sessão mudou isso: nenhum push foi feito, então o workflow continua sem
  ter rodado nem uma vez. É a pendência que mais custa hoje, porque três das
  outras dependem dela para sair do «não dá para conferir daqui».

  **E agora ela tem preço medido.** No fechamento da v0.11.0 a bateria passou
  aqui e reprovou na máquina Windows de quem opera — o Git de lá converte LF em
  CRLF no checkout, e uma assinatura vale sobre bytes. Quem descobriu foi a
  pessoa, rodando à mão o que o job devia ter rodado a cada push. O `cargo`
  parou no primeiro alvo e contou dois testes; um clone com
  `core.autocrlf=true` mostrou 47, incluindo o orquestrador de release inteiro.
  O conserto é um `.gitattributes`, e cabe em três linhas. O que custou não foi
  consertar: foi não ter quem visse.

  **E logo atrás dele veio o segundo.** Com o fim de linha resolvido, a bateria
  de lá avançou até o `seele-lancador` e parou noutro:
  `o_lancamento_de_cada_versao_aponta_para_os_dados_dela` afirmava
  `dados_novos.ends_with("dados/2.0.0-teste")` sobre uma `String` — comparação
  de texto, com o separador escrito à mão. No Windows o valor termina em
  contrabarra, e o teste reprovava com o produto certo. Dois defeitos de teste
  em sequência, nenhum deles do produto, os dois invisíveis nesta máquina.
- **33 — tela entre duas máquinas Windows.** Quatro suspeitos eliminados por
  medida; o que sobra exige sessão gráfica.
- **37 — a entrada em sala se confirma pelo silêncio.** Ela previu o que
  aconteceu nesta versão, e por isso ficou **mais cara**: ver a seção.
- **44 — o interruptor de MOD tranca pares** e a tela de quem hospeda não diz.
- **46 — o caminho de Windows do som da tela** não é compilado no Mac de quem
  fechou esta versão.

### O que esta versão **não** carrega, e podia parecer que carrega

- ~~**O indexador de MODs não está no ar.**~~ **Subiu em 2026-09-17**, em
  `mods.seele.app.br`, com o MESA 1.2.0 como primeiro MOD avaliado. O catálogo
  inteiro é assinado e o aplicativo o confere contra a chave que veio compilada
  dentro dele. **O que continua valendo:** não há assinatura por autor — o que
  prova os bytes de cada versão é o hash do conteúdo, e o que fixa o que foi
  avaliado é o commit.
- **Versões lado a lado não valem no Windows.** O que o catálogo publica ali é
  um instalador `.exe`, que instala por cima da instalação única da máquina.
  Guardar versões ao lado precisa de um pacote que se abra numa pasta.

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

> **Isto deixou de valer mais abaixo, na mesma entrega.** A leitura acima estava
> certa sobre `assentar` e errada sobre a conclusão: o que faltava guardar não era
> `assentar`, era quem manda `assentar` rodar. A sétima porta foi fechada — veja
> «A sétima porta, que era o único apontamento vivo» e a prova de ligação logo
> depois. A conexão velha não responde mais ao anúncio de mudança de sala.

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

**A prova de ligação da sétima porta, que era o único achado de código vivo.**
A revisão aprovou e apontou, com razão, o que este arquivo não tinha visto: o
guarda da sétima porta — o anúncio de mudança de sala — tinha teste de unidade
sobre a função pura `server::a_mudanca_de_sala_e_desta_conexao`, e **nada
reprovava se a chamada sumisse do tratador do evento**. As outras seis portas têm
o guarda dentro de `desassentar`, onde o compilador não deixa esquecê-lo; esta
tem a pergunta solta num braço de `match`, e apagá-la compilava e passava em
tudo — inclusive no teste de unidade dela mesma, que continua verde afirmando
sobre uma função que ninguém mais chama. Era a única das sete sem prova de
ligação, e a comparação estava à mão: o fim de tela já tinha a dele.

O buraco foi fechado com um guarda do mesmo feitio, que lê o próprio arquivo:
`o_anuncio_de_mudanca_de_sala_pergunta_a_sessao_antes_de_assentar`. Ele acha o
braço `Event::PersonMoved`, caminha até o `assentar` que o braço executa e cobra
três coisas na ordem que faz o guarda valer: que a pergunta pela sessão apareça
**antes** do `assentar`, que haja saída do braço quando a resposta é não — porque
perguntar sem desviar não guarda nada — e que o braço continue existindo com um
`assentar` dentro, para que o dia em que ele mudar de forma reprove pedindo
releitura em vez de passar calado apontando para o vazio.

**A folga que a primeira versão desta prova tinha, e como ela foi fechada.** A
revisão seguinte apontou que a terceira cobrança era frouxa: a prova aceitava
*qualquer* `continue;` literal dentro do braço, inclusive um desvio alheio ao
guarda. Ela cobria a ordem, não a ligação. Agora a prova acha em que nome a
resposta da pergunta foi guardada — não pelo `let` mais próximo, que seria um
`let` de dentro do bloco da própria pergunta, mas pelo `let` cuja expressão ainda
estava aberta quando a pergunta foi feita, medido pelas chaves por fechar — e
exige a forma que liga as duas coisas: `if !<nome> {` com um `continue;` dentro
do bloco que ele abre. Um `continue;` em qualquer outro ponto do braço deixou de
servir de álibi.

**Reversão provada nesta rodada, e não citada — duas vezes.** Apagado do tratador
o bloco inteiro que pergunta a sessão e sai quando a resposta é não — o código
literal de antes do conserto —, o teste reprova com a mensagem esperada: «o braço
`Event::PersonMoved` chama `assentar` sem antes perguntar
`a_mudanca_de_sala_e_desta_conexao`», nomeando a linha 3101. E, para provar
justamente a folga que a revisão apontou, um segundo experimento: mantida a
pergunta e mantido o `continue;` no mesmo lugar, trocada apenas a condição
`if !e_minha` por uma que nunca é verdadeira — o guarda passa a ser texto morto,
que é como ele ficaria numa refatoração distraída. A versão frouxa passaria
verde; esta reprova dizendo «pergunta a sessão e guarda a resposta em `e_minha`,
mas não sai do braço por ela». Restaurado o arquivo byte a byte nas duas vezes,
passa.

**A bateria desta rodada, nas crates certas e com os números inteiros.** A
validação anexada voltou, pela sexta vez, a ser a das ferramentas de publicação
(66 testes, todos passando, nenhuma crate desta pendência): não há reprovação a
consertar ali — há a lacuna de anexo, e é ela que esta medida supre.
`seele-server` e `seele-proto` juntas: **722 passando, nenhuma falha, nenhum
ignorado** em 12 binários, sendo 474 de unidade do servidor (473 de antes mais o
guarda de ligação novo) e 230 de `seele-proto`. `seele-conformance` inteira,
serializada em 31 binários: **154 passando, nenhuma falha, um ignorado** — o que
exige duas máquinas, ignorado desde antes desta pendência —, com `desassentar`
verde em 28,04 s e saída 0. `cargo fmt --all --check` e `cargo clippy -p
seele-server --all-targets` limpos. A bateria foi refeita inteira **depois** de a
prova de ligação ser endurecida, e não herdada da medida anterior: os mesmos
números, agora com a prova que não aceita mais desvio solto.

**As quatro provas de reversão, reexecutadas ao vivo em 2026-09-15 e não
citadas.** A revisão aprovou e deixou um só apontamento de prova: quem revisa não
altera fontes, então a metade que sustenta a asserção do roster ficara apenas
documental. Ela foi refeita aqui, uma de cada vez, com a árvore restaurada e
conferida limpa entre uma e outra:

| o que foi desarmado | o que reprovou | mensagem |
|---|---|---|
| o filtro por sessão ao desocupar assento (`vacate_everywhere_da_sessao`) | `desassentar`, 28,04 s | «a sessão velha, ao morrer, apagou a nova do roster do anfitrião: ["anfitriao"]» |
| a pergunta de sessão na saída pedida à sala (`VoiceRoomCommand::Leave`) | `desassentar`, 28,03 s | «a tarefa da sala não registrou nenhuma saída de sessão velha barrada», com `saida_de_sessao_velha: 0` |
| o bloco inteiro do sétimo guarda no tratador de `PersonMoved` | a prova de ligação | «chama `assentar` sem antes perguntar `a_mudanca_de_sala_e_desta_conexao`» |
| só a condição do sétimo guarda, virando texto morto | a prova de ligação | «guarda a resposta em `e_minha`, mas não sai do braço por ela» |

Cada uma sozinha derruba o seu teste, e com o guarda de volta o mesmo teste passa
— `desassentar` em 28,04 s, a prova de ligação instantânea. As duas primeiras
levam os mesmos 28 s antes de reprovar porque esperam o prazo de desistência da
conexão velha; reprovar depressa seria sinal de que mediram outra coisa. A árvore
foi conferida idêntica byte a byte depois de cada restauração.

**Uma quinta tentativa de reversão que *não* reprovou, e o que ela ensina.**
Desarmado `Presentes::e_a_vigente` — o guarda da reserva de assento e do sétimo
ponto —, `desassentar` continuou passando. Não é guarda inútil: é que aquele
caminho não é o que o teste de ponta a ponta encena, e a cobertura dele é de
teste de unidade mais a prova de ligação. Fica registrado em vez de ficar
implícito, porque é exatamente a distinção que a revisão anterior já tinha
apontado sobre o sétimo ponto. **Deixou de valer mais abaixo**: o caminho de
ponta a ponta que faltava foi escrito, e esta reversão hoje reprova.

**A bateria desta retomada, depois de um reinício do aplicativo.**
`seele-server` e `seele-proto` juntas: **722 passando, nenhuma falha, nenhum
ignorado**, em 12 binários. `seele-conformance` inteira, serializada e com
`--no-fail-fast`: **154 passando, nenhuma falha, um ignorado** — o das duas
máquinas —, saída 0.

**A intermitência que apareceu no meio disso, medida e não deduzida.** Numa
primeira execução serializada da conformidade,
`moderacao::um_pessoa_comum_e_recusado_pelo_server_e_nao_pela_casca` reprovou com
`SemResposta` — que não é asserção de comportamento nenhuma: é o prazo por
candidato de `Enlace::conectar` (4 s) queimando sem resposta no laço de retorno.
O teste não é tocado por esta entrega. Medido em vez de suposto: sozinho, passou
**5 de 5** em 0,36 s cada; o binário inteiro em série passou **3 de 3**; e a
bateria de conformidade inteira, refeita, fechou limpa. É a instabilidade de
máquina disputada já registrada em #29, e não regressão deste guarda.


**Os dois achados da revisão que aprovou, fechados com caminho de ponta a
ponta.** A revisão aprovou a entrega e deixou dois apontamentos, os dois sobre
*cobertura* e não sobre código errado. Os dois foram fechados aqui, e os dois com
prova de reversão ao vivo — que é o que distingue «há um teste» de «o teste
prende».

O primeiro: a remoção **por sala**, `Occupancy::vacate_da_sessao`, não era
exercida contra um servidor de verdade. O teste de queda silenciosa mede uma
conexão que morre **calada**, e quem morre calado nunca pede nada — todo o
desmonte dele passa pela varredura da lotação inteira. A remoção por sala só é
alcançada por quem se lembra de onde estava, isto é, por `LeaveVoiceRoom` e pelo
anúncio de mudança de sala. Revertendo o filtro daquela função, a bateria inteira
continuava verde, e a revisão mediu isso em vez de supor.

O caminho que faltava não precisa de silêncio nenhum: precisa de duas conexões da
mesma pessoa vivas ao mesmo tempo, com a **velha ainda falando** — o aparelho que
ficou aberto, a janela antiga que o dono fecha depois de já ter voltado por
outra. A velha pede para sair da sala de que ela se lembra; a nova está sentada
nessa mesma sala. O teste novo
`a_saida_pedida_pela_conexao_velha_nao_tira_da_sala_a_conexao_nova` encena isso e
custa 2,2 s, porque aqui ninguém espera o servidor desistir de coisa alguma.

O segundo: o guarda da **escrita** — a reserva de assento da carência — tinha
teste de unidade e prova de ligação, e nenhum caminho de ponta a ponta. Era
exatamente a quinta tentativa de reversão registrada acima, a que *não* reprovava.
Agora reprova. O caminho também dispensa o tempo ocioso: a conexão velha dentro
da sala, a nova fora dela, e a velha se despedindo. O teste novo
`a_conexao_velha_nao_guarda_assento_para_quem_ja_voltou_por_outra` mede as duas
coisas — que o guarda recusou a reserva, e que a volta seguinte **não** acorda
dentro de uma sala que ela não pediu.

Junto com ele entrou o segundo contador de `Desassentamentos`,
`reservas_obsoletas`, pela mesma razão que existe o primeiro: este guarda acerta
**não** escrevendo, e do lado de fora «a reserva foi recusada» e «esta conexão
nunca chegou ao fim» são o mesmo silêncio. Contador separado, e não somado ao de
conexões velhas: dois guardas diferentes somados fazem um teste passar por causa
do outro.

**As duas provas de reversão novas, executadas em 2026-09-15 e não citadas.**

| o que foi desarmado | o que reprovou | mensagem |
|---|---|---|
| o filtro por sessão na remoção por sala (`Occupancy::vacate_da_sessao`) | `a_saida_pedida_pela_conexao_velha_nao_tira_da_sala_a_conexao_nova`, 2,23 s | «a saída pedida pela conexão velha tirou a nova do roster do anfitrião: ["anfitriao"]», com o servidor mostrando só `anfitriao/sessão 1` e zero piscadas do enlace |
| a pergunta de sessão na reserva de assento (`reservar_o_assento_da_carencia`) | `a_conexao_velha_nao_guarda_assento_para_quem_ja_voltou_por_outra`, 10,0 s | «o servidor não recusou reserva nenhuma» |

E, porque um contador que só prova a si mesmo não prova nada, a segunda reversão
foi medida uma segunda vez com a asserção do contador desligada de propósito:
sobra o efeito, e o efeito também reprova — «a volta caiu dentro da sala 1 sem ter
pedido». Quer dizer: sem aquele guarda, a pessoa que sai de uma sala de propósito
e reconecta é mesmo re-sentada nela, e não é só um número que muda. Os dois
arquivos foram restaurados byte a byte depois de cada medida, com `md5`
conferido.


**O terceiro achado, o único que restava: a sétima porta provada por efeito, e
não por leitura do próprio código.** A revisão que aprovou registrou a ressalva
sem travar a entrega, e ela era justa: das sete portas, seis são presas por um
teste que derruba conexão contra um servidor de verdade, e a sétima — o anúncio
de mudança de sala — era presa por um teste que lê o arquivo-fonte. Uma prova de
ligação diz *onde a chamada está*; ela não diz *o que acontece com quem usa* se a
chamada sumir, e é frágil ao dia em que o braço do `match` mudar de forma.

O caminho de ponta a ponta agora existe:
`o_anuncio_de_mudanca_de_sala_nao_e_respondido_pela_conexao_velha`. O anfitrião
cria a sala de destino, o visitante entra na primeira por duas conexões da mesma
conta — a velha sem se despedir —, espera-se o servidor trocar o assento para a
nova, e só então o operador move a pessoa. Custa 3,1 s: não há tempo ocioso a
esperar, porque a velha está viva e falando.

**A escolha que faz este teste valer, e que custou a primeira versão dele.** O
estrago óbvio — a pessoa acabar sentada com o `ssrc` e o canal da conexão morta —
depende de qual das duas conexões escreveu por último, que é escalonamento: um
teste apoiado nisso passaria verde em metade das execuções mesmo com o defeito
inteiro dentro. O que não depende da ordem é a **contagem de eventos**:
`assentar` anuncia `PersonJoined` sempre, ao terminar. Com o guarda, quem hospeda
vê a pessoa entrar na sala nova uma vez; sem ele, duas — em qualquer ordem. O
teste conta `PersonJoined`, e não fotografa estado. Depois vem a asserção de
sensibilidade sobre o contador `conexoes_velhas`, porque «uma entrada só» também
seria o que se veria num mundo onde o anúncio nunca chegou à conexão velha.

**A prova de reversão, executada em 2026-09-15 e não citada.**

| o que foi desarmado | o que reprovou | mensagem |
|---|---|---|
| a condição do sétimo guarda no tratador de `PersonMoved`, deixando a conexão velha responder ao anúncio | `o_anuncio_de_mudanca_de_sala_nao_e_respondido_pela_conexao_velha`, 3,1 s | «o anfitrião viu o visitante entrar 2 vez(es) na sala de destino, e a mudança foi uma só», com zero piscadas do enlace |

Repetida três vezes seguidas com o guarda desarmado, ela reprovou as três com o
mesmo número — que é a medida da promessa de independência da ordem, e não uma
afirmação sobre ela. Restaurado o arquivo, passa. A prova de ligação por leitura
do código **fica**: ela continua sendo o que reprova pedindo releitura no dia em
que o braço mudar de forma, e agora ela não é mais a única coisa que prende a
sétima porta. (Este último parágrafo caducou na oitava revisão, abaixo: ela saiu.
Reprovar «pedindo releitura» é exatamente o problema, porque uma reescrita
equivalente e correta a faria reprovar sem haver defeito.)

**A bateria que acompanha este fechamento, nas crates tocadas e medida aqui.**
A validação anexada à revisão voltou a ser a das ferramentas de publicação, que
não toca crate nenhuma desta pendência — de novo lacuna de anexo, e não
reprovação. Medido nesta árvore, com a sétima prova de efeito dentro:
`cargo test -p seele-server` fecha com saída 0 — **490 passando, nenhuma falha,
nenhum ignorado**, sendo 474 de unidade e o resto dos binários de integração.
`cargo test -p seele-conformance --no-fail-fast -- --test-threads=1`, a bateria
inteira serializada, fecha com saída 0 — **157 passando, nenhuma falha, um
ignorado**, o das duas máquinas, ignorado desde antes desta pendência. O binário
`desassentar` passa com os **quatro** testes em 37,05 s. `cargo fmt --all
--check` e `cargo clippy -p seele-server -p seele-conformance --all-targets`
saem limpos, sem um aviso.

**A oitava revisão aprovou e deixou dois apontamentos de diagnóstico; os dois
viraram código.** Nenhum dos dois era erro do produto — o guarda já defendia
corretamente as sete portas. Os dois eram sobre *o que o repositório diz quando
alguém quebra o guarda*, que é a diferença entre um vermelho que ensina e um
vermelho que manda investigar o lugar errado.

**O primeiro: o contador media o veredito, e devia medir a encenação.** O número
`conexoes_velhas` era incrementado a partir do resultado da remoção — «não removi
nada e a pessoa continua aqui, logo o guarda trabalhou». Parece a mesma coisa que
«uma conexão velha morreu depois de a pessoa já ter voltado», e não é. Desarmado
o filtro de sessão de `Presentes::saiu`, a remoção volta a devolver verdadeiro, o
contador nunca anda, e a prova de reversão reprovava na asserção **(0)**,
dizendo *«aumente a margem ou confira o tempo ocioso do transporte»* — apontando
para o relógio quando o defeito era o estado apagado. Pior: a asserção **(b)**, a
única que descreve esse defeito, nunca chegava a ser avaliada.

A pergunta passou a ser feita **antes** de remover, por `Presentes::e_a_vigente`,
e o contador passou a registrar a situação e não o desfecho: *uma conexão velha
chegou ao fim enquanto a pessoa estava aqui por outra*. Isso é verdade com o
guarda armado e com ele desarmado — de propósito. Um contador de sensibilidade
que morre junto com o guarda não serve para nada: ele sequestra toda prova de
reversão para si. A asserção **(b)** também ganhou o diagnóstico que lhe faltava:
quem o servidor acha que está presente, e por qual sessão.

**O segundo: a prova de ligação por leitura do próprio código saiu.** Ela lia
`session.rs` com `include_str!` e cobrava a forma literal `if !<nome> {` com um
`continue;` dentro. Funcionava, e reprovava se a chamada sumisse — mas reprovaria
igualmente diante de uma reescrita equivalente e correta do braço do `match`, o
que é um vermelho falso e caro. Ela existia porque a sétima porta não tinha prova
de efeito; desde a sétima revisão ela tem, contra um servidor de verdade. Com a
prova de efeito no lugar, a prova de forma deixou de ser a única coisa que
prendia a sétima porta e passou a ser só a mais frágil. O *porquê* dela ficou
como comentário no braço do evento, apontando para o teste que hoje o prende.

**As duas provas de reversão foram reexecutadas aqui, uma de cada vez, com
restauração conferida — não citadas.**

| o que foi desarmado | o que reprovou | mensagem |
|---|---|---|
| o filtro de sessão de `Presentes::saiu` | `quem_reconecta_antes_de_o_servidor_desistir_da_conexao_velha_continua_no_roster_do_host`, 28,0 s | agora na asserção **(b)**: «a sessão velha tirou o visitante dos presentes, e ele está conectado agora por outra conexão», com a lista dos presentes no servidor — `["1/sessão 1"]`, sem o visitante — junto |
| a condição do sétimo guarda no tratador de `PersonMoved` | `o_anuncio_de_mudanca_de_sala_nao_e_respondido_pela_conexao_velha`, 3,1 s | «o anfitrião viu o visitante entrar 2 vez(es) na sala de destino, e a mudança foi uma só», com zero piscadas do enlace |

A primeira linha é o conserto medindo-se a si mesmo: antes desta rodada, esse
mesmo desarme reprovava em (0) falando de margem. Agora reprova em (b) falando do
estado apagado, que é o defeito.

**Uma observação honesta sobre como desarmar.** A primeira tentativa de desarmar
o sétimo guarda o fez por `if false && !e_minha`, e a bateria **pendurou** em vez
de reprovar — precisou ser interrompida. Desarmado por `if false`, reprova em
3,1 s com a mensagem acima. A diferença não foi investigada porque é do desarme e
não do produto; fica registrada para que a próxima pessoa que refizer a prova
desarme pela condição inteira e não pela metade dela.

**A bateria desta rodada, nas crates tocadas.**

Medida nesta árvore, depois das duas correções acima e com o arquivo restaurado
byte a byte das duas reversões:

`cargo test -p seele-server` fecha com saída 0 — **489 passando, nenhuma falha,
nenhum ignorado**, sendo 473 de unidade e o resto dos binários de integração.
São 16 unidades a menos que na rodada anterior, e isso não é cobertura perdida:
é a prova de ligação por leitura do código-fonte saindo, com a prova de efeito
dela já no lugar desde a sétima revisão.

`cargo test -p seele-conformance --no-fail-fast -- --test-threads=1`, a bateria
inteira serializada, fecha com saída 0 — **157 passando, nenhuma falha, um
ignorado**, o das duas máquinas, ignorado desde antes desta pendência. O binário
`desassentar` passa com os **quatro** testes em 36,9 s.

`cargo fmt --all --check` e `cargo clippy -p seele-server -p seele-conformance
--all-targets` saem limpos, sem um aviso.

**O que esta medida não cobre.** Ela é das duas crates tocadas, e não do
workspace inteiro; um `cargo test` sem `-p` cai nas ferramentas de publicação e
não diz nada sobre esta pendência — foi o que aconteceu com o anexo das duas
revisões anteriores, e é por isso que os comandos ficam escritos aqui com o `-p`
dentro.

**Refeita depois do reinício do aplicativo, e pelo mesmo motivo.** A nona revisão
aprovou e deixou uma ressalva que não é sobre o produto: a saída de validação
anexada à tarefa vinha outra vez da bateria de empacotamento, sem um único teste
das crates tocadas. Medido de novo nesta árvore, com os comandos com `-p`
dentro: `cargo test -p seele-server` fecha com saída 0 e **489 passando, nenhuma
falha, nenhum ignorado**; `cargo test -p seele-conformance -- --test-threads=1`
fecha com saída 0 e **157 passando, nenhuma falha, um ignorado** — o das duas
máquinas, ignorado desde antes desta pendência. Dentro dela, o binário
`desassentar` passa com os **quatro** testes em 37,00 s, e o principal, o da
queda silenciosa, está entre eles. `cargo fmt --all --check` e `cargo clippy -p
seele-server -p seele-conformance --all-targets` saem limpos, sem um aviso. Os
números batem, um a um, com os das rodadas anteriores: nada mudou de
comportamento no reinício, e agora a medida está anexada onde o anexo da tarefa
não a trazia.

**A décima revisão aprovou sem achado de código, e a medida foi refeita pela
terceira vez pelo mesmo motivo.** Os quatro apontamentos da revisão são
observações, e nenhum pede mudança: dois registram que as provas de reversão são
documentais para quem revisa sem editar fontes, e que o teste da queda silenciosa
não passa por ausência de evento — ele afirma antes os dois contadores; um
registra que `assentar` continua tirando a pessoa da sala anterior sem conferir
sessão, por necessidade, com o risco fechado na porta de quem chama; e o último
repete o que já está escrito ao lado de `Presentes::chegou`, que é o único ponto
da família apoiado em ordem de `handshake` e não em identidade de sessão. Esse
último fica como está de propósito: mudá-lo pede ordenar identificadores de
sessão, que é decisão de outro escopo, e o comentário no próprio ponto já diz o
que fazer no dia em que a ordem do `handshake` mudar.

O que a revisão apontou de fato foi, de novo, o anexo: a saída de validação da
tarefa voltou a ser a bateria das ferramentas de publicação, 66 testes que não
compilam uma linha das crates desta pendência. Medido aqui, nesta árvore, com o
`-p` dentro: `cargo test -p seele-server` fecha com saída 0 e **489 passando,
nenhuma falha, nenhum ignorado** (473 de unidade e o resto dos binários de
integração); `cargo test -p seele-conformance --no-fail-fast -- --test-threads=1`
fecha com saída 0 e **157 passando, nenhuma falha, um ignorado** — o das duas
máquinas, ignorado desde antes desta pendência. Dentro dela, o binário
`desassentar` passa com os **quatro** testes em 37,01 s. `cargo fmt --all
--check` e `cargo clippy -p seele-server -p seele-conformance --all-targets` saem
limpos. Os números batem, um a um, com as duas rodadas anteriores.

**A décima primeira revisão aprovou e deixou um só ponto de cobertura; ele virou
teste de efeito.** A ressalva era esta, e era justa: das portas do desmonte, o
**fim de tela por sessão** era a única presa só por testes de unidade e por uma
prova que lê o próprio arquivo-fonte. Na mesma entrega uma prova desse feitio
tinha saído do guarda de mudança de sala por ser frágil a reescritas equivalentes
e corretas — e a que sobrava para as telas herdava a mesma fragilidade. Uma prova
de leitura diz *onde a chamada está*; ela não diz *o que a pessoa deixa de ver* se
a chamada mudar de nome.

O caminho de ponta a ponta agora existe:
`a_conexao_velha_nao_encerra_a_tela_que_a_nova_abriu`, o quinto teste do binário
`desassentar`. Ele não precisa do tempo ocioso, pelo mesmo motivo que os testes da
saída pedida e da reserva não precisam: basta que a conexão que morre já não seja
a vigente. A velha entra na sala e transmite; a nova sobe, toma a sala — e aí a
tela da velha acaba de propósito, por `assentar`, que é a troca de sala levando
embora a transmissão anterior da pessoa — e abre a dela; então a velha se
despede. Custa 2,3 s.

Ele mede as duas metades que juntas separam «o evento indevido não saiu» de «saiu
e o cliente o ignorou»: o servidor ainda tem a transmissão da conexão nova
registrada com a sessão dela, e o anfitrião ainda a desenha. Antes das duas, a
asserção de encenação de sempre — o contador `conexoes_velhas` andou, isto é, uma
conexão velha chegou mesmo ao fim depois de a pessoa já estar aqui por outra.
Isto é a metade **ver** do relato original: «não ouço nem vejo esse amigo». A voz
é a tarefa da sala, e os outros testes a prendem; a imagem é este registro.

**A prova de reversão, executada em 2026-09-15 e não citada.**

| o que foi desarmado | o que reprovou | mensagem |
|---|---|---|
| o fim de tela por sessão no desmonte (`encerrar_telas_desta_conexao` trocado por `encerrar_telas_da_pessoa`) | `a_conexao_velha_nao_encerra_a_tela_que_a_nova_abriu`, 2,28 s | «a conexão velha, ao morrer, encerrou a transmissão que a conexão nova tinha aberto», com `left: None`, `right: Some((VoiceRoomId(1), ScreenId(2)))` e «Transmitindo na sala 1 agora: []» |

Restaurado o arquivo, o teste passa em 2,28 s. A prova por leitura do código
**fica**, e agora com o escopo certo escrito no lugar dela: ela deixou de ser o
que prende o efeito e passou a ser só o que cobra classificação de uma chamada
nova, em função que ninguém listou.

**A bateria desta rodada, nas crates tocadas e com o `-p` dentro.** A validação
anexada voltou, pela sétima vez, a ser a das ferramentas de publicação — 66
testes que não compilam uma linha destas crates. É lacuna de anexo, e não
reprovação. Medido aqui: `cargo test -p seele-server` fecha com saída 0 e **489
passando, nenhuma falha, nenhum ignorado** (473 de unidade e o resto dos binários
de integração). `cargo test -p seele-conformance --no-fail-fast --
--test-threads=1` fecha com **158 passando, nenhuma falha, um ignorado** — o das
duas máquinas —, e o binário `desassentar` passa com os **cinco** testes em
39,19 s. `cargo fmt --all --check` e `cargo clippy -p seele-server -p
seele-conformance --all-targets` saem limpos.

**E a intermitência que apareceu numa segunda execução da conformidade inteira,
medida em vez de deduzida.** Repetida a bateria serializada, ela fechou com 157
passando e **uma** falha: `moderacao::expulsar_acaba_com_a_sessao_e_deixa_voltar`,
com `Error: SemResposta` e 21,46 s no binário que costuma levar 1,6 s. Não é
asserção de comportamento nenhuma: é o prazo por candidato de `Enlace::conectar`
queimando sem resposta, a mesma instabilidade de máquina disputada já registrada
em #29, e o arquivo não é tocado por esta entrega. Medido: o binário sozinho
passou **5 de 5**, em 1,6 s a 1,7 s cada, e a primeira execução da bateria inteira
já o tinha passado com os seis. Fica escrito porque o contrário — não contar a
falha vermelha que se viu — é o que este repositório já pagou caro.

**A revisão seguinte aprovou, e a nota de projeto que ela deixou virou ordem de
asserção.** Dos dois apontamentos, o primeiro é a intermitência de
`moderacao::expulsar_acaba_com_a_sessao_e_deixa_voltar` numa execução da
conformidade inteira — a mesma de #29, já escrita no parágrafo acima, com o
binário sozinho passando 15 de 15 na medida da própria revisão. O segundo é o que
mudou código, e é justo: ao desarmar o guarda da sala para provar que ele importa,
a primeira linha vermelha do teste da queda silenciosa falava do **contador**
interno, e o teste morria ali, antes de chegar às asserções de efeito. A prova de
que o efeito também cede estava escrita neste arquivo, mas ninguém a via correndo.

A correção é de ordem, e não de conteúdo: a asserção de **mecanismo** — «a tarefa
da sala barrou uma saída de sessão velha» — passou para depois das asserções de
**efeito** nos dois testes onde ela vinha antes. A leitura dos contadores continua
onde estava, porque ela é a linha de base do descarte `not_a_member` e, no teste
da saída pedida, também é a barreira que segura o teste até o pedido ter chegado à
sala. O que mudou é só quando o veredito é cobrado. Quem investiga lê primeiro a
frase do usuário e depois o número.

**A prova de reversão refeita com a ordem nova, em 2026-09-15.** Desarmado o
guarda de `VoiceRoomCommand::Leave` em `voice_room.rs` — a pergunta «esta sessão
ainda é a desta pessoa?» forçada a falso, de modo que a saída velha passe:

| o que reprovou | onde | mensagem |
|---|---|---|
| `quem_reconecta_antes_de_o_servidor_desistir_da_conexao_velha_continua_no_roster_do_host` | `desassentar.rs:501`, asserção (c) | «a voz do visitante deixou de ser encaminhada: a sessão velha o tirou da tarefa da sala ao morrer. É a metade do defeito que o usuário relata como “não ouço nem vejo esse amigo”.» |
| `a_saida_pedida_pela_conexao_velha_nao_tira_da_sala_a_conexao_nova` | `desassentar.rs:727`, asserção de mecanismo | «a tarefa da sala não registrou nenhuma saída de sessão velha barrada… e aí as asserções acima passaram por acaso, e não por defesa.» |

Os dois números dizem coisas diferentes, e é de propósito. No primeiro teste a
reprovação é de **efeito**, que era o que a revisão cobrava. No segundo ela
continua sendo de mecanismo porque ali o estado de roster e assento é defendido
por outro guarda — o de `Occupancy` — que não foi desarmado nesta prova: com a
sala aberta e a lotação fechada, o que cede é só a participação na tarefa de
mídia, e quem a mede é o primeiro teste. Restaurado o arquivo, os cinco testes do
binário passam em 39,28 s.

**A revisão seguinte aprovou com uma ressalva de diagnóstico, e ela virou a mesma
ordem de asserção no teste da reserva.** A ressalva: o teste da reserva de assento
usava `reservas_obsoletas` como **barreira**, e esse contador mede o **veredito** do
guarda — ele só anda quando o guarda recusa. Desarmado o guarda, a espera estourava
e a primeira linha vermelha dizia «o servidor não recusou reserva nenhuma», que
manda investigar prazo e transporte quando o defeito é a pessoa acordar dentro de
uma sala que não pediu. É o mesmo erro já corrigido em `conexoes_velhas`, repetido
no segundo contador.

A correção é de ordem e de qual número se lê. A barreira passou a ser
`conexoes_velhas`, que mede a **encenação**: uma conexão chegou ao fim já não sendo
a vigente da pessoa dela. Esse número anda com o guarda da reserva armado **e**
desarmado — o guarda nem participa dessa contagem —, então ele segura o teste até a
velha ter morrido de verdade sem sequestrar a prova de reversão. A leitura de
`reservas_obsoletas` foi para o fim do teste, depois das asserções de efeito, como
verificação de mecanismo: se ela reprovar sozinha, o caminho da reserva de carência
não foi exercido e o teste deixou de cobrir o que diz cobrir.

**A prova de reversão refeita com a ordem nova, em 2026-09-15.** Desarmada a
pergunta de sessão de `reservar_o_assento_da_carencia`, em `server.rs`, pela
condição inteira:

| o que reprovou | onde | mensagem |
|---|---|---|
| `a_conexao_velha_nao_guarda_assento_para_quem_ja_voltou_por_outra` | `desassentar.rs:855`, asserção de efeito | «a volta caiu dentro da sala 1 sem ter pedido: a conexão velha guardou o assento da carência depois de a pessoa já ter voltado por outra conexão, e a reserva valeu para a volta seguinte.» |

Antes desta rodada, esse mesmo desarme reprovava na linha do contador falando de
prazo. Agora reprova na linha que descreve o estrago, e o contador só é cobrado
depois. O arquivo foi restaurado e conferido por `md5` — a soma bate com a de antes
da medida, byte a byte.

**A medida que fecha a pendência, executada em 2026-09-15 neste worktree.** A
revisão anterior aprovou e deixou uma ressalva de anexo: a saída de validação que
vinha junto era a das ferramentas de publicação — 66 testes que não compilam
nenhuma das crates tocadas aqui. Falta anexada não é falta corrigida, então a
rodada foi refeita por crate, com os números abaixo:

| conferência | resultado |
|---|---|
| `cargo fmt --all --check` | limpo |
| `cargo clippy -p seele-server -p seele-conformance --all-targets` | saída 0, nenhum aviso |
| `cargo test -p seele-server` | 489 passaram, 0 reprovaram, 0 ignorados |
| `cargo test -p seele-conformance --no-fail-fast -- --test-threads=1` | 158 passaram, 0 reprovaram, 1 ignorado (o de duas máquinas) |
| binário `desassentar` dentro da rodada acima | 5 de 5, em 39,25 s |

A conformidade roda serializada de propósito: a pendência #29 registra que a
suíte reprova sob a carga da própria suíte, e os testes desta bateria esperam
`IDLE_TIMEOUT` inteiro. O ignorado é o que exige duas máquinas e não tem par nesta.

**Uma cobertura que não existe, e o que segura o lugar dela.** A sexta porta de
saída, a de `VoiceRoomDeleted`, não tem teste de conformidade próprio. Ela não
está desprotegida: passa pela mesma `desassentar(Saida::Sala(Some(id)))` que o
teste da saída pedida já exercita, e o tipo do parâmetro impede escrever a chamada
sem o identificador de sessão. O que falta é a prova de ponta a ponta de que apagar
uma sala inteira não derruba quem já reconectou — isso continua aberto, e está aqui
escrito para não ser descoberto de novo por acidente.

**A revisão de 2026-09-15 aprovou e deixou uma leitura sobre qual asserção cobre
o quê.** Ela desarmou o filtro de sessão de `Presentes::saiu` e observou que a
asserção **(a)** — o visitante continua no roster do anfitrião — continuou
passando; quem reprova ali é a **(b)**, a da lista de presentes. Isso não é
buraco de cobertura, e o teste não foi alterado por causa disso: o roster que o
anfitrião desenha vem dos eventos de lotação, defendidos pelo guarda de
`Occupancy`, e a lista de presentes vem de outro estado, defendido pelo filtro de
`Presentes`. São **duas metades independentes** do mesmo defeito relatado — sumir
da sala e sumir da lista de todo mundo —, e cada uma tem a sua asserção e a sua
prova de reversão registradas acima. Desarmar uma metade faz reprovar só a
asserção dessa metade; é exatamente o que se espera de guardas separados, e é o
que torna a linha vermelha legível para quem investiga. Se as duas reprovassem
juntas, o teste não saberia dizer qual guarda cedeu.

**A bateria do workspace inteiro, executada em 2026-09-15 neste worktree.** As
rodadas anteriores foram por crate, de propósito (a conformidade serializada, por
causa de #29). A revisão de validação rodou `cargo test` no workspace inteiro, e
essa rodada foi refeita aqui para que o número anexado seja o mesmo comando:

| conferência | resultado |
|---|---|
| `cargo test` (workspace inteiro, paralelismo padrão) | saída 0 · 74 binários de teste · 1 908 testes passaram, 0 reprovaram |
| binário `desassentar` dentro dessa rodada | 5 de 5, em 28,14 s |
| `cargo fmt --all --check` | saída 0, limpo |
| `cargo clippy --workspace --all-targets` | saída 0, nenhum aviso |

Nesta máquina, nesta rodada, a intermitência de #29 não apareceu nem com a
conformidade em paralelo. Isso não a desmente — ela é de carga, e depende da
máquina —, mas registra que a bateria completa fecha verde do jeito que a
validação a executa.

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
## 22 · Fechada em 2026-09-17 · MODs estão desenhados e não construídos

> **Fechada na v0.11.0.** O que faltava era tudo: hoje o servidor sobe os MODs
> habilitados e os executa, o anúncio e o aceite atravessam o protocolo — quem
> entra vê identidade, versão, hash, repositório e alcance antes de qualquer
> byte —, um MOD obrigatório recusado barra a entrada, e a ponte de pedidos deixa
> um MOD responder a uma pessoa só, com a identidade escrita pelo servidor.
>
> **O que não fechou junto, e não é isto aqui:** o indexador não está no ar, então
> nenhum MOD de terceiro tem como chegar a ninguém ainda. E o interruptor de MOD
> tem o seu próprio defeito aberto, na pendência 44.
>
> **Estreitado em 2026-09-17.** O indexador subiu, e a primeira frase deixou de
> valer. A segunda continua: a 44 foi tratada na mesma rodada da auditoria de
> experiência, e o que ela cobrava — dizer a consequência antes da troca — está
> feito e **não** foi testado em campo com anfitrião e convidado.
>
> O texto original fica abaixo, como a regra desta página manda.

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

## 29 · Fechada em 2026-09-16 · A conformidade reprova sob a carga da própria suíte

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

### Fechada em 2026-09-16 · uma permissão que cada teste segura pela sua duração

**O conserto.** `crates/seele-conformance/tests/vaga/mod.rs` — um semáforo de
processo com um guarda de RAII. Cada um dos **139 testes assíncronos** do crate
o toma na primeira linha (`let _vaga = vaga::minha();`) e o devolve no `Drop`,
o que inclui o caminho do pânico: um teste que reprova não leva a vaga consigo
e não tranca a suíte atrás dele. O diff é uma linha de `mod vaga;` por arquivo e
uma linha de guarda por teste, em 25 arquivos, mais o módulo novo — nenhuma
linha removida, e nenhum arquivo de `crates/seele-server/` tocado. Os testes que
não levantam servidor (`cancelamento`, `soak_audio`, `voz_na_reconexao`, e os
seis de `estados` e um de `quarto` que só leem código-fonte) não pedem vaga: não
pagam por um problema que não têm.

**Por que pela duração do teste, e não em volta do `Daemon::bind`.** Tentativas
anteriores erraram exatamente aqui, e o levantamento de custo da nona medição
(§43) já dizia por quê: *«o que estoura é o aperto de mão, não a abertura da
porta»*. Um guarda solto logo depois do `bind` devolveria a vaga antes de o
cliente tentar conectar, e o `IDLE_TIMEOUT` de 20 s continuaria estourando.

**Por que aqui e não em `RUST_TEST_THREADS`.** Aquela chave vale para o
workspace inteiro e serializaria também os ~489 testes do `seele-server`, que
não têm este problema e não têm por que pagar por ele. `-j1` no `cargo test`
também estava descartado, e pelo motivo que a §38 registra: `-j` governa
compilação, não quantos binários já compilados rodam juntos — a medição que
alegava o contrário não se reproduziu e foi revertida.

**A prova de que a permissão está agindo, e não apenas contando.** O
`acceptance_m5` é a régua da §29: em série levava **23,02 s** e passava 15 de
15; em paralelo terminava em **~20,01 s** e reprovava um teste em duas de cada
três rodadas. Com a vaga armada, dentro do `--workspace` sob carga, ele passa
**15 de 15 em 23,04 s** — o número da série, e não o do paralelo. É essa
coincidência de dois centésimos que distingue «serializou» de «compilou».

Vale registrar o que **não** serve de prova, porque custou uma rodada de
revisão: a **ordem** em que os resultados são impressos continua embaralhada
mesmo com a vaga armada. O `libtest` cria todas as threads de teste de uma vez;
a vaga não impede que nasçam, impede que **rodem juntas** — quem espera no
`Condvar` acorda na ordem que o sistema quiser. Ordem de impressão não mede
serialização; tempo de parede mede.

**As cinco rodadas do aceite**, `cargo test --workspace`, na mesma máquina de 15
núcleos, cada uma com **8 queimadores de CPU** ligados de propósito antes do
`cargo` e desligados depois (carga média de 9,9 a 20,5 no minuto de cada
rodada). Gerador `motor/final29.sh`; registros em
`docs/evidencias/pendencia-29/arquivo-entregue/verde10_*.resumo.log`:

| Rodada | Saída | Tempo de parede | Conjuntos | Testes | `acceptance_m5` |
| --- | --- | --- | --- | --- | --- |
| `verde10_1` | 0 | 260,49 s | 73 | 1.883 | 15/15 em 22,97 s |
| `verde10_2` | 0 | 260,06 s | 73 | 1.883 | 15/15 em 22,96 s |
| `verde10_3` | 0 | 260,46 s | 73 | 1.883 | 15/15 em 23,01 s |
| `verde10_4` | 0 | 261,18 s | 73 | 1.883 | 15/15 em 23,06 s |
| `verde10_5` | 0 | 259,76 s | 73 | 1.883 | 15/15 em 23,00 s |

Cinco de cinco, sem uma reprovação por prazo, com **zero linhas `FAILED`** nos
cinco registros. A dispersão de parede entre elas é de **1,4 s**: a suíte voltou
a ter um tempo, em vez de uma distribuição. E o `acceptance_m5` fecha em
**23,0 s com 15 de 15** nas cinco — o número da **série** medido em 2026-08-31,
não o do paralelo (~20,01 s, encostado no prazo). A permissão está agindo, e
está agindo nas cinco.

**Esta tabela substitui uma anterior, e o motivo importa.** A versão publicada
antes desta (293,46 / 271,21 / 272,08 / 271,31 / 271,96 s) foi medida sobre a
**versão anterior** do `crates/seele-conformance/tests/vaga/mod.rs`, SHA-256
`73bdeb64…`, sem o prazo da fila e sem o abandono coletivo. O arquivo entregue é
o `1515219068…`. Número medido num arquivo não vale para outro, e a evidência
já dizia isso enquanto esta seção seguia citando os números velhos. As cinco
rodadas acima imprimem o hash do arquivo sob medição no próprio registro, para
que a confusão não possa se repetir em silêncio.

**E há uma sexta rodada armada, sob a mesma carga, que reprovou.** Ela está
descrita com nome, número e registro em «A sexta rodada armada, que reprovou»,
mais adiante nesta mesma seção. Quem citar as cinco tem de citar a sexta: a taxa
honesta desta entrega é **1 reprovação em 6 rodadas armadas**, contra cerca de
2 em 3 antes do conserto.

**O que estas cinco rodadas não provam, dito aqui e não só lá embaixo.**
Queimador de processador, sozinho, **não reproduz** o defeito da §29: há rodadas
*desarmadas* que saíram 0 sob essa mesma carga. O que reproduz é a carga do
`motor/rodada4.sh`, que soma aos queimadores seis laços rodando binários de
conformidade **fora do `cargo`** — e sob ela, desarmado, o defeito volta
(`revws7_4`, adiante). Logo estas cinco provam **ausência de regressão, o custo
e que a permissão está agindo**; a robustez sob a carga que causava a reprovação
quem prova é o ciclo de reversão. Quem citar esta tabela — o comentário do
`ci.yml` inclusive — tem de citar as duas coisas.

**Onde está o registro bruto.** Em `docs/evidencias/pendencia-29/`, dentro do
repositório: as cinco rodadas, o alcance no servidor, o ciclo de reversão e os
geradores que produziram cada medida. Ficavam em `/tmp`, que o sistema apaga sem
avisar, e uma tabela cuja fonte sumiu volta a ser afirmação. O `README.md` de lá
diz o que foi cortado de cada registro e o que é verbatim.

**A serialização alcança somente a conformidade, e isto é medido, não
argumentado.** A medida está sobre o arquivo entregue, e não repete o erro que
a terceira revisão apontou — o par `antes/depois` que esta seção publicava saiu
da versão aposentada do módulo, e um número medido num arquivo não vale para
outro. Ele foi refeito com um desenho que nem precisa de duas árvores
(`motor/alcance2.sh`):

Primeiro, a prova estrutural: `git diff --stat 3c59eec -- crates/seele-server/
.cargo/ Cargo.toml Cargo.lock` não imprime uma linha. Nada do que o
`seele-server` compila mudou, e as duas medidas abaixo rodam o **mesmo
binário** — `seele_server-b79839397f752fd8`, com o identificador impresso nos
dois registros. Depois, o contrafactual, que é o que de fato prova: em vez de
comparar duas árvores, mede-se o preço que o critério 2 **proíbe** pagar.

| Rodada | Como | `seele_server` (lib, 453 testes) | Conjunto |
| --- | --- | --- | --- |
| `srv2_paralelo` | a árvore como é entregue | **8,01 s** | 29,34 s |
| `srv2_serie` | o mesmo binário com `--test-threads=1` | **24,47 s** | 46,63 s |

Serializar aqueles 453 testes custaria **três vezes mais** — 8,01 s viram
24,47 s, e o conjunto do crate ganha 17,3 s. A árvore entregue marca o número do
paralelo. Se a fila tivesse escapado para o `seele-server`, a linha de cima
seria a de baixo, e não é. As duas rodaram sob 8 queimadores e saíram 0;
registros em `arquivo-entregue/srv2_paralelo.resumo.log` e `srv2_serie.resumo.log`.

### A prova de reversão — o critério 3, sobre o arquivo entregue

**Leia esta subseção antes da seguinte, e saiba por quê.** A subseção logo
abaixo — «O primeiro ciclo de reversão, medido na versão aposentada do módulo» —
descreve um ciclo real, mas medido sobre a versão **anterior** do
`crates/seele-conformance/tests/vaga/mod.rs` (SHA-256 `73bdeb645b92…`), sem o
prazo da fila, sem o abandono coletivo e sem o aviso por `stderr`. Ela fica
registrada porque o que ela ensinou sobre a natureza da carga continua valendo;
ela **não** é a prova do critério 3, e a versão anterior deste documento a
apresentava como se fosse, sem dizer em que arquivo tinha sido medida. Quem
lesse saía com a prova errada. A prova do critério 3, medida sobre o arquivo que
está sendo entregue, é esta:

```
1515219068d8ee61ce24645e9c8d161afda2237d7a0bbb8a39664c963473d749  crates/seele-conformance/tests/vaga/mod.rs
1515219068d8ee61ce24645e9c8d161afda2237d7a0bbb8a39664c963473d749  docs/evidencias/pendencia-29/arquivo-entregue/vaga_ARMADO7.backup.rs
```

O backup contra o qual a restauração é conferida está **dentro do repositório**,
no caminho acima, e não mais só em `/tmp` — que é apagado sem aviso, e onde uma
conferência byte-a-byte não é reproduzível por terceiros. Ciclo completo
(gerador: `docs/evidencias/pendencia-29/motor/reversao7.sh`, registro:
`arquivo-entregue/reversao7_driver.log`), todo em `cargo test --workspace`:

| rodada | permissão | saída | parede | o que mostra |
| --- | --- | --- | --- | --- |
| `revws7_1` | desarmada | 0 | 210,81 s | — |
| `revws7_2` | desarmada | 0 | 185,29 s | — |
| `revws7_3` | desarmada | 0 | 186,37 s | — |
| `revws7_4` | desarmada | **101** | 114,39 s | `tela_por_um_par` reprova, **conjunto em 20,32 s** |
| *restauração* | — | — | — | `cmp` sem diferença, SHA-256 igual ao backup |
| `pos_rev7` | restaurada | 0 | 284,83 s | volta a passar |

A reprovação de `revws7_4` é a forma exata que o aceite pedia: em `--workspace`,
com a permissão desarmada, e **por prazo** — `Error: SemResposta`, conjunto
encerrando em 20,32 s, encostado no `IDLE_TIMEOUT` de 20 s do transporte.

**E o diferencial armado/desarmado, no mesmo arquivo e no mesmo dia**, que é a
prova que não depende de sorte nenhuma — mesmos binários, mesma carga, mesma
máquina, mudando só a largura da permissão:

| binário | desarmada (`revws7_1..3`) | armada (`verde8_1..4`, `pos_rev7`) |
| --- | --- | --- |
| `acceptance_m2` (9 testes) | 0,95 / 0,98 / 0,97 s | **3,17 – 3,19 s** |
| `acceptance_m5` (15 testes) | 20,01 / 20,02 / 20,04 s | **22,92 – 23,06 s** |
| soma dos 28 binários | 183,4 / 181,3 / 182,4 s | **254,3 – 275,8 s** |

**Uma armadilha do método, que custou uma reprovação de revisão e fica escrita.**
Desarmar a permissão é uma edição na árvore de trabalho, e os geradores de
medição desarmam, medem e só então restauram. Se a sessão termina no meio do
ciclo, a árvore fica com `VAGAS = usize::MAX` — a serialização desligada,
`cargo clippy` reprovando em `absurd_extreme_comparisons`, e toda a evidência
descrevendo um arquivo que não é o que está lá. Foi exatamente o que uma revisão
independente encontrou, e ela estava certa em reprovar. Quem for medir de novo:
**confira `const VAGAS` e o SHA-256 da árvore contra o backup antes de dar
qualquer coisa por entregue** — é uma linha de `shasum`, e é a diferença entre
uma entrega e uma entrega desarmada.

### O primeiro ciclo de reversão, medido na versão aposentada do módulo

**Tudo nesta subseção foi medido sobre `73bdeb645b92…`, a versão anterior do
módulo.** Os números não valem para o arquivo entregue; o que vale, e é por isso
que ela fica, é o que ela ensinou sobre que tipo de carga reproduz esta
pendência.

`VAGAS` é a largura da permissão. Trocar o `1` por `usize::MAX` a desarma sem
apagar uma linha — `while *ocupadas >= VAGAS` nunca bloqueia, e `vaga::minha()`
vira um contador sem efeito. É assim que se prova que ela serve.

**Primeiro, a prova de que a trava está agindo**, que não depende de sorte
nenhuma. Na mesma rodada de `--workspace`, sob a mesma carga:

| Binário | desarmada | armada |
| --- | --- | --- |
| `acceptance_m2` (9 testes) | 0,96 s | **3,18 s** |
| `acceptance_m5` (15 testes) | 20,02 s | **23,04 s** |
| soma dos 28 binários | 95,3 s | **171,3 s** |

O `acceptance_m5` desarmado para em **20,02 s** — encostado no `IDLE_TIMEOUT` de
20 s do transporte, que é a beirada do precipício desta pendência. Armado, vai a
23,04 s, que é o número da série medido em 2026-08-31. A trava age.

**Depois, a reprovação de volta.** Com a permissão desarmada e a máquina sob
carga, `cargo test -p seele-conformance` reprovou na **segunda** rodada:

```
thread 'o_server_enche_sem_passar_do_teto_e_a_mensagem_diz_que_o_arquivo_expirou'
panicked at crates/seele-conformance/tests/anexos.rs:266:28:
o anexo 18 não foi publicado
test result: FAILED. 9 passed; 1 failed; ... finished in 7.14s
error: test failed, to rerun pass `-p seele-conformance --test anexos`
```

O `ate(...)` de `anexos.rs` espera **5 s** por um evento; sob os apertos de mão
concorrentes o décimo oitavo anexo não chegou dentro deles. Na mesma máquina, no
mesmo minuto e com a mesma carga, a rodada de referência **armada** saiu 0. E
antes desta, com a permissão desarmada, já se tinha visto a assinatura clássica
de ~20 s: `tela_por_um_par::destruir_o_enlace_encerra_o_caminho_do_par_e_quem_\
emprestava_volta_a_servir`, reprovando com *«o servidor apontou um par e não o
contou como ocupado»*, conjunto encerrando em 18,96 s.

**Restaurada e conferida.** O arquivo voltou do backup e foi conferido
byte-a-byte — `cmp` sem diferença e SHA-256 igual dos dois lados:

```
73bdeb645b92ab543a0d12839fb89870799b5b830f9dcf55813266ee9ef0b501  (backup, versão aposentada)
73bdeb645b92ab543a0d12839fb89870799b5b830f9dcf55813266ee9ef0b501  (árvore, versão aposentada)
```

E, restaurada, voltou a passar: `cargo test -p seele-conformance` sob a mesma
carga de 12 queimadores, **saída 0**, 28 conjuntos, nenhuma reprovação. Os três
passos do ciclo, portanto, na ordem: referência armada 0 → desarmada reprova →
restaurada por backup, conferida por hash, 0 de novo.

**O que a reversão ensinou de novo, e que vale mais que a própria prova:
queimador de processador não reproduz esta pendência.** Sete rodadas de
`--workspace` com a permissão desarmada — quatro delas com **30** queimadores
numa máquina de 15 núcleos, carga média 49 — saíram **todas 0**. A reprovação só
voltou quando a contenção foi de **disco e de troca de páginas**, não de CPU. É
coerente com o mecanismo: o que estoura é espera de rede e de banco, e um
laço apertado de CPU não disputa nem uma coisa nem outra. Quem for medir esta
pendência de novo deve saturar **entrada e saída**, e não somente o processador.

**Um custo que eu mesmo paguei, e que fica registrado.** Ao montar essa carga de
disco, um glob mal escrito meu abriu processos até encher a tabela do sistema;
a máquina entrou em troca de páginas (quase 8 milhões de páginas trocadas) e
ficou **treze vezes mais lenta** por mais de uma hora — a mesma rodada de
conformidade que leva 191 s passou a levar 2.478 s. As medidas desta seção
marcadas como «sob carga de 12 queimadores» são as da máquina sã; as da prova de
reversão são as da máquina degradada, e por isso **os tempos de parede das duas
não se comparam entre si** — só dentro de cada par. O que se compara, e é o que
a prova precisa, é armada contra desarmada no mesmo minuto.

Nessa mesma máquina degradada, a primeira rodada **armada** depois da
restauração também reprovou — em
`tela_por_um_par::retirar_o_consentimento_de_emprestar_encerra_o_repasse_e_o_\
servidor_reassume`, com *«a paciência acabou esperando»*. Não é a §29
ressurgindo: é de novo uma espera de tempo absoluto dentro do teste, numa
máquina dezenove vezes mais lenta que o normal. A conferência foi refeita depois
que a máquina voltou ao normal — calibrada pelo `acceptance_m2`, que marcou
**3,17 s** contra os 3,18 s da medida sadia —, e aí a rodada armada saiu 0. Fica
registrado do jeito que aconteceu, e não do jeito que seria mais limpo contar —
e o registro bruto dessa rodada reprovada está arquivado com os outros, em
`docs/evidencias/pendencia-29/reversao/pos_armado.resumo.log`, para que ela não
seja conhecida apenas por esta prosa.

**E uma terceira causa, descoberta aqui, que a vaga também não cura.** Com 30
queimadores (carga 49, mais de três vezes os núcleos), a suíte **armada**
reprovou em `furo::o_aviso_sai_imediatamente_antes_do_candidato_que_precisa_dele`
(`crates/seele-conformance/tests/furo.rs:242`): um `assert!` que exige que o
`Initial` saia **menos de 600 ms** depois do aviso, e recebeu **7,07 s**.
Serializar não alcança isso — a janela é de 600 ms e o escalonador estava
entregando fatias de segundos. É a mesma família do defeito de `moderacao`
descrito abaixo: janela de tempo absoluta dentro de um teste. Numa máquina
sadia, e nas cinco rodadas do aceite, nenhum dos dois apareceu.

**O custo, escrito mesmo sendo o que é.** Somando o «finished in» dos 28
binários do `seele-conformance` dentro da mesma rodada de `--workspace`:

| | Conformidade | Workspace (parede) |
| --- | --- | --- |
| permissão desarmada (paralelo) | 95,3 s | 195,6 s |
| permissão armada (série) | 171,3 s | 271,9 s |
| **custo** | **+76,0 s (+80 %)** | **+76,3 s (+39 %)** |

As duas colunas foram medidas separadamente e chegam ao mesmo número com três
décimos de diferença, o que é a própria conferência do cálculo: tudo o que a
serialização acrescenta ao workspace, ela acrescenta dentro deste crate e em
nenhum outro lugar. Os valores se repetem entre rodadas dentro de 0,03 s na
coluna da conformidade.

**Setenta e seis segundos é caro, e mesmo assim é o lado barato.** O que se
compra por eles está medido acima: cinco rodadas seguidas saindo 0 em vez de
uma reprovação por rodada, num teste diferente a cada vez. O registro de
2026-09-14 já tinha posto preço no outro lado — «uma revisão independente ter de
gastar parágrafo distinguindo carga de regressão antes de poder aprovar» —, e
esse custo se cobra de terceiros, repetidamente, e não em segundos.

**A sexta rodada armada, que reprovou, e que é o limite honesto desta entrega.**
As cinco do aceite saíram 0. A **sexta** rodada armada sob a mesma carga do
`rodada5.sh` — a rodada de referência do `motor/custo8.sh`, logo depois da
restauração conferida por hash — saiu **101**:

```
     Running tests/moderacao.rs
Error: SemResposta
test um_operador_modera_pessoas_e_nao_o_comandante ... FAILED
test result: FAILED. 5 passed; 1 failed; ... finished in 21.28s
```

É a assinatura da §29, e não adianta chamá-la de outra coisa: conjunto encostado
nos 20 s do `IDLE_TIMEOUT`, e **o mesmo conjunto passa sozinho sob a mesma carga
em 1,4 s, quatro vezes de quatro** (`motor/moderacao3.sh`,
`arquivo-entregue/moder_so_*.resumo.log`: 1,48 / 1,39 / 1,45 / 1,40 s, saída 0).
Não é instabilidade do teste — é contenção que sobrou. O registro da rodada
**não** tem o aviso de fila abandonada, então a permissão estava agindo.

**Por que sobra contenção, e por que ela sobra por desenho.** A permissão
serializa a conformidade **contra ela mesma**. Ela não serializa a conformidade
contra o resto do workspace, e não deve: o critério 2 desta entrega exige
exatamente que os ~489 testes do `seele-server` continuem em paralelo. Enquanto
um teste de conformidade levanta o seu servidor QUIC, os binários do servidor, de
áudio e de vídeo estão rodando ao lado. Com 8 queimadores por cima, isso ainda
alcança os 20 s de vez em quando.

**O número, sem arredondar para o lado bom: 1 reprovação em 6 rodadas armadas**
sob a carga que está dentro do alcance do conserto. Antes do conserto a taxa era
de cerca de 2 em 3, e num teste diferente a cada rodada. A §29 está **muito**
reduzida e **não** está eliminada, e quem citar as cinco rodadas verdes sem citar
a sexta estará citando menos do que este documento sabe.

**O custo medido com carga casada, sobre o arquivo entregue.** A tabela acima
compara duas metades que correram sob cargas diferentes, o que compara cargas e
não permissões. Refeito com as duas metades sob o **mesmo** `motor/rodada5.sh`,
8 queimadores, e sobre o arquivo entregue (gerador `motor/custo8.sh`, registro
`arquivo-entregue/custo8_driver.log`):

| | parede do `--workspace` |
| --- | --- |
| desarmada, 3 rodadas | 204,59 / 190,27 / 181,95 s — média **192,3 s** |
| armada, as 5 do aceite | 259,76 – 261,18 s — média **260,4 s** |
| **custo** | **+68,1 s (+35 %)** |

**O que ainda mereceria medida, e não foi feito aqui:** a vaga é uma só. Duas ou
três provavelmente comprariam parte do tempo de volta sem trazer a reprovação, já
que o estouro é de 20 s e o aperto de mão sozinho leva décimos. Não foi medido,
então não foi feito: `VAGAS` está em um porque é o valor cuja prova existe.

### O risco que a fila criou, e como ele foi fechado

Ele não veio da medição: veio da **revisão independente desta entrega**, que
leu o desenho e apontou o que nenhuma das cinco rodadas podia mostrar. Fica
escrito porque é dívida que este conserto contraiu, e não defeito que ele herdou.

**O risco.** Serializar troca uma reprovação isolada por uma fila, e uma fila
tem um modo de falha próprio: quem toma a vaga e **nunca a devolve**. Na
primeira versão, `vaga::minha()` esperava no `Condvar` sem prazo nenhum. Um
teste que travasse — e a causa 2 logo abaixo documenta exatamente isso, o
CoreAudio que sob acesso restrito não recusa e sim espera — deixaria de ser uma
reprovação isolada e passaria a **estagnar os 138 testes seguintes** até o tempo
limite da CI. Com `--test-threads=1` o efeito era o mesmo, então não é
regressão; mas a partir desta entrega ele vale também para quem roda
`cargo test --workspace` na própria máquina, e isso é novo.

**O conserto, em duas partes.** A primeira: a espera passou a ter prazo, e o que
ele mede importa. **Não** é quanto tempo um teste esperou — o último da fila
espera legitimamente a suíte inteira, minutos — e sim quanto tempo se passou
**sem nenhuma vaga ser devolvida**. Um contador de devoluções distingue «a fila
anda devagar» de «a fila parou». Enquanto ela anda, ninguém desiste. Parada por
`PRAZO_SEM_PROGRESSO` = 180 s, quem espera **escreve no registro da rodada que
vai seguir sem serializar, e segue**. O teste mais longo deste crate leva
segundos, então três minutos parados só acontecem se quem tem a vez não vai mais
sair.

A segunda parte existe porque a primeira não bastava, e quem cobrou foi a
medida. Na versão inicial cada teste tinha de descobrir o travamento por conta
própria, e como quem fura a fila **devolve** a vaga ao terminar, o relógio de
«sem progresso» reiniciava para todos os outros. A fila soltava então **um teste
por prazo, em cascata**: medido com prazo de 3 s e oito testes atrás de um
travado, o último só seguiu aos **18,64 s**
(`destravamento/cascata.log`). Com os 180 s entregues e os 138 testes deste
crate, isso seria um travamento com outro nome, e o texto que prometia volta ao
paralelo estaria mentindo. Agora a desistência é **da fila**: quem desiste marca
`abandonada` e acorda todos, e a partir dali ninguém mais bloqueia. Na mesma
medida, os oito seguem **juntos, aos 3,35 s**.

O resultado é uma degradação e não um conserto, e está dito assim de propósito:
a rodada volta ao comportamento antigo — testes concorrentes, §29 de volta —
**em voz alta**. Alguns servidores a mais são ruins; 138 testes que nunca
reportam nada é pior. Isto é a regra da casa aplicada a uma ferramenta de teste:
o que falha tem de dizer que falhou.

**A prova, em `docs/evidencias/pendencia-29/destravamento/`.** O módulo real
compilado fora do `cargo`, com duas trocas de número e nenhuma de lógica (prazo
de 180 s para 3 s, passo de 5 s para 200 ms, para caber num registro): um dono
toma a vaga e nunca a devolve, **oito** testes entram atrás dele.

| Registro | Permissão | Resultado |
| --- | --- | --- |
| `com_prazo.log` | como está entregue | os oito seguem **juntos**, aos 3,35 s; saída **0** |
| `cascata.log` | desistência individual, a versão anterior | um por prazo: 3,36 s … **18,64 s**; saída **0** |
| `sem_prazo.log` | espera sem prazo, como era no começo | **ninguém** segue; o vigia do programa mata aos 30 s; saída **9** |

A terceira linha é a prova por reversão: com a espera sem prazo de volta, a fila
não anda nunca — que é o que teria acontecido com a suíte inteira. A segunda é a
prova de que a desistência coletiva não é enfeite, e é o registro que contradiz
o que este parágrafo dizia antes de ser medido.

#### O aviso saía por um canal que o `libtest` engole

Este é o achado da **segunda** revisão independente, e é um defeito de verdade,
não de prosa: o aviso acima saía por `eprintln!`. O `libtest` captura a saída de
cada teste e imprime **apenas a dos que reprovam** — e quem desiste de esperar é
um teste que depois **passa**. Ou seja: a fila podia ser abandonada, a rodada
voltar a ser paralela, a §29 voltar com ela, e não haver **uma linha** sobre isso
no registro, nem na CI, que não passa `--nocapture`. A promessa de degradar «em
voz alta» era falsa, e o modo de falha era justamente o que o `CLAUDE.md` deste
repositório nomeia: *«o produto sabe e não conta»*.

Medido com um teste que passa e escreve a mesma frase pelos dois caminhos
(`docs/evidencias/pendencia-29/voz/`):

| execução | `eprintln!` | `writeln!` em `std::io::stderr()` |
| --- | --- | --- |
| `cargo test` — o que a CI roda | **não aparece** | aparece |
| `cargo test -- --nocapture` | aparece | aparece |

A captura do `libtest` vive **dentro das macros** de impressão, que consultam um
destino por thread antes de escrever; `std::io::stderr()` escreve no descritor e
não consulta nada. O aviso passou a sair por `vaga::em_voz_alta`, que é
`writeln!` nesse descritor, com o erro de escrita ignorado de propósito — o pior
caso de um registro que falha tem de ser o silêncio, nunca um pânico dentro do
guarda. A linha `--nocapture` da tabela é a prova por reversão: é exatamente o
que se veria se o aviso voltasse para a macro, visível só para quem pede, e a CI
não pede.

Por que a prova do destravamento não pegou isso: `destravamento/prova.rs` roda
como binário comum, fora do `libtest`, onde `eprintln!` aparece sempre. Uma
prova montada fora do arnês não vê o que o arnês faz — e é por isso que a
medição nova foi feita **dentro** de `cargo test`.

**O que esta prova não cobre.** Ela exercita a fila, não um teste de
conformidade travado de verdade. Reproduzir o travamento real exigiria rodar sob
acesso restrito, que é a causa 2 e está fora deste escopo. O que está provado é
que a fila destrava e que sem o prazo ela não destravava.

### As causas que esta entrega NÃO consertou, com nome próprio

Elas ficam escritas para não serem redescobertas como se fossem a §29, o que já
aconteceu mais de uma vez. O aceite pedia duas; a medição achou **cinco**, e as
cinco estão aqui. A quarta
(`furo::o_aviso_sai_imediatamente_antes_do_candidato_que_precisa_dele`, uma
janela de 600 ms que recebeu 7,07 s com a máquina a três vezes a capacidade)
apareceu durante a prova de reversão acima, é da mesma família da primeira e da
terceira, e está registrada lá.

**1. `moderacao::expulsar_acaba_com_a_sessao_e_deixa_voltar` é instável por si.**
Medido antes desta entrega, no levantamento que a originou: reprovou **1 de 4** execuções *rodando o arquivo sozinho*, sem carga e sem
concorrência — num `assert!` de tempo dentro da função `ate(...)`. Isso não é
contenção de recurso: é o próprio teste. A vaga não alcança este defeito e não
foi desenhada para alcançá-lo. Continua **sem diagnóstico**.

**2. Sob ambiente com acesso restrito, testes de áudio e de rede travam em vez
de falhar.** `a_saida_desta_maquina_abre_como_entrada` e
`uma_saida_nao_responde_configuracao_de_entrada`
(`crates/seele-audio/src/laco.rs`) ficam presos dentro de
`manter_a_saida_tocando` / `playback_devices`: sem permissão de microfone o
CoreAudio **não recusa**, ele espera. `empacotamento` (`apps/seele-app`) faz o
mesmo com rede e chegou a levar **24 minutos** onde leva 16 s. É uma segunda
causa, independente da §29, e atinge qualquer sessão que rode com esse acesso —
foi ela, e não a §29, que produziu registros cortados em validações anteriores.
Esta sessão **não** rodou com acesso restrito: os três testes correram normais
nas cinco rodadas acima, e o único pânico que aparece nos registros é o
proposital de `laco.rs:456` («envenenando de propósito, para o teste»), cujo
teste passa.

**3. `seele-server::alcance::encontro::testes::\
a_janela_fecha_dentro_do_atender_e_nao_so_no_auxiliar` é frágil no tempo, e
ninguém a tinha escrito em lugar nenhum.** Não foi esta sessão que a achou: foi
a revisão independente desta entrega, na primeira rodada de
`cargo test --workspace --all-targets`, que saiu 101 com **53 furos contra um
teto de 60** (`crates/seele-server/src/alcance/encontro.rs:1383`). Passa 3 de 3
rodando sozinha e passou na rodada seguinte da própria revisão.

Tentei reproduzi-la e **não consegui**, o que é um dado e não uma lacuna: 10
execuções do teste sozinho sob 12 queimadores de processador, **10 verdes**. E
na rodada de `cargo test --workspace --all-targets` desta sessão, saída 0, ela
passou. Isto a coloca exatamente onde a §29 já tinha colocado o resto: o que a
derruba não é processador ocupado, é a contenção de disco, rede e banco da suíte
inteira — a mesma que o queimador não produz. Quem for consertá-la não perca
tempo com carga de CPU.

O mecanismo se lê do código, sem precisar reproduzir. O teste dispara
`FUROS_POR_JANELA + 1` = 61 avisos e conta os furos que voltam, com
`timeout(600ms)` em **cada leitura** e `break` no primeiro estouro
(`encontro.rs:1374-1378`). O servidor emite um furo a cada `INTERVALO_DO_FURO` =
120 ms, então a fila normal gasta ~7,3 s de parede e a margem por leitura é de
5×. Numa máquina saturada, um único intervalo de 120 ms que escorregue além de
600 ms encerra a contagem no meio — 53 é exatamente onde a contagem parou, não
um teto que mudou. É a mesma família da causa 1: prazo curto contra máquina
lenta, dentro do próprio teste.

**Por que não foi consertada aqui.** A restrição desta tarefa proíbe tocar em
`crates/seele-server/`, que tem alterações de outra tarefa ainda por integrar.
E o conserto não é a vaga: a permissão é do crate de conformidade e não alcança
os testes de unidade do servidor — nem deveria, porque serializar os ~453 testes
do `seele-server` é exatamente o que o critério 2 desta entrega proíbe. O
conserto certo é do teste, e provavelmente é trocar o `break` na primeira
leitura lenta por um prazo global, que o teste já tem (40 s) e não usa como
único critério.

**O que isso custa ao `ci.yml`.** O job agora roda um comando só, e esta
fragilidade pode derrubá-lo. Ela não foi introduzida aqui — vivia escondida
atrás do `--exclude seele-conformance`, que nunca a excluiu —, mas a partir
desta entrega ela é a instabilidade mais provável do comando único, e é por isso
que está nomeada.

**5. `tela_por_um_par::a_reconexao_ao_servidor_nao_deixa_a_conexao_velha_\
atrapalhar_o_par_novo` reprova com a permissão armada, e é o achado da terceira
revisão independente.** Ela não veio da minha medição: veio de a revisão ler os
registros que eu **anexei e não li até o fim** — a quinta das cinco rodadas do
aceite tinha reprovado, e a tabela que eu publiquei listava as outras quatro.
O conserto dessa omissão está em
`docs/evidencias/pendencia-29/arquivo-entregue/README.md`, com todas as rodadas
armadas em ordem e sem seleção.

O dado é este: **2 reprovações em 8 rodadas armadas** de `cargo test --workspace`
sobre o arquivo entregue, e as duas **no mesmo teste**. Isso é o que a distingue
da §29, e é o motivo de ela ser causa e não recaída: a §29 reprovava *um teste
diferente a cada rodada* — foi essa aleatoriedade que a tornou «não-evidência».
Aqui o endereço é fixo, e um endereço fixo é diagnosticável.

As duas reprovações têm naturezas diferentes dentro do mesmo teste, o que
provavelmente quer dizer que o teste é longo demais e não que haja um defeito só:

| rodada | onde reprovou | mensagem | conjunto |
| --- | --- | --- | --- |
| `verde8_5` | `tela_por_um_par.rs:3155`, a barreira de encenação | «o servidor apontou um par e não o contou como ocupado: não havia caminho de par para a queda substituir» | 52,35 s |
| `verde9_1` | espera de conexão | `Error: SemResposta`, com «running for over 60 seconds» antes | 68,59 s |

O de `verde8_5` é o mais informativo, e aponta para uma **corrida no teste, não
no servidor**: os 30 quadros pelo par já tinham chegado, com o servidor subindo
uma cópia só — `ate_o_par_estar_servindo` afirma isso quadro a quadro, então o
caminho de par **existia**. O que falhou foi a leitura de `pares.ja_servindo()`
feita **depois** dela, num `lock()` separado. Entre o último quadro e essa
leitura há uma janela, e numa máquina saturada ela é grande: se o fluxo do par
terminar ali, o servidor devolve a vaga e a barreira encontra a contabilidade já
zerada. É a mesma família das causas 1, 3 e 4 — afirmação sobre um instante que
o teste não controla —, e não contenção que a fila possa remover.

**Por que a fila não alcança nenhuma das duas.** Ela garante que só um teste de
conformidade corra por vez; ela não encurta o que um teste sozinho espera. Este
é o conjunto mais longo do crate (52 a 69 s) e o único que levanta servidor,
derruba-o e o levanta de novo. Serializar não muda nada disso.

**O discriminador foi rodado, e não discriminou.** A medida que faltava era
rodar `tela_por_um_par` sozinho, muitas vezes, sob as duas cargas, para separar
«é o teste» de «é a máquina» (`motor/final29.sh`, parte B). Ela terminou, e as
doze rodadas estão em `arquivo-entregue/disc_*.resumo.log`:

| Carga | Rodadas | Saída | Conjunto (19 testes) |
| --- | --- | --- | --- |
| 8 queimadores (`rodada5.sh`) | `disc_queimador_1..6` | 0 nas seis | 51,22 – 52,06 s |
| +6 laços fora do `cargo` (`rodada4.sh`) | `disc_fora_do_processo_1..6` | 0 nas seis | 51,71 – 54,57 s |

As duas cargas saíram **indistinguíveis**, e o conjunto sozinho **não reprovou
nenhuma vez**. A hipótese que esta bateria ia testar — que a carga de fora do
processo é o que derruba este conjunto — **não foi confirmada**. Fica escrito
assim porque é o que saiu; a atribuição continua apoiada só em 0 reprovações em
6 rodadas sob `rodada5.sh` contra 2 em 8 sob `rodada4.sh`, que é correlação com
amostra pequena e não causa medida.

Duas coisas ela mostrou, e as duas apontam para dentro do teste: a reprovação
exige a **bateria inteira** correndo junto, já que carga de máquina sozinha não
a produz; e o conjunto leva **~52 s mesmo sozinho**, de modo que `verde8_5`,
que reprovou com o conjunto em 52,35 s, reprovou **em tempo normal** — não foi
lentidão, foi a corrida de leitura descrita acima. Só `verde9_1` (68,59 s) saiu
da faixa. O que **não** foi feito é o conserto: ele é do
teste, e o teste é o arquivo que a tarefa e13d0b38 está alterando em paralelo —
mexer nele agora é o conflito que a restrição desta tarefa manda evitar.

### O `ci.yml`, conferido

A etapa avulsa `cargo test -p seele-conformance -- --test-threads=1` **foi
removida**, junto com o `--exclude seele-conformance` que ela obrigava a
existir, e o job passa a rodar o comando único do projeto:
`cargo test --workspace --all-targets`.

Ela ficou redundante porque a serialização deixou de morar no comando e passou a
morar no crate. E é melhor que redundante: `--test-threads=1` numa etapa
separada serializava o crate inteiro **e** exigia excluí-lo da outra etapa,
deixando dois comandos onde o projeto tem um — e deixando quem roda
`cargo test --workspace` na própria máquina sem a garantia que a CI tinha. Agora
os dois têm a mesma. A prova de que a remoção não regride nada é a tabela das
cinco rodadas acima, sob carga que a CI não tem.

### Corrigida em 2026-09-17 · a conferência citada não conferia o que dizia

**A afirmação anterior era falsa, e a medida que a sustentava não podia
sustentá-la.** Ficava escrito aqui que `cargo test --workspace` e
`cargo test --workspace --all-targets` «selecionam o mesmo trabalho», e que isso
fora «conferido, não suposto» por `-- --list` nos dois. **`--all-targets` não
roda doctest, e `-- --list` não enumera doctest** — a conferência citada não
alcançava a diferença que ela dizia ter descartado.

Medido nesta árvore, e é uma linha:

| comando | seções `Doc-tests` |
|---|---|
| `cargo test --workspace --all-targets` | **0** |
| `cargo test --workspace` | **8** |

A consequência não é de texto: **o job de teste da CI nunca rodou um doctest
sequer**, desde que existe. O passo passou a ser `cargo test --workspace`, sem
`--all-targets`. O que o `--all-targets` acrescentava sobre o comando simples é
o bench do `seele-audio` e os exemplos — e o job de clippy logo acima já roda
`--all-targets --all-features`, então os dois continuam sendo compilados a cada
push. Não se perdeu nada.

**E o ganho é pequeno, dito com o número na mão para não virar a próxima frase
grande demais:** as 8 seções contêm **um** doctest de verdade —
`seele-proto::version::negotiate` —, e as outras 7 estão vazias. Rodado agora:
passa. O que se consertou foi a afirmação e o furo do comando, não uma cobertura
que estivesse faltando.

### Medido em 2026-09-17 · o alcance da vaga, que faltava estabelecer

A revisão perguntou, com razão, se a permissão alcança o defeito. Faltava um
fato para responder: **o `cargo` roda os binários de teste em paralelo?** Se
rodasse, uma `static` por processo não serializaria nada entre arquivos, e 26
binários de conformidade concorrentes reporiam a §29 por cima da permissão.

Amostrado a cada 0,5 s durante uma rodada inteira de `cargo test -p
seele-conformance`, contando binários de teste vivos: **máximo 1**. Em 266
amostras com algum binário vivo, todas as 266 marcaram exatamente 1; nenhuma
marcou 2 ou mais. O `cargo` roda os binários em série, `libtest` paraleliza
dentro de cada um — que é exatamente a fatia que a vaga fecha. **A permissão
alcança o crate inteiro**, e não só o arquivo em que está.

Isso fecha a dúvida sobre o alcance e **não** desmente o resíduo: a reprovação
que sobrou sob carga deliberada não é contenção entre binários. É uma máquina
saturada estourando um prazo de **transporte** — os 20 s da `IDLE_TIMEOUT`, que
são política de produto presa à especificação (Ping a cada 5 s, `Reconectando`
depois de três perdidos) e não se alargam para fazer teste passar. Sob oito
queimadores mais seis laços rodando binários fora do `cargo`, um aperto de mão
sozinho passa de 20 s, e nenhuma serialização conserta isso porque não é disso
que se trata. É medida da máquina, não do produto, e está dito aqui para não ser
contado como a §29 de volta.

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

## 31 · Consertada em 2026-09-17, sem confirmação de campo · Trocar de fone ou microfone no Windows exige reiniciar o aplicativo

> **A causa foi achada e consertada na v0.11.0**, e o título fica como estava
> porque o sintoma é o que se procura. O `cpal` avisava dos três casos — padrão
> do sistema trocado, aparelho retirado, estalo — pelo retorno de erro do fluxo,
> e o produto contava «mais um erro» e jogava fora o **tipo** dele. O supervisor
> que saberia o que fazer existia e era código morto, não referenciado por
> nenhum arquivo fora de si mesmo.
>
> Hoje o tipo do erro é preservado e conduz o ciclo do aparelho, a reabertura
> acontece nos dois lados, a interface é avisada **enquanto** a troca acontece, e
> onze testes de conformidade exercitam comportamento — não texto-fonte.
>
> **Por que não está escrito «fechada».** Nenhuma máquina de CI tem duas placas
> de som e nenhuma tem tomada para puxar um fone: o que os testes fingem é a
> abertura do aparelho, e tudo o que decidia errado é o de verdade. Falta a
> confirmação de quem relatou, num Windows, trocando o fone no meio de uma
> conversa. Escrever «fechada» antes disso seria afirmar o que não foi medido.
>
> O texto original fica abaixo.

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

**As duas observações não bloqueantes da revisão de 2026-09-15, respondidas
com o limite à vista em vez de com código novo.** A revisão aprovou o conserto e
deixou duas ressalvas, as duas sobre guardas que leem texto. Nenhuma das duas se
fecha aqui, e o que muda é que agora está escrito **por que** e **o que sobrou
no lugar**:

- *Os guardas da casca (`apps/seele-app/tests/frontend.rs`).* Fechá-los de
  verdade pediria **executar o JavaScript** da tela, e não há máquina de JS na
  bateria — pôr uma faria a suíte depender do `node` da máquina de quem roda,
  que é trocar um limite declarado por um verde que não viaja. O que existe no
  lugar não é a leitura de texto sozinha: os nomes dos estados saem da
  **serialização de verdade** do `EstadoDoAparelho`, a lista de variantes é
  conferida por um `match` sem coringa (variante nova não compila até entrar
  nela), os nomes dos campos saem de um fecho que os nomeia em Rust (renomear
  impede o arquivo de compilar) e a corrente `desenhar → desenharTelemetria →
  desenharAparelho` é conferida elo a elo. O que fica de fora é o
  comportamento do script rodando; isso está dito, e não coberto.
- *`crates/seele-core/src/voice.rs`, o guarda da chamada dentro do laço.* A
  decisão inteira — classificar o erro, conduzir o supervisor, reabrir e
  recompor o laço — é exercida por comportamento em
  `crates/seele-conformance/tests/troca_de_aparelho.rs`. O que o guarda de texto
  cobre é só a **chamada** de `seguir_o_aparelho` dentro do laço de áudio, que
  tem `cpal` de um lado e QUIC do outro e nenhum teste desta máquina alcança.
  Não há tipo que expresse «este laço chama aquela função», e o recurso é o
  mesmo — pela mesma razão — do guarda de ordem do ganho e do portão de voz, que
  já existia neste arquivo antes desta tarefa. Fechá-lo pediria remontar o laço
  em torno de um tipo que force a chamada: é refatoração do caminho quente da
  voz, fora do escopo desta tarefa e sem defeito conhecido que a justifique.

**Nona medição, de novo pelo comando exato da validação, em 2026-09-15.**
`cargo test` no workspace inteiro, sem `--test-threads=1` e sem separar a
conformidade: **1.742 passados, 0 reprovados, 4 ignorados, saída 0**, em 69
baterias, com os **9** de `troca_de_aparelho.rs` verdes dentro da mesma corrida.
`cargo fmt --all -- --check` e `cargo clippy --workspace --all-targets` sem um
aviso. É a **quarta** vez que a reprovação relatada pela validação não se
reproduz aqui, e as quatro vezes o relato veio com a saída truncada e sem nome
de teste reprovado junto — a saída anexada à última rodada termina, ela mesma,
em `test result: ok`.

**A metade do aviso que estava solta, apontada por revisão independente e
fechada em 2026-09-15.** O fecho acima cobria o *que* mudou — estado e
contador — e deixava de fora *para onde*: a frase da troca diz o nome do
aparelho recém-aberto, e esse nome vem de `Snapshot::playback` e
`Snapshot::capture`, pelo `name` de dentro de cada um. Nenhum dos três estava
amarrado ao Rust, então renomear qualquer um deles apagaria o nome da frase sem
quebrar nada: a tela cairia calada no texto genérico «o aparelho mudou», que é
outra vez o produto sabendo e não contando. A função do fecho passou a nomear os
quatro caminhos — `aparelho`, `trocas_de_aparelho`, `playback.name` e
`capture.name` —, e o guarda exige que o script leia `playback`, `capture` e
`name` além dos dois de antes. *Prova de reversão, feita em 2026-09-15:*
trocando no script a leitura para `snapshot.playback?.nome ?? snapshot.capture
?.nome`, o guarda reprova com *«a tela não lê `name`, que é o que o instantâneo
manda»*; restaurado o arquivo e conferido por sha256, os 186 de
`apps/seele-app/tests/frontend.rs` voltam a passar.

**A chamada sem guarda, apontada por revisão independente e fechada em
2026-09-15.** Os dois guardas acima conferiam o *conteúdo* de
`desenharAparelho`, e nenhum deles conferia que alguém a chama: apagando a
linha que a invoca, a bateria inteira do frontend ficava verde com o aviso fora
da tela — o mesmo «o produto sabe e não conta» que originou esta tarefa, de
volta por outro caminho. O guarda novo,
`o_aviso_do_aparelho_nao_fica_solto_do_desenho_da_tela`, percorre a corrente
inteira e não só o último elo: a volta do relógio (`desenhar`) tem de chegar à
telemetria, e a telemetria tem de chegar ao aparelho. *Provas de reversão,
feitas em 2026-09-15:* apagando a chamada a `desenharAparelho`, o guarda
reprova com *«`function desenharTelemetria(snapshot)` não chama
`desenharAparelho`»* (`186 passed; 1 failed`); apagando a chamada a
`desenharTelemetria`, reprova com a frase equivalente do primeiro elo. Nos dois
casos o arquivo foi restaurado de cópia e conferido por `git status` limpo, e
os **187** de `apps/seele-app/tests/frontend.rs` voltam a passar.

**Oitava medição, e desta vez pelo comando exato da validação, em
2026-09-15.** `cargo test` no workspace inteiro, sem `--test-threads=1` e sem
separar a conformidade: **1.742 passados, 0 reprovados, 4 ignorados, saída 0**,
com os **9** de `troca_de_aparelho.rs` verdes dentro da mesma corrida.
`cargo clippy --workspace --all-targets` com zero avisos e `cargo fmt --all --
--check` limpo. A corrida levou perto de meia hora porque outra árvore de
trabalho compilava na mesma máquina — é a saturação da pendência 29, e ela
aparece como **lentidão**, não como reprovação: dois testes chegaram a anunciar
«has been running for over 60 seconds» (a enumeração de aparelhos reais da
ponte e o codec do macOS) e os dois terminaram passando. Quem relata «a
validação reprovou» com a saída truncada e sem nome de teste junto está, até
aqui, relatando esse prazo — pela terceira vez a reprovação não se reproduziu.

**Sétima medição da bateria inteira, em 2026-09-15**, já com o guarda da
chamada dentro: workspace sem a conformidade **1.616 passados, 0 reprovados,
saída 0**, e a conformidade sozinha em fila única (`--test-threads=1`, pela
disputa de portas efêmeras da pendência 29) **94 passados, 0 reprovados** —
**1.710** ao todo. `cargo clippy --workspace --all-targets` sem um aviso e
`cargo fmt --all --check` limpo.

**Sexta medição da bateria inteira, em 2026-09-15.** `cargo test` no workspace
inteiro, com todo o trabalho desta tarefa dentro e já com o guarda acrescentado
acima: **1.741 passados, 0 reprovados, 4 ignorados, saída 0**, sem um `FAILED`
sequer no relatório — e isto numa máquina que tinha outra árvore de trabalho
compilando ao mesmo tempo, que é justamente a saturação da pendência 29.
`cargo fmt --all -- --check` limpo e `cargo clippy --workspace --all-targets`
com zero avisos. A reprovação relatada na validação desta rodada não se
reproduziu aqui, e o relato veio, como das outras vezes, com a saída truncada e
sem nome de teste reprovado junto.

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

**O que a revisão apontou nesse mesmo teste, e como ficou.** Para envenenar o
cadeado ele precisa de um pânico proposital, e para não sujar a saída ele calava
o gancho de pânico — que é do processo inteiro, não do teste. Sob paralelismo
alto isso podia apagar a mensagem de outro teste do mesmo binário que falhasse
junto, e uma mensagem apagada é exatamente «o produto sabe e não conta» virado
contra quem depura. Agora o pânico acontece numa thread com nome próprio e o
gancho cala **só** ela, repassando todo o resto a quem já estava instalado. A
troca custou o atalho `expect` na subida da thread, negado neste workspace: ele
virou um `match` que diz o que houve. Conferido com um teste de ruído temporário
que estoura em paralelo com este: a mensagem dele aparece na saída, e o arquivo
foi restaurado depois. O que esse experimento **não** prova é a janela exata de
sobreposição — ela dura microssegundos, e nenhum teste a agenda; o que sustenta a
correção é a forma do filtro, não o cronômetro.

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

**Medido pela sétima vez em 2026-09-15, na retomada.** A validação independente
voltou outra vez com saída 101, e o acusado mudou de novo: agora
`quem_bate_a_porta_em_laco_e_recusado_por_taxa`
(`crates/seele-conformance/tests/limite_de_taxa.rs`), com *«a tentativa honesta 20
foi tratada como ataque: Some(SemResposta)»* — o vigésimo dos trinta apertos da
rajada honesta, num arquivo que este diff não toca. A assinatura é a de sempre:
`SemResposta`, e o conjunto levando **20,19 s** onde sozinho leva **0,16 s**.
Sozinho ele passou **3 de 3** (0,16 s, 0,13 s, 0,13 s), e a suíte nova de aparelho
roda inteira em **0,01 s** — quer dizer, não é ela que satura a máquina. Refeita a
mesma bateria (`seele-audio`, `seele-core`, `seele-ffi`, `seele-conformance`,
`seele-app`, `seele-proto`) com a máquina desocupada: **saída 0, 46 conjuntos,
1.157 testes passados, nenhuma reprovação**, e os nove cenários de
`troca_de_aparelho` verdes em 0,01 s. Mais um arquivo acusado, e mais um que não toca
em placa de som: continua sendo a pendência **29**, e o remédio dela é o semáforo
no `start()` da conformidade.

**Medido pela oitava vez em 2026-09-15, na retomada depois do reinício do
aplicativo.** Refeita a conferência com a máquina desocupada: `seele-audio`
passa **222 + 4 + 8**, `troca_de_aparelho` fecha **9 de 9** em 0,01 s,
`voz_na_reconexao` **1 de 1**, e `cargo fmt --all --check` sai limpo. O acusado
da rodada anterior, `limite_de_taxa`, passa **2 de 2 em 0,17 s** quando corre
sozinho — a mesma assinatura de sempre, e mais uma confirmação de que o que
reprova é a máquina saturada e não o conserto. O arquivo do seam foi conferido
por SHA-256 e continua idêntico ao restaurado depois da prova de reversão
(`5f5ae32732308d4b50cca602cc83d0034a326fac7660f0fa382ef59effc2056f`): a árvore
está no estado provado, e não no estado revertido. Continua sendo a pendência
**29**.

**Medido pela nona vez em 2026-09-15, e desta vez com uma conclusão diferente.**
A validação independente voltou com saída 101 e o registro cortado no meio da
enumeração dos conjuntos — sem nome de acusado, portanto sem o que conferir.
Refeita aqui a bateria inteira do workspace, com a máquina desocupada: **saída 0,
69 conjuntos, 1.740 testes passados, nenhuma reprovação**, com `seele-audio` em
**222** e `troca_de_aparelho` em **9 de 9** (0,01 s). `cargo fmt --all --check`
sai limpo e `cargo clippy --workspace --all-targets` não emite um erro sequer.

A conclusão diferente é sobre o **processo**, não sobre o conserto. Nove
registros seguidos dizem «é a pendência 29» e nenhum deles a consertou, porque
cada um esbarrou na mesma fronteira de escopo. Vale escrever o que custaria,
para o próximo não ter de redescobrir: o `cargo` **já roda os binários de teste
em série** — medido neste registro, o primeiro `test result` sai depois de um
único `Running` —, então o que sobra de paralelismo é o de dentro de cada
binário, mais a carga de outras sessões na mesma máquina. Serializar de dentro
exige uma permissão de RAII segurada pela **duração de cada teste**, e não só
pelo `Daemon::bind`: o que estoura é o aperto de mão, não a abertura da porta.
São 18 arquivos e 22 pontos de `bind` em `seele-conformance/tests/`, nenhum
deles de áudio. `RUST_TEST_THREADS` em `.cargo/config.toml` seria central, mas
é do workspace inteiro e serializaria também os 489 do `seele-server`.

É trabalho real, tem desenho e tem preço — e não cabe num diff de placa de som.
Continua sendo a pendência **29**, agora com o custo levantado.

**Medido pela décima vez em 2026-09-15, e desta vez sobrou nome próprio.** A
validação independente voltou com saída 101 e o registro cortado outra vez, sem
acusado. Refazendo a conferência daqui, dois fatos novos apareceram — e nenhum
dos dois é «a máquina estava cheia» no sentido vago de antes.

O primeiro é **meu**: cheguei a ter três `cargo test` do mesmo workspace
disputando o mesmo diretório de compilação, porque relancei a bateria antes de
conferir se a anterior ainda respirava. Um deles ficou parado em «Blocking
waiting for file lock on build directory» e os outros dois se atropelaram.
Medir antes de concluir vale também para o próprio processo: `ps` antes do
segundo `cargo` teria custado um segundo.

O segundo é **do ambiente**, e é o que explica o 101 sem precisar de hipótese.
Os comandos desta sessão rodam com acesso restrito, e nesse regime **dois testes
anteriores a este diff travam em vez de falhar**:
`a_saida_desta_maquina_abre_como_entrada` e
`uma_saida_nao_responde_configuracao_de_entrada`
(`crates/seele-audio/src/laco.rs`, commits `d6f74bf` e `e06d2a8`, ambos já na
principal). A pilha amostrada com `sample` mostra os dois presos dentro de
`manter_a_saida_tocando` / `playback_devices`, isto é, na abertura da placa de
som: sem permissão, o CoreAudio não recusa — ele fica esperando. `empacotamento`
(`apps/seele-app`) faz o mesmo com rede, e ficou **24 minutos** onde leva 16 s.
Nenhum dos três é de áudio no sentido deste trabalho, e nenhum foi tocado aqui.

O que dá para provar daqui, portanto, é o escopo — e ele está inteiro verde,
com a máquina compartilhada com outra sessão (`cargo test -p seele-server` no
worktree `e13d0b38`, observado em `ps`, o que desta vez torna a saturação um
fato medido e não uma suposição):

| Conjunto | Resultado |
| --- | --- |
| `seele-audio --lib supervisor::` | 20 passados, 0 reprovados |
| `seele-audio --lib rt::` | 16 passados, 0 reprovados |
| `seele-audio --lib telemetry::` | 18 passados, 0 reprovados |
| `seele-audio --lib device::tests` | 16 passados, 0 reprovados (18,83 s) |
| `seele-conformance --test troca_de_aparelho` | **9 de 9**, 0,01 s |
| `seele-conformance --test voz_na_reconexao` | 1 de 1 |

Os quatro primeiros são exatamente os módulos que este diff altera; os dois
últimos são os guardas comportamentais que o aceite pede. Todos com saída 0.

**A limitação fica escrita, e não disfarçada:** a bateria do workspace inteiro
não pôde ser executada nesta sessão, porque ela inclui testes que pedem placa de
som e rede que o ambiente restrito não concede. Isso não é a pendência 29 — é
uma segunda causa, independente dela, e que atinge qualquer sessão que rode com
esse mesmo acesso. A bateria completa continua registrada pelas execuções
anteriores e pela revisão independente, ambas feitas sem essa restrição.

**O achado de escopo incidental, medido em vez de suposto.** A revisão
independente apontou que `crates/seele-proto/tests/vetores_de_hash.rs` entra
neste diff sem relação com áudio e «suja a fronteira do commit». Está certa
quanto à origem e errada quanto ao remédio: revertendo aquele arquivo para o
estado anterior, `cargo fmt --all --check` acusa **duas** divergências e
`cargo clippy -p seele-proto --all-targets` **para com erro** — `expect()` sobre
`Result`, mais quatro avisos. O arquivo chegou ao repositório sem passar pelos
portões do próprio repositório; o que este trabalho fez foi pagar essa dívida
para que os portões voltassem a fechar. Removê-lo daqui não limparia a fronteira:
deixaria a árvore reprovando `fmt` e `clippy`. Fica, e fica explicado.

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

> **Retirado do diff em 2026-09-15.** O portão de vagas saiu desta tarefa a
> pedido da revisão — ver «O portão de vagas sai desta tarefa, e o que ficou
> medido no lugar dele», ao fim deste item. Os parágrafos abaixo ficam como
> registro datado do desenho e das medidas, para quem o refizer como trabalho
> próprio; nenhum deles descreve código que esteja no diff hoje.

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

- `crates/seele-core/src/voice.rs`, no teste
  `o_laco_de_audio_conduz_o_acompanhamento_do_aparelho`, exige a chamada de
  `seguir_o_aparelho(` depois do laço e antes do primeiro `#[cfg(test)]`. Prova
  que **alguém conduz** o supervisor, que era o buraco do achado H; que a
  condução funciona é o que os nove testes de `troca_de_aparelho.rs` provam.
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

**A segunda validação estourou de novo, e desta vez a causa foi medida em vez de
suposta.** A rodada de `cargo test` na raiz levou **57 min 53 s de relógio** e
foi encerrada por tempo — mas gastou só **2 min de CPU** nesse período inteiro
(`66,65 s user`, `53,60 s system`, `3% cpu`). Quase uma hora de espera para dois
minutos de trabalho: a bateria não estava lenta, estava **parada**. O que ela
esperava estava fora dela — outra árvore de trabalho rodando a própria bateria
na mesma máquina, com `syspolicyd` a 43% conferindo assinatura de cada binário
de teste, `target/debug/deps` com **506 mil arquivos** (só listar essa pasta
leva 16 s) e a memória virtual com 3 GB dos 4 GB de troca em uso. O sintoma mais
claro: `cargo test -p seele-server --doc`, que roda **zero** testes de
documentação, imprimiu o resultado em 0,00 s e mesmo assim demorou **7 min 39
s** para terminar de existir, com 0% de CPU.

Com a máquina livre, a **mesma bateria, o mesmo comando, a mesma árvore**:
**1.735 testes passados em 69 alvos, 0 reprovados, `EXIT=0`, em 3 min 08 s** —
com os mesmos 67 s de CPU da rodada de uma hora. É a prova de que a diferença
era espera, e não trabalho. O prazo de 900 s é folgado três vezes para a bateria
inteira quando ela é a única coisa rodando; ele não cabe uma bateria dividindo a
máquina com outra. Isso não é um defeito desta tarefa nem do código: é o mesmo
efeito do item 29, agora com número em cima.

**Medida pela décima primeira vez em 2026-09-15, na retomada que pediu a falha de
validação pelo nome.** A rodada de validação anterior voltou de novo sem nomear
teste reprovado nenhum — a saída registrada termina com suites passando. Refeita
aqui a bateria inteira, `cargo test --workspace` numa execução só, sem
serializar nada: **`EXIT=0`, 1.735 passados, 0 reprovados, 4 ignorados, 69
alvos, nenhum `FAILED` em parte alguma do registro**. Pelo nome, os que esta
tarefa criou: `troca_de_aparelho` **7 de 7**, da troca do padrão no sistema ao
fone religado depois da desistência. `cargo fmt --all --check` saiu **0** e
`cargo clippy --workspace --all-targets` saiu **0 e sem um aviso**. Inclusive
`acceptance_m2::a_second_connection_reuses_the_pin`, que a revisão viu reprovar
uma vez sob carga, passou nesta rodada — o que confirma o diagnóstico de tempo,
e não de comportamento.

**A reversão central, refeita nesta mesma retomada.** A revisão registrou, com
razão, que desta vez ela leu as reversões em vez de executá-las. Foi refeita à
mão: trocando o corpo de `classificar` em `crates/seele-audio/src/device.rs` por
`FalhaDeAparelho::Transitoria` — quer dizer, voltando a tratar a troca e o
sumiço do aparelho como estalo —, a unidade de `seele-audio` reprova
**4 de 220** (`a_troca_de_aparelho_do_sistema_pede_reabertura`,
`o_aparelho_que_sumiu_pede_reabertura`,
`a_troca_feita_no_sistema_chega_ao_laco_como_troca` e
`o_aparelho_arrancado_chega_ao_laco_como_sumico`, com «left: Transitoria, right:
Trocado» e «right: Sumiu»), e a conformidade reprova **6 de 7** em
`troca_de_aparelho`. O sétimo que continua passando é o controle negativo,
`um_estalo_no_fluxo_nao_troca_o_aparelho_de_ninguem`: ele existe justamente para
não passar a impressão de que qualquer erro reabre aparelho. Uma das mensagens,
por inteiro: «a sessão nunca desistiu, e um estado de "trocando" eterno é a mesma
mentira do silêncio calado — left: Funcionando, right: Perdido». O arquivo foi
restaurado e conferido por hash SHA-256 (idêntico byte a byte, árvore limpa), e
os dois conjuntos voltaram a passar: 220 e 7.

**A falha de validação, enfim cronometrada em 2026-09-15.** A rodada automática
voltou de novo dizendo só «excedeu o limite de 900s», sem nome de teste. Em vez
de supor de novo, medi. `cargo test --workspace` numa execução só, com a máquina
dividida com outra sessão de `cargo test` de outro worktree (média de carga 4,0
no começo): **406 s, `EXIT=0`, 1.735 passados, 0 reprovados, 4 ignorados, 69
alvos**. Quer dizer: a bateria inteira cabe em menos da metade do prazo mesmo
com a máquina ocupada, e o estouro do coordenador não é esta bateria.

O que estoura o prazo é a **conformidade serializada**, e agora com número:
`cargo test -p seele-conformance -- --test-threads=1` levou **1.580 s** (26
minutos) para 25 alvos, **122 passados, 0 reprovados, `EXIT=0`**. Quase todo
esse tempo é espera de relógio, não de processador — a soma dos tempos que os
próprios alvos relatam não chega a dois minutos; o resto é o `--test-threads=1`
esperando cada alvo de cada vez, com os que têm rodadas de rede (`limite_de_taxa`
à frente) mandando no total. Então: a serialização é necessária pelo esgotamento
de portas efêmeras (item 29), mas ela sozinha passa de 900 s. Quem for medir o
prazo desta tarefa deve medir `cargo test`, que cabe; a conformidade em série é
verificação à parte, e foi feita aqui, verde.

Isso fecha também a ressalva da revisão que dizia não ter rodado a conformidade
inteira em série: rodou, aqui, e os 25 alvos passaram — inclusive
`troca_de_aparelho` (na época 7 de 7, hoje 9 de 9; 26,05 s, o alvo mais
demorado da suíte) e
`voz_na_reconexao`.

**O que faltava medir, e que enfim foi medido em 2026-09-15: a espera por vaga
alheia.** A revisão anterior suspeitou, sem poder provar, que o portão de vagas
do item 29 fosse cúmplice do estouro de 900 s. Era. A causa tem nome e número:
o conjunto de vagas é da máquina inteira (de propósito — ver `vaga`), o `cargo`
roda os alvos de teste **um de cada vez**, e o prazo de espera era de 90 s **por
alvo**. Com outra árvore de trabalho ao lado — e havia uma, rodando
`cargo test -p seele-conformance -- --test-threads=1`, que nesse modo segura uma
vaga do primeiro ao último teste, por 26 minutos —, os 25 alvos deste crate
pagam o prazo um por um: até 37 minutos de relógio gastos só esperando, sem um
único teste reprovando. É por isso que a validação automática voltava dizendo
«excedeu o limite de 900 s» sem nome de teste nenhum: não havia teste vermelho,
havia fila.

O conserto põe um **teto no estrago**, e não desliga o portão: a espera caiu de
90 s para 20 s, e uma desistência — esperar o prazo inteiro sem conseguir vaga —
fica registrada na pasta das vagas por 60 s, calando a espera dos alvos
seguintes. A marca desliga a **espera**, nunca a **tomada**: um alvo que chegue
com o conjunto livre continua pegando a sua vaga na hora, e a proteção do item 29
contra a saturação da própria suíte continua de pé. O que some é a soma: a rodada
paga a descoberta uma vez e depois corre saturada, que é exatamente o
comportamento de antes do portão — degradação limitada, em vez de acumulada.

Isso é comportamento, e está provado como comportamento. O teste
`o_alvo_seguinte_nao_paga_a_espera_de_novo_quando_a_vaga_e_de_outro`
(`crates/seele-conformance/src/lib.rs`) toma a única vaga de um conjunto seu,
cronometra a desistência seguinte — que tem de esperar o prazo — e cronometra a
terceira chamada, que não pode mais esperar. **Prova de reversão, executada:**
voltando a linha do desvio para `let esperar = true`, o teste reprova com «o alvo
seguinte voltou a pagar a espera inteira (324,1 ms de 300 ms). Com 25 alvos neste
crate, essa conta é a diferença entre uma bateria que cabe no prazo da validação
e uma que estoura em espera de vaga alheia.» O arquivo foi restaurado e conferido
por SHA-256, idêntico byte a byte. O segundo teste,
`quem_ja_desistiu_nao_faz_o_alvo_seguinte_esperar_de_novo`, guarda as duas bordas
da marca: pasta limpa não parece desistência, e desistência velha caduca.

**A medida depois do conserto, 2026-09-15.** Com o teto no lugar e o crate
inteiro conferido pelo clippy, o `cargo test` do workspace — o mesmo comando da
validação automática que vinha estourando — terminou em **4 min 36 s**: 1737
testes, 69 alvos, nenhuma reprovação. É um terço do prazo de 900 s, contra os
mais de 18 minutos da rodada que o estourou. A diferença não está em teste
nenhum ter ficado mais rápido: está em ninguém mais pagar a fila 25 vezes.


**Os dois achados da revisão de 2026-09-15, consertados.** A revisão aprovou o
conserto e deixou duas observações não bloqueantes. Nenhuma das duas era
cosmética, e as duas viraram comportamento:

**Primeiro: dois avisos acendendo juntos, e um deles acusando a máquina errada.**
Uma troca de rota e um aparelho arrancado chegam ao produto como erro do fluxo, e
entram — como devem — em `stream_errors`, que é o que a telemetria soma. O que
não devia era a **régua de falha local** somá-los: `LocalTelemetry::tropecos`
juntava `device_errors` inteiro, então durante a reabertura a tela acendia «ÁUDIO
LOCAL FALHANDO» ao lado de «TROCANDO DE APARELHO». Os dois avisos significam
coisas diferentes — um diz «esta máquina está derrubando áudio», o outro diz «o
sistema mexeu no aparelho» —, e quem trocou de fone lia a acusação errada, no
aviso que apaga sozinho e não explica nada. Agora `LocalTelemetry` carrega
`device_events` (o subconjunto que é evento de aparelho) e `tropecos` o
desconta. O tropeço transitório continua contando: é para isso que a régua
existe.

*Prova de reversão, executada.* Devolvendo `saturating_add(self.device_errors)`,
`trocar_de_aparelho_no_sistema_nao_acende_o_aviso_de_falha_local` reprova com «a
troca de rota feita no sistema não é esta máquina derrubando áudio». O par
`um_tropeco_de_verdade_continua_acendendo_o_aviso` guarda a outra borda, para o
conserto não virar um aviso que nunca acende. Os dois montam a telemetria pelo
caminho do produto: contadores de verdade, `record_stream_error` de verdade,
`LocalTelemetry::assemble` de verdade.

**Segundo: o teste de conformidade refazia à mão a volta que devia provar.** A
`uma_volta` de `troca_de_aparelho.rs` reimplementava a troca — chamava `passo` e
trocava os contadores —, e a composição que roda em produção (ler o aviso,
reabrir, **redimensionar**, esvaziar o que era do aparelho antigo e reacertar o
relógio, nessa ordem) continuava coberta só por guarda de texto-fonte. Cada
passo tinha o seu teste de unidade; a ordem entre eles, que é o que a pessoa
ouve, não tinha nenhum. A volta inteira virou uma função de produção,
`seele_core::seguir_o_aparelho`, e o teste de conformidade **conduz essa
função**: os nove cenários passaram a exercer o código que roda na máquina de
quem usa. O que sobra do outro lado do traço é a atribuição das peças
devolvidas e o `cpal`, que nenhuma máquina de CI tem como exercer — o guarda de
texto-fonte encolheu de um bloco de quinze linhas para uma chamada só, e a
mensagem dele foi reapontada.

Dois cenários novos, que a composição passou a permitir:
`a_troca_nao_toca_no_aparelho_novo_o_que_era_do_antigo` põe amostras na taxa do
aparelho de antes nas quatro filas do laço e exige que a troca para 48 kHz as
largue — é o estalo que seria o primeiro som do aparelho novo. E
`um_aparelho_que_abre_e_nao_da_laco_nao_e_desenhado_como_funcionando` cobre o
caminho estreito em que o aparelho abre e o reamostrador recusa as taxas dele:
era o único em que o painel ficava escrito como «funcionando», com o nome do
aparelho novo, e o laço encerrava depois — a pessoa sem som nenhum lendo
normalidade na tela.

*Provas de reversão, executadas, três.* Tirando `esvaziar_o_que_era_do_antigo`
de `recomecar_no_aparelho_novo`, o primeiro cenário reprova com «amostras do
aparelho de antes ficaram a caminho do de agora; elas saem como um estalo no
primeiro instante do aparelho novo». Tirando a escrita de `sem_laco_possivel`, o
segundo reprova com «a tela diz "funcionando" sobre um aparelho que não produz
som nenhum — left: Funcionando, right: Perdido». E apagando do laço de
`pipeline` o bloco que chama `seguir_o_aparelho`, o guarda de texto-fonte
`o_laco_de_audio_conduz_o_acompanhamento_do_aparelho` entra em pânico com a
mensagem que nomeia o defeito de origem. Nas três, a árvore foi restaurada e os
conjuntos voltaram a passar: `troca_de_aparelho` **9 de 9**.

**As três ressalvas da revisão de 2026-09-15, respondidas.** A revisão aprovou o
conserto e deixou três observações não bloqueantes. A primeira era a única com
trabalho pendente, e virou medida.

**Primeira: a reversão central estava lida, não executada, e contra uma suíte
menor.** A revisão não podia alterar arquivo, então conferiu as reversões por
leitura, e notou com razão que os números registrados acima — «7 de 7» — são de
uma rodada anterior, enquanto a suíte de hoje tem **nove** cenários. *Prova de
reversão, executada em 2026-09-15 contra a suíte de nove:* trocando o corpo de
`retorno_de_erro` (`crates/seele-audio/src/device.rs`) por
`move |_error| errors.record_stream_error(FalhaDeAparelho::Transitoria)` — que é
exatamente o descarte original, com o tipo do erro do `cpal` jogado fora —,
`troca_de_aparelho` reprova **8 de 9**, e as mensagens nomeiam o defeito de
origem: «a voz continuou presa ao aparelho antigo; era isto que só reiniciar o
aplicativo resolvia», «a sessão nunca desistiu, e um estado de *trocando* eterno
é a mesma mentira do silêncio calado», «a tela ficou congelada no aparelho
anterior». O nono é o controle negativo —
`um_estalo_no_fluxo_nao_troca_o_aparelho_de_ninguem` —, e ele **tem** de
continuar passando: um estalo transitório era transitório antes e depois, e se
ele reprovasse a reversão estaria provando outra coisa. O arquivo foi restaurado
e conferido por SHA-256, idêntico byte a byte
(`5f5ae32732308d4b50cca602cc83d0034a326fac7660f0fa382ef59effc2056f`), e a suíte
voltou a **9 de 9**. Os «7 de 7» dos parágrafos acima ficam onde estão: são
registro datado de rodadas anteriores, e reescrevê-los apagaria a história em vez
de contá-la.

**Segunda: o diff carrega três consertos que não são desta tarefa.** É verdade, e
eles ficam, com o motivo à vista: os três apareceram **reprovando a validação
desta tarefa**, e nenhum deles é do áudio. Em `tipo_de_fluxo.rs` o servidor larga
o fluxo assim que recusa o tipo, o `STOP_SENDING` volta e a escrita seguinte do
cliente falha com `Stopped` — o servidor acertando, o teste reprovando por
corrida; a tolerância é estreita de propósito (só `Stopped`, só naqueles dois
testes, e qualquer outro erro continua reprovando), e as asserções que provam o
comportamento não mudaram nenhuma. Em `par_lento.rs` a gravação sai em lote a
cada 200 ms e o contador era lido no instante seguinte ao desligamento: a espera
de até cinco segundos não afrouxa a asserção, que é a mesma e continua exigindo
todas as mensagens. Em `vetores_de_hash.rs` só há `fmt` e `clippy`. Desfazê-los
devolveria a reprovação por tempo à bateria desta tarefa, que é o que a validação
pede para não acontecer.

**A bateria, refeita inteira depois disso, em 2026-09-15.** `cargo test` no
workspace terminou com **código de saída 0** e **1742 testes passados, nenhuma
reprovação** — o código de saída conferido explicitamente, e não inferido do fim
da saída. `cargo fmt --all --check` saiu limpo e o `clippy --all-targets` de
`seele-audio`, `seele-core` e `seele-ffi` não levantou um aviso.

**Terceira: `DeviceEvent::UserSelected` não tem chamador em produção.** É
declarado no próprio `supervisor.rs`, e é por desenho: a troca feita na tela do
SEELE monta um caminho de voz novo, e não passa pelo supervisor. As duas
armadilhas dele foram consertadas mesmo assim, porque o aceite pedia, e estão
cobertas por teste de unidade. Ligar a tela ao supervisor é reescrever o caminho
de troca dentro do app — o que esta tarefa foi explicitamente instruída a não
fazer.

**As duas ressalvas não bloqueantes da revisão seguinte, de 2026-09-15,
respondidas com o limite à vista.** A revisão aprovou de novo e deixou duas
observações; nenhuma pede código, e as duas ficam escritas aqui em vez de
sumirem no corpo de um relatório:

- *As armadilhas do supervisor foram consertadas no mesmo commit que liga o
  seam, e não num commit anterior, como o aceite pede ao pé da letra
  («ANTES»).* É verdade, e não dá para desfazer sem reescrever histórico já
  publicado — coisa que esta tarefa foi instruída a não fazer, e que não
  compraria nada. O que o aceite quer é que nenhum estado com o seam ligado
  exista com as armadilhas abertas, e isso está garantido: as duas mudanças são
  atômicas no mesmo commit, e cada armadilha tem teste dedicado que reprova ao
  ser revertida (as duas reversões estão registradas acima). O que difere é a
  granularidade do histórico, não o comportamento em nenhum ponto da linha.
- *Sobrou um guarda de texto-fonte novo em `voice.rs`
  (`o_laco_de_audio_conduz_o_acompanhamento_do_aparelho`).* Sobrou, e está
  registrado como tal em dois lugares deste arquivo: na lista dos guardas de
  texto que ficaram e no parágrafo que explica por que ele não fecha. A
  decisão inteira que ele protege é coberta por comportamento em
  `troca_de_aparelho.rs`; o que só o texto alcança é a **chamada** dentro de
  `pipeline`, que tem `cpal` de um lado e QUIC do outro. Ele é complemento, e
  não o guarda principal de nada.

**A reprovação que a validação trouxe, e de quem ela é.** A rodada de `cargo
test` do espaço de trabalho voltou com **um** teste reprovado, e ele não é desta
tarefa: `expulsar_acaba_com_a_sessao_e_deixa_voltar`, em
`crates/seele-conformance/tests/moderacao.rs`, com *«quem foi expulso continua
desenhado na sala de voz»* — quer dizer, o assento do expulso ainda aparecia
para quem ficou depois dos 10 s de `PRAZO`. Nada de áudio passa por ali.

A suspeita óbvia era esta tarefa: ela é a única que mexeu nesse arquivo, para
acrescentar a chamada de `vaga()` do item 29. **Foi medido, e é o contrário.**
Compilando o mesmo teste duas vezes — um binário com a chamada e outro sem —, e
rodando os dois intercalados sob a mesma carga (uma bateria de `seele-server`
correndo ao lado, três vezes seguidas):

| binário | reprovações |
| --- | --- |
| com `vaga()` (como está no diff) | 1 em 58 |
| sem `vaga()` (como estava antes) | 4 em 38 |

O portão de vagas **reduz** a instabilidade; não a criou. Sozinho, sem carga
nenhuma, o teste passa em 0,4 s e não reprova. Sob 45 processos queimando CPU,
também não reprova — o que descarta «máquina lenta» como causa: o que derruba o
teste é disputa de porta e de rede local, não de núcleo.

**O que fica em aberto, e para quem.** A causa dentro do servidor não foi
fechada aqui, e não se fecha nesta tarefa sem sair do escopo: o candidato é a
corrida entre o ramo que trata `Event::SessionEnded` — que zera
`current_voice_room` antes de se despedir — e o caminho de saída do laço, que
guarda o assento por `SESSION_GRACE` (cinco minutos) quando a conexão morre com
`current_voice_room` ainda preenchido. Se a conexão do expulso cair pelo segundo
caminho, o assento fica reservado e quem ficou continua vendo a pessoa sentada.
Isto é hipótese, e está escrito como hipótese: o que está **medido** é que a
reprovação não vem do diff desta tarefa.

Foi tentado aqui o conserto barato — subir o `PRAZO` de 10 s para 90 s — e ele
foi **desfeito**: afrouxar a régua de um teste alheio para fechar esta tarefa é
esconder o defeito de outra pessoa dentro de um diff de áudio. O arquivo está
como estava, com `PRAZO` em 10 s.

### O portão de vagas sai desta tarefa, e o que ficou medido no lugar dele

**A revisão de 2026-09-15 não aprovou, e o que a segurou não foi o áudio.** Ela
disse com clareza qual era o pedido: ou o portão de vagas sai desta tarefa e
volta como trabalho próprio de infraestrutura de teste, ou ele ganha um teto que
não estanque alvos inteiros — e, de qualquer modo, falta uma bateria completa
verde dentro do prazo, anexada como resultado de verdade. As duas coisas foram
feitas, e a segunda desmentiu uma parte da primeira.

**O portão saiu inteiro.** `crates/seele-conformance/src/lib.rs` voltou a ser o
arquivo vazio que era, e com ele foram os dois testes de comportamento do próprio
portão (é por isso que a bateria conta **1.740** e não 1.742). As 21 chamadas de
`seele_conformance::vaga()` saíram dos 18 arquivos de teste, todos devolvidos ao
estado publicado, e o comentário do `Cargo.toml` voltou a dizer que o `src/lib.rs`
é vazio — porque voltou a ser. Junto saíram as duas tolerâncias de `seele-server`
(`tipo_de_fluxo.rs` e `par_lento.rs`), pelo mesmo motivo: não são aparelho de som.
Fora do áudio, o diff carrega agora **um** arquivo alheio, `vetores_de_hash.rs`, e
ele fica porque sem ele o `clippy` do espaço de trabalho reprova — o parágrafo
acima conta essa história inteira.

**A atribuição do estouro de prazo estava errada, e agora está medida em vez de
lida.** A revisão viu `acceptance_m5` em 20,02 s e `bateria_interna` em 21,07 s,
reconheceu ali o teto de espera de 20 s do portão, e concluiu que o portão
estancava alvos inteiros que sozinhos custam menos de dois segundos. Era a leitura
natural, e está errada. **Com o portão removido do arquivo e das chamadas**, os
mesmos dois alvos, rodados sozinhos, sem vaga alguma no caminho:

| alvo | com portão (medida da revisão) | sem portão (medido aqui) |
| --- | --- | --- |
| `acceptance_m5` | 20,02 s | **20,02 s** |
| `bateria_interna` | 21,07 s | **21,04 s** |

Os vinte segundos são dos próprios testes — há espera de relógio dentro deles —, e
não fila por vaga. O portão não estancava esses alvos; ele coincidia com eles.

**O custo de tirar o portão, escrito para não virar surpresa de ninguém.** Ele
mitigava, e mitigava de verdade, a saturação do item 29: a medida está na tabela
acima nesta mesma seção — o mesmo teste de `moderacao.rs` reprovou **1 em 58** com
`vaga()` e **4 em 38** sem. Tirando o portão, essa instabilidade volta ao que era
antes desta tarefa. É uma troca consciente e é a certa: o remédio não é de áudio,
e esconder infraestrutura de teste dentro de um diff de aparelho de som foi
exatamente a objeção da revisão. Fica registrado como trabalho do item 29, com o
desenho já provado uma vez — teto de espera curto, marca de desistência que cala a
espera dos alvos seguintes sem desligar a tomada.

**A bateria completa, dentro do prazo, duas vezes.** `cargo test` no espaço de
trabalho, o mesmo comando da validação automática, depois da remoção:

| rodada | relógio | CPU | resultado |
| --- | --- | --- | --- |
| primeira | 17 min 04 s | 12% (68 s + 65 s) | `EXIT=0`, 1.740 passados, 0 reprovados, 69 alvos |
| segunda | **3 min 10 s** | 69% (71 s + 62 s) | `EXIT=0`, 1.740 passados, 0 reprovados, 69 alvos |

As duas passaram inteiras; só a segunda cabe no prazo de 900 s. A diferença entre
elas não é teste nenhum: é a máquina. O trabalho é o mesmo nas duas — cerca de
dois minutos de processador —, e a soma dos tempos que os próprios alvos relatam
caiu de 453 s para 173 s entre uma e outra. O alvo de unidade de `seele-audio` é o
retrato: **297,60 s** na rodada disputada, **15,02 s** na tranquila e 11,27 s
rodado sozinho, sem uma linha de diferença no binário. Isto é o mesmo efeito
descrito no item 29 e medido nesta seção: em máquina dividida a bateria fica
**parada**, não lenta. E a conformidade, que a revisão viu reprovar em 1 de 2
rodadas completas, passou nas **duas** rodadas acima, `moderacao.rs` incluída.

Pelo nome, o que esta tarefa criou, na segunda rodada: `troca_de_aparelho`
**9 de 9** e a unidade de `seele-audio` **222 de 222**. `cargo fmt --all --check`
saiu **0** e `cargo clippy --workspace --all-targets` saiu **0 e sem um aviso**
depois da remoção — quer dizer, nada do que saiu estava sustentando lint nenhum.


**A terceira bateria, na retomada de 2026-09-15, e o que ela diz da reprovação
da validação.** A validação automática voltou com `exit 101` e um log cortado na
palavra «Runnin» — sem nome de teste nenhum. Como não dá para diagnosticar pelo
que não está escrito, a bateria foi refeita aqui, com o mesmo comando:

| rodada | relógio | CPU | resultado |
| --- | --- | --- | --- |
| terceira | 17 min 14 s (máquina disputada) | — | sem reprovação; código de saída perdido pelo `tail` do próprio registro |
| quarta | **3 min 05 s** | 69 s de usuário + 57 s de sistema | `EXIT=0`, **1.740** passados, 0 reprovados, 61 alvos |

Pelo nome, nesta quarta rodada: `troca_de_aparelho` **9 de 9** em 0,01 s e a
unidade de `seele-audio` **222 de 222** em 3,28 s. `cargo fmt --all --check` saiu
**0** e `cargo clippy --workspace --all-targets` saiu **0 e sem um aviso**.

Somando com as duas rodadas da seção anterior e com as cinco da revisão
independente, são **nove** baterias completas sem uma reprovação. A reprovação da
validação segue **não atribuída**, e não se atribui a esta tarefa por medida e não
por opinião: fora do áudio, o diff toca **um** arquivo, `vetores_de_hash.rs`, e os
testes de conformidade e de servidor voltaram **idênticos ao commit-base** — foi
conferido arquivo a arquivo. O que esse único arquivo alheio carrega é formatação
e um `allow` de `clippy`, e ele fica porque o próprio commit-base o publicou
desformatado: `rustfmt --check` reprova a versão publicada dele. Sem essa
arrumação, o estilo do espaço de trabalho reprova por causa de outra tarefa, não
desta.

O candidato conhecido continua sendo o item 29 — `moderacao.rs` sob disputa de
porta —, medido nesta mesma seção como anterior a esta tarefa (4 reprovações em 38
com o arquivo no estado publicado). Se a intermitência voltar, o que fecha o caso
é o log inteiro da validação, com o nome do teste: sem ele, qualquer atribuição
seria suposição sobre a máquina de outra pessoa.

### A décima bateria, e os quatro apontamentos da revisão que aprovou

**A revisão de 2026-09-15 aprovou**, com quatro apontamentos todos marcados como
não bloqueantes, e a validação automática voltou outra vez sem nome de teste
reprovado — o trecho de log anexado termina com alvos passando. A bateria foi
refeita aqui inteira, pelo mesmo comando da validação:

| rodada | resultado |
| --- | --- |
| décima | `EXIT=0`, **1.740** passados, 0 reprovados, **69** alvos |

Pelo nome, nesta rodada: `troca_de_aparelho` **9 de 9** em 0,01 s e a unidade de
`seele-audio` **222 de 222** em 17,06 s. `cargo fmt --all --check` saiu **0** e
`cargo clippy --workspace --all-targets -- -D warnings` saiu **0**. A máquina
estava disputada (a rodada levou cerca de onze minutos de relógio contra três em
máquina tranquila), o que é o efeito do item 29 já medido acima — e mesmo assim
não houve uma reprovação.

São **dez** baterias completas sem uma reprovação atribuível a esta tarefa. A
reprovação da validação continua **não atribuída**, pelo mesmo motivo de antes:
nenhum log recebido nomeia o teste que caiu.

**Os quatro apontamentos, e o que cada um já tem de resposta.**

- **A reabertura de verdade não tem teste** — `ReabrirAparelhos` → `open_preferring`
  → `cpal` depende de placa de som, que a integração não tem. É limitação inerente,
  está escrita no cabeçalho de `crates/seele-conformance/tests/troca_de_aparelho.rs`
  e o teste entra pelo fechamento de erro que o próprio `cpal` chama. Fica aberto
  como o que é: o que só uma máquina com aparelho de som fecha.
- **As provas de reversão foram conferidas por leitura** — porque a revisão não
  altera fontes. Elas estão registradas nesta mesma seção com o comando, a
  mensagem de reprovação e a conferência de restauração de cada arquivo.
- **`vetores_de_hash.rs` é ruído alheio no diff** — e fica, pelo motivo já escrito
  duas seções acima: a versão publicada no commit-base reprova `rustfmt --check`, e
  sem a arrumação o estilo do espaço de trabalho reprova por causa de outra tarefa.
  Foi reconferido agora: `rustfmt --check` sobre o arquivo **como veio do
  commit-base** aponta três pontos. Nenhuma asserção mudou.
- **O commit final precisa capturar a remoção do portão de vagas** — capturado.
  As alterações que devolvem `crates/seele-conformance/src/lib.rs`, as 21 chamadas
  de `vaga()` nos 18 arquivos e os dois testes de `seele-server` ao estado do
  commit-base estão agora comitadas na branch, e não só na árvore de trabalho. O
  escopo entregue volta a ser aparelho de som mais um arquivo alheio de formatação.

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

### A previsão desta seção se cumpriu em 2026-09-17

Ficava escrito aqui que *«qualquer novo motivo de recusa que o protocolo venha a
ganhar exigiria lembrar de novo de desfazer o assento à mão»*. A v0.11.0 ganhou
**dois**: o teto declarado da sala (`VoiceRoomFull`) e a permissão de entrar.

E foi exatamente isso que aconteceu: o conserto do teto só ficou correto depois
de acrescentar `VoiceRoomFull` ao braço de `Room::apply` que desfaz o assento.
Sem essa linha, o defeito inteiro voltava com um motivo novo — a pessoa se vendo
dentro de uma sala onde ninguém a vê. A recusa por permissão escapou por sorte:
ela reusa `VoiceRoomEntryRefused`, que já estava na lista.

**Isto não fecha esta pendência; encarece-a.** A cada motivo de recusa novo, a
lista do cliente precisa crescer junto, e nada no tipo obriga. O conserto de
raiz continua sendo o mesmo do relatório de origem — confirmação do servidor no
fio — e continua exigindo mensagem de protocolo nova.

**Quando dói.** Sempre que uma sala de voz tem senha, está cheia, ou ganha algum
outro motivo de recusa no futuro: o intervalo entre pedir e ouvir a recusa
continua existindo, só não sobrevive mais a ele.


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

> **Estreitada em 2026-09-17, e não fechada.** A auditoria de experiência
> apontou que esta página descrevia como vigente um estado que o código já tinha
> mudado. Conferido nas duas metades, e nesta ordem — implementado, testado,
> publicado:
>
> - **`EnterVoiceRoom` confere `Permission::EnterVoiceRoom`.** Implementado e
>   com teste. Havia um comentário em `session.rs` afirmando o contrário, três
>   mil linhas depois da linha que confere; ele saiu junto com esta nota.
> - **A bateria distingue queda de expulsão.** `a_sessao_acabou_aqui` trata
>   `Kicked`, `Banned` e `ModsMudaram` como fim, e não como rede caída.
>   Implementado e com teste.
>
> **O que continua aberto**, e é o motivo de a pendência não fechar: nada disto
> foi provado em campo, com duas máquinas e um operador expulsando alguém de
> verdade. E a terceira parte do texto acima — o servidor responder à entrada em
> vez de «confirmar por silêncio» — não foi tocada.
>
> A regra desta página vale aqui também: o texto original fica, e o que mudou é
> dito por cima dele. Uma pendência que descreve o passado como presente é pior
> que uma pendência aberta — alguém a lê e reabre um conserto que já existe, que
> foi exatamente o que quase aconteceu.

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

## 46 · O caminho de Windows do som da tela não é compilado neste Mac

> **Nasceu como 45 e foi renumerada no mesmo dia.** Havia outra 45 — «expulsar
> não mantém ninguém fora da sala» — que chegou por um merge, e escrevi por
> cima do número sem conferir. O commit que a criou, `2cfffce`, diz «pendência
> 45» querendo dizer esta. Quem renumera é a mais nova, pela regra que o
> `README.md` dos ADRs já dá para os renomes.

**Aberta em 2026-09-17**, junto do conserto que faz o som da transmissão seguir
a troca de aparelho.

**O que foi consertado.** `seele-core/src/video.rs` abria o *loopback* uma vez —
o padrão do sistema naquele instante — e o segurava pela transmissão inteira.
Agora a decisão mora em `seele_core::som_que_segue`, que conduz o mesmo
`CicloDoAparelho` que a voz conduz e reabre no padrão de agora. Três testes de
conformidade o exercitam, e a prova de reversão está registrada.

**O que não foi verificado, e é honesto separar.** `som_da_maquina` e o braço
`SomDaTela::DaMaquina` de `tomar_som` estão atrás de `#[cfg(target_os =
"windows")]`. **Nesta máquina eles não são compilados**, e não há como compilar
cruzado: `cargo check -p seele-core --target x86_64-pc-windows-msvc` para no
`cc`, no `ring`, com `fatal error: 'assert.h' file not found` — faltam os
cabeçalhos C do Windows.

**O que foi feito a respeito, em vez de deixar por isso mesmo.** A parte de
plataforma foi encolhida até quase nada:

- a decisão inteira saiu do `cfg` e virou `som_que_segue`, compilada e testada
  aqui;
- os dois adaptadores — `AbrirOSomDaMaquina` e `SomAberto for CapturaDaSaida` —
  **também** saíram do `cfg`. `CapturaDaSaida` compila em toda plataforma de
  propósito, então eles têm os tipos conferidos a cada `cargo build` no Mac;
- os acoplamentos que restavam estão presos por asserções de tipo em
  `video.rs`, no mesmo recurso que o arquivo já usava para o `Send` do
  `CapturaComSom`. Conferido que elas mordem: apagando o `impl SomAberto`, o
  build sai com `the trait bound CapturaDaSaida: SomAberto is not satisfied`.

O que sobra sem compilação nesta máquina é `som_da_maquina` — uma chamada e um
`match` — e um braço de `match` de três linhas. **É pouco e não é zero**, e só
deixa de ser suposição no dia em que o job `windows` da CI rodar de verdade, que
é a pendência 38 e continua aberta.

## 47 · A suíte verde não prova que um MOD abre

**Sintoma.** `cargo test -p seele-app --test frontend` passou com 191 verdes no
dia em que a auditoria de experiência reproduziu, no aplicativo nativo, um
convite inválido sem mensagem e a frase literal `Até NaN KiB, undefined px`. Os
dois defeitos estavam no código auditado. Nenhum teste os viu.

**Por que.** O que esta suíte alcança é texto: ela lê o HTML, o CSS e os
scripts, e cobra propriedades sobre eles. É muito, e não é tudo. Três coisas
ficam fora por construção:

1. **O DOM de verdade.** Nada aqui monta a página e aperta um botão. Um
   `getElementById` que devolve o elemento errado, um `undefined` que chega
   inteiro até uma frase, um `<script type="module">` recusado por origem — os
   três só aparecem quando a página roda.
2. **O WebView nativo.** O motor do Tauri é o WebKit do sistema no macOS e o
   WebView2 no Windows, e eles discordam justamente onde dói: `-webkit-app-region`
   vale num e não no outro, e `mod://localhost` é servido como
   `http://mod.localhost` no segundo. Medir num terceiro motor não responde por
   nenhum dos dois.
3. **A abertura de um MOD de ponta a ponta.** Que os bytes atravessem origem,
   CSP, IPC e cheguem a desenhar o botão do MESA é a única pergunta que
   interessa, e ela não é respondida por nenhuma das duas suítes atuais.

**O que foi feito em 2026-09-17**, e é a parte que texto alcança: unicidade de
`id` na página; referências de acessibilidade apontando para `id` que existe;
contratos de IPC cobrados a partir do que o Rust **serializa de verdade**, e não
de uma lista escrita no teste. Os dois primeiros guardas falharam antes do
conserto, nomeando exatamente o que a auditoria viu na tela.

**O que falta.** Um teste de fumaça por sistema operacional, sobre o binário que
vai ser distribuído: subir a janela, instalar um MOD conhecido, e conferir que
ele abre — e os caminhos de recuperação junto, com hash divergente, script
ausente e catálogo recusado. Isso precisa de decisão sobre ferramenta e sobre
CI, e não de mais um guarda de texto.

**Não vale fechar isto acrescentando guardas.** Eles já cobrem o que sabem
cobrir, e a armadilha é a que a auditoria nomeia: um teste que verifica se a
correção está escrita passa a valer pela correção, e a suíte fica verde
afirmando o que ninguém mediu.

## 48 · O `glib` do build de Linux está numa versão com unsoundness conhecida

**O aviso.** RUSTSEC-2024-0429, no `glib` 0.18.5: `VariantStrIter::impl_get`
passava um `&p` para uma função C variádica que escreve no ponteiro por baixo —
a mutabilidade errada não virou erro de compilação justamente por ser argumento
variádico. Com as otimizações dos compiladores recentes, essas escritas passaram
a ser simplesmente descartadas, e o `CStr::from_ptr` seguinte recebia `NULL`.

**O que ele é, e o que não é.** O próprio arquivo da advisory marca
`informational = "unsound"`: não é falha explorável de fora, é desreferência de
ponteiro nulo — **queda**. As funções alcançadas são
`VariantStrIter::{next, nth, last, next_back, nth_back}`, nas entranhas do
GVariant do GTK. Nenhuma linha nossa as chama; quem as alcançaria é o WebKitGTK
por dentro.

**Onde ele chega, medido e não deduzido** — `cargo tree -e normal` por alvo:

| alvo | ocorrências de `glib` |
|---|---|
| `aarch64-apple-darwin` | 0 |
| `x86_64-pc-windows-msvc` | 0 |
| `x86_64-unknown-linux-gnu` | 28 |

Ou seja: o `.dmg` e o `.exe` não carregam uma linha disto. **É risco só do
`.deb`.**

**Por que não é consertável aqui.** `patched = [">=0.20.0"]`, e a versão é
presa por `atk 0.18.2 → gtk 0.18.2 → muda 0.19.3 → tauri 2.11.5`. Um
`cargo update -p glib` trava zero pacotes. Sair da 0.18 exige o Tauri mover a
pilha gtk-rs inteira, que é decisão dele.

**Condição de saída.** Fecha quando o Tauri publicar uma versão que traga
`glib >= 0.20`, e a conferência é a mesma tabela acima. Fica registrada para
não voltar a ser um alerta vermelho sem contexto a cada push — que é o modo
mais rápido de ensinar uma equipe a ignorar alerta.
