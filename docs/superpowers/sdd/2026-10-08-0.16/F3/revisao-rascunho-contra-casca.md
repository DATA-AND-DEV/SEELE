# O rascunho da API 6 contra a casca — a revisão de 10/10

Condição do `v6.json` (tarefa 12), aceita pela orquestradora. O rascunho é
`docs/superpowers/specs/2026-10-08-api-6-rascunho.md` e o `.json` ao lado; a
casca é a da `0.16/api-6` em `78edf0a7`.

## Como

1. **O juiz do `v6.json`, contra o rascunho.** O `.json` do rascunho posto por
   um instante como `api/v6.json` e o `cargo xtask check-api` rodado, que
   confere pontos, tipos, nomes do runtime e o apontador de cada `action`,
   `read`, `own` e `world` (o arquivo foi apagado depois).
2. **Cada lista do `.json` contra a tabela do código que diz a mesma coisa**,
   nos dois sentidos, por script.
3. **As 101 referências `arquivo:linha` do rascunho**: se o arquivo existe e a
   linha está dentro dele; e, onde há um identificador entre crases antes da
   referência, se ele ainda está a até cinco linhas da linha citada.

## O que bate, nos dois sentidos

| lista do `.json` | contra | resultado |
|---|---|---|
| os pedidos do prelúdio | os `case` do roteiro da janela (`atenderOMod`) | iguais: 23 e 23 |
| `actions` | os pedidos, na convenção do `api/v5.json` | coerentes: a 6 tira `cartoes` e `regiao` e soma `atualizar-contribuicao`, `formulario-erros`, `superficie-atualizar` e `retrato` |
| `eventos` (24) | os `falar({ nome })` da casca | iguais |
| `formas` (30) | `FORMAS_DA_REGIAO` | iguais |
| `figuras` (4) | os `case` de `figura.tipo` | iguais |
| `recusas.superficies` (8) | `recusaDaSuperficie` e as recusas nomeadas | iguais |
| `recusas.contribuicoes` (8) | os nomes do registro | iguais |
| `estilos.animacoes` (5) | `mods-estilos.js` | iguais |
| `tema.nomes` (12) e as capacidades | `CHAVES_DO_TEMA` e `CAPACIDADES_POR_API` | iguais (e já guardados pelos espelhos de `frontend.rs`) |
| as referências `arquivo:linha` | os arquivos | as 101 existem, com a linha dentro do arquivo |

## O que não batia, e já foi consertado

- **`recusas.tema` sem `regiao-desconhecida`** (da 17i), e o §9 em prosa sem
  `chave-desconhecida` e `valor-invalido` (da 17j) — `3235ff33`.
- **O `check-api` acusava o `painel`**, que a 6 recusa de propósito: ele
  comparava a publicação com todos os tipos da casca. A regra de qual tipo vale
  em qual API passou a ser dado da tabela (`desde`/`ate`), lido pelo
  `tiposDaApi` e pelo guarda — `78edf0a7`.

## O que espera decisão

1. **O servidor não confere os pares de contraste.** O §9 diz que `mundo.tema`
   «confere o tema como a janela confere (os pares) e devolve a recusa com o
   par», e `recusas.tema` lista `contraste`. Medido: não há recusa `contraste`
   no `seele-server`. Quem confere os pares é a casca, e o MOD fica sabendo
   pelo evento `tema` (`recusado`, com os pares e, desde a 17i, a região) — ou
   pela resposta da prévia. Duas saídas:
   - **(a) o rascunho diz o que é** (recomendo): o servidor confere a forma (as
     chaves, os valores, o `modo`, as regiões); os pares são da casca, que
     conhece a palheta do produto e as cores derivadas; `contraste` sai da
     lista de `mundo.tema` e fica no evento e na prévia;
   - (b) o servidor passa a conferir os pares: precisa da palheta do produto
     em Rust e das derivações (OKLCH) da `tonsDoTema`, com um vetor comum para
     as duas pontas.
2. **Dois pontos que a casca aceita e o `.json` não publica em
   `contribuicoes.pontos`** (o `check-api` reprova): o `chamada.palco` (que o
   rascunho descreve em `camadas`) e o `servidor.aparencia` (que tem chave
   própria). O primeiro é publicação: o `v6.json` o lista com os modos
   `substituir` e `adicionar`. O segundo é decisão: na 6 o tema é do servidor;
   ou a 6 publica o ponto (um MOD de janela pinta a aparência como na 5), ou a
   casca o recusa na 6 como recusa o `painel` — o que pede o mesmo `desde`/
   `ate` na tabela de pontos, e o guarda lendo-o.
3. **As referências derivaram.** Das 56 que dá para conferir pelo
   identificador, 52 não o têm mais a até cinco linhas da linha citada (uma
   parte é falso positivo, de identificador genérico; a maioria é deriva: o
   `TEMA_DA_API` saiu de `base.js:1844` para a 2130). Elas vão continuar
   derivando até o congelamento. Proposta: no congelamento, uma passada que
   troca `arquivo:linha` por `arquivo`, `símbolo` — que não deriva —, com o
   script da conferência como prova.

## O que é da tarefa 12, e não do rascunho

- **Os 12 apontadores com prosa no lugar do símbolo** (`contexto.canal`,
  `contexto.sala`, `retrato.onde`, `retrato.regioes`, `servidor.contexto`,
  `servidor.dados.uso`, `atualizar-contribuicao`, `formulario-erros`,
  `retrato`, `superficie-atualizar`, `mundo.avisar`, `mundo.tema`): o
  `v6.json` aponta para o símbolo do código, como o `v5.json` faz
  (`"contribuir": "RegistroDeContribuicoes::registrar"`); a prosa fica no
  rascunho.
- **O `contribuicoes.teto` como número**: o modelo está decidido, os números
  saem da M3.
