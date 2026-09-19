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
| Arredondamento e brilho | sim | não | **pendente** — não há token no produto | — |

**Pendente concreto:** arredondamento (`radius`) e brilho (`glow`). O servidor os
guarda e este MOD os devolve intactos; aplicá-los exige tokens que `tokens.css`
não tem. Não é ausência de API de MOD: é ausência de régua no produto, e criar
uma a partir de um MOD é a ordem errada.

## PERFIS — 28 provas

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
| Cartão na lista de pessoas do produto | sim | não | **pendente** — a região é o lugar de um MOD | — |

**Pendente concreto:** decorar a lista de pessoas do produto. Um MOD desenha na
região dele, e alcançar outras superfícies da janela é a decisão que o ADR 0049
tomou ao contrário — o MOD saiu da janela. Reabri-la é um ADR, não uma função
que falta implementar.

## MESA — 37 provas

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
| Ficha: magias preparadas e espaços por nível | sim | leitura | **pendente concreto** | — |
| Ficha: ações com fórmula, usos e recuperação | sim | leitura | **pendente concreto** | — |
| Compêndio: editar verbete existente, publicar | sim | leitura | **pendente concreto** | — |
| Cena: redimensionar grade, descrição, notas do GM | sim | leitura | **pendente concreto** | — |

**Pintar parede é um modo, e não adivinhação.** A mesma tela serve para arrastar
peça e para trocar parede, e um botão diz qual dos dois está ligado. Decidir pela
figura sob o dedo seria errado no caso que importa: quem pinta uma parede quer
pintá-la **onde há peça** também.

**Os quatro pendentes que sobram são de cliente, não de API.** Cada um tem
operação no servidor e forma que os atende — `campo`, `escolha`, `botao`. O que
falta é a tela, e o custo é tamanho. Estão nomeados um a um em vez de resumidos
como «gestão avançada», que é a forma de esconder uma lista dentro de uma
palavra.

## O que vale para os três

Tudo o que está marcado «sim · agora» roda contra o servidor de verdade de cada
MOD, no harness que executa as duas metades. Nenhuma linha desta tabela foi
marcada por um aviso ter sumido da tela.
