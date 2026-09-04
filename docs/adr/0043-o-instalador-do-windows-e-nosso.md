# ADR 0043 — O instalador do Windows é nosso

**Estado:** aceito
**Data:** 2026-09-02

O instalador do Windows deixa de ser o modelo do `tauri-bundler` com páginas
trocadas e passa a ser um programa deste repositório, com janela própria
desenhada em Win32. Ele continua instalando para a máquina, continua sendo o
arquivo que o atualizador roda em silêncio, e assume por escrito tudo o que o
NSIS fazia por baixo.

## Contexto

O instalador é a primeira tela que alguém vê do SEELE, e até aqui ela era a
tela padrão do NSIS: cinza de sistema, tipografia de sistema, e uma sequência de
páginas que não é a do produto. O projeto tem identidade própria — `tokens.css`,
ADR 0014 — e ela parava na porta.

A primeira tentativa foi bifurcar o modelo do `tauri-bundler` e substituir as
páginas por páginas próprias em `nsDialogs`. Ela funcionou no sentido estrito:
compila, roda, e as quatro páginas do desenho aparecem. E não serviu, por três
limites que não são de esforço:

- **A moldura não é nossa.** A barra de título é a do Windows, com o ícone e o
  texto que o NSIS põe. O desenho pede uma barra própria, com a marca.
- **Os botões não são nossos.** `SetCtlColors` não alcança um botão desenhado
  pelo tema do sistema. Pintá-los à mão dentro do NSIS obrigaria a redesenhar
  foco, pressionado e desabilitado — e um botão que não mostra foco é um
  instalador que ninguém navega pelo teclado.
- **A tipografia não é nossa.** A `Saira Condensed` não está instalada em máquina
  nenhuma, e um instalador que instala fonte para desenhar a si mesmo é um
  instalador que suja o sistema antes de perguntar qualquer coisa.

## Decisão

Um programa próprio, em Rust, com janela Win32 desenhada por GDI.

**Sem WebView2, e isto não é preferência.** O desenho é HTML, e a saída óbvia
seria renderizá-lo numa janela WebView2 — o SEELE já depende dele. Mas o
instalador de hoje tem uma seção inteira que **instala o runtime do WebView2
quando ele falta**, e é ela que faz o SEELE abrir numa máquina limpa. Um
instalador que dependesse do WebView2 precisaria do WebView2 para dizer que está
instalando o WebView2.

GDI dá conta porque o desenho colabora: retângulo chapado, borda de 1px, zero
arredondamento, zero sombra, zero gradiente — é o que `tokens.css` já impõe ao
produto inteiro. O que o NSIS não conseguia e isto consegue: a moldura, os
botões e a fonte de verdade, carregada da memória com `AddFontMemResourceEx`,
sem instalar nada.

**Continua instalando para a máquina.** `Program Files`, com elevação. O ADR não
muda isso, e a razão está no `instalador.nsh`: sem elevação não há como criar a
regra de firewall de entrada, e sem ela quem hospeda fica invisível sem
descobrir por quê. O UAC uma vez, no começo, é o preço.

**O instalador não depende do produto.** Nem uma crate. A regra do `check-deps`
o trata como o `xtask`: se o instalador dependesse do `seele-core`, construir o
instalador exigiria construir o produto, e uma mudança no produto poderia
quebrar a instalação de todo mundo por um caminho que ninguém liga aos dois.

## O contrato que ele assume

O que o NSIS fazia, e que agora é obrigação escrita. A lista existe porque o
modo de falhar de um instalador é esquecer um item dela — e o esquecimento só
aparece semanas depois, na máquina de outra pessoa.

| obrigação | o que quebra se faltar |
|---|---|
| copiar os arquivos e escrever o desinstalador | — |
| a entrada em «Aplicativos instalados» | o app não sai mais pelo painel do Windows |
| `EstimatedSize`, `DisplayVersion`, `UninstallString` e os demais valores | a entrada aparece pela metade, sem tamanho nem versão |
| atalhos do menu Iniciar e da área de trabalho | e **migrar** os antigos, que apontam para o nome velho do binário |
| instalar o WebView2 se faltar | o SEELE não abre numa máquina limpa |
| a regra de firewall da 8383, do programa, em rede confiável | quem hospeda fica invisível |
| apagar a instalação por usuário da 0.7.1 | o app «volta de versão» — o atalho velho abre a cópia velha |
| **o modo silencioso** | **ninguém mais recebe atualização** |
| recusar-se a rodar com o app aberto, em arquitetura errada ou Windows velho | arquivo em uso, e uma instalação pela metade |

O modo silencioso é o item que mais assusta e o que menos se vê: ele não tem
tela, ninguém o exercita à mão, e é por onde passa **toda** atualização do
produto. Quem mexer neste instalador mexe nele.

## Consequências

**Ganha-se** a tela que o desenho pede, inteira, e o fim da dívida da
bifurcação: o `instalador.nsi` bifurcado do `tauri-bundler` sai da árvore, e com
ele o imposto de rebifurcar a cada versão do Tauri.

**Perde-se** o que o NSIS dava de graça e agora é nosso para escrever e manter:
a elevação, a compressão do payload, o desinstalador, a detecção de instalação
anterior, e cada um dos valores de registro da tabela acima. São semanas, e o
que substituem funciona hoje.

**O risco concreto** é o modo silencioso quebrar sem ninguém notar: o instalador
novo sai, uma pessoa instala à mão e fica tudo bem, e a atualização automática
para em silêncio para quem já tinha o produto. É por isso que ele precisa de
teste próprio antes de substituir o que existe, e não depois.
