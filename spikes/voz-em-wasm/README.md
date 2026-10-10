# A voz do desktop no navegador (Tarefa 3.3 da F4)

**A pergunta.** O ADR 0057 manda que o JavaScript da voz só mova bytes opacos,
e deixa em aberto onde rodam o portão, o buffer de jitter e o mixer: uma cópia
em JavaScript só como exceção nomeada, com data para sair. No desktop eles
moram no `seele-audio`, que puxa o cpal e a libopus em C e não compila para o
navegador. Dá para levar os três ao navegador, e eles rodam dentro de um
`AudioWorklet`, que é onde o buffer de jitter e o mixer têm de rodar (o
`process()` é o relógio da voz, e nada da voz depende de temporizador)?

## O que este spike roda

O `gate.rs`, o `jitter.rs` e o `mixer.rs` de verdade, lidos do `seele-audio`
pelo caminho (`#[path]`), com as três constantes de que precisam
(`SAMPLE_RATE_HZ`, `FRAME_MS`, `FRAME_SAMPLES`). O `quadro()` faz o que o
desktop faz a cada 20 ms: o portão sobre a captura, um buffer de jitter por
fonte (oito) e o mixer das oito.

## Medido em 09/10/2026

- **Compila** para `wasm32-unknown-unknown` em **90 KB, sem importação
  nenhuma**. O `netsim.rs`, que os testes do buffer de jitter usam, também não
  tem dependência.
- **No Node (V8):** um quadro de 20 ms custa 5,5 µs com uma fonte, 7,2 µs com
  oito e 13,6 µs com 32 (0,04% a 0,07% de um núcleo).
- **Dentro de um `AudioWorklet`**, num `OfflineAudioContext` (que roda sem
  gesto, e por isso também no Simulador), 10 s de áudio com um quadro a cada
  960 amostras:

  | | Chromium 151 | Safari do Simulador (iOS 27) |
  |---|---|---|
  | o módulo compilado na página, mandado pela porta | **recusado** (`messageerror`) | aceito |
  | os bytes, compilados no worklet (`new WebAssembly.Module`) | aceito | aceito |
  | 2000 quadros cronometrados no worklet | 17 a 20 ms (≈ 9 µs por quadro) | 17 a 19 ms (≈ 9 µs por quadro) |
  | 10 s de áudio, 500 quadros | 26 a 27 ms | 26 ms |

## Os achados

1. **As três peças são puras e cabem no navegador**: dependem só de três
   constantes. O que as prende ao desktop é o crate em que moram, e não o
   código. O caminho é o do `seele-sessao`: um crate só com elas, reexportado
   pelo `seele-audio`.
2. **O Chromium recusa um `WebAssembly.Module` mandado à porta do worklet**, e
   a recusa chega como `messageerror`: **sem um ouvinte para ele, ela é
   silenciosa**, e foi o que a primeira rodada viu (uma espera sem resposta). O
   caminho que serve aos dois motores é mandar os bytes e compilar lá dentro, e
   o worklet da voz ouve o `messageerror` e o diz.
3. **O mixer guarda os ganhos por SSRC num `HashMap`**, e o SSRC é o servidor
   que dá. No navegador as chaves do hash são fixas (`spikes/snapshot-no-wasm/`),
   e o mixer vai para `BTreeMap`, como os mapas do `Room`.
4. **O custo não pesa**: cerca de 9 µs a cada 20 ms dentro do worklet, nos dois
   motores. O iPhone de verdade continua sem medida (o Simulador usa a CPU do
   Mac), mas a folga é de mais de mil vezes.

## Rodar

    cargo build --release --target wasm32-unknown-unknown
    python3 servir.py chromium
    SEELE_SIMULADOR=<UDID> python3 servir.py simulador
    VIA=bytes python3 servir.py chromium   # só o caminho dos bytes
