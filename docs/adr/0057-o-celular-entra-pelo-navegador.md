# 0057 — No celular, o SEELE é uma página: web app instalável, sem app nativo

Status: **aceito** — decidido por quem responde pelo projeto em 2026-10-07. A
decisão é de plataforma. O aceite do M6 continua aberto e sem medida (ver «O que
não foi medido»).
Data: 2026-10-07
Sobre os commits `7f23706` e `a058457` (spike `voz-no-navegador`) e o
`docs/ios-sem-assinatura-2026-10-07.md`. Abandona o caminho do `8632668`.
Fecha o `[EM ABERTO — decisão de plataforma]` de `specs/06-clientes-gui.md:117`
(M6; D22 de `docs/plano-m0-m1.md:288`).
Linhas conferidas em `a058457`, no branch `mobile/ios`. O spike e a pesquisa
citados aqui ainda não estão no `main` (ver «O destino do branch `mobile/ios`»).

> **No celular, o SEELE abre no navegador e fica na Tela de Início. Não há app
> nativo nem loja. O celular só entra em servidores e nunca hospeda.** A voz vai
> por WebTransport direto ao `seeled`, com TLS do navegador até o servidor e
> nenhum gateway no meio.

> **Emenda os ADRs 0003, 0006, 0007, 0010, 0017, 0018, 0019 e 0039, e abre
> exceção nomeada, só no navegador, ao 0026 e ao 0046.** No 0006, revoga para o
> navegador a recusa de `0006:43`. A tabela está em «O que acontece com os ADRs
> que já existem».

> **Numerado 0057, e não 0056.** O desenho da sala pessoal por código já chama
> de «ADR 0056» a página dos MODs na tela inteira
> (`docs/superpowers/specs/2026-09-23-sala-pessoal-por-codigo-design.md:540`,
> `:852`), e o guarda do índice não percebe número repetido: ele guarda os
> números num `BTreeSet` (`xtask/tests/adr_index.rs:30-44`).

Nada disto está publicado. Pela API de releases, conferida em 07/10, a última
versão é a v0.15.0 (`e2fac4dab`, 2026-09-23), só com arquivos de desktop:
`.dmg`, `.app.tar.gz`, `.exe`, a CLI, `SHA256SUMS` e `latest.json` (também
`docs/ios-sem-assinatura-2026-10-07.md:6`). Por isso abandonar o nativo não
deixa ninguém para trás.

## O que estava em aberto, e por que o critério mudou

A spec tinha três candidatas, todas app instalado e nenhuma web: Flutter com
FFI, Swift e Kotlin, e Tauri Mobile (`specs/06-clientes-gui.md:119-123`).
Mandava decidir «com um protótipo descartável de áudio em background em cada
candidata» (`:125`), e o M6 repetia «só então decidir Flutter vs nativo»
(`specs/09-roadmap.md:99`). Nenhuma candidata foi prototipada para isso. O que
aconteceu foi outra coisa:

- **o Tauri iOS foi construído e roda.** No `8632668` a casca vira biblioteca, o
  projeto do Xcode entra no repositório e o celular ganha uma casca de uma mão;
- **e ele não chega a terceiros sem contrariar a licença ou pagar.** A pesquisa
  de 07/10 (`docs/ios-sem-assinatura-2026-10-07.md:14-25`, tabela em `:31-39`)
  concluiu:
  - o Personal Team serve de graça ao dono, renovado a cada 7 dias (`:16`);
  - para amigos, há o Personal Team em até 2 aparelhos extras e o AltStore
    Classic ou o SideStore com o Apple ID grátis de cada um (`:17`; rotas A′ e B
    em `:34-35`). Funciona, mas com renovação semanal, com bans de perfil desde
    julho de 2026 e contra a §2.4 da licença (`:18-20`);
  - a loja alternativa do Brasil, a AltStore PAL, exige o programa pago, de
    R$ 549,90/ano (`:25`, `:38`);
  - **para o público, sem pagar, só resta o navegador** (`:21`).

O critério deixou de ser «qual nativo segura o áudio em background» e passou a
ser **chegar a terceiros sem o programa pago e sem contrariar a licença**. Só
uma rota atende a isso, e o áudio em background foi medido nela, mas só em
parte.

## O que foi medido

Salvo indicação, a fonte é `spikes/voz-no-navegador/README.md`. O spike não fala
o protocolo do `seeled` (`:26-28`). Ele mede o navegador chegando a um QUIC com
voz.

| O quê | Medida | Fonte |
|---|---|---|
| Safari do Simulador iOS 27, em localhost | WebTransport abre em 6–71 ms; hash adulterado recusado; Opus codificado, ecoado e decodificado: **250/250** quadros, 0 erros, com um tom sintético de 440 Hz no lugar do microfone. O `wtransport` 0.7.2, que anuncia SETTINGS de rascunhos antigos, conversa com o Safari | `:97-120`; o risco em `:102-105` |
| iPhone, Safari, iOS 27.0.1, Wi-Fi | abre em **27–142 ms**; hash adulterado recusado **3/3**, mesmo com a exceção de certificado aceita na página; Opus por WebCodecs com 439 ↑, 2740 ↓, **0 perdidos**, 0 erros; rtt p50 **5 ms**, p95 8–10 ms; eco p50 **7 ms** | `:128` |
| Rodada 1: aba padrão, tela bloqueada | o `AudioContext` passa a `interrupted` 0,1 s antes de a página ficar oculta; nada sai do microfone; a sessão WebTransport continua | `:136-151` |
| Rodada 3: aba, `play-and-record` + `<audio>` | tudo é cortado no mesmo milissegundo aos 15,7 s, **16 s antes** de a página ficar oculta | `:153-165`, `:215-218` |
| Rodada 4: a mesma configuração | oculta por 19,6 s, **sem corte**: 18,9 s de som tocado e 981 quadros de microfone, sem envio porque «Falar» estava desmarcado. O ícone da Tela de Início não chegou a existir | `:182-201` |
| Rodada 5: a mesma configuração | 1º bloqueio, de 128,6 s: **115 s de voz tocada** no iPhone, sem envio porque «Falar» estava desmarcado. 2º bloqueio, de «40 s ou mais»: o iPhone manda **50 quadros/s** e o Mac recebe 1104, com **0 perdidos**. Só o 2º teve voz nos dois sentidos | `:203-214`; `a058457` |
| Distribuição nativa no iOS | nenhuma rota para terceiros dispensa o programa pago sem contrariar a licença | `docs/ios-sem-assinatura-2026-10-07.md:14-25`, `:31-39` |

O eco de 7 ms não inclui o quadro de 20 ms, o buffer de 40 ms nem a latência de
saída (`:93-95`). Não é medida de boca a ouvido.

**Quem vale é a rodada 5.** O assunto do `7f23706` conclui que numa aba do
Safari «a tela bloqueada cala microfone e som no mesmo instante, com ou sem a
Audio Session API e a tag `<audio>`». A rodada 5 e o `a058457` mostram o
contrário, e o README do spike já marca aquela conclusão como desmentida. Na
rodada 5, o iPhone bloqueado **ouviu por 128 s e falou por 40 s ou mais**: os
dois sentidos foram medidos, mas não no mesmo bloqueio.

**Medido em `spikes/alpn-no-mesmo-socket/`**, em 07/10, no Chromium e no Safari
do Simulador, nunca no aparelho. Vale como indício, não como prova:
- um `quinn::Endpoint` só, com ALPN `seele/1` e `h3` e o certificado escolhido
  pelo ALPN. O Chromium 151 e o Safari do Simulador iOS 27 entram pelo hash do
  certificado curto, e o cliente `seele/1` recebe o certificado longo de sempre;
- os dois navegadores **recusam o certificado de hoje**, válido de 1975 a 4096,
  mesmo com o hash certo. Também aceitam uma lista de 60 hashes e não conferem o
  nome do certificado;
- a recusa chega ao servidor como alerta TLS (46 no Chromium, «TLS alert error»
  no Safari) e volta à página em 1–24 ms, em localhost. O cabeçalho `Origin`
  chega ao servidor;
- WebCrypto Ed25519 com chave não extraível funciona nos dois. A chave pública
  sai com 32 B e a assinatura com 64 B, que é o que o aperto de mão espera
  (`crates/seele-server/src/session.rs:550-554`, `:587-590`). Se a assinatura
  randomizada do Safari passa no `verify` (`:594-599`) não foi medido.

## O que não foi medido

- **Nada foi medido na forma decidida, e nenhuma medida de hoje conta para o
  aceite.** O aceite se mede no web app instalado, com a página vinda da origem
  pública, contra o `seeled` de produto (com `h3` pelo ALPN e o certificado
  curto) e falando o protocolo de produto. Hoje:
  - as rodadas 1, 3, 4 e 5 dizem «aba» (`README:138`, `:155`, `:184`, `:205`).
    A rodada 2 não diz, e a detecção de web app só entrou no `7f23706`
    (`spikes/voz-no-navegador/pagina/pagina.js:71-73`);
  - a página vinha do próprio servidor, aceita por exceção de certificado
    (`README:39-43`; `pagina.js:195`);
  - o servidor era o do spike, com keepalive de 3 s e ocioso de 30 s
    (`spikes/voz-no-navegador/src/main.rs:191-192`), e não o do `seeled`, com
    ocioso de 20 s e keepalive de 5 s
    (`crates/seele-proto/src/transport.rs:33`, `:39`).
- **O aceite do M6**: 30 minutos com a tela bloqueada, sem queda e com consumo
  de bateria documentado (`specs/09-roadmap.md:103`), além de chamada telefônica
  recebida e troca de rede (`:101`). O que existe são 128,6 s ouvindo e «40 s ou
  mais» falando, em bloqueios diferentes (`README:211-212`).
- **Internet e 4G.** Só foi medido Wi-Fi (`README:128`) na mesma rede (`:39`). O
  próprio README diz o que falta (`:221`).
- **Android: nenhuma linha** (`README:130-131`).
  - Pela leitura do código do Chromium em 07/10, a captura em segundo plano fica
    atrás de uma flag desligada por padrão, `kAndroidEnableBackgroundMediaCapturing`,
    e só com ela o Chrome sobe um serviço em primeiro plano do tipo microfone.
    Pela documentação do Android («Behavior changes» do Android 9 e
    «fgs/service-types»), sem esse serviço o app em segundo plano perde o
    microfone. Por isso a voz provavelmente não sobrevive ao bloqueio no Chrome
    do Android. **É inferência.**
  - Segundo o browser-compat-data, lido em 07/10, o Firefox do Android não tem
    WebCodecs, e o Chrome do Android não conhece `requireUnreliable`.
- **O web app instalado, que é justamente a forma que esta página decide.**
  - O ícone da Tela de Início não chegou a existir (`README:184-186`).
  - Um bug do WebKit aberto desde 2022 (236509) diz que o microfone do web app
    para ao minimizar e o do Safari não.
  - Que URL o iOS grava no ícone, e se uma `CryptoKey` não extraível guardada em
    IndexedDB sobrevive a fechar e reabrir o web app e a reiniciar o iPhone.
- **A causa da rodada 3** (`README:215-218`).
- **O tamanho de datagrama.** O Safari diz `maxDatagramSize` 65535 e o servidor
  diz 1295 (`README:222-223`).
- **DTX, modo VOIP e troca de bitrate** no WebCodecs (`specs/03-audio.md:50-53`,
  ADR 0036). O spike só usou 32 kbit/s e quadro de 20 ms (`pagina.js:14-15`).
- **Uma página de origem pública abrindo WebTransport para `192.168.x.y`.** O
  spike servia a página do próprio servidor. Segundo o Chrome Platform Status,
  lido em 07/10, o Chrome 147 estende ao WebTransport o pedido de permissão de
  rede local.
- **O `seele-proto` compilado para `wasm32-unknown-unknown`**: o alvo não está
  instalado em nenhuma toolchain desta máquina.
- **O que `spikes/alpn-no-mesmo-socket/` viu, repetido num iPhone**: o despacho por
  ALPN com o `wtransport` adotado, o ocioso de 20 s do `seeled` com a página
  oculta, um certificado curto com a mesma chave do longo e uma lista longa de
  hashes.
- **O tempo de falha de um servidor fora do ar.** Ninguém mediu, em lugar
  nenhum. Só a recusa foi medida, e só pelo protótipo.
- **O bug de controle de fluxo do WebKit** (16 MB ou 7600 fluxos) no iOS 27.0.1.
  Ele foi reproduzido no iOS 26.6.1 (`docs/ios-sem-assinatura-2026-10-07.md:218`,
  `:250`).
- **Outras lacunas:**
  - o iCloud Private Relay (`docs/ios-sem-assinatura-2026-10-07.md:408`);
  - a saída de som no iOS, se pelo alto-falante ou pelo fone de ligação.

## A decisão

1. **No celular, iOS e Android, o SEELE é um web app instalável**: abre no
   navegador e fica na Tela de Início. Não há app nativo nem loja. O Tauri iOS
   do `8632668` deixa de ser caminho de produto.
2. **O celular só conecta e nunca hospeda.** `specs/06-clientes-gui.md:113` diz
   «Sem administração» e `specs/09-roadmap.md:100` diz «Nada de administração»,
   mas nenhum dos dois diz «não hospeda», e a pesquisa pedia essa decisão por
   escrito (`docs/ios-sem-assinatura-2026-10-07.md:364`). Esta linha diz. O
   HOSPEDAR AQUI (`specs/06-clientes-gui.md:86-101`) não existe no celular.
   Também não existe a chamada privada do 0047 §4.3, que é «o servidor de quem
   liga» (`0047:325-329`).
3. **Transporte: WebTransport (HTTP/3) direto ao `seeled`**, com o certificado
   aceito por `serverCertificateHashes`. O TLS vai do navegador até o `seeled`,
   sem gateway no meio.
4. **Áudio: WebCodecs, Opus a 48 kHz, mono, quadros de 20 ms**
   (`specs/03-audio.md:47-49`). No iOS, a página declara
   `navigator.audioSession.type = "play-and-record"` e toca por uma tag
   `<audio>` alimentada por `MediaStreamDestination`
   (`spikes/voz-no-navegador/pagina/pagina.js:319-337`). A rodada 5 tinha mais
   três peças: os metadados da Media Session (`:338-340`), o `AudioContext` a
   48 kHz com `latencyHint: "interactive"` (`:326`) e o eco, a supressão e o
   ganho do navegador (`:361-362`). **O produto parte da configuração inteira**,
   porque tirar qualquer peça não foi medido.

Fecha «plataforma mobile (M6)», que estava na lista «O que ainda não tem ADR»
do `docs/adr/README.md` (linha 96 em `a058457`). **Não declara o M6 cumprido.**

### O que muda no `seeled`

- **O `h3` entra no mesmo socket e no mesmo `quinn::Endpoint`**, separado pelo
  ALPN.
  - A porta continua única (`specs/01-arquitetura.md:45`), e um `Endpoint` é o
    único leitor do socket (`0037:42-45`). Essas duas restrições decidem.
  - Hoje o aceite não olha o ALPN, porque só existe um
    (`crates/seele-server/src/lib.rs:789-815`;
    `crates/seele-server/src/tls.rs:118`).
  - Que isso funciona só foi visto em `spikes/alpn-no-mesmo-socket/`, fora do
    aparelho. Se a repetição no iPhone falhar (ver «O que fica pendente»), esta
    seção reabre.
- **A camada HTTP/3 é o `wtransport` 0.7.2**, adotando a conexão do `Endpoint`
  compartilhado em vez de abrir um `Endpoint` próprio.
  - É a implementação contra a qual os navegadores foram medidos. A medida no
    iPhone foi com o endpoint próprio do spike e os temporizadores dele
    (`spikes/voz-no-navegador/src/main.rs:191-192`), então ela só vale em parte
    para a conexão adotada.
  - Os crates `h3` ficam de fora. Segundo a API do crates.io, lida em 07/10, o
    último release deles é de maio de 2025, e o `h3-webtransport` exige do `h3`
    a feature `i-implement-a-third-party-backend-and-opt-into-breaking-changes`.
  - O risco que fica: o `wtransport` anuncia SETTINGS de rascunhos antigos
    (`docs/ios-sem-assinatura-2026-10-07.md:219`). Um navegador que só fale o
    rascunho novo quebraria.
  - A dependência entra pelo `cargo deny` (`deny.toml`). O `check-deps` não a
    vê, porque só olha arestas entre crates do workspace
    (`xtask/src/check_deps.rs:161-163`, `:179`).
- **A sessão passa a correr sobre dois transportes.** O `session.rs` foi escrito
  contra `quinn::Connection` (`:303`, `:1548`), assim como a tela
  (`crates/seele-server/src/tela.rs:960`) e os anexos
  (`crates/seele-server/src/transfer.rs:436`). Essa é a maior parte do trabalho.
- **O `seeled` não serve a página e não abre TCP.**
- **Quando um navegador recusa o certificado, isso vai para o log** de quem
  hospeda, com a frase «um navegador recusou o certificado: link velho, ou link
  de outro servidor que atende neste endereço noutra rede». A segunda metade é a
  colisão de LAN do `0003:27-29`. Hoje uma falha antes do aperto de mão vira um
  aviso genérico (`crates/seele-server/src/lib.rs:797-801`), e que o alerta TLS
  chega ao servidor só foi visto no protótipo.
- **Quem hospeda é avisado quando o convite não serve ao navegador**, ou seja,
  quando o servidor só é alcançável pelo degrau 4 (ver «O que isto custa»).
- **Uma sessão vinda por `h3`, num servidor com MOD habilitado, é recusada já no
  `Hello`, antes de gastar o convite**, com um `DisconnectReason` próprio.
  - Hoje o `ModsExigidos` sai (`session.rs:1153-1158`) depois de o convite ser
    gasto (`:686-688`) e depois de `register_or_find` prender o apelido à chave
    (`:735-736`). A ordem está em `specs/02-protocolo.md:40-48`.
  - Uma chave que nunca entra ficaria ocupando o nome, que é o que
    `session.rs:611-614` existe para impedir.
  - Uma variante nova é mudança de protocolo, e vai na mesma subida de versão da
    mensagem dos hashes (ver a emenda ao 0003).

### Emenda ao ADR 0003: o certificado curto

**O certificado de hoje não muda.**
- Ele vale de 1975 a 4096: o `seeled` chama `rcgen::generate_simple_self_signed`
  (`crates/seele-server/src/tls.rs:38-47`), e o padrão do `rcgen` 0.14.8
  (`Cargo.lock:4477-4478`) está em `rcgen-0.14.8/src/certificate.rs:84-85`, no
  registro do cargo. Ele fica no banco (`tls.rs:66-89`).
- O pino do desktop é o SHA-256 do DER dele
  (`crates/seele-proto/src/transport.rs:302-316`, chamado em
  `crates/seele-core/src/tofu.rs:414-415`).
- As marcas do ponto de encontro saem desse mesmo número; o código diz que «são
  os da v0.15.0 e não mudam» (`crates/seele-proto/src/encontro.rs:267-269`).

Girar esse certificado poria todo cliente nativo em chave trocada, mudaria as
marcas e invalidaria o `fp=` de todo link guardado.

Decidido:
- **um segundo certificado, só para `h3`**, escolhido no aperto de mão pelo ALPN
  que o navegador pede. O nome (SNI) não serve: o endereço pode ser um IP
  literal, e aí não há SNI. O convite também aceita nome (`0006:17`), e a saída
  recomendada para quem muda de IP é DDNS (`0047:145-148`), mas o ALPN cobre os
  dois casos;
- **com a mesma chave P-256 do certificado de hoje** (o porquê está logo
  abaixo);
- **validade de no máximo 13 dias**, como no spike
  (`spikes/voz-no-navegador/src/main.rs:15-17`, `:140-145`). A especificação
  recusa acima de 14 (`docs/ios-sem-assinatura-2026-10-07.md:206`);
- **certificados pré-gerados em janelas sobrepostas e guardados no banco.** A
  assinatura ECDSA do `ring` usa um nonce aleatório
  (`ring-0.17.14/src/ec/suite_b/ecdsa/signing.rs:172-177`, no registro do
  cargo), então reassinar gera outro DER e outro hash. Todo certificado
  anunciado precisa existir guardado, e nenhum é regenerado;
- **os certificados curtos são herdados junto com a identidade** entre os
  servidores guardados da mesma máquina, como o longo já é (`tls.rs:180-230`).
  Gerados em cada banco, teriam DERs diferentes, e os hashes do celular
  quebrariam a cada troca de servidor: é o mesmo defeito que `tls.rs:186-190`
  conserta.

**A frase «sem renovação» do 0003 (`0003:7`) deixa de valer para o `seeled`.**
Ele passa a emitir certificados, sem CA e sem rede, mas emite.

**Recusado: mover o pino para a chave pública (SPKI).**
- Mexe no que o `0003:9` protege («tirar TOFU depois quebra clientes que já
  pinaram»), nas marcas da v0.15.0 e no sentido do `fp=` para cliente velho.
- No navegador não ajuda, porque o hash do DER muda a cada emissão de qualquer
  jeito.

**No navegador, a âncora é a lista de hashes, e não o `fp` — até existir uma
conferência antes do `Hello`.**
- O navegador confere só a lista passada a `serverCertificateHashes`. A página
  nunca vê qual certificado foi apresentado.
- O cliente fala primeiro: o `Hello` leva o convite, o apelido e a chave pública
  (`crates/seele-core/src/client.rs:1901-1909`), e a assinatura sai logo depois
  (`:1934-1937`). O que a sessão entregar chega depois de o convite e a
  assinatura terem saído. Isso contraria o pré-requisito escrito para a 1.0,
  «conferir o `fp` **dentro do TLS**, antes do `Hello`»
  (`docs/analise-para-a-1.0-2026-09-22.md:287-290`), e o próprio adendo do 0003
  (`0003:31-38`).
- Por isso, enquanto essa conferência não existir, **os hashes do link têm o
  peso do `fp`**. Um link com o `fp=` intacto e os hashes trocados é um
  intermediário completo, invisível antes do `Hello`.
- **Por que a mesma chave, então**: é o único arranjo em que o `fp` pode voltar
  a ser a âncora no navegador. Se, antes do `Hello`, o servidor entregar o DER
  do longo e o de **todo** curto cujo hash a página passou ao navegador, a
  página confere que cada DER bate com o hash que ela passou, que o longo bate
  com o `fp` e que cada curto tem a chave pública do longo. O TLS provou a posse da chave do certificado apresentado, e
  todos os da lista têm a chave do `fp`: quem respondeu tem a chave do `fp`.
  Conferir só parte da lista não prova nada, porque a página não sabe qual
  certificado foi usado. Com uma chave por janela, essa conferência não existe.
- **A regra de precedência**, versão web de `0003:13-15`:
  - os hashes guardados ficam **por `fp`, nunca por endereço**;
  - para um `fp` já conhecido, os hashes guardados e ainda válidos têm
    precedência sobre os do link;
  - o hash do link só é usado quando nenhum guardado vale. Juntar os dois
    conjuntos poria o hash de um atacante dentro do pino.
- **O pino deixa de ser por endereço no navegador.** O adendo de LAN
  (`0003:40-50`) vale de graça: o navegador confere o hash em todo candidato, e
  o servidor de outra casa é recusado antes do `Hello`.

**No navegador não existe primeiro contato cego.** Sem o hash, o WebTransport
não abre. O 0006 chama o contato sem impressão de primeiro contato «que na
prática é cego» (`0006:12`), e o 0047 registra que, para o endereço digitado à
mão, «sem `fp` também não haveria verificação» (`0047:149-150`). No navegador,
esse caminho é impossível. A tela diz isso, em vez de deixar a pessoa descobrir.

**No navegador, o aviso de chave trocada não tem como existir.**
- O `0003:5` quer esse aviso «bloqueante, impossível de ignorar», mas o
  navegador não mostra o certificado à página. No Safari, a recusa volta como
  `WebTransportError` sem motivo (`README:110`). No Chromium, a mensagem traz o
  código QUIC (`QUIC_TLS_CERTIFICATE_UNKNOWN`, `README:84`; a página guarda
  `${e.name}: ${e.message}`, `pagina.js:204`), fora de qualquer padrão.
- Nos dois, chave trocada e link vencido chegam iguais à página. É «o produto
  sabe e não conta».
- O que dá para contar:
  - o link carrega a data de validade, e a tela diz «este link venceu em DD/MM»;
  - a recusa voltou em 1–24 ms no protótipo, em localhost. Se um servidor fora
    do ar demorar de forma distinguível, o que ninguém mediu, a página pode dizer
    «alguém respondeu e não é quem você esperava». Até a medida, isso não é
    produto.

EM ABERTO, com o critério de cada item:
- **Quantos hashes futuros a sessão entrega, por qual mensagem, e como os DERs
  chegam antes do `Hello` no caminho `h3`.**
  - Mensagem nova é protocolo novo. Subir para v9 corta quem está na v8
    (`crates/seele-proto/src/version.rs:89-96`, `:124`).
  - Critérios: nenhum segredo sai antes da conferência da lista inteira; a
    próxima subida de versão já marcada; e o comprimento de lista que o Safari
    aceitar **num iPhone**.
- **Se o navegador aceita um certificado curto com a mesma chave do longo.**
  - Isso não foi medido.
  - Se não aceitar, cada janela ganha chave própria, a conferência acima deixa
    de existir e a lista de hashes fica sendo a única âncora para sempre. Esse
    custo precisa ser escrito aqui antes do código.

### Emenda ao ADR 0006: o link

- **O convite do celular é um link `https://<origem>/…#<corpo>`.**
  - O navegador não abre `seele://`.
  - O fragmento não vai ao servidor da página.
  - **Fica proibido pôr parâmetro de convite em query string.**
- **O `0006:43` se aplica, e esta página revoga a recusa só para o navegador.**
  Ele temia «um servidor nosso no meio». O fragmento não chega ao servidor da
  página, mas o código que a origem entrega o lê, guarda a chave e escolhe para
  quais hashes discar. A origem não está no caminho dos dados, e está no caminho
  do **código** (ver «O que isto custa»).
- **A página nunca conecta ao abrir um link.** Ela mostra «ENTRAR EM
  `<servidor>`?» e espera o toque.
  - O `0006:44` já mandava «perguntar antes de conectar» quando o esquema fosse
    clicável, e a análise da 1.0 pede uma tela que «nunca conecta sozinha»
    (`docs/analise-para-a-1.0-2026-09-22.md:294-297`).
  - No navegador todo link é clicável. Conectar ao abrir entregaria o IP,
    gastaria o convite (`session.rs:686-688`) e assinaria o desafio de um
    servidor escolhido por quem escreveu o link, sem nenhum toque.
- **A página lê o fragmento e o apaga com `history.replaceState` antes de
  qualquer outra coisa.** O `start_url` do manifest não tem fragmento. O web app
  aceita link colado, e a aba oferece «Copiar link».
- **Campos novos: os hashes do certificado curto (nome provisório `wh=`) e a
  data de validade.**
  - O cliente nativo os ignora (`0006:38`; `crates/seele-proto/src/uri.rs:493-496`).
  - Cada hash é validado com o rigor do `fp`: truncado é recusado, nunca
    ignorado (`0006:36`, `:49`).
- **Na forma `https`, o `fp=` e os hashes passam a ser obrigatórios.** Hoje o
  `fp` é opcional (`uri.rs:43`, `:102-103`; no 0006 só o endereço é
  obrigatório, `0006:20-24`). A forma `seele://` continua aceitando link sem
  `fp`. Um teste prende as duas coisas.
- **Um analisador só.** Hoje o `uri::analisar` aceita só `seele://`
  (`uri.rs:67`, `:421-425`), e um teste recusa `https://` (`uri.rs:686-691`).
  Ele passa a aceitar também a forma `https` desta origem, como a análise da 1.0
  já recomendava (`docs/analise-para-a-1.0-2026-09-22.md:305-306`).
- **O convite do navegador não leva `enc=`**: o bilhete do degrau 4 não serve
  a ele.
- **O `v=` é a versão do SEELE que hospeda, e não a do protocolo**
  (`uri.rs:108-123`), preenchida por `Hospedagem::versao_desta_build()`
  (`crates/seele-server/src/hospedagem.rs:218-220`).
  - O `seeled convite` não o põe (`crates/seele-server/src/main.rs:200-203`), e
    um build feito à mão também não (`uri.rs:120-122`).
  - **O `v=` nunca é concatenado num caminho.** A página escolhe o pacote numa
    lista de versões publicadas. O filtro de hoje deixa passar `..`, porque
    aceita qualquer sequência de alfanuméricos, `.`, `-` e `+` (`uri.rs:483-491`).

EM ABERTO: **o caminho do link (`/e`, como em
`docs/analise-para-a-1.0-2026-09-22.md:280`, ou a raiz) e se o mesmo link leva o
desktop ao `seele://`** (`:300-301`).
- O desktop v0.15.0 publicado recusa `https://` (`uri.rs:689`). Para ele, só uma
  página `/e` que redirecione para `seele://` serve (`:298-301`).
- Decide-se junto com a origem. O critério é um link só servir às duas
  plataformas, inclusive à v0.15.0.

### Onde a página mora

Decidido:
- **Não no `seeled`.** Uma origem por servidor daria um web app por servidor,
  cada um com aviso de certificado. E, segundo o código do Chromium lido em
  07/10 (`installable_evaluator.cc`), o Chrome não instala web app de página sem
  certificado válido.
- **Uma origem só: estática, HTTPS com certificado válido e sem script de
  terceiros.** Em 22/09, o revisor da 1.0 mediu que `seele.app.br` injetava o
  beacon do Cloudflare e não mandava CSP
  (`docs/analise-para-a-1.0-2026-09-22.md:302-304`). Do jeito que estava, não
  serve, e precisa ser medido de novo.
- **A CSP e os cabeçalhos da origem:**
  - `script-src 'self' 'wasm-unsafe-eval'`, sem `unsafe-inline`;
  - `frame-ancestors 'none'` e HSTS, que só existem como cabeçalho: uma CSP em
    `<meta>` ignora `frame-ancestors`;
  - o `connect-src` tem de aceitar WebTransport para IPs arbitrários (pela
    especificação o WebTransport cai nele; não medido). Por isso **a CSP não
    bloqueia exfiltração, só injeção**. A CSP do desktop é
    `connect-src ipc: http://ipc.localhost`
    (`apps/seele-app/tauri.conf.json:23`).
- **Nenhum `innerHTML` na casca, com guarda.** Hoje a `ui/` não tem
  `innerHTML` nem `insertAdjacentHTML`: a busca só acha comentários
  (`apps/seele-app/ui/mods-contribuicoes.js:15`, `:33`). No navegador, um XSS
  alcança a chave da pessoa, então essa propriedade ganha um guarda, provado
  revertendo.
- **A origem é escolhida uma vez.** A identidade e os hashes guardados do
  cliente web ficam no armazenamento dela, e trocá-la cria pessoas novas para o
  ADR 0030.
- **Um pacote da página por versão publicada do SEELE, guardado.**
  - Todo quadro sai carimbado com a versão global (`version.rs:89-96`), então um
    pacote único deixa de fora todo servidor de outra versão.
  - O pacote é escolhido pelo `v=` do link, quando existe, ou pela última
    visita.
  - Vale com ou sem o ADR 0046, que continua proposto.
- **Um pacote revogado deixa de ser servido pela origem.** O 0046 quer que uma
  versão revogada «não é alcançável», conferido «ao entrar» (`0046:105-116`). No
  navegador quem conferiria seria o próprio pacote, então quem recusa é a
  origem. Sem isso, um link com o `v=` de uma versão com falha rodaria o código
  velho com a chave da pessoa.

EM ABERTO:
- **Qual domínio.** A pesquisa sugeriu o github.io
  (`docs/ios-sem-assinatura-2026-10-07.md:230`), que não deixa definir
  cabeçalho e por isso é reprovado pelo critério 2. Critérios, nesta ordem:
  1. nenhum script de terceiros;
  2. CSP, `frame-ancestors` e HSTS por cabeçalho;
  3. guardar um pacote por versão para sempre;
  4. não mudar nunca.
- **A primeira visita sem `v=`.** É todo convite do `seeled convite` e de build
  feito à mão. Ainda não tem critério.
- **Se o `seeled` aceita só a origem oficial pelo cabeçalho `Origin`.**
  - A favor: um navegador não forja o `Origin`, então uma origem parecida, com
    um cliente adulterado, deixaria de alcançar os servidores.
  - Contra: forks e páginas próprias ficam de fora, e a origem fica gravada em
    todo `seeled`.
- **O canal de diagnóstico.** O spike mandava o relatório por HTTPS ao próprio
  servidor (`pagina.js:178-181`; `README:50-55`). Na forma decidida, o `seeled`
  não abre TCP e a origem é estática, então não há canal quando o WebTransport
  nem abre. A restrição: o relatório nunca leva o convite.

### O cliente web

Decidido:
- **É a mesma `apps/seele-app/ui/` do desktop, com uma ponte web no lugar de
  `window.__TAURI__`** (`apps/seele-app/ui/base.js:36-37`).
  - O precedente existe: `tools/carga-da-casca.py:22-27`, `:117-122` sobe a
    casca no Chromium com um `__TAURI__` falso.
  - O produto continua com uma casca só (ADR 0039).
  - **A ponte não herda o `nenhuma_casca_reabre_a_voz_jogando_fora_os_controles`**
    (`crates/seele-conformance/tests/voz_na_reconexao.rs:79`). Ele lê fonte Rust
    procurando `Aviso::Reconectado` e `reopen` (`:67-75`, `:93-99`), nomes que
    uma ponte em JavaScript não tem. A promessa de `0039:96-98` («sem que
    ninguém precise reescrever o teste») não vale para a ponte. O defeito que ele
    guarda (mudo, isolamento, modo e ganhos zerados na reconexão) ganha um guarda
    próprio sobre a ponte, ou sobre o crate `wasm` se a reconexão morar lá.
  - **Nada amarra a ponte ao conjunto de comandos.**
    `apps/seele-app/tests/frontend.rs:472-486` amarra cada `invoke` ao
    `generate_handler!` de `lib.rs`. A ponte precisa do guarda equivalente; sem
    ele, é um guarda que existe e não funciona no web.
- **Nenhuma linha do protocolo é escrita em JavaScript**
  (`specs/06-clientes-gui.md:19`).
  - O codec é o `seele-proto` compilado para `wasm32`.
  - **O enquadramento de 4 bytes e o cabeçalho de mídia também vão no `wasm`,
    e o JavaScript só move bytes opacos.** Hoje o enquadramento mora no
    `seele-core` e no `seele-server` (`crates/seele-core/src/frame.rs:9`,
    `crates/seele-server/src/frame.rs:5`), e não no `seele-proto`; o cabeçalho
    `ver ssrc seq ts` está em `crates/seele-proto/src/media.rs:5-9`. Uma casca
    que sabe nomear um `ssrc` já tem lógica dentro (`0002:18`).
  - O motivo: o `postcard` «amarra clientes a Rust» (`0001:4`) e indexa
    variante por posição (`0046:49-54`), e um segundo codificador escrito à mão
    divergiria a cada versão.
  - **O crate web entra em `RULES`, em `xtask/src/check_deps.rs`, ao lado do
    `seele-core`** (`:70`), dependendo do `seele-proto` e das peças sem E/S que
    forem extraídas. Não como exceção: a exceção de `:96-100` existe porque
    aquele crate não entrega nada. Um crate do workspace sem entrada é erro
    (`:44-46`, `:168-174`), o que exige que o crate web seja membro do
    workspace.
- **Sem npm, sem bundler, sem `package.json`** (`0019:15`). O `cargo build`
  gera o `.wasm`, mas a cola em JavaScript sai do `wasm-bindgen-cli`, um binário
  fora da árvore do workspace a não ser que entre como dependência do `xtask`.
  Ele tem de ficar sob o `cargo deny`; **o como está EM ABERTO**. A frase «sem
  passo de build no frontend» ganha essa exceção.
- **A identidade é uma chave Ed25519 do WebCrypto, não extraível**, guardada no
  armazenamento da origem.
  - O que ela protege: um script que passe uma vez não leva a chave embora. O
    que não protege: qualquer script na origem pode usá-la enquanto roda (ver «O
    que isto custa»).
  - O custo: sem backup e sem troca de aparelho (`0004:14`, `:18`). Perder o
    armazenamento é perder a identidade.
  - **A chave só nasce no ato explícito de entrar com um convite**, nunca ao
    carregar a página. A página não distingue «nunca teve chave» de «perdeu a
    chave», e uma chave criada ao carregar substituiria em silêncio uma
    identidade perdida, contra o `0017:38` («reportado, nunca substituído»).
  - A página pede `navigator.storage.persist()` e mostra a resposta.
  - No web, o `NicknameTaken` (`crates/seele-server/src/permissions.rs:212-213`;
    `session.rs:751-758`) ganha tela própria: «este aparelho pode ter perdido a
    identidade; quem hospeda libera o nome». É o caso do `0017:37`.
- **Nenhum MOD roda na origem do app.** Ali ele alcançaria a chave da pessoa e
  poderia assinar o desafio de outro servidor. No v1, um servidor que exige MOD
  (`specs/02-protocolo.md:40-48`) é recusado com o motivo dito: «este servidor
  exige MODs que o celular não roda». A recusa vem do `seeled`, no `Hello`, antes
  de o convite ser gasto (ver «O que muda no `seeled`»).

EM ABERTO: **quanto do núcleo vai para `wasm` além do `seele-proto`.**
- As duas opções e o custo de cada uma:
  - extrair as peças sem E/S do `seele-core` e do `seele-audio` (estado,
    bateria, jitter, gate, mixer) mexe no núcleo do desktop publicado;
  - deixá-las em JavaScript duplica o `state.rs` e a política do `enlace.rs`, e
    é **exceção nomeada a `specs/06-clientes-gui.md:19` e a `0019:11`**: põe
    estado e lógica de sessão no frontend.
- Critério, em duas etapas:
  1. rodar `cargo check -p seele-proto --target wasm32-unknown-unknown`. Se
     falhar, o item do codec acima reabre;
  2. um teste de conformidade que entrega a mesma sequência de `ServerMessage`
     aos dois produtores de `Snapshot`.
- A exceção em JavaScript só entra com data para sair.

EM ABERTO: **o aperto de mão assina o nonce cru.**
- O cliente assina qualquer nonce que receber (`client.rs:1921-1937`), e o
  servidor confere sobre o nonce cru, sem contexto e sem `fp`
  (`session.rs:559-560`, `:594-599`).
- Pela leitura do código, não demonstrado: qualquer servidor em que a pessoa
  entra pode repassar o `Challenge` de outro servidor onde ela é membro, e
  entrar lá como ela. O defeito já existe no nativo. No navegador surge mais um
  oráculo: qualquer script na origem.
- A saída candidata é assinar `contexto ‖ fp ‖ nonce`. Isso quebra o repasse
  por um anfitrião de outro `fp`, e não o de script na origem. Servidores
  guardados na mesma máquina dividem o `fp` (`tls.rs:180-230`), então o vínculo
  vale por máquina, o que é aceitável: o operador é o mesmo.
- Critério: a mesma subida de versão da mensagem dos hashes.

### Áudio e transporte: emendas para o cliente web

- **O DSP da plataforma é aceito** (emenda o ADR 0007 para o web).
  - O motivo do 0007 era o risco de build em C++ (`0007:5-6`), que aqui não
    existe.
  - O spike ligou o cancelamento de eco, a supressão e o ganho do navegador
    (`pagina.js:361-362`).
  - Celular sem fone é o caso que o 0007 deixou sem suporte (`0007:9`).
  - O efeito no limiar do 0055 e no VAD do 0015 não foi medido.
- **Quadro perdido vira silêncio com fade** (emenda `specs/03-audio.md:73` e o
  ADR 0010 para o web).
  - Pela leitura da especificação, ainda não conferida, o `AudioDecoder` não tem
    chamada para ocultar quadro ausente.
  - No web, o contador `quadros_ocultados` (`specs/03-audio.md:75`) passa a
    contar outra coisa.
  - A alternativa é Opus em `wasm`, que reabre o ADR 0008.
- **Nada da voz depende de timer.** Com a página oculta, o `setInterval` cai
  para 1 por segundo (`README:144-145`). A interrupção é detectada pelo estado
  do `AudioContext` e pelo `muted` da trilha. O `navigator.audioSession.state`
  não serve: o bug 283417 do WebKit diz que ele é `undefined`, e o spike o lia
  (`pagina.js:323-324`).
- **Quando a captura ou a reprodução para** (trilha `muted` ou `ended`,
  `AudioContext` fora de `running`), **a pessoa e a sala ficam sabendo.** Sem
  isso, a rodada 3 e a provável falha do Android cairiam caladas. O que fazer
  além de contar fica em aberto.
- **O controle é o primeiro fluxo bidirecional da sessão, e não o «#0»**
  (`specs/02-protocolo.md:9`). No WebTransport, o fluxo 0 é o próprio CONNECT. O
  servidor já aceita assim (`session.rs:303`).
- **`requireUnreliable: true`**, porque o `seeled` não fala HTTP/2 sobre TCP
  (`docs/ios-sem-assinatura-2026-10-07.md:202`, `:207`). O spike não usava essa
  opção (`pagina.js:199-201`).
- **Reconectar dentro dos 5 minutos** em que o servidor guarda o lugar
  (`specs/02-protocolo.md:130`; `transport.rs:47-52`). O navegador não expõe
  migração de conexão, então cada troca de rede **provavelmente** é uma
  reconexão completa. É inferência: a migração não foi medida no Safari
  (`specs/02-protocolo.md:129`).
- **Um bug aberto do WebKit trava a conexão em 16 MB ou 7600 fluxos**
  (`docs/ios-sem-assinatura-2026-10-07.md:218`, fonte em `:250`).
  - O texto vai no fluxo de controle (`ClientMessage::SendMessage`,
    `crates/seele-proto/src/control.rs:1071`), e não num fluxo por mensagem,
    como `specs/02-protocolo.md:10` diz.
  - Quem abre fluxo novo: os anexos (`transfer.rs:436`), cada transmissão de
    tela (`tela.rs:960-962`) e as imagens de MOD (`session.rs:1548`). Um fluxo
    de tela assistida passa dos 16 MB.
  - O cliente conta os fluxos **e os bytes**, e reconecta antes do teto.

## O que acontece com os ADRs que já existem

**Nenhum é substituído.**

| ADR | O que este faz | O que muda |
|---|---|---|
| 0003 | **emenda** | certificado curto para `h3`, com a mesma chave, emitido e herdado pelo `seeled`; o pino nativo e as marcas ficam; «sem renovação» deixa de valer. No navegador o pino é por `fp` e não por endereço, não há primeiro contato cego nem aviso de chave trocada, e o adendo de LAN (`0003:40-50`) vale de graça |
| 0006 | **emenda, e revoga `:43` no navegador** | o link `https://…#…`; os campos novos, validados como o `fp`; `fp=` e hashes obrigatórios na forma `https`; a página nunca conecta ao abrir um link |
| 0007 | **emenda, só no web** | o DSP da plataforma é aceito |
| 0010 | **emenda, só no web** | sem PLC do Opus; silêncio com fade |
| 0017 | **emenda, só no web** | identidade e hashes no armazenamento da origem, e não em `$SEELE_HOME` (`0017:21-24`); o `:38` se cumpre porque a chave só nasce ao entrar com convite |
| 0018 | **emenda** | o `uniffi` não entra em M6 (`0018:17`), porque não haverá binding Swift nem Kotlin. A forma da `seele-ffi` continua |
| 0019 | **emenda** | o passo do `cargo` que gera `wasm` é permitido; o resto vale |
| 0026, 0046 | **exceção nomeada, só no web** | o código do cliente vem da origem a cada carga, contra `0026:44` (a alternativa 5); o service worker consulta a origem sozinho, contra `0026:36` e `0046:113-116`; a revogação é servida pela origem |
| 0039 | **emenda** | uma casca só continua valendo; a promessa de `0039:96-98` não vale para a ponte web |
| 0004 | respeita | a pendência de vínculo de aparelhos (`0004:18`) passa a ser do M6: desktop e celular são duas identidades |
| 0021, 0030 | respeita | o convite é gasto antes do `NicknameTaken` (`session.rs:686-688` vem antes de `:735-757`); trocar de origem cria pessoas novas para a portaria |
| 0042 | respeita (é proposto) | sem ele, o mesmo apelido no desktop e no celular dá `NicknameTaken`, já com o convite gasto |
| 0045 | respeita (é proposto) | um MOD de túnel no anfitrião é a única saída para quem só é alcançável pelo degrau 4 (ver «Riscos») |
| 0047 | respeita (é rascunho) | no navegador, «o link volta a funcionar amanhã» só vale até o último hash anunciado, e não existem o quarto nem o degrau 4 |
| 0001, 0002, 0014, 0020, 0022, 0037 | respeita | o que cada um passa a significar no navegador está nas seções acima e em «O que isto custa». O desktop continua no Tauri (0020) |

## Alternativas

1. **As três da spec** (`specs/06-clientes-gui.md:119-123`): Flutter, Swift e
   Kotlin, e Tauri Mobile. Todas são app instalado e batem na mesma parede de
   distribuição no iOS. Nenhuma foi prototipada para áudio em background.
2. **O Tauri iOS já construído** (`8632668`).
   - Roda no iPhone de quem o compila, com perfil de 7 dias
     (`docs/ios-sem-assinatura-2026-10-07.md:16`).
   - Para terceiros, exige o programa pago, ou o Personal Team em até 2
     aparelhos e o AltStore ou SideStore, com bans e contra a §2.4 (`:17-20`).
   - Hospedaria, mas só com áudio ativo, e a abertura de porta por UPnP «deve
     falhar», segundo a pesquisa. É previsão, não medida (`:22-24`).
3. **Pagar e distribuir pela AltStore PAL** (R$ 549,90/ano; `:25`, `:38`).
   - Compra distribuição só no Brasil, na UE e no Japão.
   - Exige notarização a cada versão, e os MODs esbarram na regra 2.5.2 (`:38`).
   - **Adiada, não recusada.** A pesquisa diz quando ela passa a valer a pena
     (`:383-390`).
4. **Um gateway no meio**: WebSocket ou HTTP/3 terminando o TLS num serviço
   nosso. Vai além do degrau 5 do ADR 0022 (`0022:96`, `:125-129`): além de
   retransmitir, termina o TLS e lê a voz, contra «nada no meio vê conteúdo»
   (`docs/analise-para-a-1.0-2026-09-22.md:276`). Recusada.
5. **Certificado de uma CA.** São duas opções, e elas não custam o mesmo:
   - o ACME com o domínio de quem hospeda, que o `0003:5` deixou como opção e
     nunca foi construído. Não é serviço nosso; o 0003 o descartou como padrão
     por exigir domínio e as portas 80/443 (`0003:6`). É a única rota para um
     link de navegador que não vence, para quem tem domínio;
   - um domínio nosso por servidor. É um serviço com estado e uma autoridade de
     nomes nossa, do outro lado da linha do 0022 (`0047:192-199`).
   - As duas ficam adiadas.
6. **Um cliente web separado e mais enxuto**, nos moldes do spike. É o mais
   rápido para demonstrar e o mais caro de manter: o protocolo seria escrito à
   mão em JavaScript, contra `specs/06-clientes-gui.md:19`, e haveria uma segunda
   casca, contra o 0039.

## O que isto custa

- **O código do cliente é tão confiável quanto a origem que o serve.**
  - O TLS vai do navegador ao `seeled` sem gateway, mas a página vem da origem a
    cada carga.
  - É exatamente o que o ADR 0026 recusou para atualização: «TLS diz de qual
    servidor o arquivo veio, e não quem o produziu» (`0026:44`).
  - O service worker consulta a origem sozinho, o que o `0026:36` disse
    contradizer o argumento do produto, e o 0046 reafirmou («nunca ao abrir o
    app», `0046:113-116`).
  - Quem controla a origem (o registrador, o DNS, a hospedagem ou a conta que
    publica), numa única carga:
    - lê todo convite aberto;
    - troca os hashes e se põe no meio de qualquer servidor;
    - assina desafios com a identidade de toda pessoa;
    - lê as conversas;
    - usa a permissão de microfone já concedida.
  - Esta página não mitiga isso. Apenas nomeia.
- **O host da página vê o clique** (IP, navegador e hora;
  `docs/analise-para-a-1.0-2026-09-22.md:280`). Com o service worker, isso vale
  para toda abertura.
- **O link do navegador vence.**
  - No nativo, a impressão é eterna (`0047:37-38`).
  - No navegador, o link funciona enquanto houver hash válido nele ou anunciado
    numa visita anterior. Depois disso, é preciso um link novo.
  - Para quem entra pela primeira vez, nada muda: o convite já vence em 7 dias
    (`crates/seele-server/src/admissao.rs:42`).
- **O alcance encolhe.**
  - O navegador não abre socket UDP; o WebKit fechou essa API como WONTFIX
    (`docs/ios-sem-assinatura-2026-10-07.md:217`).
  - Por isso perde o degrau 4 (`0022:207-224`) e o quarto (`MORO`/`QUEM`).
    **O cliente web não reencontra um servidor que mudou de IP.** A saída
    continua sendo DDNS (`0047:145-148`).
  - **Quem limita é o anfitrião, não o amigo.** Um servidor que só é alcançável
    pelo degrau 4 fica fora do navegador. O
    `docs/ios-sem-assinatura-2026-10-07.md:221` atribui ao amigo uma condição
    que é do anfitrião. Que o celular em 4G saia sem problema é inferência: 4G
    não foi medido, e não há dado de CGNAT em rede celular no Brasil (`:434`).
- **A identidade fica presa à origem e ao recipiente.**
  - No iOS, a aba do Safari e o web app da Tela de Início não dividem
    armazenamento (bug 181849 do WebKit).
  - O convite é de uso único (ADR 0021). Quem entra pela aba e depois instala
    vira outra pessoa, e já sem convite. Por isso a página, aberta numa aba, pede
    para instalar **antes** de gastar o convite.
  - Desktop e celular são duas identidades (`specs/08-seguranca.md:35`). O
    critério «mesma sessão pode ser retomada em outro cliente»
    (`specs/06-clientes-gui.md:178`) fica inalcançável pelo celular.
  - Sem o ADR 0042, que é proposto (`docs/adr/README.md:76`), entrar pelo
    celular com o apelido do desktop dá `NicknameTaken`, e isso acontece depois
    de o convite ser gasto.
- **O escopo é de consumo** (`specs/06-clientes-gui.md:113`):
  - não hospeda;
  - não compartilha tela: o Safari do iOS não tem `getDisplayMedia`
    (`docs/ios-sem-assinatura-2026-10-07.md:37`, fonte [C24] em `:257`). No
    Android, não conferido;
  - não administra;
  - exige iOS 26.4 ou mais novo (`:21`).
- **Os tokens servidos ao web têm de ser os congelados** (ADR 0014). O guarda
  `the_served_tokens_are_the_frozen_tokens` (`apps/seele-app/tests/tokens.rs:53`)
  compara `ui/tokens.css` com `design/seele-tokens.css` e não cobre o pacote
  **copiado** para a origem. Ele passa a cobrir a cópia; sem isso, é um guarda
  que existe e não funciona.
- **Esforço.** A pesquisa dá M ao endpoint no `seeled` e G a GG ao cliente
  (`docs/ios-sem-assinatura-2026-10-07.md:229`), mas é anterior a esta análise.
  Pela leitura do servidor de 07/10, que não está no repositório, a sessão sobre
  dois transportes sozinha é G, e servidor mais protocolo (despacho, sessão,
  certificados curtos, link, recusas e testes) somam cerca de 4 a 6 semanas. O
  codec do cliente fica à parte. É estimativa, não medida.

## Riscos

- **A rodada 3 cortou tudo sem causa conhecida**, 16 s antes de a página ficar
  oculta. Se o motivo for uma interrupção comum (notificação, Siri, ligação), a
  voz com tela bloqueada cai, e o produto tem de contar (ver «Áudio e
  transporte»).
- **O Android pode não passar no M6** (ver «O que não foi medido»), e lá não
  existe a Audio Session API.
- **O web app instalado pode se comportar pior que a aba** (bug 236509), e é a
  forma decidida.
- **30 minutos de bloqueio, 4G e bateria**: nada foi medido.
- **CGNAT do lado de quem hospeda**: sem furo de NAT, o servidor fica fora do
  alcance do navegador. A exceção seria um MOD de túnel do 0045, que é proposto,
  no anfitrião (`0022:3-10`; `0045:108-109`). Ele manteria o TLS do navegador
  até o `seeled`, o que não foi medido, mas põe um terceiro no caminho de todo o
  tráfego.
- **O repasse do desafio** (ver o EM ABERTO do aperto de mão): já existe no
  nativo pela leitura do código, e o navegador aumenta a superfície.
- **A confiança passa a depender de quem serve a página** (a lista está em «O
  que isto custa»).
- **O `wtransport` anuncia SETTINGS antigos** e pode falhar contra um navegador
  que só fale o rascunho novo.

## O destino do branch `mobile/ios`

Esta página não decide nenhuma operação de git.

**Abandonado como caminho de produto** (tudo veio do `8632668`):
- `apps/seele-app/gen/apple/**`, incluindo `sessao-de-audio.m`, `project.yml` e
  o `.pbxproj`;
- `apps/seele-app/gen/schemas/iOS-schema.json`,
  `apps/seele-app/gen/schemas/mobile-schema.json` e
  `apps/seele-app/Info.ios.plist`;
- a casca como biblioteca: o `crate-type = ["staticlib", "cdylib", "rlib"]`
  (`apps/seele-app/Cargo.toml:139-142`), o `mobile_entry_point`
  (`apps/seele-app/src/lib.rs:8656`) e os ramos `cfg(mobile)`;
- `tools/ios-simulador.sh` e `tools/ios-aparelho.sh`;
- hospedar no celular, que a revisão de 05/10 exercitou
  (`docs/revisao-celular-2026-10-05.md:3-4`).

O próprio `8632668` registra que não rodou clippy, `cargo test` do workspace nem
os builds de Windows e Linux com a casca como biblioteca. Enquanto essa mudança
estiver lá, é risco para o desktop. Desfazê-la exige que os testes voltem a ler
`main.rs`:
- `apps/seele-app/tests/frontend.rs` lê `src/lib.rs` em 36 chamadas (por exemplo
  `:444` e `:474`);
- também leem `lib.rs`: `apps/seele-app/tests/empacotamento.rs:405`, `:563` e
  `:586`, e `apps/seele-app/tests/encerramento.rs:19`;
- o `8632668` também mexeu em `apps/seele-app/tests/permissoes.rs`.

**Serve ao web app:**
- `apps/seele-app/ui/celular.css` (572 linhas). Ele lê `data-plataforma`
  (`celular.css:32-44`), que hoje vem de `invoke("plataforma")`
  (`ui/base.js:353-356`) e que a ponte passa a responder;
- as mudanças de casca de uma mão em `ui/`;
- `tools/auditoria-celular.py`, que veio do `8632668`, e
  `tools/carga-da-casca.py`, que já existia antes dele;
- `design/marca/gerar-icones.py`, alterado no `8632668`, para o ícone e o
  manifest;
- os documentos e o spike.

**As correções de desktop continuam valendo e não dependem do branch.** São
`266e6a9` (troca de microfone), `67b5f85` (jitter), `1f04382` (tema de MOD) e
`67d9069` (SAIR DA SALA). As quatro também estão em `conserto/desktop-do-ios` e
foram integradas em `versao/0.15.1` pelo `d9f38e7`. Nenhuma está no `main` nem
publicada.

**O que esta página cita precisa chegar ao `main` com ela.** O spike
(`spikes/voz-no-navegador/`), a pesquisa
(`docs/ios-sem-assinatura-2026-10-07.md`) e a revisão de 05/10 não existem no
`main`. Sem eles, as citações daqui apontam para arquivos que não estão lá, e o
`docs/adr/README.md:19-22` trata endereço errado como defeito. Isso é condição
das citações, e não uma operação de git decidida aqui: elas precisam chegar sem
o código do `8632668`.

## O que fica pendente

**Medir antes do código, nesta ordem:**
1. se a assinatura Ed25519 randomizada do Safari passa no `verify` do `seeled`
   (`session.rs:594-599`). Decide se a autenticação funciona;
2. `cargo check -p seele-proto --target wasm32-unknown-unknown`;
3. repetir `spikes/alpn-no-mesmo-socket/` num iPhone: o despacho por ALPN
   com o `wtransport` adotado, os temporizadores do `seeled` com a página
   oculta, um certificado curto com a mesma chave do longo, uma lista longa de
   hashes e o tempo de um servidor fora do ar contra o da recusa;
4. o spike num web app instalado no iPhone, com a página vinda de uma origem
   pública: 30 minutos bloqueado, em 4G, com a bateria anotada; que URL o iOS
   grava no ícone; e a `CryptoKey` sobrevivendo a fechar o web app e a
   reiniciar o aparelho;
5. o spike no Chrome do Android;
6. uma página de origem pública discando para a LAN.

**Notas de cabeçalho nos ADRs emendados**, no molde de `0022:3` e `0026:3`: 0003,
0006, 0007, 0010, 0017, 0018, 0019, 0026, 0039 e 0046. A linha de cada um no
índice já diz que foi emendada por este.

**Specs a atualizar** (`specs/10-convencoes.md:73`):
- `specs/06-clientes-gui.md:117-125`: o marcador vira «Fechado pelo ADR 0057»;
- `specs/06-clientes-gui.md:129`: o `uniffi`;
- `specs/06-clientes-gui.md`, seção Mobile: ganha «não hospeda»;
- `specs/06-clientes-gui.md:177-178`: a troca de rede vira reconexão, e a sessão
  retomada em outro cliente fica fora do alcance do celular;
- `specs/01-arquitetura.md:5` e `:80`: toda lógica em `seele-core`, e «três
  aplicativos»;
- `specs/01-arquitetura.md:20` e `:23`: o `uniffi` para Flutter e o
  `apps/mobile/` em Flutter;
- `specs/01-arquitetura.md:40` e `specs/02-protocolo.md:129`: a migração de
  conexão;
- `specs/00-visao-geral.md:9` («compartilham o mesmo núcleo») e `:36` («App
  mobile»);
- `specs/09-roadmap.md:99`;
- `specs/02-protocolo.md:9` (o controle no «#0») e `:10` (o texto, que já vai no
  controle);
- `specs/03-audio.md:73`, com a nota sobre o web, e `:87-95`, o fone
  obrigatório, por causa da emenda ao 0007;
- `specs/08-seguranca.md:12`, `:26` e `:29`: o TOFU e o aviso de chave trocada,
  que não existem no navegador; `:35`, os vários aparelhos; e uma linha nova na
  tabela de ameaças (`:9-18`): «origem da página adulterada → código do
  cliente»;
- `docs/plano-m0-m1.md:288`, a D22.

Ao editar as specs: o guarda `xtask/tests/textos_publicos.rs`, que só existe no
branch `conserto/l2-textos-publicos`, reprova «nunca vê áudio em claro» em
`specs/*.md`. Escrever «TLS do navegador até o `seeled`».

**Documentos que envelheceram:**
- `docs/ios-sem-assinatura-2026-10-07.md:8` diz 0 commits à frente do `main`, e
  hoje são 7;
- `docs/ios-sem-assinatura-2026-10-07.md:21`, `:37` e `:224` dizem que nada foi
  medido no iPhone e que a tabela está vazia;
- `docs/ios-sem-assinatura-2026-10-07.md:221`: o CGNAT, corrigido acima;
- `docs/monetizacao-2026-10-04.md` é citado em
  `docs/ios-sem-assinatura-2026-10-07.md:254` e está fora do git.

## O que este ADR não decide

- o domínio da origem e o caminho do link;
- a primeira visita sem `v=`;
- se o `seeled` confere o `Origin`;
- o canal de diagnóstico;
- quantos hashes futuros a sessão entrega, e por qual mensagem;
- quanto do núcleo vai para `wasm`;
- o contexto da assinatura do aperto de mão;
- notificações (Web Push);
- MODs no celular além da recusa nomeada;
- pagar o programa da Apple mais tarde;
- o que fazer se o Android falhar com a tela bloqueada;
- as operações de git sobre `mobile/ios`.

## Custo de reverter

**Médio enquanto nada estiver construído; alto depois que a origem existir.**
Voltar ao nativo é reabrir o `8632668` e pagar R$ 549,90 por ano. O lado do
`seeled` (o `h3` e o certificado curto) não afeta o desktop e pode ficar. A
origem é a parte que não tem volta, porque as identidades do celular moram nela.
