# Batida de teclado na supressão de ruído: o filtro de hoje contra o RNNoise

**Data:** 2026-11-11 · **Árvore:** `entrega/1.0`
**Por quê:** relato do dono: quem ouve ainda escuta as batidas do teclado.
**O que é:** uma medida em **sinal sintético**, e não uma avaliação com voz e teclado de verdade.

```sh
cargo run --release -p seele-audio --example teclado_na_supressao
```

O sinal: piso de sala a −55 dBFS e, a cada 180 ms, um estalo de banda larga a −18 dBFS com queda exponencial. A «voz» é uma série de harmônicos de 140 Hz com envelope de sílaba. O RNNoise é o `nnnoiseless` 0.5.2 (Rust puro, BSD-3-Clause), que entra **só como dependência de teste**.

| | estalo (dB abaixo da entrada) | sala entre estalos | voz que sobra |
|---|--:|--:|--:|
| supressão de hoje, força 1,0 | 0,6 | 12,1 | 81% |
| RNNoise | 8,5 | 8,0 | 100% |
| RNNoise e depois a de hoje | 8,6 | 8,2 | 85% |

- **O RNNoise atrasa 10 ms**, o mesmo que a supressão de hoje, e custa 0,3% de um núcleo.
- **A chance de voz que ele devolve não separa o estalo sintético da voz**: mediana 0,68 e p95 0,96 só com teclado, contra 1,00 com voz. Um portão de voz guiado por ela abriria com o teclado.

## O que mostram

1. A supressão de hoje **não atenua a batida** (0,6 dB). É o limite do método, que o cabeçalho de `supressao.rs` já previa: ela aprende ruído que fica, e a batida é um transiente.
2. O RNNoise atenua o estalo em 8,5 dB: fica em cerca de um terço da amplitude. **Reduz, não remove.**
3. Pôr os dois em série não soma.

## O que NÃO mostram

Um estalo de ruído branco não é uma tecla, e uma série de harmônicos não é fala: o RNNoise foi treinado com gravações, e o que ele faz com uma tecla de verdade pode ser melhor ou pior que isto. A decisão de trocar o filtro do produto pede uma gravação real (fala e digitação no mesmo microfone), passada pelos candidatos.
