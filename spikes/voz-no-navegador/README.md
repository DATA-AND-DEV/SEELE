# spike `voz-no-navegador` — um celular fala voz com o SEELE pelo navegador?

**Descartável.** Existe para responder uma pergunta e morre com a resposta.

## A pergunta

Não há app mobile por enquanto, e o M6 (`specs/09-roadmap.md`) ainda não
escolheu plataforma. Quem está no celular só tem o navegador. O plano é o
celular ser **apenas cliente**: ele nunca hospeda um servidor, só entra em um.

Um navegador não abre QUIC cru e não aceita o certificado TOFU do `seeled`
(`crates/seele-server/src/tls.rs`, ADR 0003). O caminho que mantém o TLS ponta
a ponta, sem um gateway no meio, é o **WebTransport** com
`serverCertificateHashes`. Neste caminho, o navegador aceita um certificado
autoassinado pelo hash dele, que é quase o pino de hoje.

Antes de um ADR, quatro perguntas que só um aparelho responde:

1. O navegador do celular aceita o certificado pelo hash? O Safari do iOS é a
   dúvida principal.
2. O WebCodecs do celular codifica e decodifica Opus?
3. A voz por datagrama chega inteira e a tempo?
4. **O que acontece quando a tela apaga?** O aceite do M6 é de 30 minutos com
   a tela bloqueada. O spike não promete isso: ele mede o que sobra.

Ele **não** fala o protocolo do `seeled`. A pergunta é se o navegador chega
até um QUIC com voz. O postcard por cima é trabalho conhecido, e misturar os
dois esconderia em qual metade estaria a falha.

## Como roda

```sh
cd spikes/voz-no-navegador
cargo run --release
```

O servidor imprime dois endereços:

- `https://<ip-na-lan>:4433/`: abra no celular, na mesma rede. O navegador
  avisa do certificado **uma vez** (Safari: «Mostrar detalhes → visitar este
  site»; Chrome: «Avançado → continuar»). O aviso é da **página**. O
  WebTransport entra pelo hash e não depende dele, e é isso que se está
  medindo.
- `http://localhost:8080/`: a mesma página nesta máquina.

No celular: **1 · Conectar**, **2 · Ligar áudio**, **3 · Falar**. Com duas
pessoas (ou celular + computador), uma ouve a outra. Depois, bloqueie a tela
por um minuto, volte e aperte **enviar relatório**.

Tudo o que a página mede volta ao servidor e fica em `relatorios.jsonl`, um
JSON por linha, com o resumo no terminal. Os caminhos são três: pelo
WebTransport a cada 5 s; por HTTPS quando o WebTransport nem abre, que é o
caso que mais importa saber; e no `pagehide`. O terminal também imprime, a
cada 5 s, quantos datagramas cada sessão mandou. Assim, o que a tela apagada
fez aparece do lado do servidor mesmo que o relatório do celular nunca chegue.

## O formato

| tipo | do navegador                | do servidor                                        |
|------|-----------------------------|----------------------------------------------------|
| 0x01 | `seq u16 · t f64 · opus`    | `0x01 · de u16 · seq u16 · t f64 · opus` aos outros |
| 0x02 | `t f64` (ping)              | o mesmo datagrama, de volta                        |
| 0x03 | igual ao 0x01, pedindo eco  | o datagrama de volta, e 0x01 aos outros            |

Opus mono a 48 kHz, 32 kbit/s, quadros de 20 ms, como no produto
(`specs/03-audio.md`). O buffer de reprodução espera 40 ms e descarta acima de
200 ms.

## A prova de bancada

`python3 prova.py` (depois de `cargo build --release`) põe dois Chromium com
microfone falso numa sala e cobra a voz de A chegando a B, o eco, os
relatórios e, **antes de tudo**, que um hash com um bit trocado seja recusado.
Sem este último, as outras cobranças passariam igual com um navegador que
aceitasse qualquer certificado.

Ela prova que a página e o servidor funcionam **antes** de alguém levar um
celular para a mesa: um defeito no celular passa a ser do celular.

### O que ela mediu — 2026-10-04, macOS aarch64, Chromium 151 do Playwright

| | |
|---|---|
| hash adulterado | recusado: `QUIC_TLS_CERTIFICATE_UNKNOWN` |
| WebTransport aberto | 20–23 ms, datagrama máximo 1024 B (o servidor diz 1295) |
| voz A → B | 501/501 quadros, 0 perdidos, 1 falta (o fim da fala) |
| captura → Opus → rede → decode | p50 0,6 ms · p95 1,0 ms |
| ping por datagrama | p50 0,30 ms · p95 0,60 ms |
| escrita de datagrama | `datagrams.writable` (o Chromium ainda não tem `createWritable`) |

Isso é localhost. O número que importa sai do celular na LAN e pela internet.

A medida de eco **não** inclui o acúmulo de 20 ms do quadro, o buffer de 40 ms
nem a latência de saída do aparelho, que vai no relatório como
`outputLatencyMs`. Ela mede o que o navegador acrescenta ao caminho.

### O que o Simulador do iOS 27 mediu — 2026-10-07

O Safari do Simulador (iOS 27.0, 24A434; Xcode 27) contra o servidor desta
pasta, em localhost. Não é o aparelho: a rede é a do Mac, e não há microfone
nem tela que apaga. Mas é o WebKit do iOS 27, e responde a pergunta que podia
matar o caminho antes do celular — a pesquisa de 07/10/2026
(`docs/ios-sem-assinatura-2026-10-07.md` §3.3) temia que o `wtransport` 0.7.2,
que anuncia os códigos de SETTINGS de rascunhos antigos, não conversasse com o
Safari.

| | |
|---|---|
| WebTransport aberto | sim, 6–71 ms; escrita por `createWritable` |
| hash adulterado (`?hash=errado`) | recusado: `WebTransportError` |
| capacidades | Opus no encoder e no decoder, AudioWorklet, microfone, Wake Lock e Media Session: todos sim |
| Opus de ponta a ponta (`?sintetico`) | 250/250 quadros codificados, ecoados e decodificados, 0 erros; eco p50 1 ms, p95 2 ms |
| ping por datagrama | 23/23 e 68/68; p50 1 ms |
| datagrama máximo | o Safari diz 65535; o servidor, 1358 |
| aba oculta 10 s | os pings seguiram (↑10 ↓10) — numa aba de fundo do Simulador, não com a tela bloqueada |

Os três parâmetros servem para medir sem tocar na tela (`xcrun simctl openurl
booted "http://localhost:8080/?auto=1"`): `?auto` conecta sozinho, `?hash=errado`
troca um bit do hash antes, e `?sintetico` manda um tom de 440 Hz pelo encoder,
com eco, no lugar do microfone.

## As medidas no celular

A tabela se preenche a partir do `relatorios.jsonl`:

| aparelho · navegador | hash aceito | Opus enc/dec | rtt p50/p95 | eco p50 | tela apagada: o que sobrou |
|---|---|---|---|---|---|
| iPhone · Safari, iOS 27.0.1, Wi-Fi (07/10/2026) | sim, 27–142 ms; hash adulterado recusado 3/3, mesmo com a exceção de certificado aceita na página | sim: 439 ↑, 2740 ↓, 0 perdidos, 0 erros | 5 / 8–10 ms | 7 ms | na aba padrão, **nada**; com `?sessao=play-and-record&saida=elemento`, **ouviu por 128 s e falou por 40 s ou mais** bloqueado (rodada 5, abaixo) |
| iPhone · Chrome (é WebKit) | | | | | |
| Android · Chrome | | | | | |
| Android · Firefox | | | | | |

No iOS, todo navegador é WebKit, então «iPhone · Chrome» responde o mesmo que
o Safari. Ele está na tabela para ninguém achar que é outra resposta.

### iPhone, rodada 1 — 07/10/2026: a tela apagada

Uma aba do Safari, a página oculta por 35 s, com voz nos dois sentidos antes:

- o `AudioContext` vai a `interrupted` 0,1 s **antes** do `visibilitychange`;
- o microfone continua `live` e sem `muted`, mas nada sai dele: o ↑ de voz
  para em 308 e não anda até a página voltar;
- a reprodução para junto — sem `AudioContext` rodando, o worklet não toca;
- **a sessão WebTransport segue**: os pings caem para 1 por segundo (o
  `setInterval` estrangulado), e 2348 datagramas de voz continuam chegando;
- ao voltar, o `AudioContext` volta a `running` sozinho, e a voz retoma.

O aceite do M6 (30 min com a tela bloqueada) **não passa** assim. A rodada 2
mede as duas saídas que o WebKit oferece: `?sessao=play-and-record` (a Audio
Session API declara a página como ligação) e `?saida=elemento` (a voz toca por
uma tag `<audio>`, que o iOS trata como mídia), e o web app da Tela de Início.

### iPhone, rodadas 2 e 3 — 07/10/2026: as duas saídas do WebKit não seguram

A rodada 3 é a que vale: aba do Safari com `?sessao=play-and-record&saida=elemento`,
o Simulador mandando um tom contínuo de 440 Hz, o microfone do iPhone ligado,
e a tela bloqueada pelo botão lateral por um minuto. Os eventos da página:

| t | o que aconteceu |
|---|---|
| 1,5 s | `audioSession` declarada `play-and-record`; saída pela tag `<audio>` |
| 3,1 s | áudio ligado; o tom toca (3761 blocos com som, ~10 s) e o microfone manda 444 quadros |
| **15,7 s** | **bloqueio: no mesmo milissegundo, a tag `<audio>` é pausada, o microfone fica `muted` pelo sistema e o `AudioContext` vai a `interrupted`** |
| 32,2 s | só agora o `visibilitychange` diz «oculta» — 16 s depois do bloqueio |
| 67,5 s | `WebTransport caiu`: o servidor fechou a sessão por 30 s sem nada do iPhone |

Com a página oculta, os contadores dos worklets deram **0 quadros de microfone
e 0 ms de som tocado**. O «bipando» que se ouviu durante o bloqueio era o Mac,
que tinha a página aberta e tocava o tom do Simulador.

A rodada 2 tinha sugerido o contrário — 102 datagramas de voz saindo com a
página oculta por 2 s, e o `AudioContext` em `running` por 12 s ocultos. Aquelas
ocultações não eram bloqueio (a pessoa não bloqueou; ver a conversa de
07/10/2026): eram a página saindo da frente por outro motivo. **Só o bloqueio
de verdade mede o aceite do M6**, e a página não distingue um do outro sozinha.

O que isto parecia decidir — **e a rodada 5 desmentiu**: numa aba do Safari do
iOS 27, voz com a tela bloqueada não existiria, com ou sem a Audio Session API e
a tag `<audio>`. Falta o web app da
Tela de Início, cujo `Web.app` declara `audio` em `UIBackgroundModes`
(`docs/ios-sem-assinatura-2026-10-07.md` [C11]).

### iPhone, rodada 4 — 07/10/2026: na mesma aba, a voz seguiu com a tela apagada

O teste 4 de novo, **numa aba do Safari** — a intenção era o web app, mas o
ícone não chegou a existir na Tela de Início, e a detecção de modo da página
disse «aba do navegador» corretamente. A pessoa bloqueou. A página ficou
oculta por 19,6 s, e desta vez **nada parou**: nenhum evento de tag `<audio>`
pausada, de microfone `muted` ou de `AudioContext` interrompido. Com a página
oculta, os worklets contaram **18,9 s de som tocado** e **981 quadros de
microfone** (19,6 s), e 1819 datagramas de voz chegaram. A pessoa ouviu o tom
com a tela bloqueada.

Três ressalvas, antes de chamar isto de aceite:

- é a mesma aba e o mesmo teste da rodada 3, onde o bloqueio cortou tudo no
  mesmo milissegundo. Uma das duas ocultações pode não ter sido o bloqueio, ou
  o comportamento varia — a rodada 5 (5 min, «Falar» marcado, sem trocar de
  app) é para separar uma coisa da outra;
- «Falar» estava desmarcado durante o bloqueio (456 codificados = 456 enviados,
  sem erro de encoder): o microfone capturou, mas a voz não saiu para a rede;
- 19,6 s, e não os 30 min do M6.

### iPhone, rodada 5 — 07/10/2026: os dois sentidos com a tela bloqueada

O teste 4 numa aba do Safari, a pessoa no Mac conversando, o iPhone bloqueado
pelo botão lateral, sem trocar de app. Dois bloqueios seguidos:

| bloqueio | o que os worklets e o servidor contaram com a página oculta |
|---|---|
| 1º, 128,6 s | **115 s de som tocado** no iPhone (a voz do Mac), 6231 quadros de microfone capturados, `AudioContext` em `running` o tempo todo, nenhum evento de pausa ou `muted`. Nenhum quadro enviado (623 codificados = 623 enviados, 0 erros): «Falar» desmarcado |
| 2º, 40 s ou mais | **voz do iPhone a 50 quadros/s** (250 por 5 s no servidor); o Mac recebeu 1104 quadros, 0 perdidos, e tocou. Pings a 1/s: a página estava oculta de fato |

Com `?sessao=play-and-record&saida=elemento`, então, uma aba do Safari no iOS
27.0.1 mantém voz nos dois sentidos com a tela bloqueada — o oposto da aba
padrão da rodada 1. A rodada 3, em que tudo parou no mesmo milissegundo, fica
como pergunta: lá o corte veio 16 s **antes** de a página ficar oculta, o que
não parece o bloqueio. Uma interrupção de outra origem (notificação, Siri,
ligação) explicaria; ninguém mediu.

Falta o aceite do M6 inteiro: 30 min bloqueado, e em 4G.

Outro dado da rodada 1: o Safari diz `maxDatagramSize` 65535, e o servidor diz
1295. O cliente não pode confiar no número do navegador para medir o quadro.
