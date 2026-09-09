# spike `mod-em-js` — que interpretador JavaScript cabe num SFU de 1 vCPU?

**Descartável.** Existe para responder uma pergunta e morre com a resposta.

## A pergunta

O [ADR 0044](../../docs/adr/0044-mods-o-produto-base-tem-regras-e-um-mod-nao.md)
decide que um MOD é **JavaScript nos dois lados**, e deixa uma pendência escrita
como pendência em vez de resolvida por intuição:

> Qual interpretador embutir no servidor é a única decisão deste ADR que depende
> de um número que ninguém tem — tamanho de binário, memória por contexto, custo
> de chamada, contra 1 vCPU / 512 MB.

O orçamento não é retórica: `xtask/src/check_deps.rs` proíbe `seele-server` de
depender de `seele-core` e de `seele-audio` **por causa dele**, e escreve o
motivo — *«the server is an SFU… supposed to fit in 1 vCPU / 512 MB»*.

Duas candidatas, e a segunda existia para desafiar a primeira:

- **QuickJS**, pelo binding `rquickjs` — o interpretador em C, pequeno por
  desenho.
- **Boa**, puro Rust — sem toolchain de C, o que importaria para três
  plataformas e um instalador nosso (ADR 0043).

## As medidas

macOS, Apple Silicon, `--release`, `opt-level = 3`. Baseline de binário: um
`main` que só imprime, **430 832 bytes**.

| | QuickJS (`rquickjs` 0.12.2) | Boa (`boa_engine` 0.22.0) |
|---|---|---|
| **delta de binário** | **1 098 KiB** | 12 088 KiB |
| **custo por chamada** (1 M chamadas) | **39 ns** | 73 ns |
| **RSS máximo, 50 contextos** | **5,9 MB** | 24,8 MB |
| subida de 50 contextos | 2,8 ms | 8,1 ms |
| tempo de compilação, do zero | 7,2 s | 27,7 s |
| licença | MIT | Unlicense OR MIT |

**QuickJS ganha em todos os eixos, e não por pouco:** 11× menor no binário, ~2×
mais rápido por chamada, ~4× menos memória com 50 contextos.

## A intuição que estava errada, e é por isso que se mede

A suposição corrente ao escrever o ADR era que **um runtime WASM seria mais leve
que um interpretador JS**, e que embutir JS num SFU seria a dependência cara.
É o contrário: 1 MiB de binário e 5,9 MB de RSS para cinquenta contextos é
menos que qualquer runtime WASM com JIT custaria, e cabe folgado nos 512 MB.

`CLAUDE.md`, sobre este repositório: *«Três vezes uma hipótese confiante custou
mais que a medida teria custado.»*

## A segunda metade: os dois tetos existem?

O ADR promete **teto de tempo e de memória por chamada**, porque um MOD em laço
infinito não pode ser a diferença entre a sala funcionar e não funcionar. Sem
isso, a medida acima não decidiria nada. `src/bin/tetos.rs` confere as quatro
propriedades, e as quatro valem:

| pergunta | resposta |
|---|---|
| `set_memory_limit` corta um MOD que aloca sem parar? | **sim** |
| o contexto sobrevive ao estouro? | **sim** |
| `set_interrupt_handler` corta um `while (true) {}`? | **sim**, em 169 ms com 10 000 passos |
| o contexto sobrevive à interrupção? | **sim** |

«O contexto sobrevive» é a metade que importa para a **falha isolada** do ADR:
um MOD que estoura é desabilitado e a sala continua, em vez de o runtime inteiro
cair junto.

## O que fica escrito como custo

**QuickJS exige um compilador de C em tempo de build.** `rquickjs-sys` compila o
fonte de QuickJS. Nos três alvos isso já existe — Xcode CLT no macOS, MSVC nos
runners de Windows, `cc` no Linux —, mas é uma exigência nova de um projeto que
até aqui compilava só com o Rust. Boa não teria essa.

O número acima diz que o preço vale: 11 MiB de binário a mais, em cada
instalador de cada plataforma, é caro demais para não ter compilador de C.

## Como refazer

```sh
cd spikes/mod-em-js
cargo build --release
./target/release/quickjs 1 1000000
./target/release/boa 1 1000000
/usr/bin/time -l ./target/release/quickjs 50 1000
/usr/bin/time -l ./target/release/boa 50 1000
./target/release/tetos
```
