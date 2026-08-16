# SEELE · marca reduzida

Arquivos para trocar a marca reduzida dentro do app. Nada aqui tem raio de canto,
sombra, gradiente ou filtro — a profundidade é sempre cor plana deslocada.

## Faixas de tamanho

A marca troca de desenho conforme o tamanho, nunca só reduz. Cada faixa vem nas duas
medidas: altura da PEÇA (marca usada solta) e lado do BLOCO (marca dentro de um ícone).
A peça ocupa 72% do bloco.

| faixa         | peça      | bloco     | arquivo solto                  | arquivo de ícone     |
|---------------|-----------|-----------|--------------------------------|----------------------|
| 4 placas      | 92+       | 128+      | seele-reduzida-4placas.svg     | icone-app-128.svg    |
| 2 placas      | 92–46     | 128–64    | seele-reduzida-2placas.svg     | icone-app-64.svg     |
| 1 placa       | 46–23     | 64–32     | seele-reduzida-1placa.svg      | icone-app-32.svg     |
| muda          | < 23      | < 32      | seele-muda.svg                 | icone-app-16.svg     |

## Onde usar

- **solto** (laranja sobre preto): dentro da interface — avatar de Dogma Central,
  cabeçalho, indicador de plug. Área livre ao redor = metade da largura da peça.
- **ícone de app** (placa laranja, peça em preto): dock, lançador, aba. Quadrado de
  canto reto; macOS e Windows não recortam, entregue a forma como está.
  macOS .icns 1024/512/256/128/32/16 · Windows .ico 256/48/32/16 · Linux PNG 512→16.
  Cada tamanho usa o desenho da sua faixa, não a redução do maior.
- **bandeja do sistema**: monocromático, herda a cor do tema via currentColor.
  icone-bandeja.svg = plug inserido (barra cheia).
  icone-bandeja-ejetado.svg = plug ejetado (barra vazia).

## O nome

No ícone a cinta vai vazia: em 128px cada katakana teria seis pixels de largura.
O nome fica no boot, na assinatura e na cartela — não na marca reduzida pequena.

## Cores

laranja  #F2521F   marca e acento institucional
negro    #050403   fundo e contra-cor
placas   #A83A10 · #7A2A0B · #4A1806   (escalonamento sobre preto)
placas   #FFA070 · #C4400F · #8E2A08   (escalonamento sobre laranja)

Vermelho nunca — é reservado a estado de falha.
