# SEELE · o símbolo, e como ele é construído

Material de referência. **Nada aqui é servido, compilado ou lido em tempo de
execução** — a arte que o produto usa está em `design/marca/`, e o normativo é
`docs/marca.md`.

Esta pasta guardava a exportação do desenhista da marca velha: quatro arquivos
`seele-reduzida-*.svg` e um `seele-muda.svg`, todos com a silhueta do plug de
entrada. **Saíram com o [ADR 0034](../../../docs/adr/0034-a-marca-abandona-as-duas-citacoes-do-anime.md)**,
que abandonou as duas citações diretas do anime. Guardar a origem de uma marca
que não existe mais não é histórico, é a próxima pessoa desenhando a partir dela.

## O símbolo

Dois nós e uma ligação: **o cheio é quem hospeda, o vazio é quem chega, a
diagonal é o enlace.**

`construcao.svg` é o painel de construção — a grade, a área de respiro e o eixo
de 45°. Os números:

| Parte | Medida na grade de 96 |
|---|---|
| caixa | 96 × 96 |
| unidade de traço e respiro | 4 |
| nó cheio | 24 × 24, em 12,12 |
| nó vazio | 20 × 20 em 62,62, traço 4 centrado |
| extensão externa do nó vazio | 24 — **igual à do cheio** |
| enlace | de 34,34 a 62,62, traço 4, a 45° |
| área de respiro | 24 em toda a volta, um nó cheio |

As massas iguais são a regra que sustenta o resto: quem hospeda e quem chega
pesam o mesmo, e o que os separa é o furo, não o tamanho. Engrossar o traço do
nó vazio fecha o furo — **nunca** cresce o nó.

O enlace é desenhado antes dos nós. As pontas dele entram 2 unidades dentro de
cada um e ficam escondidas embaixo; sem essa ordem a linha aparece pingando para
fora do nó cheio.

## Faixas de tamanho

A marca troca de desenho conforme o tamanho, nunca só reduz. **São duas faixas**,
e o corte está em 48 px de bloco:

| faixa | traço | furo do nó vazio | arquivo de ícone | arquivo solto |
|---|---|---|---|---|
| larga (48 px e acima) | 4 | 16 | `icone-app-128.svg` | `simbolo.svg` |
| miúda (abaixo de 48) | 6 | 12 | `icone-app-16.svg` | `muda.svg` |

O traço de 4 vale `lado / 24` px. A 16 px isso é 0,67 px — meio pixel cinza, que
é como um ícone some numa aba. O desenho da faixa miúda vale 1 px inteiro a 16 e
2 px a 32, e o furo nunca cai abaixo de 2 px: fechado, o nó vazio vira o cheio, e
a marca deixa de dizer que são dois papéis diferentes.

Eram **quatro** faixas quando a marca era o plug, que tinha contorno de octógono,
cinta e placas de profundidade quebrando em quatro pontos diferentes. Um valor de
traço, um limiar.

## Onde usar

- **solto** (laranja e osso sobre negro): dentro da interface — avatar de
  servidor, cabeçalho, indicador de conexão. Área livre = um nó cheio.
- **ícone de app** (placa laranja, marca em negro): dock, lançador, aba. Canto
  reto, com uma exceção nomeada — a superelipse do `.icns`, e o porquê está na
  regra 1 de `docs/marca.md`.
  macOS `.icns` 1024/512/256/128/32/16 · Windows `.ico` 256/128/64/48/32/16 ·
  Linux PNG 512→32.
- **bandeja do sistema**: monocromático, herda a cor do tema via `currentColor`.

### Os dois de bandeja não têm implementação

`icone-bandeja.svg` (com enlace) e `icone-bandeja-sem-enlace.svg` (queda: a
diagonal some, os dois nós ficam) são os dois estados que a bandeja precisaria
distinguir. `docs/marca.md` cita a bandeja como uso da forma muda, mas o produto
ainda não põe ícone em bandeja nenhuma. Estão guardados porque, no dia em que
puser, o desenho já existe.

O par sucede `icone-bandeja.svg` / `icone-bandeja-ejetado.svg`, que eram plug
inserido e plug ejetado. **Inserir e ejetar era vocabulário**, e saiu no
[ADR 0033](../../../docs/adr/0033-o-vocabulario-sai-da-interface-a-estetica-fica.md);
o que restou é o que o ícone de fato mostra — se há enlace ou não.

## Cores

```
laranja  #F2521F   os dois nós
osso     #EAE3CF   o enlace e o nome
negro    #050403   fundo e contra-cor
apagado  #7A7061   tagline, e as guias do painel de construção
```

Vermelho nunca — é reservado a estado de falha, inclusive na queda.

As seis placas de profundidade (`#A83A10`, `#7A2A0B`, `#4A1806`, `#FFA070`,
`#C4400F`, `#8E2A08`) eram do plug: cor plana deslocada por trás de um contorno
de octógono, para dar volume sem sombra. O símbolo não tem contorno para
deslocar, e elas saíram da marca junto com ele.
