# A matriz dos três MODs oficiais

O que cada um fazia na API 2, o que sobrou depois da migração para a API 3, e o
que está de pé agora. **Cada linha diz implementado e exercitado, ou pendente
concreto** — e «o aviso de indisponibilidade saiu da tela» não conta como
recuperação de função. Onde há prova, ela é nomeada.

A coluna «exercitado» aponta o teste que roda o comportamento, não um guarda que
confere que a linha existe.

## ESTILO — 15 provas

| Comportamento | API 2 | Depois da migração | Agora | Exercitado por |
| --- | --- | --- | --- | --- |
| Ver o tema do servidor | sim | sim | sim | `os seis tokens, a densidade e a fonte` |
| Aplicar o tema na sessão | sim | 4 de 6 cores | 6 cores | idem |
| Editar cada cor | sim | **não** | sim | `quem administra edita e grava` |
| Densidade | sim | **não** | sim | idem |
| Família de tipo | sim | **não** | sim, entre as duas pilhas do produto | idem |
| Gravar no servidor | sim | **não** | sim | idem |
| Restaurar o padrão | sim | **não** | sim | `reset libera a camada` |
| Recusa de contraste dita na tela | sim | **não** | sim | `a recusa do servidor vira frase` |
| Só quem administra edita | sim | n/a | sim | `quem não administra vê o tema` |
| Preservar o que este MOD não edita | n/a | n/a | sim | `quem administra edita e grava` |
| Arredondamento e brilho | sim | não | sim, editados e aplicados | `arredondamento e brilho se escolhem, gravam e aplicam` |

**Implementados, e não recusados.** Uma versão anterior desta matriz os deu por
fechados como «recusa por desenho», citando `docs/marca.md`. A citação estava
errada: aquele documento diz de si mesmo que governa a imagem do produto e que
**nada nele alcança a estética**, e a regra que eu citei é sobre o símbolo. Quem
governa o recuo da interface é `specs/07-estetica.md`.

`--seele-raio` e `--seele-sombra` já existiam em `tokens.css`, valendo `0` e
`none`, lidos por três folhas. O que faltava era alguém poder escrevê-los.

O produto continua abrindo em canto reto e sem sombra — é a estética dele —, e
`specs/07-estetica.md` passou a nomear a exceção: um tema de servidor levanta os
dois, **só naquela sessão**, e eles saem com ela. `arredondamento` é inteiro de
0 a 24; `brilho` é sim ou não, e a sombra que ele liga é montada pelo produto a
partir de um token do produto. O ESTILO manda número e booleano, e nunca CSS.

## PERFIS — 32 provas

| Comportamento | API 2 | Depois da migração | Agora | Exercitado por |
| --- | --- | --- | --- | --- |
| Listar quem está, por ID | sim | sim | sim | `a lista traz cada pessoa por ID` |
| Ver a ficha de outra pessoa | sim | sim (tudo junto) | sim (ficha própria) | idem |
| Editar a própria ficha | sim | **não** | sim | `editar e gravar muda o perfil` |
| Efeito (aurora, brilho, pulso) | sim | **não** | sim, escolhido e gravado | idem |
| Cor de destaque | sim | **não** | sim | idem |
| Ver retrato e faixa | sim | **não** | sim | `a imagem vem do servidor deste MOD` |
| **Enviar** retrato e faixa | sim | **não** | sim | `a pessoa escolhe uma imagem e ela chega inteira` |
| Remover retrato e faixa | sim | **não** | sim | `editar e gravar` (botões) |
| Recusa do servidor dita na tela | sim | **não** | sim | `editar e gravar muda o perfil` |
| Consulta em lotes de 32 | sim | sim | sim | `consulta pessoas em lotes` |
| Cartão na lista de pessoas do produto | sim | não | sim, declarado e montado pelo renderer do produto | `o cartão de quem escreveu algo aparece na lista` |

**O cartão de verdade, e sem reabrir a janela.** Uma versão anterior fechou esta
linha com uma **etiqueta** — um texto curto e uma cor. Era menos do que a API 2
tinha, e chamar isso de cartão era a mesma troca de nome que esta matriz existe
para não deixar passar.

O que existe agora é o cartão: retrato, nome exibido, pronome e status, por
pessoa. E ele não devolve a janela ao MOD, porque **o renderer é o mesmo** — a
declaração é a da região, e quem a monta é `planejar`/`reconciliar` com
`createElement` e `textContent`.

O que muda em relação à região são duas coisas. A **gramática é menor**: nada de
`campo`, `escolha`, `botao`, `arquivo` ou `tela`, porque a linha do roster já tem
um botão do produto e dividir foco e área de toque com um terceiro é
indepurável. E os **tetos são próprios**: 64 pessoas, 24 nós por cartão, 4
níveis, com contador e orçamento de mídia separados dos da região.

O retrato vem do servidor deste MOD, pela mesma origem da ficha — nunca de um
endereço. O tamanho dele é da folha do produto. E o cartão sai com a região: é a
mesma `soltar` que para o som.

Os limites, a gramática, a reconciliação e o descarte rodam em
`apps/seele-app/bancada/regiao-do-mod.cjs`, contra o código de verdade num DOM
mínimo. Que o desenho continua sendo do produto é guardado por
`o_cartao_de_um_mod_e_declarado_e_quem_desenha_e_o_produto`.

## MESA — 41 provas

| Comportamento | API 2 | Depois da migração | Agora | Exercitado por |
| --- | --- | --- | --- | --- |
| Ver a campanha | sim | sim | sim | `o tabuleiro é figura declarada` |
| **Criar** a campanha | sim | **não** | sim | `a mesa se cria daqui` |
| Criar cena | sim | **não** | sim | idem |
| Trocar a cena em cima da mesa | sim | **não** | sim | idem (escolha) |
| **Enviar** o mapa de uma cena | sim | **não** | sim | `o mapa escolhido chega inteiro` |
| Ver o mapa | sim | **não** | sim | idem |
| Tabuleiro desenhado, com grade | sim | **não** (era lista de texto) | sim | `o tabuleiro é figura declarada` |
| Pôr peça | sim | **não** | sim | `a mesa se cria daqui` |
| **Arrastar** peça | sim | **não** | sim | `o tabuleiro é figura declarada` |
| Recusa de movimento dita | sim | **não** | sim | `a recusa do servidor devolve a peça` |
| Criar ficha | sim | **não** | sim | `a mesa se cria daqui` |
| Ferir e curar | sim | **não** | sim | `a ficha aberta fere, cura` |
| Condições | sim | **não** | sim (atordoado) | idem |
| Ordem de iniciativa: pôr | sim | **não** | sim | idem |
| Ordem de iniciativa: passar turno, limpar | sim | **não** | sim | `a mesa se cria daqui` |
| Rolar dados | sim | **não** | sim | `rolar dados vai ao servidor` |
| Ver fichas, compêndio e registro | sim | sim | sim | `o tabuleiro é figura declarada` |
| Paredes desenhadas | sim | **não** | sim (figura) | `o tabuleiro é figura declarada` |
| Editar paredes | sim | **não** | sim, num modo próprio da tela | `pintar parede troca a casa` |
| Editar compêndio (`entry-save`) | sim | **não** | sim (criar verbete) | `trilha da mesa e da cena, e verbete` |
| Ficha: nome, classe, nível, ancestralidade, antecedentes | sim | leitura | sim | `a ficha inteira se edita e grava` |
| Ficha: PV, PV máximo, CA, deslocamento | sim | leitura | sim | idem |
| Ficha: os seis atributos | sim | leitura | sim | idem |
| Ficha: inventário, perícias, notas | sim | leitura | sim | idem |
| Retrato de personagem | sim | **não** | sim, enviado e mostrado | `o retrato de uma ficha é enviado` |
| Trilha da mesa e da cena | sim | **não** | sim | `trilha da mesa e da cena` |
| Ficha: magias preparadas e espaços por nível | sim | leitura | sim | `espaços de magia, preparar e conjurar gastam o espaço` |
| Ficha: ações com fórmula, usos e recuperação | sim | leitura | sim | `ações com fórmula, usos e recuperação se criam, usam e saem` |
| Compêndio: editar verbete existente, publicar | sim | leitura | sim | `um verbete se edita e se publica` |
| Cena: redimensionar grade, descrição, notas do GM | sim | leitura | sim | `a cena se ajusta — grade, descrição e notas do GM` |

**Pintar parede é um modo, e não adivinhação.** A mesma tela serve para arrastar
peça e para trocar parede, e um botão diz qual dos dois está ligado. Decidir pela
figura sob o dedo seria errado no caso que importa: quem pinta uma parede quer
pintá-la **onde há peça** também.

**Os quatro que faltavam eram de cliente, e viraram tela.** Cada um já tinha
operação no servidor e forma que o atendia — `campo`, `escolha`, `botao` —, e o
que faltava era desenhá-los. Nenhum exigiu API nova. Estavam nomeados um a um em
vez de resumidos como «gestão avançada», que é a forma de esconder uma lista
dentro de uma palavra; é por estarem nomeados que se pôde fechar cada um e dizer
qual teste o roda.

## O que vale para os três

Tudo o que está marcado «sim · agora» roda contra o servidor de verdade de cada
MOD, no harness que executa as duas metades. Nenhuma linha desta tabela foi
marcada por um aviso ter sumido da tela.

**Nenhuma linha está pendente, e nenhuma está fechada por recusa ou por
etiqueta.** As duas que estavam — arredondamento/brilho e o cartão — foram
implementadas. A lição das duas é a mesma: fechar uma linha trocando o que ela
pede por algo menor, e dar outro nome ao que sobrou, é a forma de a matriz
deixar de medir o que ela existe para medir.

O harness dos três MODs é um arquivo só, igual nos três repositórios, que se
ramifica pelo `id` do manifesto. Ele passou a guardar **o que o MOD pediu** além
do que o produto aceitou: a primeira reversão da marca do PERFIS passou porque o
teste media a peneira do produto, e não a regra do MOD — um MOD que mandasse um
selo vazio para cada pessoa teria passado escondido atrás dela.
