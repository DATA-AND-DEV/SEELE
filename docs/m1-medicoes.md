# M1 — registro de medições

Log corrido das medições de M1. Cada entrada diz **o que** foi medido, **como**,
e em **qual máquina** — sem isso um número de latência não significa nada.

Ferramenta: `spikes/device-latency` (descartável, morre com M1).

---

## M1.1 — latência de ida-e-volta do dispositivo

**Máquina:** MacBook Pro, Apple Silicon, macOS 26.5, CoreAudio.
**Dispositivos:** microfone e alto-falantes internos. Sala silenciosa.
**Método:** chirp linear de 5 ms, 1–8 kHz, com envelope de cosseno elevado.
Tocado pelo alto-falante, capturado pelo microfone, localizado por correlação
cruzada normalizada. A diferença de tempo vem dos `StreamInstant` do `cpal`
(`playback` no callback de saída, `capture` no de entrada), que vivem no mesmo
domínio de relógio.

**Validação do método:** o frame do pico de correlação varia entre execuções
(52991 → 51216) enquanto a latência não varia. Ou seja, o resultado não depende
de quando o chirp disparou — que é exatamente a propriedade que se quer.
Pico de correlação entre 0,796 e 0,826 em todas as execuções.

### Resultado

Medido primeiro em `cpal 0.16`, depois refeito em **`cpal 0.18`**, que é a versão
que `magi-audio` usa. A versão da biblioteca importa mais do que se esperava.

| `cpal` | round-trip |
|---|---|
| 0.16 | 24,42 · 24,43 · 24,44 · 24,44 ms |
| **0.18** | **20,64 · 20,63 ms** |

**20,6 ms** é o número que vale. Dispersão de 0,01 ms.
A troca de 0.16 para 0.18 sozinha economizou 3,8 ms.

Inclui: buffer de saída, DAC, ~1 ms de ar (~30 cm), ADC, buffer de entrada.
Exclui: codec, rede, jitter buffer.

**Portão de M1.1: PASSA.** O limite era 35 ms.

### Achado inesperado: buffer menor piora a latência

Varredura forçando `BufferSize::Fixed` nos dois lados:

| buffer | `cpal` 0.16 | `cpal` 0.18 |
|---|---|---|
| 64 frames | 43,10 · 43,09 ms | — |
| 128 frames | 40,43 · 40,42 ms | 36,64 ms |
| 256 frames | 35,10 ms | — |
| 512 frames | 24,44 · 24,43 ms | 20,63 ms |
| default | 24,42 ms | 20,64 ms |

O efeito **se reproduz nas duas versões** do `cpal`, o que descarta a hipótese de
ser artefato de uma implementação específica.

O default **é** 512 frames, e é o mínimo real. Pedir buffer menor que o do
hardware faz o `cpal`/CoreAudio inserir uma camada de adaptação, que **acrescenta**
latência em vez de tirar.

Consequência direta para o ADR 0009: a alternativa "configurar buffer de baixa
latência por backend" **não existe no macOS através do `cpal`** — ela piora as
coisas. Resta verificar se WASAPI e PipeWire se comportam igual; se sim, esse
caminho está fechado no projeto inteiro.

### Orçamento recomposto com números medidos

| Estágio | Piso | Padrão | Origem |
|---|---|---|---|
| Dispositivo (captura + reprodução + conversores) | 19,6 ms | 19,6 ms | **medido em M1.1** com `cpal` 0.18, menos ~1 ms de ar |
| Acúmulo de quadro | 20 ms | 20 ms | `specs/03-audio.md` |
| Lookahead do encoder | 6,5 ms | 6,5 ms | **medido em M0.4** |
| Encode | 0,04 ms | 0,04 ms | **medido em M0.10** |
| Rede (LAN) | < 1 ms | < 1 ms | — |
| Jitter buffer | 20 ms | 40 ms | `specs/03-audio.md` |
| Decode | 0,01 ms | 0,01 ms | **medido em M0.10** |
| **Total** | **≈ 67 ms** | **≈ 87 ms** | |

A estimativa original do ADR 0009 era ~68 / ~88 ms — com a versão certa do `cpal`,
ela estava correta.

**Os 60 ms de `specs/00-visao-geral.md` continuam inalcançáveis** — nem com o
jitter buffer no piso, nem com buffer de dispositivo menor, porque essa alavanca
não existe. O piso realista é ~67 ms.

### Ressalva que ainda pode mudar o número

Medido com **alto-falante e microfone internos**. O ADR 0007 exige fone de
ouvido, que é outra configuração:

- some o ~1 ms de ar;
- some o processamento de proteção e equalização que a Apple aplica no
  alto-falante interno, que pode ser vários milissegundos.

É plausível que com fone o número caia. Vale rodar antes de fechar o orçamento:

```
cd spikes/device-latency
cargo run --release -- acoustic
```

Com fone conectado, o caminho acústico precisa existir de alguma forma — encoste
o fone no microfone, ou use um cabo de loopback da saída para uma entrada.


---

## M1.4 — custo da conversão de taxa

**Máquina:** MacBook Pro, Apple Silicon. Perfil release, `criterion`.
`cargo bench --package magi-audio --bench opus_frame`

Orçamento: 20 ms de tempo real por quadro, ou 20 000 µs.

| operação | por quadro de 20 ms | % do tempo real |
|---|---|---|
| resample 44 100 → 48 000 | 14,96 µs | 0,075 % |
| resample 48 000 → 44 100 | 12,74 µs | 0,064 % |
| passthrough (taxas iguais) | 0,034 µs | 0,0002 % |

O resampler sinc do `rubato` (128 taps, oversampling 128) é barato o bastante
para rodar por fonte sem preocupação. O atalho de passthrough é **450× mais
barato** que converter, o que justifica a ramificação.

### Caminho completo, hardware real

`cargo run --release --package magi-audio --example device_smoke`

Nesta máquina os dois dispositivos estão a 48 kHz, então ambos os conversores
entram em passthrough e o atraso de filtro é zero. Em 3 segundos: 144 896 quadros
drenados, 144 896 depois da conversão, **zero overruns, zero underruns, zero erros
de stream**.

O caminho de reamostragem de verdade continua coberto apenas por teste unitário —
não há hardware a 44,1 kHz aqui. Entra no checklist de plataforma de `M1.15`.

### Achado: escala assimétrica na conversão de formato

O teste de round-trip `f32 → i16 → f32` reprovou na primeira escrita. Causa: eu
codificava com escala 32767 e decodificava com 32768, e truncava em vez de
arredondar. Erro de 1,2 passos de quantização, com viés na direção do zero.

Corrigido usando 2^15 nos dois sentidos, com `round()` e clamp nas pontas. É
pequeno, mas é o tipo de coisa que ninguém encontra depois — some no meio do
ruído de fundo e só aparece como "o áudio parece um pouco pior".


---

## M1.6 — calibração do simulador de rede

`cargo run --release --package magi-audio --example netsim_profiles`
30 000 quadros (10 minutos, a mesma duração do soak de `specs/09-roadmap.md`),
semente 20260807.

| perfil | perda | rajada máx | reordenados | trânsito p50 | p95 | pior |
|---|---|---|---|---|---|---|
| `perfect` | 0,00 % | 0 | 0 | 1,0 ms | 1,0 ms | 1,0 ms |
| `lan` | 0,00 % | 0 | 0 | 1,5 ms | 1,9 ms | 2,0 ms |
| `wifi` | 1,09 % | 14 | 301 | 14,1 ms | 19,5 ms | 99,9 ms |
| `regional` | 0,61 % | 12 | 137 | 37,6 ms | 44,3 ms | 105,0 ms |
| `mobile_poor` | 4,34 % | 36 | 4 205 | 80,8 ms | 99,1 ms | 349,9 ms |
| `acceptance 5%` | 4,48 % | 24 | 0 | 10,0 ms | 14,5 ms | 15,0 ms |

Rajada em quadros; 1 quadro = 20 ms.

### Três coisas que isto diz para M1.7

**1. O critério de aceite de M1 contém buracos de meio segundo.** O perfil de 5%
com rajada média de 4 quadros produz uma rajada máxima de **24 quadros = 480 ms**
de silêncio consecutivo em dez minutos. `specs/03-audio.md` manda usar PLC do
Opus por **um** quadro e silêncio com fade a partir do segundo — ou seja, 460 ms
daquele buraco vão ser silêncio, não ocultação.

Isso não invalida "perda induzida de 5% permanece inteligível", mas define o que
a frase significa: inteligível **apesar de** interrupções de meio segundo. Vale
confirmar que é essa a intenção antes de M1.16 cobrar o critério.

**2. O perfil de aceite não exercita reordenação — zero em dez minutos.** É
correto: o jitter dele (10 ms) é menor que o intervalo entre quadros (20 ms), e
sem isso um quadro nunca ultrapassa o anterior. Consequência prática: **os testes
de M1.7 não podem usar só o perfil de aceite.** Reordenação precisa de `wifi`,
`regional` ou `mobile_poor`.

**3. Amostra de um minuto não caracteriza perda em rajada.** Na primeira rodada,
com 3 000 quadros, `mobile_poor` mediu 2,83 % contra 5 % configurados — cerca de
vinte episódios de perda, com erro relativo perto de 25 %. Com 30 000 quadros
converge para 4,34 %. **Testes devem ser dimensionados por número de episódios,
não de quadros.**

### Decisões de modelagem

- **PRNG próprio (PCG32), não `rand`.** Os geradores do `rand` podem mudar entre
  versões; um teste de propriedade que falha hoje tem que ser replicável em dois
  anos, e uma atualização de dependência não pode mudar silenciosamente o que
  "semente 42" significa.
- **Perda por Gilbert-Elliott, não moeda viciada.** Há um teste que exige que o
  modelo em rajada produza corridas mais que duas vezes mais longas que o modelo
  independente na mesma taxa média — sem ele, seria uma forma cara de jogar moeda.
- **Reordenação não é injetada.** Ela emerge: cada quadro sorteia o próprio
  atraso, e quando o quadro N sorteia mais que o N+1, o N+1 chega primeiro. É
  assim que acontece numa rede real, e modelar separadamente permitiria produzir
  reordenações que nenhuma distribuição de atraso explicaria.


---

## M1.9 — separar silêncio de perda

A lacuna G5, medida.

`cargo run --release --package magi-audio --example jitter_profiles`

Um falante em rajadas — 25 quadros de fala, 50 de silêncio, 40 ciclos — sobre um
link **perfeito**, que não descarta nada:

| | quadros |
|---|---|
| tocados | 1 000 |
| silêncio (`Comfort`) | 1 950 |
| ocultados | 0 |
| **perda reportada** | **0,00 %** |

Contar aquele silêncio como perda reportaria **66,1 % de perda num link que não
perdeu um único pacote**. Como `specs/07-tema-evangelion.md` põe a Taxa de
Sincronização como o elemento mais visível da tela, e `specs/02-protocolo.md` dá
à perda até 30 pontos de penalidade, o efeito seria todo mundo aparecendo em
vermelho sempre que a sala ficasse quieta.

### Como a distinção é feita

A resposta já estava em `specs/02-protocolo.md`, na linha que diz que o `ts`
"detecta gaps de silêncio". Em semântica RTP os dois contadores andam diferente:

- `seq` incrementa por pacote **transmitido**;
- `timestamp` avança pelas amostras **decorridas**.

Então um buraco com `seq` contíguo é alguém que parou de falar, e um buraco que
pula sequência é a rede. Os dois podem acontecer juntos, e a aritmética separa
exatamente: o salto de timestamp dá o total de slots, o salto de sequência dá
quantos foram perdidos, e o resto é silêncio. Há teste para o caso misto.

### Consequência de arquitetura

**O playout passou a ser indexado por timestamp, não por sequência.** Tocar
números de sequência consecutivos um atrás do outro comprimiria dois segundos de
silêncio em 20 ms. Isso foi uma reescrita do miolo do M1.7, não um acréscimo.

### Efeito nas medições anteriores

Nenhum. Os perfis de rede do netsim geram um falante contínuo, sem silêncio, e a
tabela de M1.7 permanece idêntica — o que é o resultado certo: a mudança não
altera o comportamento sob perda, só para de mentir sobre silêncio.


---

## M1.8 — deriva de clock (risco R3)

`cargo run --release --package magi-audio --example clock_drift`
30 000 quadros (10 minutos), perfil `lan`, semente 20260807.

| deriva | corrigido | medido | prof. @1min | prof. @9min | creep |
|---|---|---|---|---|---|
| 0 ppm | não | 0,1 | 20,0 ms | 20,0 ms | 0,0 ms |
| 50 ppm | não | 50,5 | 40,0 ms | 60,0 ms | **+20,0 ms** |
| 50 ppm | **sim** | 50,5 | 40,0 ms | 40,0 ms | **0,0 ms** |
| 100 ppm | não | 100,2 | 40,0 ms | 80,0 ms | **+40,0 ms** |
| 100 ppm | **sim** | 100,2 | 40,0 ms | 40,0 ms | **0,0 ms** |
| −100 ppm | não | −99,2 | 20,0 ms | **0,0 ms** | −20,0 ms |
| −100 ppm | **sim** | −99,2 | 20,0 ms | 20,0 ms | **0,0 ms** |
| 200 ppm | não | 200,2 | 40,0 ms | 140,0 ms | **+100,0 ms** |
| 200 ppm | **sim** | 200,2 | 40,0 ms | 40,0 ms | **0,0 ms** |

Precisão da medição: dentro de 1 ppm em toda a faixa, inclusive sob jitter e
perda de `mobile_poor`.

### Por que dez minutos, e não um

`specs/09-roadmap.md` pede "sem estalo em 10 minutos contínuos" sem dizer por
quê. Este é o motivo: um cristal especificado a ±50 ppm é comum, dois lados podem
estar 100 ppm afastados e ambos dentro da especificação, e 100 ppm são 60 ms de
desvio acumulado em dez minutos — um jitter buffer inteiro.

A linha de **−100 ppm sem correção** é a mais eloquente: a profundidade cai de
20 ms para **zero**. O buffer não transbordou, ele secou — e a partir dali cada
slot é um underrun. É exatamente o sintoma de "funcionou por uns minutos e
começou a estalar", que é o mais caro de diagnosticar em áudio.

### Como a deriva é medida

Trânsito é `chegada − timestamp`, poluído por jitter. Mas **jitter só adiciona
atraso** — um pacote não pode chegar antes do que o caminho permite — então o
**mínimo** do trânsito numa janela aproxima o atraso real do caminho, e a
tendência do mínimo é deriva, não jitter. Média seguiria a média da distribuição
de jitter, que se move por razões que nada têm a ver com relógios.

Há teste exigindo que `wifi`, `regional` e `mobile_poor` com relógio perfeito
leiam menos de 25 ppm de deriva fantasma.

### Como é corrigida

Razão a alguns partes por milhão de 1,0, aplicada via `RateConverter::adjust_ratio`
— o gancho criado em M1.4 justamente para isto. Precisou de
`RateConverter::new_adjustable`, porque o atalho de passthrough a 48 kHz não tem
filtro para dirigir.

A alternativa — descartar ou inserir quadros inteiros — custa um estalo de 20 ms
por correção. Reamostrar a 100 ppm é uma mudança de afinação de cerca de 0,002 de
semitom, que ninguém ouve.

Banda morta de 10 ppm e teto de 500 ppm: abaixo da banda morta o buffer absorve
sem notar, e acima do teto não é cristal — é emissor reiniciando relógio, e
dirigir forte naquilo piora.


---

## M1.11 — pipeline completo sobre UDP

`spikes/voice-link` (descartável). Dois processos na mesma máquina, tom sintético
de 440 Hz no lugar do microfone, ambos surdos para não haver microfonia:

```
spike-voice-link --listen 127.0.0.1:9001 --peer 127.0.0.1:9002 --ssrc 101 --tone --deafen
spike-voice-link --listen 127.0.0.1:9002 --peer 127.0.0.1:9001 --ssrc 202 --tone --deafen
```

Quinze segundos, cada lado:

| | valor |
|---|---|
| quadros enviados | 751 (50/s × 15 s = 750 ✓) |
| quadros recebidos e tocados | 750 |
| perda | **0,00 %** |
| ocultados | 0 |
| overruns de captura | 0 |
| profundidade do buffer | 20,0 ms (piso) |
| deriva medida | 0,0 ppm |

O caminho inteiro está provado: captura → ring → resample → gate → Opus →
cabeçalho de mídia → UDP → jitter buffer → decode → mixer → resample → ring →
reprodução.

**O alvo do jitter buffer encolhendo em tempo real:** 32,1 → 27,4 → 24,5 ms ao
longo dos quinze segundos. É a assimetria de `specs/03-audio.md` — cresce rápido,
encolhe devagar — visível fora do teste unitário.

### O que ainda não está provado

- **Duas máquinas de verdade.** Localhost não tem jitter, perda nem deriva de
  clock: os 0,0 ppm são o mesmo cristal medindo a si mesmo. Precisa dos rigs.
- **Áudio inteligível.** O tom prova que os bytes trafegam e o pipeline não os
  corrompe; não prova que a voz sai compreensível. É julgamento humano, e é
  `M1.16`.
- **Latência boca-a-ouvido ponta a ponta.** Precisa das duas máquinas e do rig
  acústico de `M1.2`.

### Nota sobre o cabeçalho de mídia

O harness é descartável, mas o **cabeçalho não é**: `specs/01-arquitetura.md`
torna `magi-proto` dono de todo byte que cruza a rede. Ele foi para
`crates/magi-proto/src/media.rs`, com 11 testes, layout fixado byte a byte
(round-trip sozinho passaria com os campos trocados) e alvo de fuzzing próprio:
**70,8 milhões de execuções, zero crashes**, 7 casos novos no corpus.

Byte order é big-endian, que `specs/02-protocolo.md` não especifica. Escolhido
porque é o que todo protocolo de mídia em tempo real usa, então uma captura
aberta no Wireshark lê do jeito que um engenheiro espera. **Registrar em `02`.**


---

## M2.1 — o fuzzer achou um bug na primeira execução

`cargo +nightly fuzz run control_frame`

Sessenta segundos de fuzzing no parser de controle produziram um crash. O caso:

```
01 08 01 02 04 01 3a 4c ff ff ff ff 0b 3d 00 0b
```

Versão 1, variante 8 de `ServerMessage` = `Telemetry`. Os bytes `3a 4c ff ff`
lidos como `f32` little-endian são **NaN**.

### Por que isso importa mais do que parece

`specs/02-protocolo.md` deriva a Taxa de Sincronização de RTT, jitter e perda. Um
NaN em qualquer um propaga para a métrica que `specs/07-tema-evangelion.md` chama
de elemento assinatura do produto.

E o modo de falha é silencioso: **toda comparação contra NaN é falsa**. A lógica
de faixas de `07` — `≥ 90` nominal, `70–89` aceitável, `40–69` degradado — não
daria erro, ela cairia no ramo final e pintaria a cor errada com confiança.

### A correção

Rejeitar não-finitos e valores fora de faixa, nas duas direções — codificação e
decodificação, porque um par hostil monta o frame na mão e pula o encoder:

- `rtt_ms` e `jitter_ms`: finitos, entre 0 e 10 000 ms
- `loss_fraction`: finito, entre 0 e 1
- `sync_ratio`: `u8` cabe 255, mas `02` define escala 0–100

Seis testes cobrem cada caso, incluindo o do par hostil.

### Depois da correção

29,4 milhões de execuções, zero crashes, 5 758 casos novos no corpus.

O aceite de M2 em `specs/09-roadmap.md` pede "fuzzing do parser sem crash". Está
cumprido — e o valor real não foi o carimbo, foi o bug encontrado em sessenta
segundos que nenhuma revisão de código teria pego.


---

## M2 — aceite cumprido

`specs/09-roadmap.md`, aceite de M2:

> Três clientes entram no mesmo Cage e conversam por voz através do servidor.
> Cliente sem permissão é rejeitado. Fuzzing do parser sem crash.

`cargo test --package magi-conformance` — oito testes, todos passando, em
processo e em porta efêmera. Roda em CI numa máquina sem placa de som e sem
segunda máquina, que é por que `magi-server` é biblioteca além de binário.

| teste | o que prova |
|---|---|
| `three_clients_in_one_cage_hear_each_other` | o critério principal, com payload byte a byte idêntico e sem eco de volta ao falante |
| `a_client_without_permission_is_refused` | Observador entra e ouve, mas não é encaminhado |
| `a_forged_ssrc_is_refused` | lacuna G2: Shinji com o `ssrc` de Ayanami é recusado, e o dele próprio continua passando |
| `media_before_entering_a_cage_goes_nowhere` | conexão autenticada mas sem plug inserido não alcança o Cage |
| `the_first_connection_pins_the_certificate` | TOFU do ADR 0003 |
| `a_second_connection_reuses_the_pin` | o pin persiste entre conexões |
| `a_ping_comes_back_as_a_pong` | base do RTT da Taxa de Sincronização |
| `the_session_names_the_dogma_and_its_cage` | a `Sessao` traz a árvore de que a casca desenha a primeira tela |

Fuzzing: 29,4 milhões de execuções no parser de controle e 70,8 milhões no de
mídia, zero crashes — depois de o primeiro achar o bug de NaN.

### `magid` rodando de verdade

```
magid listening on 127.0.0.1:8383
certificate fingerprint: d3acf4ac8ba8922d7a150ab28f375c95654a1caea42d287035eee53276e91778
```

Uma porta UDP, TLS 1.3 obrigatório, sem caminho em claro.

### Um crate novo, e por quê

O teste de aceite precisa das duas pontas, e o ADR 0002 proíbe tanto
`magi-server` quanto `magi-core` de depender do outro — com razão: o daemon não
pode linkar o cliente, e o cliente não pode linkar o daemon.

Em vez de abrir um buraco na regra, criei **`magi-conformance`**: biblioteca
vazia, todas as dependências são de desenvolvimento, e o guarda de dependência
tem uma entrada explícita para ele com dois testes garantindo que a exceção **não
vaza** para nenhum outro crate.
