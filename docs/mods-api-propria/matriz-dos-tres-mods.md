# A matriz dos três MODs oficiais

O que cada um fazia na API 2, o que sobrou depois da migração para a API 3, e o
que está de pé agora. **Cada linha diz implementado e exercitado, ou pendente
concreto** — e «o aviso de indisponibilidade saiu da tela» não conta como
recuperação de função. Onde há prova, ela é nomeada.

A coluna «exercitado» aponta o teste que roda o comportamento, não um guarda que
confere que a linha existe.

## ESTILO — 14 provas

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
| Arredondamento e brilho | sim | não | **recusado por desenho**, com a razão dita | `o_que_a_marca_proibe_e_recusado_com_a_razao` |

**Fechado como recusa, e não como pendência.** `docs/marca.md` proíbe «sombra,
gradiente, contorno extra, raio» — a palavra que ele usa é «nunca». A API de tema
recusa `arredondamento` e `brilho` **pelo nome**, com a razão e a citação da
marca, antes da recusa genérica de «a API de tema não conhece»: quem pede
descobre que a resposta é não, e por quê, em vez de descobrir que o nome não
existe.

O servidor continua guardando os dois, e o ESTILO continua devolvendo-os
intactos. Quando há algo guardado, ele escreve uma linha sobre **os dados de
quem está ali** — «guardado neste servidor e não desenhado aqui» — e não um
aviso de indisponibilidade. Zerá-los seria apagar a escolha de outra pessoa por
não saber mostrá-la.

## PERFIS — 31 provas

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
| Marca na lista de pessoas do produto | sim | não | sim, como dado que o produto desenha | `o pronome de quem o escreveu aparece na lista` |

**Fechado sem reabrir a janela.** O que a API 2 chamava de «cartão» era o MOD
desenhando dentro da lista do produto, e isso continua fora: o ADR 0049 tirou o
MOD da janela e nada aqui o traz de volta. O que existe agora é o inverso — o MOD
**entrega dado** e o produto desenha.

Uma marca é um texto de até 24 caracteres e uma cor `#rrggbb`, por `id` de
pessoa, até 128 pessoas por MOD. Quem escolhe posição, tamanho, tipografia e
vizinho é `linhaDoRoster`, em `tela-sessao.js`. A cor pinta **o contorno** e
nunca o texto: no texto ela atropelaria o contraste que aquela tela mede. E a
marca sai junto com o MOD — um selo de um MOD que não está mais de pé é uma
informação que ninguém pode corrigir nem tirar.

O PERFIS usa isso para o **pronome**, e não para o nome exibido: a lista já
escreve um nome, e dois nomes na mesma linha é a linha dizendo duas coisas. Quem
não escreveu pronome não ganha selo nenhum.

Os limites, a recusa inteira e o descarte rodam em
`apps/seele-app/bancada/marcas-na-lista.cjs`, contra o código de `base.js`. Que o
desenho continua sendo do produto é guardado por
`a_marca_de_um_mod_e_dado_e_quem_desenha_e_o_produto`.

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

**Nenhuma linha está pendente.** A única que não é «sim» é a recusa do ESTILO, e
ela é uma decisão da marca escrita antes destes MODs existirem: aparece como
recusa dita — com razão e citação — e não como função que não veio.

O harness dos três MODs é um arquivo só, igual nos três repositórios, que se
ramifica pelo `id` do manifesto. Ele passou a guardar **o que o MOD pediu** além
do que o produto aceitou: a primeira reversão da marca do PERFIS passou porque o
teste media a peneira do produto, e não a regra do MOD — um MOD que mandasse um
selo vazio para cada pessoa teria passado escondido atrás dela.
