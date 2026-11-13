# Vídeo por RTP sob perda, medido contra o canal SCTP de hoje

**Data:** 2026-11-10
**Árvore:** `entrega/1.0`, commit `7d4e04d3`, mais o exemplo novo e um ajudante de teste, não commitados (o produto não foi tocado por esta medida)
**O que é:** uma medida, não uma decisão. `docs/medidas/2026-11-10-tela-sob-perda.md` terminou dizendo que um transporte de mídia fora do SCTP era a única alternativa que ela não descartava, e `docs/medidas/2026-11-10-tela-do-produto-sob-perda.md` terminou dizendo que a 3% e 5% de perda o que resta é do transporte, e que «RTP não foi medido». Esta medida põe o vídeo em RTP (o do `str0m` 0.24.1 que o enlace já usa: NACK/RTX, pedido de quadro-chave e estimativa de banda) no mesmo cano, nos mesmos cenários, e pergunta se ele entrega a tela melhor que o canal de hoje, e quanto.

## Como reproduzir

```sh
B=(cargo run --release -p seele-enlace --example rtp_sob_perda --)

# a grade: 7 cenários × (voz, A, RTP fixo, RTP adaptativo) × 3 rodadas de 20 s, ~32 min
"${B[@]}" --variante voz,a,fixo,adaptativo

# o regime do adaptativo: 40 s de transmissão antes de abrir a janela de 20 s, ~22 min
"${B[@]}" --variante adaptativo --aquecimento 40

# a espera pela retransmissão (o padrão do str0m, 2000 ms, é o da grade)
for t in 50 100 200 500; do
  "${B[@]}" --variante fixo --so "3%" --espera-ms $t
  "${B[@]}" --variante fixo,adaptativo --so "5%" --espera-ms $t
done

# fora da grade
"${B[@]}" --variante fixo,adaptativo --perda 0.10
"${B[@]}" --variante fixo,adaptativo --perda 0.10 --sem-teto-rtx
"${B[@]}" --variante adaptativo --inicial-bps 2790000 --so "0%"
"${B[@]}" --variante adaptativo --inicial-bps 2790000 --so "1,0 Mbps, 0%"
"${B[@]}" --variante fixo --so "3%" --reordem-voz 15
"${B[@]}" --variante fixo,adaptativo --so "0%" --jitter-ms 20
"${B[@]}" --variante fixo,adaptativo --so "3%" --jitter-ms 20

# acrescentadas depois de ver a grade
"${B[@]}" --variante fixo,adaptativo --so "3%" --sem-teto-rtx
"${B[@]}" --variante fixo,adaptativo --so "5%" --sem-teto-rtx
"${B[@]}" --variante adaptativo --sem-teto-rtx --espera-ms 500                  # a grade «ajustada»
"${B[@]}" --variante fixo --sem-teto-rtx --espera-ms 500 --so "3%"              # e --so "5%"
"${B[@]}" --variante adaptativo --sem-teto-rtx --espera-ms 500 --aquecimento 40 --so "3%"   # e --so "5%"
"${B[@]}" --variante adaptativo --so "1,0 Mbps, 0%" --segundos 60 --serie       # e --so "2,0 Mbps, 1%"
"${B[@]}" --variante adaptativo --so "1,0 Mbps, 0%" --segundos 60 --espera-ms 500 --serie
"${B[@]}" --variante adaptativo --so "1,0 Mbps, 0%" --segundos 60 --espera-ms 500 --pli-ms 1000 --entre-chaves-ms 1000 --serie

# investigação (6 rodadas cada, com a série segundo a segundo no STDERR)
"${B[@]}" --variante fixo --so "5%" --rodadas 6 --serie
"${B[@]}" --variante fixo --perda 0.10 --sem-teto-rtx --rodadas 6 --serie
```

Semente `20261110` (rodada *n* usa semente + *n* − 1; as variantes de um cenário usam a mesma). Uma célula por vez. O código é `crates/seele-enlace/examples/rtp_sob_perda.rs`; o emulador é o `Cano` de `crates/seele-enlace/src/teste.rs`, o mesmo das duas medidas anteriores. `--so` com o nome exato de um cenário escolhe só ele («3%» não traz o «0,3%»); com um pedaço, todos os que o contêm («0%» traz também o «1,0 Mbps, 0%»). O exemplo escreve no STDERR, por célula, a conferência do cano.

**O que mudou fora do exemplo:** `teste.rs` ganhou uma função, `config_de_rtc_cru()`, só acrescentada: devolve um `RtcConfig` do str0m com o provedor de cripto do enlace (`cripto.rs`, que é privado) e um certificado efêmero. Sem ela um exemplo não monta um `Rtc`, porque o str0m entra neste crate sem backend de cripto próprio. Nada no produto a chama.

**A ordem em que as coisas foram feitas.** A grade, o regime, a varredura da espera, o 10%, a partida alta, a voz, o jitter, as passagens sem o teto e a configuração ajustada rodaram com um binário. Depois delas o exemplo ganhou `--serie` (a série segundo a segundo e o aviso de quadro com tamanho errado, no STDERR) e um contador; o caminho da medida não mudou. As passagens de 60 s e as duas de investigação rodaram com esse segundo binário.

## A máquina

Apple M5 Pro, 15 núcleos, 24 GB, macOS 27.2 (26B123), rustc 1.97.1, perfil `release`. Tudo em 127.0.0.1, num processo só, uma célula por vez. **A máquina não estava sozinha**: havia outras sessões abertas. A carga média (`uptime`, 1 min) no começo e no fim de cada passagem:

| passagem | começo | carga | fim | carga |
|---|--:|--:|--:|--:|
| a grade | 17:08 | 3,98 | 17:38 | 4,38 |
| o regime | 17:38 | 4,38 | 18:00 | 1,22 |
| espera de 50 ms, 3% | 18:00 | 1,22 | 18:01 | 1,71 |
| espera de 50 ms, 5% | 18:01 | 1,71 | 18:03 | 1,41 |
| espera de 100 ms, 3% | 18:03 | 1,41 | 18:04 | 2,00 |
| espera de 100 ms, 5% | 18:04 | 2,00 | 18:06 | 2,47 |
| espera de 200 ms, 3% | 18:06 | 2,47 | 18:07 | 1,61 |
| espera de 200 ms, 5% | 18:07 | 1,61 | 18:09 | 3,22 |
| espera de 500 ms, 3% | 18:09 | 3,22 | 18:10 | 3,90 |
| espera de 500 ms, 5% | 18:10 | 3,90 | 18:13 | 1,99 |
| 10% | 18:13 | 1,99 | 18:15 | 1,57 |
| 10% sem o teto | 18:15 | 1,57 | 18:17 | 1,88 |
| partida alta, 0% | 18:17 | 1,88 | 18:18 | 2,24 |
| partida alta, 1,0 Mbps | 18:18 | 2,24 | 18:19 | 1,92 |
| voz com reordenação 15 | 18:19 | 1,92 | 18:20 | 1,69 |
| jitter, 0% | 18:20 | 1,69 | 18:22 | 1,82 |
| jitter, 3% | 18:22 | 1,82 | 18:24 | 2,41 |
| sem o teto, 3% | 18:24 | 2,41 | 18:27 | 2,48 |
| sem o teto, 5% | 18:27 | 2,48 | 18:29 | 2,46 |
| a grade ajustada | 18:29 | 2,35 | 18:36 | 3,13 |
| ajustado, fixo, 3% | 18:36 | 3,13 | 18:38 | 2,64 |
| ajustado, fixo, 5% | 18:38 | 2,64 | 18:39 | 2,71 |
| ajustado, regime, 3% | 18:39 | 2,71 | 18:42 | 2,19 |
| ajustado, regime, 5% | 18:42 | 2,19 | 18:45 | 1,97 |
| 60 s, 1,0 Mbps | 18:51 | 3,06 | 18:55 | 2,68 |
| 60 s, 2,0 Mbps com 1% | 18:55 | 2,68 | 18:58 | 1,77 |
| 60 s, 1,0 Mbps, espera 500 | 18:58 | 1,77 | 19:01 | 1,85 |
| 60 s, 1,0 Mbps, pedidos calmos | 19:01 | 1,85 | 19:04 | 2,00 |

A grade começou com a carga em 3,98, logo depois de uma compilação deste próprio trabalho. As duas passagens de investigação (fora da tabela) rodaram com carga entre 1,7 e 2,1. Uma primeira tentativa da grade, com carga 8,3, foi interrompida aos 6 minutos para consertar a contagem de chaves (a janela abria depois do primeiro quadro) e não entra em nenhuma tabela.

## O que foi medido

**O banco.** Dois `Rtc` do str0m num processo só — quem transmite e quem recebe —, cada um com o seu socket UDP em 127.0.0.1, e o `Cano` no meio. Quem transmite só tem um candidato remoto, o endereço do cano; quem recebe não tem nenhum, e aprende o socket do cano pelas checagens STUN que chegam (peer-reflexive, o que um NAT faria). **A oferta e a resposta SDP são trocadas em memória**, de um `Rtc` para o outro, antes de qualquer candidato local existir, de modo que nenhuma das duas carrega um endereço. ICE, DTLS, SRTP, RTP e RTCP passam pelo cano, com a perda e o atraso do cenário. Não há servidor no meio, nem `Ponta`, nem `Conexao`: é o str0m cru, com o provedor de cripto do enlace. O SRTP desse provedor nunca tinha carregado mídia neste produto (`cripto.rs` diz isso); aqui carregou, dos dois lados com a mesma implementação.

**A rede** (o `Cano`), como nas medidas anteriores: em cada sentido, perda independente por datagrama com semente fixa, 30 ms de atraso fixo, sem jitter, e — nos dois cenários de banda — um gargalo com fila de 250 ms (31 250 bytes a 1,0 Mbps, 62 500 a 2,0 Mbps) e descarte pela cauda. A perda vale nos dois sentidos: o retorno (NACK, PLI, os relatórios da estimativa) também se perde. Cenários: 0%, 0,3%, 1%, 3% e 5% de perda sem limite de banda; 1,0 Mbps sem perda; 2,0 Mbps com 1%.

**O vídeo** é H.264 de mentira, 30 quadros/s, escrito no `Writer` do str0m (a API de quadros, não a de pacotes). Cada quadro é um Annex-B que o empacotador aceita: a chave é SPS + PPS + um NAL 5; o delta é um NAL 1. Dentro do NAL vão uma marca, o número do quadro, o instante da captura e o tamanho, em texto, e o resto é enchimento (`0xA5`, para não nascer um `00 00 01` no meio). O str0m parte o NAL em FU-A de até ~1,2 kB (um delta de 6 KiB são 6 datagramas; uma chave de 80 KiB, 72), cifra, e do outro lado remonta. Quem recebe lê os campos de volta do quadro remontado.

**As variantes:**

- **RTP fixo** — deltas de 6 KiB e chave de 80 KiB (≈1,47 Mbps), a carga da linha «A» da medida do SCTP. Sem estimativa de banda e, por isso, **sem ritmo**: no str0m o ritmo de saída só existe com a estimativa ligada (leitura, abaixo), e a chave sai de uma vez. A chave sai no começo e a cada pedido que chegar mais de 300 ms depois da chave anterior.
- **RTP adaptativo** — a estimativa de banda do str0m ligada do lado de quem envia (`RtcConfig::enable_bwe`, com os relatórios TWCC que quem recebe manda; `Bwe::set_desired_bitrate` em 2,79 Mbps) e um codificador de mentira atrás de `Event::EgressBitrateEstimate`: delta = estimativa × 0,85 ÷ 30 ÷ 8 bytes, chave = 8 × o delta, com a estimativa presa entre 200 kbps e 2,79 Mbps, o piso e o teto do produto. Depois de cada chave pula 3 quadros, como a casca do aplicativo e como o codificador de mentira da medida do produto. A estimativa parte de **300 kbps** (escolha desta medida; o str0m não tem padrão, exige um valor) e, na variação «partida alta», de 2,79 Mbps. Diferente da medida do produto: uma troca de estimativa **não** força uma chave (lá forçava, porque a casca reconfigura o `VideoEncoder`).
- **A (SCTP, controle)** — o emissor da linha «A» de `tela_sob_perda.rs`, copiado para dentro deste exemplo: `par_por_cano`, canal `u` confiável e ordenado, `write_all` de um quadro por vez, a voz no canal `voz` do enlace. Existe para dizer se **esta bancada, nesta sessão**, reproduz a medida anterior.
- **voz (RTP)** — só a trilha de voz: a referência do que o vídeo custa a ela.

**A voz** vai junto em todas as células: uma trilha Opus de mentira no mesmo `Rtc`, 50 pacotes/s de 120 bytes, com o número e o instante do envio dentro. Sem retransmissão (o str0m não negocia RTX para o Opus) e com o buffer de reordenação da voz em zero (`set_reordering_size_audio(0)`): cada pacote sai assim que chega. O padrão do str0m para a voz, 15, foi medido numa variação.

**O modelo de decodificação** é o da medida do SCTP: um delta só é exibível se **todos** os quadros desde a última chave chegaram inteiros. Quem decide é o número do quadro dentro dos bytes (o exibido tem de ser o seguinte ao último exibido, e ter o tamanho que declara), e não a palavra do str0m; `MediaData::contiguous` é conferido contra ele em toda célula («desacordos», abaixo). Cadeia quebrada: o receptor manda um PLI na hora (`Writer::request_keyframe`), repete a cada 300 ms enquanto estiver sem imagem, ignora os deltas que chegarem e fica congelado até uma chave inteira. O emissor ignora o pedido que chega menos de 300 ms depois de uma chave (ela ainda está a caminho, ou sendo completada por retransmissão).

**Quanto o receptor espera uma retransmissão antes de desistir do quadro** é o parâmetro central, e nesta medida é o `RtcConfig::set_reordering_timeout_video` do str0m. **A grade usa o padrão dele, 2 s**; a varredura mede 50, 100, 200 e 500 ms. Lido em `str0m-0.24.1/src/packet/buffer_rx.rs`: o prazo conta da chegada do primeiro pacote do quadro mais velho ainda incompleto; enquanto ele espera, os quadros seguintes ficam na fila atrás dele, mesmo inteiros (o str0m entrega em ordem); vencido o prazo, o quadro é largado e o seguinte sai com `contiguous: false` — a cadeia quebra. O outro limite do str0m, `set_reordering_size_video` (quantos quadros inteiros esperam atrás de um buraco; padrão 30), foi posto em 1000 para o prazo ser o único parâmetro.

**As métricas.** Latência de quadro = do instante da captura (o relógio do processo, lido quando o quadro é gerado e escrito dentro dele) até o quadro sair do str0m em quem recebe e ser aceito como exibível, lido no **mesmo relógio**. **Não há buffer de reprodução**: um reprodutor de verdade guarda alguns quadros para tocar sem tranco, e soma isso; a medida do SCTP também não tinha. «Congelado %» = soma dos intervalos de mais de 200 ms sem quadro novo exibível, sobre os 20 s (as bordas da janela contam). «Gerados» = quadros escritos no str0m. «kbps gerados» = bytes dos quadros gerados sobre a janela. «Pedidos (PLI)» = pedidos de chave que quem recebe mandou. «Quebras» = vezes que a cadeia quebrou. «NACKs» = mensagens NACK que quem transmite recebeu (`MediaEgressStats::nacks`; uma mensagem pode pedir vários pacotes). «RTX pacotes» = datagramas com o tipo de carga de retransmissão que saíram do socket de quem transmite — o cabeçalho RTP não é cifrado, e o exemplo classifica cada datagrama que sai. «RTX % dos bytes» = bytes desses datagramas sobre os bytes dos datagramas de vídeo original. **No adaptativo o mesmo tipo de carga leva o enchimento e as sondas da estimativa de banda**, e a coluna soma os dois. «Est.» = `EgressBitrateEstimate`, amostrada a cada 250 ms na janela (mínima, mediana e a última); ela pode passar do teto de 2,79 Mbps, e o codificador de mentira não a segue acima dele. «MB no cano» = bytes UDP que saíram de quem transmite (tudo: vídeo, RTX, voz, RTCP, STUN). «Retorno kbps» = o que quem recebe mandou de volta (NACK, PLI, TWCC, relatórios). Voz: perda (não chegou até 1 s depois do fim) e atraso (ms). «Cano perda/fila %» = o que o cano tirou na ida, por sorteio e por fila cheia.

### As conferências de cada célula

O exemplo confere, e escreve no STDERR:

- **que tudo passou pelo cano**: o número de datagramas que saíram do socket de quem transmite tem de ser o que o cano viu chegar (repassados + sorteados + descartados na fila), e o que o cano repassou tem de ser o que quem recebe leu do socket dele. A diferença («sumidos») tem de ser zero; um buffer de socket estourando apareceria aqui. Quem transmite recusa mandar para qualquer endereço que não seja o cano, e quem recebe recusa mandar direto a quem transmite («fora do cano», também zero).
- **que a perda sorteada bate com a configurada**: a coluna «cano perda %» de cada linha.
- **que o str0m e o número do quadro concordam** sobre onde a cadeia quebrou («desacordos»), e que todo quadro entregue foi lido de volta («ilegíveis»).

### Lido no código do str0m 0.24.1 (não é medida)

- **NACK**: quem recebe monta os pedidos a cada 33 ms (`session.rs`, `NACK_MIN_INTERVAL`), pede cada pacote no máximo 5 vezes (`register_nack.rs`, `MAX_NACKS`) e só acompanha os últimos 100 números de sequência (`MAX_MISORDER`). Com 62 ms de ida e volta, o segundo e o terceiro pedido de um pacote saem antes de a primeira retransmissão chegar: é esperado mais de uma retransmissão por pacote perdido.
- **RTX**: quem transmite guarda 1024 pacotes por 3 s (`send.rs`, `DEFAULT_RTX_CACHE_DURATION`) e **larga as retransmissões pendentes quando elas passam de 15% dos bytes do último segundo** (`DEFAULT_RTX_RATIO_CAP`). A opção `--sem-teto-rtx` tira esse teto (`StreamTx::set_rtx_cache`).
- **Ritmo**: sem `enable_bwe` o str0m usa um ritmo nulo (`session.rs`, `PacerImpl::null()`): o que é escrito sai no mesmo instante. Com a estimativa, o ritmo é a estimativa × 1,1 (`pacer/control.rs`, `PACING_FACTOR`).
- **Estimativa**: os relatórios TWCC saem de quem recebe a cada 50 ms (`session.rs`, `TWCC_INTERVAL`); a estimativa junta um controlador por atraso e um por perda (`bwe/mod.rs`).
- **Os relatórios RTCP de vídeo** saem a cada 1 s por padrão.

## O resumo

Mediana de 3 rodadas de 20 s. Cada célula é **lat p50 ms / p95 ms / congelado % / quadros/s**. «Primeiros 20 s» é a janela aberta no instante em que o vídeo começa; «40–60 s» é a mesma transmissão, medida do segundo 40 ao 60. As três colunas de RTP usam o prazo de espera padrão do str0m (2 s) e o teto de retransmissão padrão. «A (SCTP), nesta sessão» é o controle, rodado aqui; «A (medida anterior)» é copiada de `2026-11-10-tela-sob-perda.md` (a de 0,3% é de uma rodada só, lá); «produto de hoje» é copiado do segundo adendo de `2026-11-10-tela-do-produto-sob-perda.md`, e é **o produto inteiro, com servidor e espectador**, não este banco.

| cenário | RTP fixo | RTP adaptativo, primeiros 20 s | est. mediana kbps | RTP adaptativo, 40–60 s | est. mediana kbps | A (SCTP), nesta sessão | A (medida anterior) | produto de hoje, primeiros 20 s | orçamento kbps | produto de hoje, 40–60 s | orçamento kbps |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | 32 / 33 / 0,0 / 29,9 | 40 / 42 / 0,0 / 29,8 | 3532 | 41 / 43 / 0,0 / 30,0 | 4174 | 32 / 33 / 1,6 / 30,0 | 32 / 33 / 1,6 / 30,0 | 32 / 33 / 1,5 / 29,9 | 2790 | 32 / 33 / 0,0 / 30,1 | 2790 |
| 0,3% | 32 / 58 / 0,0 / 29,9 | 40 / 52 / 0,0 / 29,8 | 3067 | 42 / 51 / 0,0 / 30,0 | 3088 | 33 / 167 / 1,6 / 30,0 | 32 / 115 / 1,6 / 30,0 | 36 / 845 / 4,7 / 29,3 | 1369–1416 | 32 / 117 / 0,0 / 29,9 | 519–1458 |
| 1% | 32 / 97 / 0,0 / 29,9 | 39 / 102 / 0,0 / 29,8 | 2748 | 42 / 108 / 0,0 / 30,0 | 3192 | 505 / 1148 / 4,5 / 27,2 | 437 / 848 / 3,1 / 28,0 | 62 / 1090 / 5,0 / 27,9 | 576–740 | 32 / 216 / 0,0 / 29,5 | 585–640 |
| 3% | 33 / 128 / 0,0 / 29,9 | 42 / 136 / 1,1 / 29,7 | 2412 | 55 / 135 / 0,0 / 30,1 | 3215 | 1432 / 2506 / 18,6 / 15,1 | 1528 / 2505 / 17,6 / 14,2 | 378 / 3691 / 34,0 / 20,1 | 265–493 | 32 / 2076 / 24,8 / 29,9 | 234–268 |
| 5% | 62 / 149 / 1,2 / 29,9 | 80 / 148 / 13,9 / 26,3 | 2535 | 93 / 141 / 0,0 / 30,0 | 3340 | 2474 / 5339 / 57,1 / 7,3 | 2518 / 3629 / 43,5 / 8,4 | 2756 / 7178 / 69,0 / 8,7 | 385–731 | 129 / 1028 / 46,7 / 30,6 | 200 |
| 1,0 Mbps, 0% | — / — / 100,0 / 0,0 | 117 / 361 / 13,2 / 25,8 | 1000 | 62 / 347 / 1,7 / 29,9 | 944 | 1228 / 2912 / 23,3 / 15,5 | 1229 / 2764 / 24,5 / 15,5 | 48 / 3477 / 29,9 / 20,4 | 490–523 | 51 / 365 / 6,5 / 29,1 | 529–626 |
| 2,0 Mbps, 1% | — / — / 100,0 / 0,0 | 58 / 130 / 0,0 / 29,8 | 1626 | 74 / 197 / 2,1 / 30,0 | 2015 | 725 / 1376 / 10,6 / 24,3 | 719 / 1438 / 8,1 / 24,5 | 86 / 1611 / 8,9 / 26,4 | 602–739 | 41 / 312 / 3,2 / 29,4 | 466–562 |

## A grade: os primeiros 20 s (mediana de 3 rodadas)

Espera de 2 s, teto de retransmissão padrão, estimativa partindo de 300 kbps.

| cenário | variante | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | kbps gerados | chaves | pedidos (PLI) | quebras | NACKs | RTX pacotes | RTX % dos bytes | est. mín kbps | est. mediana | est. final | MB no cano | retorno kbps | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | voz (RTP) | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 0,00 | 30 | 32 | 0,00 | 0,00 |
| 0% | A (SCTP, controle) | 32 | 33 | 171 | 321 | 1,6 | 321 | 30,0 | 601 | 1511 | 1 | — | — | — | — | — | — | — | — | 4,21 | — | 0,00 | 32 | 33 | 0,00 | 0,00 |
| 0% | RTP fixo | 32 | 33 | 34 | 37 | 0,0 | 49 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 0 | 0 | 0,0 | — | — | — | 4,09 | 10 | 0,00 | 32 | 33 | 0,00 | 0,00 |
| 0% | RTP adaptativo | 40 | 42 | 46 | 163 | 0,0 | 167 | 29,8 | 597 | 1920 | 1 | 0 | 0 | 0 | 20 | 0,3 | 666 | 3532 | 3987 | 5,18 | 10 | 0,00 | 32 | 33 | 0,00 | 0,00 |
| 0,3% | voz (RTP) | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 0,40 | 31 | 32 | 0,38 | 0,00 |
| 0,3% | A (SCTP, controle) | 33 | 167 | 233 | 324 | 1,6 | 318 | 30,0 | 601 | 1511 | 1 | — | — | — | — | — | — | — | — | 4,21 | — | 0,50 | 32 | 72 | 0,35 | 0,00 |
| 0,3% | RTP fixo | 32 | 58 | 120 | 157 | 0,0 | 158 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 26 | 26 | 0,7 | — | — | — | 4,12 | 10 | 0,30 | 32 | 33 | 0,34 | 0,00 |
| 0,3% | RTP adaptativo | 40 | 52 | 124 | 165 | 0,0 | 169 | 29,8 | 597 | 1904 | 1 | 0 | 0 | 22 | 36 | 0,7 | 626 | 3067 | 3067 | 5,16 | 11 | 0,50 | 32 | 33 | 0,32 | 0,00 |
| 1% | voz (RTP) | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 1,10 | 31 | 32 | 1,06 | 0,00 |
| 1% | A (SCTP, controle) | 505 | 1148 | 1500 | 1543 | 4,5 | 897 | 27,2 | 563 | 1418 | 1 | — | — | — | — | — | — | — | — | 3,86 | — | 0,80 | 56 | 98 | 0,91 | 0,00 |
| 1% | RTP fixo | 32 | 97 | 127 | 153 | 0,0 | 154 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 52 | 56 | 1,6 | — | — | — | 4,16 | 11 | 1,00 | 32 | 33 | 0,90 | 0,00 |
| 1% | RTP adaptativo | 39 | 102 | 132 | 165 | 0,0 | 169 | 29,8 | 597 | 1761 | 1 | 0 | 0 | 67 | 108 | 2,4 | 534 | 2748 | 4022 | 4,83 | 11 | 0,90 | 32 | 33 | 0,91 | 0,00 |
| 3% | voz (RTP) | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 3,10 | 31 | 32 | 3,28 | 0,00 |
| 3% | A (SCTP, controle) | 1432 | 2506 | 2599 | 2620 | 18,6 | 1090 | 15,1 | 322 | 824 | 1 | — | — | — | — | — | — | — | — | 2,33 | — | 2,20 | 61 | 142 | 2,88 | 0,00 |
| 3% | RTP fixo | 33 | 128 | 160 | 164 | 0,0 | 165 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 171 | 210 | 5,8 | — | — | — | 4,32 | 12 | 3,10 | 32 | 33 | 2,99 | 0,00 |
| 3% | RTP adaptativo | 42 | 136 | 163 | 240 | 1,1 | 223 | 29,7 | 597 | 1765 | 1 | 0 | 0 | 197 | 258 | 6,2 | 548 | 2412 | 3119 | 5,06 | 13 | 2,80 | 32 | 33 | 3,01 | 0,00 |
| 5% | voz (RTP) | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,17 | 8 | 5,60 | 31 | 32 | 5,59 | 0,00 |
| 5% | A (SCTP, controle) | 2474 | 5339 | 5421 | 5472 | 57,1 | 2089 | 7,3 | 173 | 457 | 1 | — | — | — | — | — | — | — | — | 1,31 | — | 5,70 | 75 | 1483 | 5,54 | 0,00 |
| 5% | RTP fixo | 62 | 149 | 167 | 227 | 1,2 | 239 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 262 | 378 | 10,4 | — | — | — | 4,50 | 14 | 5,10 | 32 | 34 | 5,02 | 0,00 |
| 5% | RTP adaptativo | 80 | 148 | 178 | 287 | 13,9 | 2377 | 26,3 | 594 | 1648 | 2 | 2 | 1 | 293 | 416 | 10,2 | 633 | 2535 | 3603 | 4,96 | 15 | 5,02 | 32 | 34 | 5,04 | 0,00 |
| 1,0 Mbps, 0% | voz (RTP) | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 0,00 | 32 | 34 | 0,00 | 0,00 |
| 1,0 Mbps, 0% | A (SCTP, controle) | 1228 | 2912 | 3146 | 3180 | 23,3 | 2160 | 15,5 | 330 | 844 | 1 | — | — | — | — | — | — | — | — | 2,36 | — | 0,40 | 211 | 297 | 0,00 | 1,55 |
| 1,0 Mbps, 0% | RTP fixo | — | — | — | — | 100,0 | 20002 | 0,0 | 600 | 2384 | 30 | 56 | 1 | 567 | 2015 | 36,3 | — | — | — | 8,64 | 20 | 0,00 | 261 | 275 | 0,00 | 62,33 |
| 1,0 Mbps, 0% | RTP adaptativo | 117 | 361 | 565 | 664 | 13,2 | 2355 | 25,8 | 594 | 869 | 2 | 2 | 1 | 15 | 33 | 1,6 | 610 | 1000 | 955 | 2,46 | 10 | 0,00 | 99 | 245 | 0,00 | 0,25 |
| 2,0 Mbps, 1% | voz (RTP) | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 1,10 | 31 | 33 | 1,06 | 0,00 |
| 2,0 Mbps, 1% | A (SCTP, controle) | 725 | 1376 | 1733 | 1763 | 10,6 | 1090 | 24,3 | 507 | 1280 | 1 | — | — | — | — | — | — | — | — | 3,48 | — | 1,00 | 59 | 108 | 0,86 | 0,00 |
| 2,0 Mbps, 1% | RTP fixo | — | — | — | — | 100,0 | 20001 | 0,0 | 600 | 2414 | 31 | 59 | 1 | 306 | 1584 | 28,8 | — | — | — | 8,31 | 16 | 1,00 | 229 | 269 | 1,05 | 33,45 |
| 2,0 Mbps, 1% | RTP adaptativo | 58 | 130 | 164 | 196 | 0,0 | 178 | 29,8 | 597 | 1303 | 1 | 0 | 0 | 86 | 105 | 3,2 | 607 | 1626 | 1787 | 3,68 | 11 | 0,80 | 36 | 56 | 0,98 | 0,00 |

### Menor e maior valor entre as 3 rodadas

| cenário | variante | lat p50 | lat p95 | congelado % | quadros/s | chaves | pedidos (PLI) | RTX % dos bytes | est. mediana kbps | voz perda % | voz p95 |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | voz (RTP) | — | — | — | — | — | — | — | — | 0,00–0,00 | 32–32 |
| 0% | A (SCTP, controle) | 32–32 | 33–38 | 1,6–1,6 | 30,0–30,0 | 1–1 | — | — | — | 0,00–0,00 | 33–37 |
| 0% | RTP fixo | 32–32 | 33–39 | 0,0–0,0 | 29,9–29,9 | 1–1 | 0–0 | 0,0–0,0 | — | 0,00–0,00 | 33–40 |
| 0% | RTP adaptativo | 39–40 | 42–53 | 0,0–0,0 | 29,8–29,8 | 1–1 | 0–0 | 0,3–0,3 | 3383–3681 | 0,00–0,00 | 33–37 |
| 0,3% | voz (RTP) | — | — | — | — | — | — | — | — | 0,10–0,50 | 32–34 |
| 0,3% | A (SCTP, controle) | 32–33 | 115–259 | 1,6–1,6 | 29,9–30,0 | 1–1 | — | — | — | 0,10–0,50 | 61–81 |
| 0,3% | RTP fixo | 32–32 | 33–65 | 0,0–0,0 | 29,9–29,9 | 1–1 | 0–0 | 0,3–0,7 | — | 0,20–0,30 | 33–33 |
| 0,3% | RTP adaptativo | 40–40 | 43–60 | 0,0–0,0 | 29,7–29,8 | 1–1 | 0–0 | 0,7–0,7 | 3049–3168 | 0,40–0,90 | 33–33 |
| 1% | voz (RTP) | — | — | — | — | — | — | — | — | 0,80–1,10 | 32–32 |
| 1% | A (SCTP, controle) | 360–566 | 737–1316 | 3,1–7,6 | 25,2–29,1 | 1–1 | — | — | — | 0,50–1,00 | 93–98 |
| 1% | RTP fixo | 32–33 | 96–107 | 0,0–0,0 | 29,8–29,9 | 1–1 | 0–0 | 1,5–2,2 | — | 0,90–1,40 | 33–34 |
| 1% | RTP adaptativo | 39–40 | 101–122 | 0,0–0,0 | 29,8–29,8 | 1–1 | 0–0 | 1,9–2,5 | 2233–2977 | 0,60–1,00 | 33–34 |
| 3% | voz (RTP) | — | — | — | — | — | — | — | — | 2,80–3,20 | 32–33 |
| 3% | A (SCTP, controle) | 1335–1566 | 2407–2791 | 13,4–19,8 | 12,6–15,3 | 1–1 | — | — | — | 1,80–3,50 | 135–173 |
| 3% | RTP fixo | 33–33 | 126–133 | 0,0–1,0 | 29,8–29,9 | 1–1 | 0–0 | 5,5–6,2 | — | 3,00–3,80 | 33–34 |
| 3% | RTP adaptativo | 38–43 | 131–301 | 0,0–11,5 | 26,4–29,8 | 1–2 | 0–1 | 5,9–6,5 | 1252–2879 | 2,70–3,20 | 33–34 |
| 5% | voz (RTP) | — | — | — | — | — | — | — | — | 4,90–5,70 | 32–33 |
| 5% | A (SCTP, controle) | 2389–2757 | 3914–5358 | 37,5–63,0 | 6,4–8,5 | 1–1 | — | — | — | 4,40–7,00 | 191–1550 |
| 5% | RTP fixo | 60–73 | 145–151 | 0,0–10,9 | 26,6–29,9 | 1–2 | 0–1 | 9,5–10,8 | — | 4,50–5,51 | 33–34 |
| 5% | RTP adaptativo | 69–95 | 142–281 | 0,0–30,5 | 22,7–29,8 | 1–5 | 0–7 | 10,0–11,6 | 1935–2615 | 5,00–5,30 | 33–40 |
| 1,0 Mbps, 0% | voz (RTP) | — | — | — | — | — | — | — | — | 0,00–0,00 | 34–41 |
| 1,0 Mbps, 0% | A (SCTP, controle) | 1228–1229 | 2147–3056 | 17,8–30,2 | 15,3–16,7 | 1–1 | — | — | — | 0,20–0,50 | 296–492 |
| 1,0 Mbps, 0% | RTP fixo | — | — | 100,0–100,0 | 0,0–0,0 | 27–31 | 49–56 | 35,4–38,0 | — | 0,00–0,10 | 275–276 |
| 1,0 Mbps, 0% | RTP adaptativo | 113–121 | 361–1357 | 4,9–45,0 | 17,4–29,7 | 1–13 | 0–22 | 1,1–8,9 | 978–1012 | 0,00–0,00 | 240–273 |
| 2,0 Mbps, 1% | voz (RTP) | — | — | — | — | — | — | — | — | 0,70–1,10 | 32–34 |
| 2,0 Mbps, 1% | A (SCTP, controle) | 631–799 | 1125–1800 | 3,6–12,5 | 21,5–26,1 | 1–1 | — | — | — | 0,50–1,60 | 102–110 |
| 2,0 Mbps, 1% | RTP fixo | — | — | 100,0–100,0 | 0,0–0,0 | 31–32 | 59–59 | 27,6–29,4 | — | 0,90–1,50 | 269–270 |
| 2,0 Mbps, 1% | RTP adaptativo | 50–59 | 129–130 | 0,0–1,2 | 29,8–29,8 | 1–1 | 0–0 | 2,9–4,0 | 1130–1787 | 0,70–0,90 | 51–57 |

## O regime do adaptativo: do segundo 40 ao 60 (mediana de 3 rodadas)

| cenário | variante | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | kbps gerados | chaves | pedidos (PLI) | quebras | NACKs | RTX pacotes | RTX % dos bytes | est. mín kbps | est. mediana | est. final | MB no cano | retorno kbps | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | RTP adaptativo | 41 | 43 | 44 | 49 | 0,0 | 41 | 30,0 | 600 | 2371 | 0 | 0 | 0 | 0 | 13 | 0,1 | 3158 | 4174 | 4025 | 6,34 | 10 | 0,00 | 32 | 34 | 0,00 | 0,00 |
| 0,3% | RTP adaptativo | 42 | 51 | 117 | 153 | 0,0 | 142 | 30,0 | 600 | 2371 | 0 | 0 | 0 | 24 | 33 | 0,6 | 3088 | 3088 | 3109 | 6,37 | 11 | 0,30 | 32 | 34 | 0,20 | 0,00 |
| 1% | RTP adaptativo | 42 | 108 | 134 | 162 | 0,0 | 153 | 30,0 | 600 | 2371 | 0 | 0 | 0 | 80 | 87 | 1,6 | 3192 | 3192 | 3192 | 6,44 | 12 | 0,80 | 31 | 33 | 0,81 | 0,00 |
| 3% | RTP adaptativo | 55 | 135 | 161 | 168 | 0,0 | 161 | 30,1 | 600 | 2371 | 0 | 0 | 0 | 250 | 325 | 6,0 | 3215 | 3215 | 3238 | 6,71 | 14 | 2,60 | 32 | 34 | 2,99 | 0,00 |
| 5% | RTP adaptativo | 93 | 141 | 169 | 203 | 0,0 | 187 | 30,0 | 600 | 2371 | 0 | 0 | 0 | 348 | 542 | 9,9 | 3251 | 3340 | 3287 | 6,95 | 16 | 5,30 | 31 | 34 | 4,87 | 0,00 |
| 1,0 Mbps, 0% | RTP adaptativo | 62 | 347 | 476 | 608 | 1,7 | 326 | 29,9 | 600 | 813 | 0 | 0 | 0 | 6 | 10 | 0,5 | 727 | 944 | 1060 | 2,30 | 9 | 0,00 | 50 | 214 | 0,00 | 0,20 |
| 2,0 Mbps, 1% | RTP adaptativo | 74 | 197 | 281 | 358 | 2,1 | 217 | 30,0 | 600 | 1711 | 0 | 0 | 0 | 102 | 120 | 2,9 | 1739 | 2015 | 1998 | 4,76 | 11 | 0,90 | 48 | 119 | 0,90 | 0,00 |

### Menor e maior valor entre as 3 rodadas

| cenário | variante | lat p50 | lat p95 | congelado % | quadros/s | chaves | pedidos (PLI) | RTX % dos bytes | est. mediana kbps | voz perda % | voz p95 |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0% | RTP adaptativo | 41–41 | 43–43 | 0,0–0,0 | 30,0–30,0 | 0–0 | 0–0 | 0,0–0,1 | 3766–4333 | 0,00–0,00 | 34–34 |
| 0,3% | RTP adaptativo | 41–42 | 44–88 | 0,0–0,0 | 30,0–30,0 | 0–0 | 0–0 | 0,3–0,8 | 3064–3809 | 0,20–0,40 | 34–34 |
| 1% | RTP adaptativo | 42–42 | 105–120 | 0,0–0,0 | 29,9–30,0 | 0–0 | 0–0 | 1,6–2,1 | 3169–4027 | 0,80–0,80 | 33–34 |
| 3% | RTP adaptativo | 45–59 | 134–135 | 0,0–0,0 | 29,9–30,1 | 0–0 | 0–0 | 5,6–6,4 | 3058–3359 | 2,60–3,10 | 34–34 |
| 5% | RTP adaptativo | 89–95 | 138–144 | 0,0–0,0 | 30,0–30,1 | 0–0 | 0–0 | 9,6–10,8 | 3287–4041 | 4,00–6,10 | 34–34 |
| 1,0 Mbps, 0% | RTP adaptativo | 60–166 | 306–415 | 0,0–4,9 | 29,5–30,1 | 0–0 | 0–1 | 0,0–2,4 | 924–944 | 0,00–0,00 | 107–252 |
| 2,0 Mbps, 1% | RTP adaptativo | 65–79 | 169–321 | 0,0–3,8 | 29,8–30,0 | 0–0 | 0–0 | 1,8–3,1 | 1998–2111 | 0,90–1,20 | 85–129 |

## A espera pela retransmissão

Só o prazo muda (`--espera-ms`); a linha de 2000 ms é a da grade. Mediana de 3 rodadas de 20 s, primeiros 20 s; as duas últimas colunas são o menor e o maior valor entre as rodadas.

| cenário | variante | espera ms | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | chaves | pedidos (PLI) | quebras | RTX % dos bytes | congelado % (faixa) | quebras (faixa) |
|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 3% | RTP fixo | 50 | 32 | 51 | 84 | 84 | 95,8 | 6351 | 1,5 | 36 | 64 | 6 | 6,1 | 95,6–97,0 | 5–6 |
| 3% | RTP fixo | 100 | 33 | 123 | 133 | 225 | 36,0 | 1023 | 21,5 | 19 | 20 | 16 | 6,1 | 24,7–39,0 | 11–18 |
| 3% | RTP fixo | 200 | 33 | 130 | 160 | 183 | 0,0 | 173 | 29,9 | 1 | 0 | 0 | 6,1 | 0,0–0,0 | 0–0 |
| 3% | RTP fixo | 500 | 33 | 129 | 159 | 166 | 0,0 | 165 | 29,9 | 1 | 0 | 0 | 6,2 | 0,0–0,0 | 0–0 |
| 3% | RTP fixo | 2000 | 33 | 128 | 160 | 164 | 0,0 | 165 | 29,9 | 1 | 0 | 0 | 5,8 | 0,0–1,0 | 0–0 |
| 5% | RTP fixo | 50 | 32 | 36 | 36 | 36 | 99,4 | 13353 | 0,3 | 35 | 66 | 2 | 10,3 | 98,7–99,8 | 2–3 |
| 5% | RTP fixo | 100 | 61 | 127 | 161 | 213 | 62,2 | 1919 | 14,0 | 28 | 37 | 18 | 10,1 | 49,5–71,9 | 17–20 |
| 5% | RTP fixo | 200 | 63 | 142 | 165 | 220 | 1,7 | 336 | 29,4 | 2 | 1 | 1 | 10,6 | 0,0–2,0 | 0–1 |
| 5% | RTP fixo | 500 | 65 | 146 | 189 | 222 | 2,2 | 240 | 29,9 | 1 | 0 | 0 | 10,2 | 0,0–5,3 | 0–1 |
| 5% | RTP fixo | 2000 | 62 | 149 | 167 | 227 | 1,2 | 239 | 29,9 | 1 | 0 | 0 | 10,4 | 0,0–10,9 | 0–1 |
| 5% | RTP adaptativo | 50 | — | — | — | — | 100,0 | 20001 | 0,0 | 33 | 66 | 1 | 11,2 | 100,0–100,0 | 1–1 |
| 5% | RTP adaptativo | 100 | 39 | 221 | 221 | 221 | 98,7 | 15157 | 0,5 | 33 | 64 | 2 | 10,7 | 88,5–100,0 | 1–3 |
| 5% | RTP adaptativo | 200 | 57 | 140 | 166 | 219 | 15,7 | 3134 | 25,3 | 6 | 9 | 1 | 10,2 | 3,1–19,1 | 1–1 |
| 5% | RTP adaptativo | 500 | 67 | 138 | 167 | 272 | 1,4 | 273 | 29,7 | 1 | 0 | 0 | 10,2 | 1,3–6,8 | 0–1 |
| 5% | RTP adaptativo | 2000 | 80 | 148 | 178 | 287 | 13,9 | 2377 | 26,3 | 2 | 2 | 1 | 10,2 | 0,0–30,5 | 0–2 |

## O teto de retransmissão do str0m

`--sem-teto-rtx` tira o teto de 15% (`DEFAULT_RTX_RATIO_CAP`). Espera de 2 s. O cenário de 10% está fora da grade (`--perda 0.10`). Mediana de 3 rodadas, primeiros 20 s.

| teto de RTX | cenário | variante | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | kbps gerados | chaves | pedidos (PLI) | quebras | NACKs | RTX pacotes | RTX % dos bytes | est. mín kbps | est. mediana | est. final | MB no cano | retorno kbps | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| com o teto (grade) | 3% | RTP fixo | 33 | 128 | 160 | 164 | 0,0 | 165 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 171 | 210 | 5,8 | — | — | — | 4,32 | 12 | 3,10 | 32 | 33 | 2,99 | 0,00 |
| com o teto (grade) | 3% | RTP adaptativo | 42 | 136 | 163 | 240 | 1,1 | 223 | 29,7 | 597 | 1765 | 1 | 0 | 0 | 197 | 258 | 6,2 | 548 | 2412 | 3119 | 5,06 | 13 | 2,80 | 32 | 33 | 3,01 | 0,00 |
| sem o teto | 3% | RTP fixo | 33 | 130 | 160 | 177 | 0,0 | 174 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 182 | 225 | 6,1 | — | — | — | 4,33 | 13 | 2,90 | 32 | 33 | 2,98 | 0,00 |
| sem o teto | 3% | RTP adaptativo | 36 | 132 | 155 | 193 | 0,0 | 198 | 29,8 | 597 | 1281 | 1 | 0 | 0 | 139 | 188 | 5,9 | 611 | 1262 | 3416 | 3,70 | 12 | 2,90 | 32 | 33 | 2,93 | 0,00 |
| com o teto (grade) | 5% | RTP fixo | 62 | 149 | 167 | 227 | 1,2 | 239 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 262 | 378 | 10,4 | — | — | — | 4,50 | 14 | 5,10 | 32 | 34 | 5,02 | 0,00 |
| com o teto (grade) | 5% | RTP adaptativo | 80 | 148 | 178 | 287 | 13,9 | 2377 | 26,3 | 594 | 1648 | 2 | 2 | 1 | 293 | 416 | 10,2 | 633 | 2535 | 3603 | 4,96 | 15 | 5,02 | 32 | 34 | 5,04 | 0,00 |
| sem o teto | 5% | RTP fixo | 60 | 144 | 161 | 196 | 0,0 | 200 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 255 | 366 | 9,7 | — | — | — | 4,49 | 14 | 4,90 | 32 | 33 | 5,01 | 0,00 |
| sem o teto | 5% | RTP adaptativo | 69 | 146 | 169 | 225 | 1,1 | 213 | 29,7 | 597 | 1652 | 1 | 0 | 0 | 282 | 426 | 10,3 | 636 | 2715 | 4135 | 4,93 | 15 | 4,60 | 32 | 33 | 5,07 | 0,00 |
| com o teto | 10% | RTP fixo | 185 | 1765 | 1867 | 1899 | 84,2 | 3864 | 12,2 | 600 | 1929 | 15 | 24 | 8 | 420 | 757 | 16,5 | — | — | — | 6,02 | 18 | 10,50 | 32 | 33 | 10,27 | 0,00 |
| com o teto | 10% | RTP adaptativo | 705 | 1871 | 2139 | 2305 | 85,6 | 4332 | 11,3 | 546 | 1306 | 18 | 30 | 5 | 355 | 525 | 16,8 | 418 | 1459 | 1754 | 4,03 | 16 | 9,90 | 32 | 33 | 10,03 | 0,00 |
| sem o teto | 10% | RTP fixo | 104 | 168 | 198 | 260 | 5,6 | 241 | 29,8 | 600 | 1505 | 1 | 0 | 0 | 394 | 768 | 20,3 | — | — | — | 4,95 | 16 | 8,70 | 32 | 33 | 10,14 | 0,00 |
| sem o teto | 10% | RTP adaptativo | 112 | 428 | 515 | 585 | 2,4 | 266 | 29,6 | 597 | 1175 | 1 | 0 | 0 | 328 | 623 | 21,5 | 404 | 1300 | 2246 | 3,88 | 15 | 10,30 | 32 | 34 | 10,04 | 0,00 |

### Menor e maior valor entre as 3 rodadas

| teto de RTX | cenário | variante | lat p50 | lat p95 | congelado % | quadros/s | chaves | pedidos (PLI) | RTX % dos bytes | est. mediana kbps | voz perda % | voz p95 |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| com o teto (grade) | 3% | RTP fixo | 33–33 | 126–133 | 0,0–1,0 | 29,8–29,9 | 1–1 | 0–0 | 5,5–6,2 | — | 3,00–3,80 | 33–34 |
| com o teto (grade) | 3% | RTP adaptativo | 38–43 | 131–301 | 0,0–11,5 | 26,4–29,8 | 1–2 | 0–1 | 5,9–6,5 | 1252–2879 | 2,70–3,20 | 33–34 |
| sem o teto | 3% | RTP fixo | 33–33 | 130–132 | 0,0–0,0 | 29,8–29,9 | 1–1 | 0–0 | 5,3–6,1 | — | 2,70–3,70 | 33–33 |
| sem o teto | 3% | RTP adaptativo | 35–41 | 130–356 | 0,0–1,1 | 29,7–29,8 | 1–1 | 0–0 | 5,7–6,1 | 855–2357 | 2,10–3,60 | 33–33 |
| com o teto (grade) | 5% | RTP fixo | 60–73 | 145–151 | 0,0–10,9 | 26,6–29,9 | 1–2 | 0–1 | 9,5–10,8 | — | 4,50–5,51 | 33–34 |
| com o teto (grade) | 5% | RTP adaptativo | 69–95 | 142–281 | 0,0–30,5 | 22,7–29,8 | 1–5 | 0–7 | 10,0–11,6 | 1935–2615 | 5,00–5,30 | 33–40 |
| sem o teto | 5% | RTP fixo | 58–63 | 141–151 | 0,0–3,2 | 29,1–29,9 | 1–2 | 0–2 | 9,7–10,2 | — | 4,80–6,60 | 33–33 |
| sem o teto | 5% | RTP adaptativo | 61–71 | 141–153 | 0,0–1,3 | 29,6–29,8 | 1–1 | 0–0 | 10,0–10,6 | 1281–3052 | 4,40–5,40 | 33–33 |
| com o teto | 10% | RTP fixo | 180–1100 | 1565–1765 | 81,9–92,3 | 9,7–12,5 | 12–16 | 19–26 | 16,2–16,8 | — | 9,40–11,60 | 33–33 |
| com o teto | 10% | RTP adaptativo | 115–1233 | 1168–2113 | 65,5–91,6 | 10,1–14,2 | 10–19 | 18–32 | 16,2–16,9 | 905–1549 | 9,80–10,10 | 33–33 |
| sem o teto | 10% | RTP fixo | 102–107 | 163–179 | 1,0–12,0 | 26,5–29,8 | 1–2 | 0–1 | 20,1–21,7 | — | 8,10–11,00 | 33–34 |
| sem o teto | 10% | RTP adaptativo | 103–118 | 181–566 | 2,4–5,0 | 28,9–29,7 | 1–2 | 0–2 | 21,4–21,6 | 1033–1414 | 10,09–10,89 | 33–34 |

## A configuração ajustada: espera de 500 ms, sem o teto de retransmissão

Escolhida **depois** de ver a varredura e a medida do teto, e rodada uma vez: `--espera-ms 500 --sem-teto-rtx`. Mediana de 3 rodadas.

| janela | cenário | variante | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | kbps gerados | chaves | pedidos (PLI) | quebras | NACKs | RTX pacotes | RTX % dos bytes | est. mín kbps | est. mediana | est. final | MB no cano | retorno kbps | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| primeiros 20 s | 0% | RTP adaptativo | 39 | 42 | 43 | 165 | 0,0 | 166 | 29,8 | 597 | 1906 | 1 | 0 | 0 | 0 | 20 | 0,2 | 680 | 3247 | 4469 | 5,14 | 10 | 0,00 | 32 | 33 | 0,00 | 0,00 |
| primeiros 20 s | 0,3% | RTP adaptativo | 40 | 54 | 127 | 166 | 0,0 | 168 | 29,8 | 597 | 1928 | 1 | 0 | 0 | 20 | 38 | 0,8 | 687 | 3406 | 3627 | 5,22 | 11 | 0,30 | 32 | 33 | 0,31 | 0,00 |
| primeiros 20 s | 1% | RTP adaptativo | 39 | 102 | 133 | 162 | 0,0 | 158 | 29,8 | 597 | 1689 | 1 | 0 | 0 | 66 | 83 | 2,2 | 622 | 2371 | 3766 | 4,67 | 11 | 0,80 | 32 | 33 | 0,91 | 0,00 |
| primeiros 20 s | 3% | RTP adaptativo | 38 | 132 | 161 | 192 | 0,0 | 189 | 29,7 | 597 | 1298 | 1 | 0 | 0 | 148 | 213 | 6,2 | 602 | 1167 | 3561 | 3,75 | 12 | 3,40 | 32 | 33 | 3,00 | 0,00 |
| primeiros 20 s | 5% | RTP adaptativo | 63 | 146 | 173 | 200 | 0,0 | 186 | 29,7 | 597 | 1535 | 1 | 0 | 0 | 251 | 383 | 10,2 | 564 | 1747 | 3249 | 4,59 | 14 | 5,99 | 32 | 33 | 5,06 | 0,00 |
| primeiros 20 s | 1,0 Mbps, 0% | RTP adaptativo | 136 | 664 | 868 | 1102 | 10,6 | 838 | 27,6 | 594 | 841 | 2 | 1 | 1 | 45 | 182 | 9,3 | 531 | 940 | 914 | 2,53 | 10 | 0,00 | 92 | 267 | 0,00 | 1,86 |
| primeiros 20 s | 2,0 Mbps, 1% | RTP adaptativo | 55 | 131 | 164 | 196 | 0,0 | 168 | 29,7 | 597 | 1237 | 1 | 0 | 0 | 81 | 96 | 3,2 | 549 | 1458 | 1951 | 3,47 | 11 | 0,90 | 35 | 53 | 0,99 | 0,00 |
| primeiros 20 s | 3% | RTP fixo | 33 | 129 | 160 | 188 | 0,0 | 168 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 181 | 234 | 6,5 | — | — | — | 4,35 | 13 | 2,50 | 32 | 34 | 2,98 | 0,00 |
| primeiros 20 s | 5% | RTP fixo | 65 | 152 | 181 | 197 | 0,0 | 196 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 267 | 385 | 10,4 | — | — | — | 4,50 | 14 | 3,80 | 32 | 34 | 5,01 | 0,00 |
| 40–60 s | 3% | RTP adaptativo | 53 | 134 | 158 | 177 | 0,0 | 167 | 29,9 | 600 | 2371 | 0 | 0 | 0 | 244 | 309 | 5,7 | 3288 | 3288 | 3288 | 6,69 | 14 | 3,10 | 32 | 34 | 2,94 | 0,00 |
| 40–60 s | 5% | RTP adaptativo | 93 | 139 | 166 | 201 | 0,0 | 162 | 29,9 | 600 | 2371 | 0 | 0 | 0 | 347 | 530 | 9,8 | 3303 | 3314 | 3314 | 6,94 | 16 | 5,50 | 32 | 34 | 4,94 | 0,00 |

### Menor e maior valor entre as 3 rodadas

| janela | cenário | variante | lat p50 | lat p95 | congelado % | quadros/s | chaves | pedidos (PLI) | RTX % dos bytes | est. mediana kbps | voz perda % | voz p95 |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| primeiros 20 s | 0% | RTP adaptativo | 39–40 | 42–43 | 0,0–0,0 | 29,8–29,8 | 1–1 | 0–0 | 0,2–0,3 | 3026–4176 | 0,00–0,00 | 33–34 |
| primeiros 20 s | 0,3% | RTP adaptativo | 40–40 | 51–75 | 0,0–0,0 | 29,8–29,8 | 1–1 | 0–0 | 0,8–1,0 | 2641–3547 | 0,20–0,60 | 33–34 |
| primeiros 20 s | 1% | RTP adaptativo | 35–40 | 97–106 | 0,0–0,0 | 29,8–29,8 | 1–1 | 0–0 | 1,7–2,3 | 964–3766 | 0,80–1,40 | 33–34 |
| primeiros 20 s | 3% | RTP adaptativo | 37–40 | 128–132 | 0,0–1,0 | 29,6–29,7 | 1–1 | 0–0 | 5,7–6,3 | 1081–1269 | 3,30–4,20 | 33–34 |
| primeiros 20 s | 5% | RTP adaptativo | 62–75 | 135–153 | 0,0–0,0 | 29,7–29,7 | 1–1 | 0–0 | 9,8–10,4 | 1166–2969 | 4,90–6,30 | 33–34 |
| primeiros 20 s | 1,0 Mbps, 0% | RTP adaptativo | 116–151 | 561–757 | 9,4–40,2 | 17,4–28,3 | 1–14 | 0–24 | 7,6–106,6 | 902–1001 | 0,00–0,00 | 265–276 |
| primeiros 20 s | 2,0 Mbps, 1% | RTP adaptativo | 54–59 | 128–134 | 0,0–0,0 | 29,6–29,8 | 1–1 | 0–0 | 2,5–3,4 | 1436–1660 | 0,50–1,10 | 51–67 |
| primeiros 20 s | 3% | RTP fixo | 33–33 | 127–131 | 0,0–0,0 | 29,8–29,9 | 1–1 | 0–0 | 5,8–6,5 | — | 2,40–3,20 | 34–34 |
| primeiros 20 s | 5% | RTP fixo | 65–71 | 149–153 | 0,0–1,0 | 29,7–29,9 | 1–1 | 0–0 | 10,3–10,8 | — | 3,40–5,00 | 34–34 |
| 40–60 s | 3% | RTP adaptativo | 43–64 | 134–136 | 0,0–0,0 | 29,9–30,0 | 0–0 | 0–0 | 5,1–6,2 | 3253–3989 | 2,10–4,00 | 34–34 |
| 40–60 s | 5% | RTP adaptativo | 78–94 | 138–156 | 0,0–0,0 | 29,9–30,0 | 0–0 | 0–0 | 9,2–10,4 | 3303–3809 | 4,80–5,59 | 34–34 |

## As variações (primeiros 20 s; mediana de 3 rodadas)

### A estimativa partindo de 2,79 Mbps em vez de 300 kbps

| estimativa inicial | cenário | variante | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | kbps gerados | chaves | pedidos (PLI) | quebras | NACKs | RTX pacotes | RTX % dos bytes | est. mín kbps | est. mediana | est. final | MB no cano | retorno kbps | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 300 kbps (grade) | 0% | RTP adaptativo | 40 | 42 | 46 | 163 | 0,0 | 167 | 29,8 | 597 | 1920 | 1 | 0 | 0 | 0 | 20 | 0,3 | 666 | 3532 | 3987 | 5,18 | 10 | 0,00 | 32 | 33 | 0,00 | 0,00 |
| 300 kbps (grade) | 1,0 Mbps, 0% | RTP adaptativo | 117 | 361 | 565 | 664 | 13,2 | 2355 | 25,8 | 594 | 869 | 2 | 2 | 1 | 15 | 33 | 1,6 | 610 | 1000 | 955 | 2,46 | 10 | 0,00 | 99 | 245 | 0,00 | 0,25 |
| 2,79 Mbps | 0% | RTP adaptativo | 40 | 52 | 286 | 359 | 1,8 | 360 | 29,8 | 597 | 2055 | 1 | 0 | 0 | 0 | 0 | 0,0 | 1287 | 3110 | 3843 | 5,52 | 10 | 0,00 | 32 | 34 | 0,00 | 0,00 |
| 2,79 Mbps | 1,0 Mbps, 0% | RTP adaptativo | 106 | 472 | 597 | 662 | 5,6 | 508 | 29,8 | 597 | 852 | 1 | 0 | 0 | 11 | 45 | 2,2 | 835 | 930 | 859 | 2,44 | 9 | 0,00 | 62 | 236 | 0,00 | 0,44 |

| estimativa inicial | cenário | variante | lat p50 | lat p95 | congelado % | quadros/s | chaves | pedidos (PLI) | RTX % dos bytes | est. mediana kbps | voz perda % | voz p95 |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 300 kbps (grade) | 0% | RTP adaptativo | 39–40 | 42–53 | 0,0–0,0 | 29,8–29,8 | 1–1 | 0–0 | 0,3–0,3 | 3383–3681 | 0,00–0,00 | 33–37 |
| 300 kbps (grade) | 1,0 Mbps, 0% | RTP adaptativo | 113–121 | 361–1357 | 4,9–45,0 | 17,4–29,7 | 1–13 | 0–22 | 1,1–8,9 | 978–1012 | 0,00–0,00 | 240–273 |
| 2,79 Mbps | 0% | RTP adaptativo | 40–41 | 44–59 | 1,8–1,9 | 29,8–29,8 | 1–1 | 0–0 | 0,0–0,0 | 3045–3209 | 0,00–0,00 | 34–34 |
| 2,79 Mbps | 1,0 Mbps, 0% | RTP adaptativo | 70–165 | 466–478 | 5,6–6,7 | 28,9–29,8 | 1–1 | 0–0 | 1,5–2,7 | 923–941 | 0,00–0,00 | 234–240 |

### A voz com o buffer de reordenação padrão do str0m

| reordenação da voz | cenário | variante | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | kbps gerados | chaves | pedidos (PLI) | quebras | NACKs | RTX pacotes | RTX % dos bytes | est. mín kbps | est. mediana | est. final | MB no cano | retorno kbps | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 0 (grade) | 3% | RTP fixo | 33 | 128 | 160 | 164 | 0,0 | 165 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 171 | 210 | 5,8 | — | — | — | 4,32 | 12 | 3,10 | 32 | 33 | 2,99 | 0,00 |
| 15 (padrão do str0m) | 3% | RTP fixo | 33 | 128 | 157 | 181 | 0,0 | 165 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 181 | 225 | 6,3 | — | — | — | 4,34 | 13 | 2,70 | 33 | 292 | 2,98 | 0,00 |

### 20 ms de jitter em cada sentido

O cano sorteia um atraso a mais, uniforme entre 0 e 20 ms, por datagrama, sem trocar a ordem.

| jitter | cenário | variante | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | kbps gerados | chaves | pedidos (PLI) | quebras | NACKs | RTX pacotes | RTX % dos bytes | est. mín kbps | est. mediana | est. final | MB no cano | retorno kbps | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| sem jitter (grade) | 0% | RTP fixo | 32 | 33 | 34 | 37 | 0,0 | 49 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 0 | 0 | 0,0 | — | — | — | 4,09 | 10 | 0,00 | 32 | 33 | 0,00 | 0,00 |
| sem jitter (grade) | 0% | RTP adaptativo | 40 | 42 | 46 | 163 | 0,0 | 167 | 29,8 | 597 | 1920 | 1 | 0 | 0 | 0 | 20 | 0,3 | 666 | 3532 | 3987 | 5,18 | 10 | 0,00 | 32 | 33 | 0,00 | 0,00 |
| 20 ms | 0% | RTP fixo | 50 | 53 | 54 | 55 | 0,0 | 71 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 0 | 0 | 0,0 | — | — | — | 4,09 | 10 | 0,00 | 44 | 52 | 0,00 | 0,00 |
| 20 ms | 0% | RTP adaptativo | 55 | 61 | 63 | 145 | 0,0 | 149 | 29,8 | 597 | 1753 | 1 | 0 | 0 | 0 | 18 | 0,3 | 486 | 2983 | 3655 | 4,74 | 10 | 0,00 | 44 | 51 | 0,00 | 0,00 |
| sem jitter (grade) | 3% | RTP fixo | 33 | 128 | 160 | 164 | 0,0 | 165 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 171 | 210 | 5,8 | — | — | — | 4,32 | 12 | 3,10 | 32 | 33 | 2,99 | 0,00 |
| sem jitter (grade) | 3% | RTP adaptativo | 42 | 136 | 163 | 240 | 1,1 | 223 | 29,7 | 597 | 1765 | 1 | 0 | 0 | 197 | 258 | 6,2 | 548 | 2412 | 3119 | 5,06 | 13 | 2,80 | 32 | 33 | 3,01 | 0,00 |
| 20 ms | 3% | RTP fixo | 52 | 169 | 196 | 222 | 0,0 | 196 | 29,8 | 600 | 1505 | 1 | 0 | 0 | 237 | 335 | 9,1 | — | — | — | 4,45 | 13 | 3,20 | 43 | 51 | 3,00 | 0,00 |
| 20 ms | 3% | RTP adaptativo | 59 | 172 | 192 | 223 | 11,3 | 2254 | 26,3 | 595 | 1631 | 2 | 1 | 1 | 227 | 343 | 8,7 | 468 | 1858 | 3287 | 4,80 | 13 | 3,10 | 43 | 51 | 2,99 | 0,00 |

| jitter | cenário | variante | lat p50 | lat p95 | congelado % | quadros/s | chaves | pedidos (PLI) | RTX % dos bytes | est. mediana kbps | voz perda % | voz p95 |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| sem jitter (grade) | 0% | RTP fixo | 32–32 | 33–39 | 0,0–0,0 | 29,9–29,9 | 1–1 | 0–0 | 0,0–0,0 | — | 0,00–0,00 | 33–40 |
| sem jitter (grade) | 0% | RTP adaptativo | 39–40 | 42–53 | 0,0–0,0 | 29,8–29,8 | 1–1 | 0–0 | 0,3–0,3 | 3383–3681 | 0,00–0,00 | 33–37 |
| 20 ms | 0% | RTP fixo | 50–50 | 52–53 | 0,0–0,0 | 29,9–29,9 | 1–1 | 0–0 | 0,0–0,0 | — | 0,00–0,00 | 52–52 |
| 20 ms | 0% | RTP adaptativo | 54–55 | 61–61 | 0,0–0,0 | 29,8–29,8 | 1–1 | 0–0 | 0,2–0,5 | 1915–3156 | 0,00–0,00 | 51–51 |
| sem jitter (grade) | 3% | RTP fixo | 33–33 | 126–133 | 0,0–1,0 | 29,8–29,9 | 1–1 | 0–0 | 5,5–6,2 | — | 3,00–3,80 | 33–34 |
| sem jitter (grade) | 3% | RTP adaptativo | 38–43 | 131–301 | 0,0–11,5 | 26,4–29,8 | 1–2 | 0–1 | 5,9–6,5 | 1252–2879 | 2,70–3,20 | 33–34 |
| 20 ms | 3% | RTP fixo | 52–52 | 164–171 | 0,0–1,1 | 29,7–29,9 | 1–1 | 0–0 | 8,5–9,2 | — | 2,60–3,80 | 51–52 |
| 20 ms | 3% | RTP adaptativo | 53–67 | 168–172 | 1,1–14,4 | 26,2–29,8 | 1–2 | 0–2 | 8,6–8,9 | 786–2793 | 3,10–3,20 | 50–51 |

### As bandas estreitas em 60 s seguidos (sem aquecimento; 3 rodadas de 60 s)

Acrescentadas por último. «Padrão» é a configuração da grade; «espera 500» muda só o prazo; «pedidos calmos» é espera de 500 ms com o PLI repetido a cada 1 s e chave no máximo a cada 1 s.

| configuração | cenário | variante | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | kbps gerados | chaves | pedidos (PLI) | quebras | NACKs | RTX pacotes | RTX % dos bytes | est. mín kbps | est. mediana | est. final | MB no cano | retorno kbps | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| padrão | 1,0 Mbps, 0% | RTP adaptativo | 138 | 375 | 561 | 656 | 7,0 | 2330 | 28,6 | 1794 | 861 | 2 | 1 | 1 | 38 | 95 | 1,5 | 522 | 978 | 908 | 7,36 | 9 | 0,00 | 86 | 245 | 0,00 | 0,25 |
| espera 500 | 1,0 Mbps, 0% | RTP adaptativo | 128 | 575 | 717 | 1101 | 14,1 | 2056 | 26,6 | 1768 | 854 | 11 | 15 | 4 | 114 | 192 | 3,3 | 557 | 965 | 1081 | 7,31 | 10 | 0,00 | 94 | 261 | 0,00 | 2,12 |
| pedidos calmos | 1,0 Mbps, 0% | RTP adaptativo | 112 | 553 | 817 | 1060 | 28,6 | 7324 | 22,2 | 1767 | 871 | 11 | 15 | 6 | 259 | 341 | 5,5 | 602 | 943 | 1179 | 7,64 | 10 | 0,00 | 125 | 269 | 0,00 | 3,57 |
| padrão | 2,0 Mbps, 1% | RTP adaptativo | 65 | 165 | 216 | 299 | 0,0 | 197 | 29,9 | 1797 | 1478 | 1 | 0 | 0 | 303 | 366 | 3,1 | 508 | 1868 | 2131 | 12,51 | 11 | 0,90 | 43 | 74 | 0,96 | 0,00 |

| configuração | cenário | variante | lat p50 | lat p95 | congelado % | quadros/s | chaves | pedidos (PLI) | RTX % dos bytes | est. mediana kbps | voz perda % | voz p95 |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| padrão | 1,0 Mbps, 0% | RTP adaptativo | 136–154 | 340–413 | 4,5–11,2 | 27,4–29,2 | 1–3 | 0–4 | 0,9–2,0 | 969–1022 | 0,00–0,00 | 236–245 |
| espera 500 | 1,0 Mbps, 0% | RTP adaptativo | 97–164 | 415–601 | 4,0–14,5 | 26,5–29,8 | 1–11 | 0–16 | 2,1–3,6 | 959–976 | 0,00–0,00 | 250–261 |
| pedidos calmos | 1,0 Mbps, 0% | RTP adaptativo | 60–220 | 542–593 | 20,3–29,7 | 21,6–26,1 | 8–12 | 7–15 | 4,5–6,1 | 921–977 | 0,00–0,03 | 264–269 |
| padrão | 2,0 Mbps, 1% | RTP adaptativo | 61–65 | 140–403 | 0,0–4,3 | 28,9–29,9 | 1–1 | 0–0 | 3,1–3,9 | 1846–1968 | 0,73–0,90 | 71–159 |

## As conferências

- **O vídeo passou pelo cano.** Nas 210 células RTP de todas as passagens: zero datagramas sumidos entre o socket de quem transmite, o cano e o socket de quem recebe; zero datagramas mandados por fora do cano; zero quadros ilegíveis; zero escritas recusadas pelo str0m.
- **A perda sorteada bate com a configurada.** Média (e faixa) da perda da ida nas 12 células de cada cenário da grade: 0,3%: 0,31% (0,10–0,48); 1%: 0,93% (0,77–1,07); 3%: 3,04% (2,51–3,63); 5%: 5,24% (4,68–5,78); 2,0 Mbps, 1%: 0,96% (0,77–1,06). A semente é a mesma para as quatro variantes de uma rodada, então são três sorteios por cenário, não doze.
- **O relógio é um só.** O instante da captura e o da exibição são lidos da mesma função, no mesmo processo. Sem perda e sem gargalo o RTP fixo dá 32 ms de p50 contra 30 ms de atraso do cano.
- **O número do quadro e o str0m.** Em nenhuma célula o str0m deu como contíguo um quadro com o número fora de sequência, nem o contrário («desacordos» = 0). Houve quadros entregues como contíguos **com o tamanho errado**: ver o item 14.
- **O caminho do PLI foi exercitado**: na grade com perda ele quase não aparece (o NACK conserta antes), mas nas passagens de 50 e 100 ms de espera, no RTP fixo com gargalo e a 10% de perda há dezenas de pedidos e de chaves por célula, e a imagem volta depois de cada chave inteira.

## O que os números mostram

1. **A bancada reproduz a medida anterior.** A linha «A» rodada aqui dá 32 ms e 30,0 quadros/s sem perda (lá 32 e 30,0); 505 ms a 1% (lá 437; aqui a faixa é 360–566); 1432 ms, 18,6% congelado e 15,1 quadros/s a 3% (lá 1528, 17,6% e 14,2); 2474 ms, 57,1% e 7,3 a 5% (lá 2518, 43,5% e 8,4; a faixa aqui é 37,5–63,0%); 1228 ms e 15,5 a 1,0 Mbps (lá 1229 e 15,5); 725 ms e 24,3 a 2,0 Mbps com 1% (lá 719 e 24,5).
2. **Com perda sorteada e sem gargalo, o vídeo por RTP não atrasa.** RTP fixo, mesma carga da linha «A»: p50 de 32 ms a 1%, 33 ms a 3% e 62 ms a 5%, com p95 de 97, 128 e 149 ms e 29,9 quadros/s nos três. A linha «A» na mesma sessão: 505, 1432 e 2474 ms de p50, e 27,2, 15,1 e 7,3 quadros/s. Tempo congelado (mediana): 0,0%, 0,0% e 1,2% contra 4,5%, 18,6% e 57,1%.
3. **Quem conserta é a retransmissão, não o pedido de chave.** Na grade, de 0,3% a 5%, o RTP fixo mandou uma chave por célula (a primeira) e nenhum PLI em 11 das 12 células. Com a espera em 50 ms — menos que uma ida e volta, ou seja, sem tempo para retransmissão nenhuma chegar — a mesma carga fica 95,8% do tempo congelada a 3% e 99,4% a 5%, com 64–66 PLIs e 35–36 chaves por célula: é o desenho «B» da medida do SCTP de novo, e dá no mesmo.
4. **O custo em banda é cerca de duas vezes a perda.** «RTX % dos bytes» no RTP fixo: 0,7% a 0,3%, 1,6% a 1%, 5,8% a 3%, 10,4% a 5% (e ~20% a 10%, sem o teto). Em pacotes, a 5%, são 378 retransmissões para uns 180 pacotes de vídeo perdidos: duas por perda, o que a leitura do NACK a cada 33 ms previa. O total no cano sobe de 4,09 MB sem perda para 4,50 MB a 5% (+10%). O retorno (NACK, TWCC, relatórios) custa 10 a 16 kbps. Chaves: uma por célula, salvo nas células com quebra.
5. **O custo em latência é a cauda, de uns 100 ms.** O p95 fica em 97–149 ms e o máximo em 153–227 ms (fixo, 1% a 5%): o caminho (30 ms) mais uma retransmissão. Um reprodutor de verdade teria de segurar isso em buffer (ver «NÃO mostram»).
6. **A estimativa de banda do str0m não leu perda sorteada como congestionamento.** Partindo de 300 kbps, a mediana da estimativa nos primeiros 20 s foi 3532 kbps sem perda, 3067 a 0,3%, 2748 a 1%, 2412 a 3% e 2535 a 5%; do segundo 40 ao 60 ficou entre 3088 e 4174 kbps nos cinco, e o codificador de mentira gerou 2371 kbps — o teto de 2,79 Mbps × 0,85 — em todos, com 0,0% de tempo congelado nas 15 rodadas. O orçamento do produto de hoje, no mesmo intervalo, é 585–640 kbps a 1%, 234–268 a 3% e 200 a 5%. A 10% a mediana da estimativa caiu a 1300–1459 kbps.
7. **As quebras de cadeia são raras, e com os padrões do str0m cada uma custa mais de 2 s.** Quando uma perda não é consertada, o receptor espera o prazo inteiro (2 s), larga o quadro, pede chave, e a imagem volta 0,2 a 0,6 s depois: os maiores intervalos nas células com uma quebra só são 2177 a 2570 ms. É isso, e só isso, que tira o RTP do critério nos primeiros 20 s a 5%: no adaptativo, 13,9% de tempo congelado na mediana (0,0–30,5), com p50 de 80 ms; no fixo, 1,2% (0,0–10,9).
8. **A 5% essas quebras vêm, quase todas, do teto de retransmissão do str0m.** Contando as células de 3%, 5% e 10% com espera de 500 ms ou mais (grade, regime, varredura, passagens sem o teto, ajustada e as duas de investigação): a 5%, **8 de 21 células com o teto tiveram quebra por perda não consertada, e 0 de 15 sem o teto**; a 3%, 1 de 15 e 0 de 15; a 10%, 6 de 6 (várias por célula: 84–86% de tempo congelado) e 2 de 12 (5,6% e 2,4% congelado, p50 de 104–112 ms). O mecanismo é leitura de código: acima de 15% de retransmissão no último segundo o str0m joga fora as retransmissões pendentes; a média a 5% é ~10%, e um segundo com mais perda passa do teto. **Não foi isolado** instrumentando o str0m. As 2 quebras a 10% sem o teto são coerentes com as 5 tentativas de NACK esgotadas (conta, não medida).
9. **A espera pela retransmissão tem um mínimo e, acima dele, só muda o preço de cada quebra.** 50 ms: nada funciona. 100 ms: 36,0% congelado a 3% e 62,2% a 5% (fixo). 200 ms: o fixo passa (0,0% a 3%; 1,7% a 5%, com quebras de 336–407 ms), e o adaptativo não (15,7% a 5%): com ritmo, a chave de 8 deltas leva uns 200 ms para sair, e o prazo conta do primeiro pacote. 500 ms: 0,0% a 3%; a 5%, 2,2% (0,0–5,3) no fixo e 1,4% (1,3–6,8) no adaptativo, com cada quebra custando 616 a 1153 ms em vez de mais de 2 s. O padrão de 2 s do str0m é o pior dos valores que funcionam. O que não foi explicado: por que a recuperação do adaptativo a 200 ms leva 3,1–3,8 s em duas rodadas e 0,6 s na terceira.
10. **Com a espera em 500 ms e sem o teto, 3% e 5% passam em todas as rodadas** (uma passagem, escolhida depois de ver as outras). Adaptativo, primeiros 20 s: 38 ms e 0,0% (0,0–1,0) a 3%; 63 ms e 0,0% (0,0–0,0) a 5%. Do segundo 40 ao 60: 53 ms e 93 ms, 0,0% nas seis rodadas, a 2371 kbps. Fixo: 33 ms e 65 ms, 0,0% (0,0–1,0). **A mesma configuração piora o gargalo de 1,0 Mbps**: 136 ms, 10,6% congelado (9,4–40,2), com a retransmissão chegando a 106,6% dos bytes de vídeo numa rodada. Sem o teto, uma fila que transborda vira enxurrada de retransmissão; o teto existe para isso. Um valor intermediário não foi medido.
11. **Nos gargalos, o RTP sem controle de taxa não exibe nada.** O RTP fixo ficou 100% do tempo congelado a 1,0 Mbps e a 2,0 Mbps com 1%, nas seis rodadas, pondo 8,3–8,6 MB no cano (a linha «A» põe 2,4–3,5) com 33–62% descartado na fila. A chave de 80 KiB sai de uma vez (não há ritmo sem a estimativa), não cabe na fila de 250 ms, as retransmissões dela saem em dobro, e cada PLI traz outra chave. A linha «A» exibe nesses cenários porque o SCTP tem controle de congestionamento. A 2,0 Mbps a carga caberia (1,47 Mbps): o que mata é a rajada.
12. **Com a estimativa, 2,0 Mbps com 1% passa, e 1,0 Mbps fica na beira.** A 2,0 Mbps com 1%: p50 de 58 ms e 0,0% congelado (0,0–1,2) nos primeiros 20 s; 74 ms e 2,1% (0,0–3,8) do segundo 40 ao 60; 65 ms e 0,0% (0,0–4,3) em 60 s seguidos; estimativa de 1,6 a 2,0 Mbps. A 1,0 Mbps: 117 ms e 13,2% (4,9–45,0) nos primeiros 20 s; 62 ms e 1,7% (0,0–4,9) do segundo 40 ao 60; **138 ms e 7,0% (4,5–11,2) em 60 s seguidos**, e uma quarta rodada de 60 s, a da investigação, deu 172 ms e 26,4%. A série segundo a segundo mostra por quê: a estimativa sobe em dente de serra até 1,2–1,3 Mbps num caminho de 1,0 Mbps, a cada ~8 s, e desce a ~0,9; cada subida enche a fila (o p50 de 138 ms é fila), algumas transbordam, e de zero a quatro por minuto acabam em quebra. A linha «A» dá 1228 ms e 23,3% ali; o produto de hoje, 48–51 ms e 6,5% em regime (29,9% nos primeiros 20 s). **Mudar a espera para 500 ms (14,1%, 4,0–14,5) ou espaçar os pedidos de chave (28,6%, 20,3–29,7) não melhorou** em 60 s, e por que o segundo piora não foi investigado. A fração de 85% da estimativa não basta quando a estimativa passa 20–30% do caminho; uma fração menor não foi medida.
13. **A partida.** Partindo de 300 kbps a menor amostra da estimativa na janela já é de 534–666 kbps, e a imagem não espera por ela (0,0% congelado sem perda). Partindo de 2,79 Mbps: sem perda, 1,8% congelado (um soluço de ~360 ms na primeira chave, que sai no ritmo); a 1,0 Mbps, 106 ms e 5,6% (5,6–6,7), **melhor** que os 13,2% (4,9–45,0) da partida em 300 kbps. Três rodadas não sustentam a comparação: o que varia a 1,0 Mbps é se a oscilação do item 12 produz ou não uma quebra dentro da janela.
14. **O primeiro quadro pode sair sem o primeiro pacote.** Em 3 células sem o teto (uma a 5%, duas a 10%) a cadeia quebrou sem o str0m dar nenhum quadro como não contíguo. Na reprodução com o aviso ligado apareceu o caso: a primeira chave entregue com 81 900 bytes em vez de 81 920, como contígua — faltava o pacote com o SPS e o PPS, o primeiro do fluxo, que se perdeu. Lido em `buffer_rx.rs`: antes de emitir qualquer coisa o str0m considera contíguo o primeiro quadro completo que tiver (`is_following_last` devolve `true` sem quadro anterior). O decodificador de mentira recusa (o tamanho não bate), pede chave, e a imagem chega 0,6 a 1,05 s depois. Um caso foi visto diretamente; os outros dois são a mesma assinatura. Acontece quando o primeiro pacote do fluxo se perde, com ou sem o teto.
15. **A voz não sente o vídeo, fora do gargalo.** Sozinha por RTP: p95 de 32–34 ms em todos os cenários. Com o vídeo por RTP (fixo ou adaptativo): 33–34 ms de 0% a 5%. Com o vídeo na linha «A»: 98 ms a 1%, 142 ms a 3%, 1483 ms a 5%. A perda da voz é a do caminho (não há retransmissão de voz): 3,1% a 3%, 5,1% a 5%. Nos gargalos a voz divide a fila com o vídeo: p95 de 245 ms (adaptativo) a 1,0 Mbps, contra 297 ms na linha «A»; a 2,0 Mbps com 1%, 56 ms contra 108. Com o buffer de reordenação padrão do str0m (15 pacotes) o p95 da voz a 3% vai de 33 para 292 ms: cada perda segura os pacotes seguintes.
16. **Com 20 ms de jitter a estimativa não desabou.** Sem perda: mediana de 2983 kbps (3532 sem jitter), p50 de 55 ms. A 3%: o fixo fica em 52 ms e 0,0% (0,0–1,1), com a retransmissão subindo de 5,8% para 9,1% dos bytes; o adaptativo teve 11,3% de tempo congelado (1,1–14,4), com uma quebra de 2 s em duas das três rodadas. Com mais retransmissão o teto do item 8 fica mais perto; isso é hipótese.

### A resposta à pergunta

- **A 3% o RTP fica utilizável** (p50 < 300 ms e < 5% congelado) em todas as medianas: 33 ms / 0,0% (fixo), 42 ms / 1,1% (adaptativo, primeiros 20 s; uma rodada de três com 11,5%), 55 ms / 0,0% (40–60 s). A linha «A» dá 1432 ms / 18,6%, e o produto de hoje 378 ms / 34,0% nos primeiros 20 s e 32 ms / 24,8% em regime, a 234–268 kbps.
- **A 5% a latência passa com folga e o tempo congelado depende de dois parâmetros do str0m.** Com os padrões: fixo 62 ms / 1,2% (0,0–10,9); adaptativo 80 ms / 13,9% (0,0–30,5) nos primeiros 20 s e 93 ms / 0,0% (0,0–0,0) do segundo 40 ao 60. Com espera de 500 ms e sem o teto de retransmissão: 63 ms / 0,0% e 93 ms / 0,0%, em todas as seis rodadas. A linha «A» dá 2474 ms / 57,1%; o produto de hoje, 2756 ms / 69,0% e 129 ms / 46,7%, a 200 kbps.
- **Nos outros cenários:** sem perda empata (40 ms contra 32: o ritmo de saída custa 8 ms); a 0,3% e 1% ganha na cauda (p95 de 52 e 102 ms contra 845 e 1090 ms do produto nos primeiros 20 s, 117 e 216 ms em regime) e no orçamento (2,4 Mbps contra 0,5–1,5); a 2,0 Mbps com 1% ganha da linha «A» e fica perto do produto (58–74 ms / 0,0–2,1% contra 41–86 ms / 3,2–8,9%), com três vezes o orçamento (1,6–2,0 Mbps contra 0,47–0,74); a 1,0 Mbps ganha da linha «A» e **não ganha do produto de hoje em regime** (138 ms / 7,0% em 60 s contra 51 ms / 6,5%), com orçamento maior (0,86 Mbps gerados contra 0,53–0,63). **RTP sem estimativa de banda perde de tudo nos dois gargalos.**

## O que os números NÃO mostram

- **Não é o produto.** São dois `Rtc` crus lado a lado. Não há `Ponta`, `Conexao`, envelope, sala, nem **o servidor no meio repassando para N espectadores**: no produto o vídeo sobe de quem transmite ao servidor e desce a cada espectador por outro caminho, com outra perda. Quem responde ao NACK de um espectador (o servidor, de um cache dele, ou quem transmite), o que um PLI de **um** espectador custa aos outros (uma chave para todos), e de quem é a estimativa de banda que o codificador segue quando os espectadores têm caminhos diferentes são as perguntas de projeto de um transporte de mídia com servidor, e nenhuma foi tocada aqui. Esta medida diz o que um salto RTP entrega, e mais nada.
- **Sem codificador de verdade.** Os quadros têm exatamente o tamanho pedido; não há cena movimentada que estoure o orçamento, e a chave do adaptativo tem 8 × o delta por suposição (a do fixo tem 13 ×). Um `VideoEncoder` de verdade não reage à estimativa no quadro seguinte, e não foi medido.
- **Sem decodificador de verdade, e sem imagem.** «Exibível» é uma conta sobre números de quadro. O H.264 de mentira não tem quadros B, nem camadas temporais, nem fatias; não há ocultação de erro. Um decodificador de verdade, com a cadeia quebrada, pode mostrar imagem estragada em vez de congelar; aqui congela. E «30 quadros/s a X kbps» continua sendo uma frase sobre bytes: a qualidade da imagem não foi olhada.
- **Sem buffer de reprodução (jitter buffer).** A latência é até o quadro sair do str0m. Um reprodutor de verdade segura quadros para tocar num ritmo liso, e com retransmissão ele tem de segurar pelo menos o tempo de uma (aqui o p95 fica uns 100 ms acima do atraso do caminho): a latência que a pessoa vê é a mediana daqui **mais** esse buffer, e mais perto do p95 desta medida que do p50. O mesmo vale, sem tirar nem pôr, para a linha «A» e para a medida do produto, que também não têm buffer.
- **Sem navegador.** O par é str0m dos dois lados. O espectador pela página usaria o WebRTC do navegador, com o NACK, o PLI, o buffer e a estimativa **dele**, que não são os do str0m. Nada aqui diz como o Chrome ou o Safari se comportam recebendo isto.
- **Perda independente, não em rajada.** O cano sorteia cada datagrama por si. Perda de verdade (Wi-Fi, rádio) vem agrupada, e uma rajada que leva um pacote e as retransmissões dele é exatamente o caso em que o NACK falha. Não foi medido, e é a ressalva que mais pesa sobre os números de 3% e 5%.
- **Sem reordenação, e (na grade) sem jitter.** O cano não troca a ordem. O str0m pede retransmissão de qualquer buraco, e numa rede que reordena isso vira retransmissão à toa. A variação com 20 ms de jitter é uma rodada de três, em dois cenários, sem troca de ordem.
- **Um atraso só.** 30 ms por sentido. O custo de uma retransmissão é uma ida e volta mais o intervalo do NACK: a 150 ms por sentido a cauda cresce na mesma proporção, e as 5 tentativas de NACK do str0m (uma a cada 33 ms) saem todas antes de a primeira resposta chegar. Não foi medido.
- **Sem congestionamento de verdade.** O gargalo é uma fila fixa com um fluxo só. Não há tráfego concorrente, nem outro fluxo com controle de congestionamento disputando a fila com a estimativa do str0m.
- **O RTP fixo não é um desenho.** É a carga da linha «A» sem ritmo e sem controle nenhum; serve para comparar com a mesma carga. Nos dois cenários de banda ele não exibe nada, e a linha «A» exibe porque o SCTP **tem** controle de congestionamento. Um RTP de taxa fixa **com ritmo** não foi medido (o str0m só dá ritmo com a estimativa ligada).
- **A política de pedido de chave é uma entre muitas.** PLI na hora, repetido a cada 300 ms, e chave no máximo a cada 300 ms foram escolhidos aqui e não variados. FIR não foi usado.
- **Parâmetros do str0m quase todos no padrão.** Só o prazo de espera foi variado, e o teto de retransmissão numa variação. O intervalo do NACK, as 5 tentativas, a janela de 100 pacotes e os parâmetros da estimativa são constantes do str0m que não se configuram.
- **FEC não foi medido.** Não achei FEC de vídeo no fonte do str0m 0.24.1 (leitura); RED existe só para a voz e ficou desligado.
- **A voz é de mentira e sem reprodutor.** Perda e atraso de pacote, sem Opus de verdade, sem ocultação, sem buffer. E é a voz **por RTP**, no mesmo `Rtc` do vídeo: a do produto vai pelo canal `voz` do SCTP, e só a linha «A» a mede assim.
- **As causas apontadas são contagem mais leitura de código.** Nenhuma foi isolada instrumentando o str0m: nem o teto de retransmissão como origem das quebras a 5%, nem a oscilação da estimativa a 1,0 Mbps. O que ficou sem explicação está dito nos itens 8, 9 e 12.
- **20 s, 3 rodadas, um sentido de vídeo, um par.** As quebras de cadeia são eventos raros (zero a duas por célula de 20 s), e três rodadas não dão a frequência delas com precisão: onde a faixa entre rodadas vai de 0% a 30% de tempo congelado, a mediana depende de quantas das três rodadas tiveram um evento. Uma conversa de dez minutos não foi medida.
- **A máquina não estava sozinha.** Havia outras sessões abertas (carga média entre 3 e 9 ao longo da medida; os valores de cada passagem estão em «A máquina»).
- **O que o RTP custa ao produto não foi medido.** Empacotar H.264 de verdade, o caminho pelo servidor, a página, o que acontece com a tela do sistema (que hoje espera em vez de perder referência), a convivência com os canais SCTP que continuam existindo: nada disso entra aqui.

## Apêndice: todas as rodadas

### A grade (primeiros 20 s)

| passagem | cenário | variante | rodada | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | kbps gerados | chaves | pedidos (PLI) | quebras | NACKs | RTX pacotes | RTX % dos bytes | est. mín kbps | est. mediana | est. final | MB no cano | retorno kbps | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| grade | 0% | voz (RTP) | 1 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 0,00 | 30 | 32 | 0,00 | 0,00 |
| grade | 0% | A (SCTP, controle) | 1 | 32 | 38 | 171 | 322 | 1,6 | 322 | 30,0 | 601 | 1511 | 1 | — | — | — | — | — | — | — | — | 4,21 | — | 0,00 | 32 | 37 | 0,00 | 0,00 |
| grade | 0% | RTP fixo | 1 | 32 | 39 | 48 | 59 | 0,0 | 61 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 0 | 0 | 0,0 | — | — | — | 4,09 | 10 | 0,00 | 32 | 40 | 0,00 | 0,00 |
| grade | 0% | RTP adaptativo | 1 | 40 | 53 | 64 | 155 | 0,0 | 155 | 29,8 | 597 | 1920 | 1 | 0 | 0 | 0 | 17 | 0,3 | 666 | 3532 | 4340 | 5,18 | 10 | 0,00 | 32 | 37 | 0,00 | 0,00 |
| grade | 0,3% | voz (RTP) | 1 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 0,10 | 31 | 34 | 0,10 | 0,00 |
| grade | 0,3% | A (SCTP, controle) | 1 | 32 | 115 | 172 | 318 | 1,6 | 318 | 30,0 | 601 | 1511 | 1 | — | — | — | — | — | — | — | — | 4,21 | — | 0,10 | 32 | 61 | 0,17 | 0,00 |
| grade | 0,3% | RTP fixo | 1 | 32 | 33 | 107 | 151 | 0,0 | 154 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 12 | 12 | 0,3 | — | — | — | 4,11 | 10 | 0,20 | 32 | 33 | 0,19 | 0,00 |
| grade | 0,3% | RTP adaptativo | 1 | 40 | 43 | 124 | 162 | 0,0 | 164 | 29,8 | 597 | 1904 | 1 | 0 | 0 | 15 | 37 | 0,7 | 626 | 3168 | 5388 | 5,16 | 10 | 0,40 | 32 | 33 | 0,22 | 0,00 |
| grade | 1% | voz (RTP) | 1 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 0,80 | 31 | 32 | 0,77 | 0,00 |
| grade | 1% | A (SCTP, controle) | 1 | 360 | 737 | 806 | 835 | 3,1 | 629 | 29,1 | 597 | 1502 | 1 | — | — | — | — | — | — | — | — | 4,11 | — | 0,50 | 55 | 93 | 0,83 | 0,00 |
| grade | 1% | RTP fixo | 1 | 32 | 96 | 127 | 146 | 0,0 | 147 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 51 | 54 | 1,5 | — | — | — | 4,15 | 11 | 1,00 | 32 | 33 | 0,80 | 0,00 |
| grade | 1% | RTP adaptativo | 1 | 39 | 101 | 130 | 158 | 0,0 | 155 | 29,8 | 598 | 1761 | 1 | 0 | 0 | 60 | 84 | 1,9 | 516 | 2748 | 4598 | 4,83 | 11 | 0,60 | 32 | 33 | 0,80 | 0,00 |
| grade | 3% | voz (RTP) | 1 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 3,10 | 31 | 32 | 3,28 | 0,00 |
| grade | 3% | A (SCTP, controle) | 1 | 1432 | 2506 | 2560 | 2620 | 18,6 | 1131 | 15,1 | 322 | 824 | 1 | — | — | — | — | — | — | — | — | 2,33 | — | 2,20 | 61 | 142 | 2,88 | 0,00 |
| grade | 3% | RTP fixo | 1 | 33 | 128 | 160 | 163 | 0,0 | 164 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 171 | 210 | 5,8 | — | — | — | 4,32 | 12 | 3,10 | 32 | 33 | 2,95 | 0,00 |
| grade | 3% | RTP adaptativo | 1 | 38 | 131 | 153 | 240 | 11,5 | 2294 | 26,4 | 594 | 1235 | 2 | 1 | 1 | 151 | 194 | 6,2 | 611 | 1252 | 2864 | 3,59 | 12 | 2,80 | 32 | 33 | 3,01 | 0,00 |
| grade | 5% | voz (RTP) | 1 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 5,70 | 31 | 32 | 5,78 | 0,00 |
| grade | 5% | A (SCTP, controle) | 1 | 2389 | 3914 | 4126 | 4200 | 37,5 | 1270 | 8,5 | 193 | 506 | 1 | — | — | — | — | — | — | — | — | 1,45 | — | 7,00 | 70 | 191 | 5,64 | 0,00 |
| grade | 5% | RTP fixo | 1 | 73 | 151 | 167 | 227 | 10,9 | 2190 | 26,6 | 600 | 1535 | 2 | 1 | 1 | 291 | 402 | 10,8 | — | — | — | 4,61 | 14 | 5,10 | 32 | 34 | 5,37 | 0,00 |
| grade | 5% | RTP adaptativo | 1 | 95 | 281 | 967 | 1195 | 30,5 | 3273 | 22,7 | 585 | 1648 | 5 | 7 | 2 | 309 | 463 | 11,6 | 509 | 2535 | 3276 | 4,96 | 15 | 5,02 | 32 | 40 | 5,32 | 0,00 |
| grade | 1,0 Mbps, 0% | voz (RTP) | 1 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 0,00 | 33 | 41 | 0,00 | 0,00 |
| grade | 1,0 Mbps, 0% | A (SCTP, controle) | 1 | 1228 | 2147 | 2435 | 2457 | 17,8 | 2457 | 16,7 | 355 | 905 | 1 | — | — | — | — | — | — | — | — | 2,52 | — | 0,40 | 211 | 297 | 0,00 | 1,52 |
| grade | 1,0 Mbps, 0% | RTP fixo | 1 | — | — | — | — | 100,0 | 20002 | 0,0 | 600 | 2414 | 31 | 56 | 1 | 566 | 1973 | 35,4 | — | — | — | 8,69 | 20 | 0,10 | 261 | 275 | 0,00 | 62,52 |
| grade | 1,0 Mbps, 0% | RTP adaptativo | 1 | 121 | 1357 | 1892 | 2090 | 45,0 | 4798 | 17,4 | 561 | 926 | 13 | 22 | 2 | 149 | 196 | 8,9 | 358 | 1000 | 358 | 2,79 | 12 | 0,00 | 124 | 273 | 0,00 | 9,54 |
| grade | 2,0 Mbps, 1% | voz (RTP) | 1 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 0,70 | 32 | 33 | 0,77 | 0,00 |
| grade | 2,0 Mbps, 1% | A (SCTP, controle) | 1 | 631 | 1125 | 1246 | 1328 | 3,6 | 722 | 26,1 | 543 | 1368 | 1 | — | — | — | — | — | — | — | — | 3,71 | — | 0,50 | 59 | 102 | 0,81 | 0,00 |
| grade | 2,0 Mbps, 1% | RTP fixo | 1 | — | — | — | — | 100,0 | 20001 | 0,0 | 600 | 2444 | 32 | 59 | 1 | 306 | 1584 | 28,8 | — | — | — | 8,38 | 16 | 1,50 | 229 | 269 | 1,06 | 33,82 |
| grade | 2,0 Mbps, 1% | RTP adaptativo | 1 | 59 | 129 | 166 | 180 | 0,0 | 170 | 29,8 | 597 | 1343 | 1 | 0 | 0 | 70 | 95 | 2,9 | 607 | 1787 | 1787 | 3,77 | 11 | 0,80 | 37 | 56 | 0,77 | 0,00 |
| grade | 0% | voz (RTP) | 2 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 0,00 | 30 | 32 | 0,00 | 0,00 |
| grade | 0% | A (SCTP, controle) | 2 | 32 | 33 | 170 | 316 | 1,6 | 316 | 30,0 | 601 | 1511 | 1 | — | — | — | — | — | — | — | — | 4,21 | — | 0,00 | 32 | 33 | 0,00 | 0,00 |
| grade | 0% | RTP fixo | 2 | 32 | 33 | 33 | 37 | 0,0 | 49 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 0 | 0 | 0,0 | — | — | — | 4,09 | 10 | 0,00 | 32 | 33 | 0,00 | 0,00 |
| grade | 0% | RTP adaptativo | 2 | 39 | 42 | 44 | 166 | 0,0 | 167 | 29,8 | 597 | 1912 | 1 | 0 | 0 | 0 | 26 | 0,3 | 664 | 3383 | 3987 | 5,16 | 10 | 0,00 | 31 | 33 | 0,00 | 0,00 |
| grade | 0,3% | voz (RTP) | 2 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,17 | 8 | 0,50 | 31 | 32 | 0,48 | 0,00 |
| grade | 0,3% | A (SCTP, controle) | 2 | 33 | 259 | 305 | 349 | 1,6 | 318 | 30,0 | 600 | 1509 | 1 | — | — | — | — | — | — | — | — | 4,21 | — | 0,50 | 32 | 81 | 0,35 | 0,00 |
| grade | 0,3% | RTP fixo | 2 | 32 | 58 | 120 | 157 | 0,0 | 158 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 26 | 26 | 0,7 | — | — | — | 4,12 | 10 | 0,30 | 32 | 33 | 0,34 | 0,00 |
| grade | 0,3% | RTP adaptativo | 2 | 40 | 52 | 110 | 194 | 0,0 | 199 | 29,7 | 597 | 1957 | 1 | 0 | 0 | 24 | 36 | 0,7 | 679 | 3049 | 3055 | 5,30 | 11 | 0,50 | 32 | 33 | 0,32 | 0,00 |
| grade | 1% | voz (RTP) | 2 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,17 | 8 | 1,10 | 30 | 32 | 1,06 | 0,00 |
| grade | 1% | A (SCTP, controle) | 2 | 505 | 1316 | 1688 | 1718 | 7,6 | 1194 | 27,2 | 563 | 1418 | 1 | — | — | — | — | — | — | — | — | 3,86 | — | 0,80 | 56 | 98 | 0,91 | 0,00 |
| grade | 1% | RTP fixo | 2 | 32 | 97 | 127 | 154 | 0,0 | 154 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 52 | 56 | 1,6 | — | — | — | 4,16 | 11 | 1,40 | 32 | 33 | 0,90 | 0,00 |
| grade | 1% | RTP adaptativo | 2 | 39 | 102 | 132 | 165 | 0,0 | 169 | 29,8 | 597 | 1709 | 1 | 0 | 0 | 67 | 108 | 2,4 | 698 | 2233 | 4022 | 4,73 | 11 | 1,00 | 32 | 33 | 0,91 | 0,00 |
| grade | 3% | voz (RTP) | 2 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 3,20 | 31 | 32 | 3,28 | 0,00 |
| grade | 3% | A (SCTP, controle) | 2 | 1566 | 2791 | 2898 | 2916 | 19,8 | 1069 | 12,6 | 273 | 703 | 1 | — | — | — | — | — | — | — | — | 1,97 | — | 3,50 | 65 | 173 | 3,63 | 0,00 |
| grade | 3% | RTP fixo | 2 | 33 | 126 | 147 | 164 | 0,0 | 165 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 181 | 221 | 6,2 | — | — | — | 4,33 | 13 | 3,00 | 32 | 33 | 2,99 | 0,00 |
| grade | 3% | RTP adaptativo | 2 | 43 | 301 | 378 | 412 | 0,0 | 178 | 29,7 | 597 | 1860 | 1 | 0 | 0 | 197 | 258 | 5,9 | 548 | 2879 | 3119 | 5,29 | 13 | 3,20 | 32 | 33 | 3,01 | 0,00 |
| grade | 5% | voz (RTP) | 2 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,17 | 8 | 5,60 | 31 | 32 | 5,59 | 0,00 |
| grade | 5% | A (SCTP, controle) | 2 | 2757 | 5358 | 5421 | 5486 | 63,0 | 2089 | 6,4 | 148 | 395 | 1 | — | — | — | — | — | — | — | — | 1,15 | — | 5,70 | 94 | 1483 | 5,54 | 0,00 |
| grade | 5% | RTP fixo | 2 | 60 | 145 | 165 | 188 | 0,0 | 188 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 259 | 378 | 10,4 | — | — | — | 4,50 | 14 | 4,50 | 32 | 33 | 5,02 | 0,00 |
| grade | 5% | RTP adaptativo | 2 | 80 | 142 | 178 | 287 | 13,9 | 2377 | 26,3 | 594 | 1760 | 2 | 2 | 1 | 293 | 416 | 10,0 | 639 | 2615 | 4791 | 5,21 | 15 | 5,00 | 32 | 33 | 5,02 | 0,00 |
| grade | 1,0 Mbps, 0% | voz (RTP) | 2 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 0,00 | 32 | 34 | 0,00 | 0,00 |
| grade | 1,0 Mbps, 0% | A (SCTP, controle) | 2 | 1228 | 2912 | 3146 | 3180 | 23,3 | 2160 | 15,3 | 328 | 839 | 1 | — | — | — | — | — | — | — | — | 2,34 | — | 0,50 | 191 | 296 | 0,00 | 1,61 |
| grade | 1,0 Mbps, 0% | RTP fixo | 2 | — | — | — | — | 100,0 | 20001 | 0,0 | 600 | 2384 | 30 | 56 | 1 | 567 | 2017 | 36,3 | — | — | — | 8,64 | 20 | 0,00 | 261 | 275 | 0,00 | 62,33 |
| grade | 1,0 Mbps, 0% | RTP adaptativo | 2 | 117 | 361 | 530 | 647 | 13,2 | 2355 | 25,8 | 594 | 869 | 2 | 2 | 1 | 11 | 27 | 1,1 | 640 | 1012 | 955 | 2,46 | 10 | 0,00 | 99 | 240 | 0,00 | 0,18 |
| grade | 2,0 Mbps, 1% | voz (RTP) | 2 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,17 | 8 | 1,10 | 31 | 32 | 1,06 | 0,00 |
| grade | 2,0 Mbps, 1% | A (SCTP, controle) | 2 | 725 | 1376 | 1733 | 1763 | 10,6 | 1090 | 24,3 | 507 | 1280 | 1 | — | — | — | — | — | — | — | — | 3,48 | — | 1,60 | 58 | 108 | 0,86 | 0,00 |
| grade | 2,0 Mbps, 1% | RTP fixo | 2 | — | — | — | — | 100,0 | 20002 | 0,0 | 600 | 2414 | 31 | 59 | 1 | 316 | 1498 | 27,6 | — | — | — | 8,20 | 16 | 0,90 | 230 | 270 | 1,03 | 32,67 |
| grade | 2,0 Mbps, 1% | RTP adaptativo | 2 | 58 | 130 | 163 | 196 | 0,0 | 178 | 29,8 | 597 | 1303 | 1 | 0 | 0 | 86 | 114 | 3,2 | 652 | 1626 | 1883 | 3,68 | 11 | 0,70 | 36 | 57 | 0,98 | 0,00 |
| grade | 0% | voz (RTP) | 3 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 0,00 | 31 | 32 | 0,00 | 0,00 |
| grade | 0% | A (SCTP, controle) | 3 | 32 | 33 | 174 | 321 | 1,6 | 321 | 30,0 | 601 | 1511 | 1 | — | — | — | — | — | — | — | — | 4,21 | — | 0,00 | 32 | 33 | 0,00 | 0,00 |
| grade | 0% | RTP fixo | 3 | 32 | 33 | 34 | 35 | 0,0 | 43 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 0 | 0 | 0,0 | — | — | — | 4,09 | 10 | 0,00 | 32 | 33 | 0,00 | 0,00 |
| grade | 0% | RTP adaptativo | 3 | 40 | 42 | 46 | 163 | 0,0 | 167 | 29,8 | 597 | 1964 | 1 | 0 | 0 | 0 | 20 | 0,3 | 681 | 3681 | 3754 | 5,30 | 10 | 0,00 | 32 | 33 | 0,00 | 0,00 |
| grade | 0,3% | voz (RTP) | 3 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 0,40 | 30 | 32 | 0,38 | 0,00 |
| grade | 0,3% | A (SCTP, controle) | 3 | 33 | 167 | 233 | 324 | 1,6 | 324 | 29,9 | 601 | 1511 | 1 | — | — | — | — | — | — | — | — | 4,22 | — | 0,50 | 32 | 72 | 0,39 | 0,00 |
| grade | 0,3% | RTP fixo | 3 | 32 | 65 | 127 | 163 | 0,0 | 163 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 27 | 28 | 0,7 | — | — | — | 4,12 | 10 | 0,30 | 32 | 33 | 0,38 | 0,00 |
| grade | 0,3% | RTP adaptativo | 3 | 40 | 60 | 131 | 165 | 0,0 | 169 | 29,8 | 597 | 1899 | 1 | 0 | 0 | 22 | 36 | 0,7 | 612 | 3067 | 3067 | 5,15 | 11 | 0,90 | 32 | 33 | 0,38 | 0,00 |
| grade | 1% | voz (RTP) | 3 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 1,10 | 31 | 32 | 1,06 | 0,00 |
| grade | 1% | A (SCTP, controle) | 3 | 566 | 1148 | 1500 | 1543 | 4,5 | 897 | 25,2 | 523 | 1319 | 1 | — | — | — | — | — | — | — | — | 3,61 | — | 1,00 | 56 | 98 | 1,07 | 0,00 |
| grade | 1% | RTP fixo | 3 | 33 | 107 | 131 | 153 | 0,0 | 155 | 29,8 | 600 | 1505 | 1 | 0 | 0 | 74 | 79 | 2,2 | — | — | — | 4,18 | 11 | 0,90 | 32 | 34 | 1,05 | 0,00 |
| grade | 1% | RTP adaptativo | 3 | 40 | 122 | 137 | 185 | 0,0 | 185 | 29,8 | 597 | 1791 | 1 | 0 | 0 | 73 | 111 | 2,5 | 534 | 2977 | 3059 | 4,95 | 11 | 0,90 | 32 | 34 | 1,05 | 0,00 |
| grade | 3% | voz (RTP) | 3 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,17 | 8 | 2,80 | 31 | 33 | 2,80 | 0,00 |
| grade | 3% | A (SCTP, controle) | 3 | 1335 | 2407 | 2599 | 2615 | 13,4 | 1090 | 15,3 | 328 | 839 | 1 | — | — | — | — | — | — | — | — | 2,33 | — | 1,80 | 60 | 135 | 2,51 | 0,00 |
| grade | 3% | RTP fixo | 3 | 33 | 133 | 163 | 199 | 1,0 | 202 | 29,8 | 600 | 1505 | 1 | 0 | 0 | 166 | 207 | 5,5 | — | — | — | 4,31 | 12 | 3,80 | 32 | 34 | 3,08 | 0,00 |
| grade | 3% | RTP adaptativo | 3 | 42 | 136 | 163 | 222 | 1,1 | 223 | 29,8 | 597 | 1765 | 1 | 0 | 0 | 201 | 283 | 6,5 | 511 | 2412 | 3711 | 5,06 | 13 | 2,70 | 32 | 34 | 3,00 | 0,00 |
| grade | 5% | voz (RTP) | 3 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,17 | 8 | 4,90 | 31 | 33 | 4,92 | 0,00 |
| grade | 5% | A (SCTP, controle) | 3 | 2474 | 5339 | 5468 | 5472 | 57,1 | 3067 | 7,3 | 173 | 457 | 1 | — | — | — | — | — | — | — | — | 1,31 | — | 4,40 | 75 | 1550 | 4,68 | 0,00 |
| grade | 5% | RTP fixo | 3 | 62 | 149 | 182 | 282 | 1,2 | 239 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 262 | 346 | 9,5 | — | — | — | 4,46 | 14 | 5,51 | 32 | 34 | 4,90 | 0,00 |
| grade | 5% | RTP adaptativo | 3 | 69 | 148 | 174 | 199 | 0,0 | 197 | 29,8 | 597 | 1646 | 1 | 0 | 0 | 267 | 401 | 10,2 | 633 | 1935 | 3603 | 4,89 | 14 | 5,30 | 32 | 34 | 5,04 | 0,00 |
| grade | 1,0 Mbps, 0% | voz (RTP) | 3 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 0,00 | 32 | 34 | 0,00 | 0,00 |
| grade | 1,0 Mbps, 0% | A (SCTP, controle) | 3 | 1229 | 3056 | 3566 | 3666 | 30,2 | 1575 | 15,5 | 330 | 844 | 1 | — | — | — | — | — | — | — | — | 2,36 | — | 0,20 | 215 | 492 | 0,00 | 1,55 |
| grade | 1,0 Mbps, 0% | RTP fixo | 3 | — | — | — | — | 100,0 | 20003 | 0,0 | 600 | 2293 | 27 | 49 | 1 | 567 | 2015 | 38,0 | — | — | — | 8,42 | 20 | 0,00 | 263 | 276 | 0,00 | 61,47 |
| grade | 1,0 Mbps, 0% | RTP adaptativo | 3 | 113 | 361 | 565 | 664 | 4,9 | 392 | 29,7 | 597 | 850 | 1 | 0 | 0 | 15 | 33 | 1,6 | 610 | 978 | 1126 | 2,42 | 9 | 0,00 | 65 | 245 | 0,00 | 0,25 |
| grade | 2,0 Mbps, 1% | voz (RTP) | 3 | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | — | 0,16 | 8 | 1,10 | 31 | 34 | 1,06 | 0,00 |
| grade | 2,0 Mbps, 1% | A (SCTP, controle) | 3 | 799 | 1800 | 2020 | 2057 | 12,5 | 1201 | 21,5 | 451 | 1142 | 1 | — | — | — | — | — | — | — | — | 3,14 | — | 1,00 | 59 | 110 | 1,05 | 0,00 |
| grade | 2,0 Mbps, 1% | RTP fixo | 3 | — | — | — | — | 100,0 | 20000 | 0,0 | 600 | 2414 | 31 | 59 | 1 | 305 | 1606 | 29,4 | — | — | — | 8,31 | 16 | 1,00 | 229 | 269 | 1,05 | 33,45 |
| grade | 2,0 Mbps, 1% | RTP adaptativo | 3 | 50 | 130 | 164 | 249 | 1,2 | 249 | 29,8 | 597 | 1093 | 1 | 0 | 0 | 86 | 105 | 4,0 | 453 | 1130 | 1704 | 3,14 | 11 | 0,90 | 34 | 51 | 1,04 | 0,00 |

### O regime do adaptativo (40–60 s)

| passagem | cenário | variante | rodada | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | kbps gerados | chaves | pedidos (PLI) | quebras | NACKs | RTX pacotes | RTX % dos bytes | est. mín kbps | est. mediana | est. final | MB no cano | retorno kbps | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| regime | 0% | RTP adaptativo | 1 | 41 | 43 | 44 | 48 | 0,0 | 41 | 30,0 | 600 | 2371 | 0 | 0 | 0 | 0 | 5 | 0,0 | 3766 | 3766 | 4025 | 6,34 | 10 | 0,00 | 32 | 34 | 0,00 | 0,00 |
| regime | 0,3% | RTP adaptativo | 1 | 41 | 51 | 106 | 131 | 0,0 | 124 | 30,0 | 600 | 2371 | 0 | 0 | 0 | 24 | 33 | 0,6 | 3296 | 3809 | 3296 | 6,37 | 11 | 0,20 | 32 | 34 | 0,20 | 0,00 |
| regime | 1% | RTP adaptativo | 1 | 42 | 108 | 134 | 157 | 0,0 | 150 | 30,0 | 600 | 2371 | 0 | 0 | 0 | 80 | 87 | 1,6 | 3169 | 3169 | 3169 | 6,44 | 12 | 0,80 | 32 | 34 | 0,81 | 0,00 |
| regime | 3% | RTP adaptativo | 1 | 59 | 135 | 162 | 167 | 0,0 | 161 | 29,9 | 600 | 2371 | 0 | 0 | 0 | 250 | 325 | 6,0 | 3215 | 3215 | 3223 | 6,71 | 14 | 2,60 | 32 | 34 | 2,99 | 0,00 |
| regime | 5% | RTP adaptativo | 1 | 95 | 144 | 169 | 203 | 0,0 | 191 | 30,0 | 600 | 2371 | 0 | 0 | 0 | 353 | 581 | 10,8 | 3340 | 3340 | 3340 | 7,00 | 16 | 5,30 | 32 | 34 | 5,40 | 0,00 |
| regime | 1,0 Mbps, 0% | RTP adaptativo | 1 | 62 | 306 | 476 | 608 | 1,7 | 334 | 29,9 | 600 | 813 | 0 | 0 | 0 | 6 | 10 | 0,5 | 727 | 944 | 1092 | 2,30 | 9 | 0,00 | 50 | 214 | 0,00 | 0,06 |
| regime | 2,0 Mbps, 1% | RTP adaptativo | 1 | 79 | 321 | 481 | 592 | 3,8 | 309 | 30,0 | 600 | 1749 | 0 | 0 | 0 | 111 | 120 | 2,9 | 1772 | 2111 | 1861 | 4,86 | 12 | 0,90 | 51 | 129 | 0,90 | 0,00 |
| regime | 0% | RTP adaptativo | 2 | 41 | 43 | 45 | 49 | 0,0 | 41 | 30,0 | 600 | 2371 | 0 | 0 | 0 | 0 | 13 | 0,1 | 2917 | 4333 | 3889 | 6,35 | 10 | 0,00 | 32 | 34 | 0,00 | 0,00 |
| regime | 0,3% | RTP adaptativo | 2 | 42 | 88 | 129 | 153 | 0,0 | 143 | 30,0 | 600 | 2371 | 0 | 0 | 0 | 45 | 44 | 0,8 | 3088 | 3088 | 3109 | 6,39 | 11 | 0,30 | 32 | 34 | 0,39 | 0,00 |
| regime | 1% | RTP adaptativo | 2 | 42 | 120 | 139 | 166 | 0,0 | 158 | 29,9 | 600 | 2371 | 0 | 0 | 0 | 96 | 116 | 2,1 | 4024 | 4027 | 5543 | 6,47 | 12 | 0,80 | 31 | 33 | 1,02 | 0,00 |
| regime | 3% | RTP adaptativo | 2 | 55 | 134 | 161 | 194 | 0,0 | 171 | 30,1 | 600 | 2367 | 0 | 0 | 0 | 254 | 353 | 6,4 | 2754 | 3058 | 3238 | 6,72 | 14 | 3,10 | 31 | 34 | 3,00 | 0,00 |
| regime | 5% | RTP adaptativo | 2 | 89 | 138 | 167 | 195 | 0,0 | 187 | 30,0 | 600 | 2371 | 0 | 0 | 0 | 348 | 542 | 9,9 | 2939 | 4041 | 3217 | 6,95 | 16 | 6,10 | 31 | 34 | 4,87 | 0,00 |
| regime | 1,0 Mbps, 0% | RTP adaptativo | 2 | 60 | 347 | 386 | 387 | 0,0 | 43 | 29,5 | 600 | 774 | 0 | 0 | 0 | 0 | 0 | 0,0 | 618 | 924 | 918 | 2,19 | 9 | 0,00 | 43 | 107 | 0,00 | 0,20 |
| regime | 2,0 Mbps, 1% | RTP adaptativo | 2 | 74 | 197 | 281 | 358 | 2,1 | 217 | 30,0 | 601 | 1711 | 0 | 0 | 0 | 102 | 125 | 3,1 | 1739 | 2015 | 2169 | 4,76 | 11 | 0,90 | 48 | 119 | 0,92 | 0,00 |
| regime | 0% | RTP adaptativo | 3 | 41 | 43 | 44 | 49 | 0,0 | 41 | 30,0 | 600 | 2371 | 0 | 0 | 0 | 0 | 13 | 0,1 | 3158 | 4174 | 4218 | 6,34 | 10 | 0,00 | 32 | 34 | 0,00 | 0,00 |
| regime | 0,3% | RTP adaptativo | 3 | 42 | 44 | 117 | 153 | 0,0 | 142 | 30,0 | 601 | 2375 | 0 | 0 | 0 | 16 | 18 | 0,3 | 3064 | 3064 | 3064 | 6,36 | 11 | 0,40 | 31 | 34 | 0,20 | 0,00 |
| regime | 1% | RTP adaptativo | 3 | 42 | 105 | 134 | 162 | 0,0 | 153 | 30,0 | 601 | 2375 | 0 | 0 | 0 | 72 | 86 | 1,6 | 3192 | 3192 | 3192 | 6,44 | 12 | 0,80 | 31 | 33 | 0,78 | 0,00 |
| regime | 3% | RTP adaptativo | 3 | 45 | 135 | 160 | 168 | 0,0 | 157 | 30,1 | 600 | 2371 | 0 | 0 | 0 | 236 | 301 | 5,6 | 3359 | 3359 | 3359 | 6,68 | 14 | 2,60 | 32 | 34 | 2,76 | 0,00 |
| regime | 5% | RTP adaptativo | 3 | 93 | 141 | 171 | 208 | 0,0 | 183 | 30,1 | 600 | 2371 | 0 | 0 | 0 | 338 | 518 | 9,6 | 3251 | 3287 | 3287 | 6,93 | 16 | 4,00 | 31 | 34 | 4,78 | 0,00 |
| regime | 1,0 Mbps, 0% | RTP adaptativo | 3 | 166 | 415 | 621 | 728 | 4,9 | 326 | 30,1 | 600 | 861 | 0 | 1 | 0 | 16 | 49 | 2,4 | 899 | 944 | 1060 | 2,46 | 9 | 0,00 | 87 | 252 | 0,00 | 0,47 |
| regime | 2,0 Mbps, 1% | RTP adaptativo | 3 | 65 | 169 | 214 | 280 | 0,0 | 189 | 29,8 | 600 | 1682 | 0 | 0 | 0 | 64 | 72 | 1,8 | 1380 | 1998 | 1998 | 4,62 | 11 | 1,20 | 46 | 85 | 0,66 | 0,00 |

### A espera pela retransmissão

| espera | cenário | variante | rodada | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | kbps gerados | chaves | pedidos (PLI) | quebras | NACKs | RTX pacotes | RTX % dos bytes | est. mín kbps | est. mediana | est. final | MB no cano | retorno kbps | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| 50 ms | 3% | RTP fixo | 1 | 32 | 52 | 85 | 85 | 95,8 | 6300 | 1,5 | 600 | 2565 | 36 | 64 | 5 | 214 | 370 | 6,1 | — | — | — | 7,27 | 15 | 3,30 | 32 | 34 | 3,18 | 0,00 |
| 50 ms | 3% | RTP fixo | 2 | 33 | 51 | 84 | 84 | 97,0 | 6351 | 1,2 | 600 | 2535 | 35 | 65 | 6 | 217 | 355 | 5,9 | — | — | — | 7,16 | 15 | 3,20 | 32 | 34 | 3,09 | 0,00 |
| 50 ms | 3% | RTP fixo | 3 | 32 | 51 | 83 | 83 | 95,6 | 7820 | 1,7 | 600 | 2566 | 36 | 64 | 6 | 215 | 375 | 6,1 | — | — | — | 7,26 | 15 | 2,70 | 32 | 34 | 2,99 | 0,00 |
| 50 ms | 5% | RTP fixo | 1 | 32 | 35 | 35 | 35 | 98,7 | 13353 | 0,4 | 600 | 2535 | 35 | 65 | 2 | 310 | 627 | 10,3 | — | — | — | 7,46 | 17 | 6,80 | 32 | 34 | 5,46 | 0,00 |
| 50 ms | 5% | RTP adaptativo | 1 | — | — | — | — | 100,0 | 20000 | 0,0 | 507 | 2203 | 31 | 66 | 1 | 320 | 606 | 11,7 | 519 | 2992 | 3484 | 6,58 | 17 | 5,40 | 32 | 33 | 5,50 | 0,00 |
| 50 ms | 5% | RTP fixo | 2 | 32 | 84 | 84 | 84 | 99,4 | 11791 | 0,3 | 600 | 2535 | 35 | 66 | 3 | 314 | 633 | 10,4 | — | — | — | 7,46 | 17 | 4,60 | 32 | 34 | 5,09 | 0,00 |
| 50 ms | 5% | RTP adaptativo | 2 | — | — | — | — | 100,0 | 20002 | 0,0 | 501 | 2281 | 33 | 66 | 1 | 331 | 607 | 11,2 | 620 | 2611 | 5481 | 6,78 | 17 | 4,70 | 31 | 33 | 5,07 | 0,00 |
| 50 ms | 5% | RTP fixo | 3 | 34 | 36 | 36 | 36 | 99,8 | 16382 | 0,1 | 600 | 2535 | 35 | 66 | 2 | 292 | 559 | 9,3 | — | — | — | 7,39 | 16 | 6,30 | 32 | 34 | 5,00 | 0,00 |
| 50 ms | 5% | RTP adaptativo | 3 | — | — | — | — | 100,0 | 20001 | 0,0 | 498 | 2224 | 35 | 66 | 1 | 316 | 537 | 10,5 | 527 | 3829 | 4005 | 6,51 | 16 | 5,70 | 31 | 33 | 4,97 | 0,00 |
| 100 ms | 3% | RTP fixo | 1 | 33 | 124 | 132 | 225 | 39,0 | 1305 | 20,1 | 600 | 2050 | 19 | 22 | 16 | 198 | 290 | 5,8 | — | — | — | 5,82 | 14 | 3,70 | 32 | 34 | 3,08 | 0,00 |
| 100 ms | 3% | RTP fixo | 2 | 33 | 121 | 153 | 227 | 24,7 | 1023 | 24,3 | 600 | 1868 | 13 | 13 | 11 | 207 | 275 | 6,2 | — | — | — | 5,34 | 13 | 2,90 | 32 | 34 | 2,98 | 0,00 |
| 100 ms | 3% | RTP fixo | 3 | 33 | 123 | 133 | 199 | 36,0 | 1001 | 21,5 | 600 | 2080 | 20 | 20 | 18 | 205 | 302 | 6,1 | — | — | — | 5,92 | 14 | 2,80 | 32 | 34 | 3,04 | 0,00 |
| 100 ms | 5% | RTP fixo | 1 | 67 | 132 | 182 | 213 | 62,2 | 1919 | 14,0 | 600 | 2323 | 28 | 37 | 20 | 291 | 592 | 10,7 | — | — | — | 6,87 | 16 | 6,20 | 32 | 34 | 5,45 | 0,00 |
| 100 ms | 5% | RTP adaptativo | 1 | 91 | 221 | 221 | 221 | 98,7 | 15157 | 0,5 | 501 | 2139 | 33 | 64 | 2 | 354 | 608 | 12,1 | 505 | 3230 | 4251 | 6,42 | 17 | 5,00 | 32 | 33 | 5,51 | 0,00 |
| 100 ms | 5% | RTP fixo | 2 | 39 | 126 | 135 | 170 | 49,5 | 1580 | 17,8 | 600 | 2172 | 23 | 29 | 17 | 280 | 512 | 10,1 | — | — | — | 6,40 | 15 | 5,90 | 32 | 34 | 5,05 | 0,00 |
| 100 ms | 5% | RTP adaptativo | 2 | — | — | — | — | 100,0 | 20001 | 0,0 | 501 | 2160 | 33 | 65 | 1 | 320 | 540 | 10,7 | 520 | 3189 | 4098 | 6,40 | 16 | 4,80 | 31 | 33 | 4,98 | 0,00 |
| 100 ms | 5% | RTP fixo | 3 | 61 | 127 | 161 | 216 | 71,9 | 2236 | 11,0 | 600 | 2384 | 30 | 43 | 18 | 291 | 562 | 10,0 | — | — | — | 7,00 | 16 | 5,10 | 32 | 34 | 4,97 | 0,00 |
| 100 ms | 5% | RTP adaptativo | 3 | 39 | 352 | 587 | 587 | 88,5 | 8410 | 4,2 | 510 | 1499 | 30 | 58 | 3 | 236 | 364 | 10,3 | 509 | 913 | 3950 | 4,47 | 14 | 4,90 | 32 | 33 | 4,86 | 0,00 |
| 200 ms | 3% | RTP fixo | 1 | 34 | 130 | 160 | 192 | 0,0 | 192 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 184 | 227 | 6,2 | — | — | — | 4,34 | 13 | 2,60 | 32 | 34 | 2,94 | 0,00 |
| 200 ms | 3% | RTP fixo | 2 | 33 | 126 | 153 | 164 | 0,0 | 165 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 166 | 217 | 6,1 | — | — | — | 4,33 | 12 | 3,00 | 32 | 34 | 2,99 | 0,00 |
| 200 ms | 3% | RTP fixo | 3 | 33 | 133 | 161 | 183 | 0,0 | 173 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 183 | 216 | 5,7 | — | — | — | 4,32 | 12 | 3,80 | 32 | 34 | 3,09 | 0,00 |
| 200 ms | 5% | RTP fixo | 1 | 66 | 149 | 178 | 220 | 1,7 | 336 | 29,4 | 600 | 1535 | 2 | 1 | 1 | 276 | 403 | 11,0 | — | — | — | 4,61 | 14 | 5,30 | 32 | 33 | 5,38 | 0,00 |
| 200 ms | 5% | RTP adaptativo | 1 | 81 | 142 | 182 | 219 | 19,1 | 3821 | 24,1 | 579 | 1715 | 7 | 12 | 1 | 296 | 412 | 10,4 | 510 | 3544 | 3544 | 5,10 | 15 | 6,20 | 32 | 33 | 5,31 | 0,00 |
| 200 ms | 5% | RTP fixo | 2 | 63 | 138 | 161 | 177 | 0,0 | 178 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 270 | 382 | 10,6 | — | — | — | 4,51 | 14 | 4,70 | 32 | 33 | 5,02 | 0,00 |
| 200 ms | 5% | RTP adaptativo | 2 | 47 | 140 | 165 | 193 | 3,1 | 628 | 29,1 | 594 | 1300 | 2 | 1 | 1 | 227 | 317 | 9,8 | 595 | 1736 | 2280 | 3,89 | 13 | 5,40 | 32 | 33 | 4,95 | 0,00 |
| 200 ms | 5% | RTP fixo | 3 | 61 | 142 | 165 | 228 | 2,0 | 407 | 29,4 | 600 | 1535 | 2 | 1 | 1 | 260 | 354 | 9,4 | — | — | — | 4,55 | 14 | 5,20 | 32 | 33 | 4,94 | 0,00 |
| 200 ms | 5% | RTP adaptativo | 3 | 57 | 135 | 166 | 232 | 15,7 | 3134 | 25,3 | 582 | 1501 | 6 | 9 | 1 | 252 | 356 | 10,2 | 598 | 1236 | 3196 | 4,47 | 14 | 5,20 | 32 | 33 | 4,86 | 0,00 |
| 500 ms | 3% | RTP fixo | 1 | 33 | 129 | 159 | 171 | 0,0 | 164 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 177 | 213 | 5,9 | — | — | — | 4,32 | 13 | 2,80 | 32 | 33 | 2,95 | 0,00 |
| 500 ms | 3% | RTP fixo | 2 | 33 | 128 | 152 | 165 | 0,0 | 165 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 180 | 227 | 6,2 | — | — | — | 4,34 | 12 | 2,40 | 32 | 33 | 2,98 | 0,00 |
| 500 ms | 3% | RTP fixo | 3 | 33 | 129 | 160 | 166 | 0,0 | 166 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 184 | 234 | 6,5 | — | — | — | 4,35 | 13 | 3,00 | 32 | 33 | 3,08 | 0,00 |
| 500 ms | 5% | RTP fixo | 1 | 73 | 152 | 189 | 222 | 5,3 | 616 | 28,9 | 600 | 1535 | 2 | 1 | 1 | 285 | 401 | 10,6 | — | — | — | 4,60 | 14 | 5,20 | 32 | 33 | 5,38 | 0,00 |
| 500 ms | 5% | RTP adaptativo | 1 | 78 | 158 | 220 | 293 | 6,8 | 1153 | 28,2 | 594 | 1605 | 2 | 2 | 1 | 276 | 395 | 10,3 | 526 | 1433 | 5571 | 4,78 | 14 | 5,20 | 32 | 33 | 5,32 | 0,00 |
| 500 ms | 5% | RTP fixo | 2 | 65 | 146 | 167 | 195 | 0,0 | 190 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 263 | 377 | 10,2 | — | — | — | 4,50 | 14 | 4,80 | 32 | 33 | 5,02 | 0,00 |
| 500 ms | 5% | RTP adaptativo | 2 | 67 | 138 | 167 | 255 | 1,3 | 256 | 29,7 | 597 | 1760 | 1 | 0 | 0 | 275 | 427 | 10,2 | 611 | 2486 | 3070 | 5,22 | 14 | 5,50 | 32 | 33 | 5,00 | 0,00 |
| 500 ms | 5% | RTP fixo | 3 | 61 | 145 | 191 | 258 | 2,2 | 240 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 259 | 353 | 9,8 | — | — | — | 4,48 | 14 | 4,80 | 32 | 33 | 4,89 | 0,00 |
| 500 ms | 5% | RTP adaptativo | 3 | 66 | 136 | 166 | 272 | 1,4 | 273 | 29,7 | 597 | 1530 | 1 | 0 | 0 | 273 | 401 | 10,2 | 646 | 1973 | 3242 | 4,57 | 14 | 4,10 | 32 | 33 | 5,07 | 0,00 |

### O teto de retransmissão

| teto de RTX | cenário | variante | rodada | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | kbps gerados | chaves | pedidos (PLI) | quebras | NACKs | RTX pacotes | RTX % dos bytes | est. mín kbps | est. mediana | est. final | MB no cano | retorno kbps | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| sem o teto | 3% | RTP fixo | 1 | 33 | 130 | 160 | 171 | 0,0 | 171 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 157 | 191 | 5,3 | — | — | — | 4,30 | 12 | 3,70 | 32 | 33 | 2,96 | 0,00 |
| sem o teto | 3% | RTP adaptativo | 1 | 36 | 130 | 152 | 184 | 0,0 | 184 | 29,7 | 597 | 1281 | 1 | 0 | 0 | 139 | 188 | 5,9 | 611 | 1262 | 3416 | 3,70 | 12 | 3,60 | 32 | 33 | 3,00 | 0,00 |
| sem o teto | 3% | RTP fixo | 2 | 33 | 130 | 161 | 185 | 0,0 | 186 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 182 | 227 | 6,1 | — | — | — | 4,33 | 13 | 2,70 | 32 | 33 | 2,98 | 0,00 |
| sem o teto | 3% | RTP adaptativo | 2 | 41 | 132 | 155 | 193 | 0,0 | 198 | 29,8 | 597 | 1681 | 1 | 0 | 0 | 184 | 244 | 5,7 | 681 | 2357 | 4076 | 4,80 | 13 | 2,90 | 32 | 33 | 2,93 | 0,00 |
| sem o teto | 3% | RTP fixo | 3 | 33 | 132 | 159 | 177 | 0,0 | 174 | 29,8 | 600 | 1505 | 1 | 0 | 0 | 182 | 225 | 6,1 | — | — | — | 4,33 | 13 | 2,90 | 32 | 33 | 3,09 | 0,00 |
| sem o teto | 3% | RTP adaptativo | 3 | 35 | 356 | 465 | 550 | 1,1 | 214 | 29,8 | 598 | 758 | 1 | 0 | 0 | 105 | 131 | 6,1 | 595 | 855 | 995 | 2,27 | 11 | 2,10 | 32 | 33 | 2,78 | 0,00 |
| sem o teto | 5% | RTP fixo | 1 | 63 | 151 | 181 | 226 | 0,0 | 200 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 255 | 356 | 9,7 | — | — | — | 4,47 | 14 | 6,60 | 32 | 33 | 5,34 | 0,00 |
| sem o teto | 5% | RTP adaptativo | 1 | 71 | 153 | 187 | 225 | 0,0 | 190 | 29,6 | 597 | 1652 | 1 | 0 | 0 | 282 | 426 | 10,6 | 513 | 3052 | 5227 | 4,93 | 15 | 5,40 | 32 | 33 | 5,33 | 0,00 |
| sem o teto | 5% | RTP fixo | 2 | 58 | 144 | 161 | 196 | 0,0 | 195 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 253 | 369 | 10,2 | — | — | — | 4,49 | 14 | 4,90 | 32 | 33 | 5,01 | 0,00 |
| sem o teto | 5% | RTP adaptativo | 2 | 61 | 146 | 167 | 251 | 1,3 | 252 | 29,7 | 597 | 1334 | 1 | 0 | 0 | 235 | 338 | 10,0 | 636 | 1281 | 4135 | 3,99 | 13 | 4,40 | 32 | 33 | 4,98 | 0,00 |
| sem o teto | 5% | RTP fixo | 3 | 60 | 141 | 161 | 194 | 3,2 | 634 | 29,1 | 600 | 1535 | 2 | 2 | 1 | 257 | 366 | 9,7 | — | — | — | 4,56 | 14 | 4,80 | 32 | 33 | 4,93 | 0,00 |
| sem o teto | 5% | RTP adaptativo | 3 | 69 | 141 | 169 | 214 | 1,1 | 213 | 29,8 | 597 | 1803 | 1 | 0 | 0 | 283 | 451 | 10,3 | 648 | 2715 | 3073 | 5,35 | 15 | 4,60 | 32 | 33 | 5,07 | 0,00 |
| com o teto | 10% | RTP fixo | 1 | 180 | 1765 | 1898 | 1899 | 84,2 | 3909 | 9,7 | 600 | 1838 | 12 | 19 | 8 | 420 | 743 | 16,8 | — | — | — | 5,77 | 18 | 10,50 | 32 | 33 | 10,27 | 0,00 |
| com o teto | 10% | RTP adaptativo | 1 | 115 | 1168 | 1367 | 1501 | 65,5 | 3645 | 14,2 | 570 | 1306 | 10 | 18 | 4 | 355 | 525 | 16,9 | 418 | 1549 | 2181 | 4,03 | 16 | 9,80 | 32 | 33 | 10,03 | 0,00 |
| com o teto | 10% | RTP fixo | 2 | 1100 | 1765 | 1867 | 1932 | 92,3 | 3864 | 12,2 | 600 | 1959 | 16 | 24 | 10 | 428 | 766 | 16,5 | — | — | — | 6,12 | 18 | 9,40 | 32 | 33 | 10,07 | 0,00 |
| com o teto | 10% | RTP adaptativo | 2 | 705 | 1871 | 2139 | 2305 | 85,6 | 5169 | 10,1 | 546 | 1424 | 18 | 30 | 5 | 398 | 573 | 16,8 | 421 | 1459 | 1637 | 4,49 | 17 | 10,10 | 32 | 33 | 10,02 | 0,00 |
| com o teto | 10% | RTP fixo | 3 | 185 | 1565 | 1733 | 1798 | 81,9 | 3747 | 12,5 | 600 | 1929 | 15 | 26 | 6 | 420 | 757 | 16,2 | — | — | — | 6,02 | 18 | 11,60 | 32 | 33 | 10,39 | 0,00 |
| com o teto | 10% | RTP adaptativo | 3 | 1233 | 2113 | 2413 | 2580 | 91,6 | 4332 | 11,3 | 543 | 1205 | 19 | 32 | 8 | 345 | 464 | 16,2 | 358 | 905 | 1754 | 3,77 | 15 | 9,90 | 32 | 33 | 10,19 | 0,00 |
| sem o teto | 10% | RTP fixo | 1 | 102 | 179 | 226 | 260 | 5,6 | 241 | 29,8 | 600 | 1505 | 1 | 0 | 0 | 388 | 732 | 20,1 | — | — | — | 4,88 | 16 | 11,00 | 32 | 33 | 10,14 | 0,00 |
| sem o teto | 10% | RTP adaptativo | 1 | 103 | 181 | 227 | 264 | 5,0 | 798 | 28,9 | 595 | 1003 | 2 | 2 | 1 | 328 | 547 | 21,6 | 404 | 1300 | 2121 | 3,35 | 15 | 10,09 | 32 | 33 | 10,01 | 0,00 |
| sem o teto | 10% | RTP fixo | 2 | 104 | 168 | 198 | 260 | 12,0 | 2159 | 26,5 | 600 | 1535 | 2 | 1 | 1 | 405 | 768 | 20,3 | — | — | — | 4,98 | 17 | 8,10 | 32 | 33 | 10,00 | 0,00 |
| sem o teto | 10% | RTP adaptativo | 2 | 118 | 428 | 515 | 585 | 2,4 | 266 | 29,6 | 597 | 1355 | 1 | 0 | 0 | 342 | 697 | 21,4 | 475 | 1414 | 2246 | 4,46 | 15 | 10,30 | 32 | 34 | 10,04 | 0,00 |
| sem o teto | 10% | RTP fixo | 3 | 107 | 163 | 196 | 238 | 1,0 | 205 | 29,8 | 600 | 1505 | 1 | 0 | 0 | 394 | 795 | 21,7 | — | — | — | 4,95 | 16 | 8,70 | 32 | 34 | 10,50 | 0,00 |
| sem o teto | 10% | RTP adaptativo | 3 | 112 | 566 | 715 | 772 | 2,4 | 253 | 29,7 | 598 | 1175 | 1 | 0 | 0 | 305 | 623 | 21,5 | 366 | 1033 | 2311 | 3,88 | 15 | 10,89 | 32 | 34 | 10,20 | 0,00 |

### A configuração ajustada (espera de 500 ms, sem o teto)

| janela | cenário | variante | rodada | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | kbps gerados | chaves | pedidos (PLI) | quebras | NACKs | RTX pacotes | RTX % dos bytes | est. mín kbps | est. mediana | est. final | MB no cano | retorno kbps | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| primeiros 20 s | 0% | RTP adaptativo | 1 | 39 | 42 | 43 | 165 | 0,0 | 166 | 29,8 | 597 | 1925 | 1 | 0 | 0 | 0 | 23 | 0,2 | 700 | 3026 | 3766 | 5,19 | 10 | 0,00 | 32 | 33 | 0,00 | 0,00 |
| primeiros 20 s | 0,3% | RTP adaptativo | 1 | 40 | 54 | 124 | 166 | 0,0 | 168 | 29,8 | 597 | 2023 | 1 | 0 | 0 | 18 | 38 | 0,8 | 705 | 3547 | 3739 | 5,47 | 11 | 0,30 | 32 | 33 | 0,24 | 0,00 |
| primeiros 20 s | 1% | RTP adaptativo | 1 | 40 | 106 | 133 | 162 | 0,0 | 158 | 29,8 | 597 | 1885 | 1 | 0 | 0 | 66 | 83 | 1,7 | 630 | 3766 | 3766 | 5,16 | 11 | 0,80 | 32 | 33 | 0,80 | 0,00 |
| primeiros 20 s | 3% | RTP adaptativo | 1 | 40 | 132 | 161 | 198 | 1,0 | 204 | 29,6 | 597 | 1540 | 1 | 0 | 0 | 172 | 237 | 6,2 | 602 | 1167 | 3561 | 4,43 | 13 | 3,40 | 32 | 33 | 3,00 | 0,00 |
| primeiros 20 s | 5% | RTP adaptativo | 1 | 62 | 135 | 166 | 200 | 0,0 | 186 | 29,7 | 597 | 1486 | 1 | 0 | 0 | 242 | 351 | 9,8 | 514 | 1166 | 3249 | 4,42 | 14 | 6,30 | 32 | 33 | 5,36 | 0,00 |
| primeiros 20 s | 1,0 Mbps, 0% | RTP adaptativo | 1 | 136 | 561 | 728 | 797 | 10,6 | 838 | 27,6 | 594 | 860 | 2 | 1 | 1 | 37 | 182 | 9,3 | 644 | 1001 | 1014 | 2,53 | 10 | 0,00 | 92 | 267 | 0,00 | 1,86 |
| primeiros 20 s | 2,0 Mbps, 1% | RTP adaptativo | 1 | 54 | 128 | 163 | 196 | 0,0 | 159 | 29,8 | 597 | 1237 | 1 | 0 | 0 | 68 | 88 | 2,5 | 549 | 1458 | 1951 | 3,47 | 11 | 0,50 | 35 | 53 | 0,80 | 0,00 |
| primeiros 20 s | 0% | RTP adaptativo | 2 | 39 | 42 | 43 | 134 | 0,0 | 135 | 29,8 | 597 | 1906 | 1 | 0 | 0 | 0 | 17 | 0,2 | 583 | 3247 | 4469 | 5,14 | 10 | 0,00 | 32 | 33 | 0,00 | 0,00 |
| primeiros 20 s | 0,3% | RTP adaptativo | 2 | 40 | 51 | 127 | 195 | 0,0 | 199 | 29,8 | 597 | 1928 | 1 | 0 | 0 | 20 | 36 | 0,8 | 645 | 3406 | 3627 | 5,22 | 11 | 0,60 | 32 | 33 | 0,31 | 0,00 |
| primeiros 20 s | 1% | RTP adaptativo | 2 | 39 | 102 | 133 | 156 | 0,0 | 149 | 29,8 | 597 | 1689 | 1 | 0 | 0 | 72 | 100 | 2,3 | 622 | 2371 | 4340 | 4,67 | 11 | 0,80 | 32 | 33 | 0,91 | 0,00 |
| primeiros 20 s | 3% | RTP adaptativo | 2 | 37 | 128 | 150 | 166 | 0,0 | 167 | 29,7 | 597 | 1272 | 1 | 0 | 0 | 148 | 213 | 6,3 | 644 | 1269 | 3693 | 3,69 | 12 | 4,20 | 32 | 33 | 3,17 | 0,00 |
| primeiros 20 s | 5% | RTP adaptativo | 2 | 63 | 146 | 173 | 199 | 0,0 | 186 | 29,7 | 597 | 1535 | 1 | 0 | 0 | 251 | 383 | 10,4 | 564 | 1747 | 3202 | 4,59 | 14 | 4,90 | 32 | 33 | 5,01 | 0,00 |
| primeiros 20 s | 1,0 Mbps, 0% | RTP adaptativo | 2 | 151 | 664 | 868 | 1102 | 9,4 | 367 | 28,3 | 597 | 841 | 1 | 0 | 0 | 45 | 148 | 7,6 | 531 | 940 | 914 | 2,48 | 10 | 0,00 | 64 | 265 | 0,00 | 1,37 |
| primeiros 20 s | 2,0 Mbps, 1% | RTP adaptativo | 2 | 55 | 131 | 164 | 181 | 0,0 | 178 | 29,7 | 597 | 1263 | 1 | 0 | 0 | 81 | 108 | 3,2 | 688 | 1436 | 1928 | 3,57 | 11 | 0,90 | 35 | 51 | 0,99 | 0,00 |
| primeiros 20 s | 0% | RTP adaptativo | 3 | 40 | 43 | 44 | 167 | 0,0 | 167 | 29,8 | 597 | 1852 | 1 | 0 | 0 | 0 | 20 | 0,3 | 680 | 4176 | 5239 | 5,01 | 10 | 0,00 | 32 | 34 | 0,00 | 0,00 |
| primeiros 20 s | 0,3% | RTP adaptativo | 3 | 40 | 75 | 131 | 165 | 0,0 | 168 | 29,8 | 597 | 1911 | 1 | 0 | 0 | 36 | 53 | 1,0 | 687 | 2641 | 3055 | 5,20 | 11 | 0,20 | 32 | 34 | 0,38 | 0,00 |
| primeiros 20 s | 1% | RTP adaptativo | 3 | 35 | 97 | 142 | 166 | 0,0 | 167 | 29,8 | 597 | 886 | 1 | 0 | 0 | 41 | 49 | 2,2 | 383 | 964 | 1878 | 2,53 | 10 | 1,40 | 32 | 34 | 1,08 | 0,00 |
| primeiros 20 s | 3% | RTP adaptativo | 3 | 38 | 132 | 163 | 192 | 0,0 | 189 | 29,7 | 597 | 1298 | 1 | 0 | 0 | 143 | 195 | 5,7 | 495 | 1081 | 3087 | 3,75 | 12 | 3,30 | 32 | 34 | 3,00 | 0,00 |
| primeiros 20 s | 5% | RTP adaptativo | 3 | 75 | 153 | 175 | 242 | 0,0 | 198 | 29,7 | 598 | 1868 | 1 | 0 | 0 | 286 | 450 | 10,2 | 567 | 2969 | 3315 | 5,51 | 15 | 5,99 | 32 | 34 | 5,06 | 0,00 |
| primeiros 20 s | 1,0 Mbps, 0% | RTP adaptativo | 3 | 116 | 757 | 1187 | 1221 | 40,2 | 6208 | 17,4 | 558 | 791 | 14 | 24 | 2 | 58 | 1881 | 106,6 | 159 | 902 | 159 | 4,28 | 11 | 0,00 | 157 | 276 | 0,00 | 33,15 |
| primeiros 20 s | 2,0 Mbps, 1% | RTP adaptativo | 3 | 59 | 134 | 167 | 251 | 0,0 | 168 | 29,6 | 597 | 1198 | 1 | 0 | 0 | 85 | 96 | 3,4 | 407 | 1660 | 2002 | 3,39 | 11 | 1,10 | 35 | 67 | 1,03 | 0,00 |
| primeiros 20 s | 3% | RTP fixo | 1 | 33 | 129 | 160 | 188 | 0,0 | 174 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 174 | 210 | 5,8 | — | — | — | 4,32 | 13 | 3,20 | 32 | 34 | 2,95 | 0,00 |
| primeiros 20 s | 3% | RTP fixo | 2 | 33 | 127 | 153 | 163 | 0,0 | 163 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 187 | 234 | 6,5 | — | — | — | 4,35 | 13 | 2,50 | 32 | 34 | 2,98 | 0,00 |
| primeiros 20 s | 3% | RTP fixo | 3 | 33 | 131 | 161 | 191 | 0,0 | 168 | 29,8 | 600 | 1505 | 1 | 0 | 0 | 181 | 240 | 6,5 | — | — | — | 4,35 | 13 | 2,40 | 32 | 34 | 3,08 | 0,00 |
| primeiros 20 s | 5% | RTP fixo | 1 | 71 | 153 | 181 | 194 | 0,0 | 195 | 29,7 | 600 | 1505 | 1 | 0 | 0 | 283 | 393 | 10,8 | — | — | — | 4,52 | 14 | 5,00 | 32 | 34 | 5,36 | 0,00 |
| primeiros 20 s | 5% | RTP fixo | 2 | 65 | 152 | 185 | 197 | 0,0 | 196 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 260 | 385 | 10,3 | — | — | — | 4,50 | 14 | 3,40 | 32 | 34 | 5,01 | 0,00 |
| primeiros 20 s | 5% | RTP fixo | 3 | 65 | 149 | 164 | 206 | 1,0 | 207 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 267 | 382 | 10,4 | — | — | — | 4,50 | 14 | 3,80 | 32 | 34 | 4,90 | 0,00 |
| 40–60 s | 3% | RTP adaptativo | 1 | 64 | 134 | 157 | 189 | 0,0 | 182 | 30,0 | 600 | 2371 | 0 | 0 | 0 | 256 | 336 | 6,2 | 3253 | 3253 | 3253 | 6,72 | 14 | 2,10 | 32 | 34 | 2,94 | 0,00 |
| 40–60 s | 3% | RTP adaptativo | 2 | 53 | 136 | 161 | 168 | 0,0 | 159 | 29,9 | 600 | 2371 | 0 | 0 | 0 | 244 | 309 | 5,7 | 3288 | 3288 | 3288 | 6,69 | 14 | 4,00 | 32 | 34 | 3,02 | 0,00 |
| 40–60 s | 3% | RTP adaptativo | 3 | 43 | 134 | 158 | 177 | 0,0 | 167 | 29,9 | 599 | 2367 | 0 | 0 | 0 | 218 | 276 | 5,1 | 3827 | 3989 | 4075 | 6,65 | 14 | 3,10 | 32 | 34 | 2,68 | 0,00 |
| 40–60 s | 5% | RTP adaptativo | 1 | 94 | 156 | 188 | 228 | 0,0 | 173 | 29,9 | 600 | 2371 | 0 | 0 | 0 | 353 | 563 | 10,4 | 3303 | 3303 | 3303 | 6,98 | 16 | 5,59 | 31 | 34 | 5,36 | 0,00 |
| 40–60 s | 5% | RTP adaptativo | 2 | 93 | 139 | 166 | 201 | 0,0 | 158 | 29,9 | 600 | 2371 | 0 | 0 | 0 | 347 | 530 | 9,8 | 3314 | 3314 | 3314 | 6,94 | 16 | 5,50 | 32 | 34 | 4,94 | 0,00 |
| 40–60 s | 5% | RTP adaptativo | 3 | 78 | 138 | 166 | 199 | 0,0 | 162 | 30,0 | 600 | 2371 | 0 | 0 | 0 | 318 | 499 | 9,2 | 3282 | 3809 | 3809 | 6,91 | 15 | 4,80 | 32 | 34 | 4,54 | 0,00 |

### As bandas estreitas em 60 s

| configuração | cenário | variante | rodada | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | kbps gerados | chaves | pedidos (PLI) | quebras | NACKs | RTX pacotes | RTX % dos bytes | est. mín kbps | est. mediana | est. final | MB no cano | retorno kbps | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| padrão | 1,0 Mbps, 0% | RTP adaptativo | 1 | 154 | 413 | 605 | 699 | 11,2 | 2502 | 27,4 | 1791 | 861 | 3 | 4 | 2 | 49 | 123 | 2,0 | 522 | 969 | 910 | 7,37 | 9 | 0,00 | 92 | 245 | 0,00 | 0,52 |
| padrão | 1,0 Mbps, 0% | RTP adaptativo | 2 | 136 | 340 | 502 | 656 | 4,5 | 823 | 29,2 | 1797 | 870 | 1 | 0 | 0 | 32 | 57 | 0,9 | 649 | 1022 | 908 | 7,36 | 9 | 0,00 | 77 | 236 | 0,00 | 0,21 |
| padrão | 1,0 Mbps, 0% | RTP adaptativo | 3 | 138 | 375 | 561 | 630 | 7,0 | 2330 | 28,6 | 1794 | 859 | 2 | 1 | 1 | 38 | 95 | 1,5 | 520 | 978 | 880 | 7,32 | 9 | 0,00 | 86 | 245 | 0,00 | 0,25 |
| espera 500 | 1,0 Mbps, 0% | RTP adaptativo | 1 | 164 | 415 | 621 | 700 | 4,0 | 319 | 29,8 | 1797 | 854 | 1 | 0 | 0 | 38 | 126 | 2,1 | 597 | 976 | 1188 | 7,31 | 9 | 0,00 | 94 | 250 | 0,00 | 0,44 |
| espera 500 | 1,0 Mbps, 0% | RTP adaptativo | 2 | 97 | 575 | 785 | 1159 | 14,5 | 2056 | 26,5 | 1768 | 842 | 11 | 16 | 4 | 130 | 192 | 3,3 | 557 | 959 | 1081 | 7,31 | 10 | 0,00 | 71 | 261 | 0,00 | 2,14 |
| espera 500 | 1,0 Mbps, 0% | RTP adaptativo | 3 | 128 | 601 | 717 | 1101 | 14,1 | 2582 | 26,6 | 1767 | 867 | 11 | 15 | 4 | 114 | 215 | 3,6 | 538 | 965 | 897 | 7,52 | 10 | 0,00 | 95 | 261 | 0,00 | 2,12 |
| pedidos calmos | 1,0 Mbps, 0% | RTP adaptativo | 1 | 112 | 593 | 817 | 1060 | 29,7 | 7324 | 21,6 | 1764 | 871 | 12 | 15 | 6 | 278 | 341 | 5,5 | 602 | 977 | 896 | 7,68 | 11 | 0,00 | 125 | 269 | 0,00 | 3,57 |
| pedidos calmos | 1,0 Mbps, 0% | RTP adaptativo | 2 | 220 | 542 | 661 | 847 | 20,3 | 1180 | 26,1 | 1777 | 874 | 8 | 7 | 7 | 156 | 285 | 4,5 | 622 | 943 | 1303 | 7,64 | 10 | 0,03 | 133 | 264 | 0,00 | 2,07 |
| pedidos calmos | 1,0 Mbps, 0% | RTP adaptativo | 3 | 60 | 553 | 858 | 1096 | 28,6 | 7371 | 22,2 | 1767 | 785 | 11 | 15 | 6 | 259 | 355 | 6,1 | 512 | 921 | 1179 | 7,02 | 10 | 0,00 | 54 | 269 | 0,00 | 4,09 |
| padrão | 2,0 Mbps, 1% | RTP adaptativo | 1 | 61 | 140 | 185 | 287 | 0,0 | 189 | 29,9 | 1797 | 1454 | 1 | 0 | 0 | 278 | 344 | 3,1 | 508 | 1868 | 2131 | 12,24 | 11 | 0,90 | 41 | 71 | 0,96 | 0,00 |
| padrão | 2,0 Mbps, 1% | RTP adaptativo | 2 | 65 | 165 | 216 | 299 | 0,0 | 197 | 29,9 | 1797 | 1607 | 1 | 0 | 0 | 320 | 366 | 3,1 | 681 | 1968 | 2173 | 13,46 | 11 | 0,90 | 44 | 74 | 0,98 | 0,00 |
| padrão | 2,0 Mbps, 1% | RTP adaptativo | 3 | 65 | 403 | 626 | 779 | 4,3 | 1505 | 28,9 | 1797 | 1478 | 1 | 0 | 0 | 303 | 412 | 3,9 | 412 | 1846 | 1717 | 12,51 | 11 | 0,73 | 43 | 159 | 0,89 | 0,20 |

### As variações

| variação | cenário | variante | rodada | lat p50 | p95 | p99 | máx | congelado % | maior intervalo ms | quadros/s | gerados | kbps gerados | chaves | pedidos (PLI) | quebras | NACKs | RTX pacotes | RTX % dos bytes | est. mín kbps | est. mediana | est. final | MB no cano | retorno kbps | voz perda % | voz p50 | voz p95 | cano perda % | cano fila % |
|---|---|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| partida em 2,79 Mbps | 0% | RTP adaptativo | 1 | 40 | 44 | 286 | 359 | 1,8 | 360 | 29,8 | 597 | 2055 | 1 | 0 | 0 | 0 | 0 | 0,0 | 1287 | 3110 | 3766 | 5,52 | 10 | 0,00 | 32 | 34 | 0,00 | 0,00 |
| partida em 2,79 Mbps | 0% | RTP adaptativo | 2 | 40 | 52 | 298 | 366 | 1,9 | 389 | 29,8 | 597 | 2037 | 1 | 0 | 0 | 0 | 0 | 0,0 | 1260 | 3045 | 3843 | 5,47 | 10 | 0,00 | 32 | 34 | 0,00 | 0,00 |
| partida em 2,79 Mbps | 0% | RTP adaptativo | 3 | 41 | 59 | 253 | 335 | 1,8 | 356 | 29,8 | 597 | 2076 | 1 | 0 | 0 | 0 | 0 | 0,0 | 1393 | 3209 | 3875 | 5,57 | 10 | 0,00 | 32 | 34 | 0,00 | 0,00 |
| partida em 2,79 Mbps | 1,0 Mbps, 0% | RTP adaptativo | 1 | 70 | 466 | 597 | 661 | 6,7 | 508 | 28,9 | 597 | 852 | 1 | 0 | 0 | 10 | 54 | 2,7 | 817 | 941 | 932 | 2,44 | 10 | 0,00 | 54 | 236 | 0,00 | 0,81 |
| partida em 2,79 Mbps | 1,0 Mbps, 0% | RTP adaptativo | 2 | 165 | 472 | 577 | 662 | 5,6 | 506 | 29,8 | 597 | 859 | 1 | 0 | 0 | 11 | 31 | 1,5 | 835 | 923 | 835 | 2,44 | 9 | 0,00 | 72 | 234 | 0,00 | 0,31 |
| partida em 2,79 Mbps | 1,0 Mbps, 0% | RTP adaptativo | 3 | 106 | 478 | 621 | 687 | 5,6 | 508 | 29,8 | 597 | 843 | 1 | 0 | 0 | 11 | 45 | 2,2 | 859 | 930 | 859 | 2,41 | 9 | 0,00 | 62 | 240 | 0,00 | 0,44 |
| voz com reordenação 15 | 3% | RTP fixo | 1 | 33 | 129 | 160 | 169 | 0,0 | 165 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 183 | 229 | 6,4 | — | — | — | 4,34 | 13 | 2,00 | 33 | 274 | 2,94 | 0,00 |
| voz com reordenação 15 | 3% | RTP fixo | 2 | 33 | 128 | 148 | 195 | 0,0 | 197 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 181 | 225 | 6,3 | — | — | — | 4,34 | 13 | 2,70 | 33 | 292 | 2,98 | 0,00 |
| voz com reordenação 15 | 3% | RTP fixo | 3 | 33 | 126 | 157 | 181 | 0,0 | 161 | 29,8 | 601 | 1507 | 1 | 0 | 0 | 171 | 216 | 6,0 | — | — | — | 4,33 | 12 | 3,60 | 33 | 295 | 3,09 | 0,00 |
| jitter de 20 ms | 0% | RTP fixo | 1 | 50 | 52 | 54 | 56 | 0,0 | 71 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 0 | 0 | 0,0 | — | — | — | 4,09 | 10 | 0,00 | 44 | 52 | 0,00 | 0,00 |
| jitter de 20 ms | 0% | RTP adaptativo | 1 | 55 | 61 | 63 | 145 | 0,0 | 149 | 29,8 | 597 | 1842 | 1 | 0 | 0 | 0 | 18 | 0,3 | 472 | 3156 | 3655 | 4,98 | 10 | 0,00 | 44 | 51 | 0,00 | 0,00 |
| jitter de 20 ms | 0% | RTP fixo | 2 | 50 | 53 | 53 | 54 | 0,0 | 72 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 0 | 0 | 0,0 | — | — | — | 4,09 | 10 | 0,00 | 43 | 52 | 0,00 | 0,00 |
| jitter de 20 ms | 0% | RTP adaptativo | 2 | 54 | 61 | 63 | 145 | 0,0 | 148 | 29,8 | 597 | 1555 | 1 | 0 | 0 | 0 | 23 | 0,5 | 486 | 1915 | 3803 | 4,24 | 10 | 0,00 | 44 | 51 | 0,00 | 0,00 |
| jitter de 20 ms | 0% | RTP fixo | 3 | 50 | 53 | 54 | 55 | 0,0 | 64 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 0 | 0 | 0,0 | — | — | — | 4,09 | 10 | 0,00 | 44 | 52 | 0,00 | 0,00 |
| jitter de 20 ms | 0% | RTP adaptativo | 3 | 55 | 61 | 64 | 184 | 0,0 | 184 | 29,8 | 597 | 1753 | 1 | 0 | 0 | 0 | 13 | 0,2 | 523 | 2983 | 3628 | 4,74 | 10 | 0,00 | 43 | 51 | 0,00 | 0,00 |
| jitter de 20 ms | 3% | RTP fixo | 1 | 52 | 169 | 197 | 215 | 0,0 | 196 | 29,9 | 600 | 1505 | 1 | 0 | 0 | 237 | 336 | 9,1 | — | — | — | 4,45 | 13 | 2,60 | 43 | 52 | 3,00 | 0,00 |
| jitter de 20 ms | 3% | RTP adaptativo | 1 | 67 | 168 | 192 | 206 | 11,3 | 2254 | 26,2 | 594 | 1631 | 2 | 1 | 1 | 256 | 343 | 8,9 | 468 | 1858 | 3502 | 4,80 | 14 | 3,10 | 43 | 50 | 3,00 | 0,00 |
| jitter de 20 ms | 3% | RTP fixo | 2 | 52 | 164 | 192 | 222 | 0,0 | 191 | 29,8 | 600 | 1505 | 1 | 0 | 0 | 237 | 310 | 8,5 | — | — | — | 4,43 | 13 | 3,80 | 43 | 51 | 2,99 | 0,00 |
| jitter de 20 ms | 3% | RTP adaptativo | 2 | 59 | 172 | 191 | 223 | 1,1 | 215 | 29,8 | 597 | 1699 | 1 | 0 | 0 | 227 | 345 | 8,7 | 512 | 2793 | 3287 | 4,97 | 13 | 3,20 | 43 | 51 | 2,94 | 0,00 |
| jitter de 20 ms | 3% | RTP fixo | 3 | 52 | 171 | 196 | 239 | 1,1 | 225 | 29,7 | 600 | 1505 | 1 | 0 | 0 | 237 | 335 | 9,2 | — | — | — | 4,45 | 14 | 3,20 | 43 | 51 | 3,08 | 0,00 |
| jitter de 20 ms | 3% | RTP adaptativo | 3 | 53 | 172 | 252 | 367 | 14,4 | 2330 | 26,3 | 595 | 867 | 2 | 2 | 1 | 154 | 186 | 8,6 | 334 | 786 | 1916 | 2,62 | 12 | 3,10 | 43 | 51 | 2,99 | 0,00 |


## Conferência independente

A célula de 5% foi rodada de novo, depois de tudo, por quem não escreveu o
exemplo (`--variante a,fixo,adaptativo --so "5%"`, 3 rodadas de 20 s). Mediana
de p50 ms / p95 ms / congelado %, com a faixa do congelado entre parênteses:

| variante | conferência | grade acima |
|---|--:|--:|
| A (SCTP, controle) | 2290 / 3930 / 42,7 (32,7–52,8) | 2474 / 5339 / 57,1 |
| RTP fixo | 66 / 148 / 2,2 (0,0–11,8) | 62 / 149 / 1,2 |
| RTP adaptativo | 66 / 148 / 14,3 (12,5–26,5) | 80 / 148 / 13,9 |

O RTP reproduz. A linha A variou mais entre as duas passagens que o RTP, e
segue uma ordem de grandeza pior na latência.
