# spike `tela-na-cor` — a conversão BGRA→I420 do Windows vale uma biblioteca?

**Descartável.** Existe para responder uma pergunta e morre com a resposta.

## A pergunta

O `crates/seele-video/src/captura/windows.rs:557` documenta, medido num Ryzen 7
5800X3D capturando um monitor de 2560×1440:

> 16,5 ms por quadro para 1080p — **0,50 de um núcleo a 30 fps** — contra
> **0,105** que o OpenH264 gasta codificando. Converter custa cinco vezes
> codificar.

E conclui que baixar isso «é mudança de desenho — converter na GPU, ou converter
só as regiões sujas —, não ajuste».

Antes de mexer em GPU: uma biblioteca com SIMD resolve?

## A descoberta que reordenou tudo

**A conversão de cor nunca foi o custo.**

O nosso `converter()` faz duas coisas no mesmo laço: reescala 2560×1440 → 1080p
por média de área **e** converte BGRA em I420. Medindo as duas separado:

| | mediana | de núcleo a 30 fps |
|---|---|---|
| nosso, como está (escala + cor) | **8,03 ms** | 0,241 |
| só a cor, pela `dcv-color-primitives`, em 1080p | 0,51 ms | 0,015 |
| só a cor, pela `dcv`, em 1440p | 0,91 ms | 0,027 |
| só a escala, laço nosso | 5,72 ms | 0,172 |
| só a escala, pela `fast_image_resize` (filtro caixa) | **1,78 ms** | 0,053 |

A cor é **6%** do custo da função. A escala é 94%.

Trocar só a cor pela `dcv` — que era a proposta — daria **1,2×**. Trocar as duas
dá **3,0×**: 8,03 ms → 2,69 ms.

Transposto para o número do Ryzen: **16,5 ms → ~5,5 ms**, ou 0,50 → ~0,17 de
núcleo. E isso reabre os 60 fps, que a 16,5 ms eram aritmeticamente impossíveis
(dois quadros custariam 33 ms num orçamento de 16,6).

## Como terminou, medido no alvo

A troca foi feita, e o número real saiu no **Ryzen 7 5800X3D** — a mesma
máquina dos 16,5 ms originais, por SSH:

| 1440p → 1080p | antes | depois | ganho |
|---|---|---|---|
| Ryzen 7 5800X3D | 17,69 ms | **7,42 ms** | **2,4×** |
| M5 Pro (esta bancada) | 8,03 ms | 2,89 ms | 2,8× |

Duas coisas que só o alvo podia dizer: o ganho é **menor** lá que aqui, e a
previsão feita nesta máquina («~6 ms no Ryzen») errou para melhor. Os 17,69 ms
confirmam os 16,5 documentados de outra bancada.

## O que esta máquina prova e o que não prova

Rodou num **Apple M5 Pro (arm64)**; o alvo é **Windows x86_64**.

A `dcv` **tem NEON** — ela mesma reporta `instruction-set:Neon`, o que a
documentação não dizia. Então isto mediu o caminho acelerado dela, e não o
escalar. A razão deve transferir; os milissegundos absolutos não.

## A ressalva que precisa de olho humano

A reescala da `fast_image_resize` **não é idêntica à nossa**: diferença média
abaixo de 1,2 nível em 255, mas pior pixel de 24 (luma) a 42 (croma).

Hipótese testada e **descartada**: não é o ruído do conteúdo sintético —
desligar o ruído não mudou os números (0,744 → 0,781 na luma).

A causa é estrutural. O nosso `faixas()` faz balde inteiro — cada pixel de
origem pertence inteiro a um pixel de destino. O filtro caixa da biblioteca pesa
cobertura fracionária. Em 2560→1920 cada destino cobre 1,333 origens, então os
dois genuinamente discordam nas bordas de bloco, e o da biblioteca é o
matematicamente correto.

**Isso é mudança de reamostragem, não defeito** — mas o argumento original de
usar média de área era legibilidade de texto reduzido, e trocar o filtro exige
olhar texto de verdade numa tela de verdade antes de aceitar.

## Custo de dependência

Medido isolado, aqui, as duas somam ~27 pacotes. **No SEELE são 4**, porque
quase toda a árvore delas já estava na do projeto: entram elas duas, `pastey` e
`spin`, mais uma segunda versão de `itertools`.

- `dcv-color-primitives` 1.0.0, MIT-0 — precisou de uma linha nova no
  `deny.toml`; MIT-0 é MIT sem a cláusula de atribuição, ou seja mais
  permissiva que a que já era aceita.
- `fast_image_resize` 6.1.0, MIT OR Apache-2.0 — já permitida.

## Como rodar

```sh
cargo run --release            # conteúdo com ruído
SEM_RUIDO=1 cargo run --release
```
