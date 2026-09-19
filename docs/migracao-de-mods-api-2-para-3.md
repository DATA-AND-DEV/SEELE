# Migrar um MOD da API 2 para a API 3

**ADR 0049.** Um MOD deixa de rodar na janela do produto. Este documento é o
que um autor precisa para reescrever um MOD que já existe — o que sumiu, o que
entrou no lugar, e o que não tem substituto.

Ele é uma ruptura limpa: **não há caminho de compatibilidade**. Um pacote que
declara `"api": 2` é recusado pelo nome, `api-too-old`, antes de a primeira
linha dele rodar. A razão está no ADR: enquanto houvesse um MOD antigo
carregado, a promessa «o que um MOD faz some quando você sai do servidor»
continuaria falsa, e o produto prometeria duas coisas ao mesmo tempo.

> **Onde este arquivo mora, e por quê.** Ele está no repositório do produto, ao
> lado do código que define a API, e não no site do catálogo. O site descreve a
> API que o indexador aceita **hoje**; enquanto `VERSAO_DA_API` lá for 2,
> publicar ali um guia de API 3 seria publicar algo que não vale. Ele viaja para
> o guia no mesmo passo que sobe a constante dos dois lados.

## O que mudou, numa tabela

| | API 2 | API 3 |
| --- | --- | --- |
| Onde a metade de janela roda | na janela do produto | num `Worker` próprio |
| `document`, `window`, `CSS` | tudo o que o WebView oferece | não existem |
| Desenho | o MOD escreve no DOM | o MOD **declara**, o produto monta |
| Tema | `document.documentElement.style` | `SeeleUI.tema`, dentro da sessão |
| Descarregamento | evento `seele-mod-unload`, cooperativo | `terminate()`, garantido |
| `SeeleMods.request` / `snapshot` | iguais | iguais |
| Metade de servidor | QuickJS, `aoPedir` / `aoAcontecer` | **sem nenhuma mudança** |

**A metade de servidor não muda.** `aoPedir`, `aoAcontecer`, `dados`,
`arquivos`, `mundo` e `volume` são exatamente os mesmos. Se o seu MOD é só
servidor, a migração dele é trocar o número no manifesto.

## O que sumiu, e não volta

Dentro de um `Worker` não existem:

- `document`, `window`, `getComputedStyle`, `CSSStyleSheet`,
  `document.adoptedStyleSheets`;
- `MutationObserver` sobre a interface do produto, e os seletores que ele
  observava (`#tela-sessao .painel-canais .canais-rolagem` e parentes);
- `localStorage`, `sessionStorage` e o global do Tauri;
- o evento `seele-mod-unload` — ele deixou de ser disparado, porque deixou de
  ser necessário.

Isso não é uma jaula construída com cuidado: é o que um `Worker` **é**. É por
isso que a garantia é forte — `terminate()` mata temporizador, ouvinte, promessa
atrasada e áudio sem depender de o MOD cooperar.

O que **continua** existindo lá dentro é o JavaScript inteiro: `setTimeout`,
`Promise`, `JSON`, `TextEncoder`, `structuredClone`.

E continua existindo, **medido no aplicativo nativo**, mais do que se supunha:
`indexedDB`, `caches` e `BroadcastChannel`, na origem do produto. Isso não é
convite. O que um MOD gravar em `indexedDB` **não some quando você sai** — ele
sobreviveu ao encerramento do aplicativo numa medição de 18/09 —, e a promessa
de limpeza deste documento não o cobre. Guarde estado no servidor, pelo
`aoPedir` e pelo `dados`, que é onde ele pertence e onde as permissões valem.
Um caminho para fechar os três está em discussão; enquanto não fecha, um MOD que
depender deles está dependendo do que o produto pretende tirar.

`fetch` existe como função, mas um worker de `blob:` herda a política de
conteúdo da janela que o criou, e a desta janela só admite o canal interno do
Tauri. Trate rede como coisa da sua metade de servidor, que é onde ela sempre
esteve: `mundo.buscar`.

## O que entrou no lugar

Quatro funções, e só elas. Duas você já conhece:

```js
await SeeleMods.snapshot()                   // o estado que a janela já tem
await SeeleMods.request(id, canal, valor)    // pergunta à sua metade de servidor
```

E duas novas:

```js
await SeeleUI.regiao(conteudo)   // desenhar, declarando
await SeeleUI.tema(valores)      // pedir cor, dentro da sessão
```

Todas devolvem promessa. Um erro chega como `Error` com a frase do produto
dentro — inclusive as recusas descritas abaixo.

### `SeeleUI.regiao(conteudo)`

O MOD manda **o que quer ver**, e quem desenha é o produto, dentro da região
dele. Chamar de novo substitui o conteúdo inteiro da região.

A gramática é pequena de propósito. Cada forma nova é uma decisão de API, em vez
de um MOD descobrir que consegue:

| `forma` | Vira | Para |
| --- | --- | --- |
| `titulo` | um cabeçalho | o nome do bloco |
| `texto` | um parágrafo | uma frase |
| `linha` | um agrupamento | juntar partes |
| `lista` / `item` | uma lista e seus itens | várias coisas do mesmo tipo |

`dentro` é uma string, uma forma, ou uma lista de qualquer um dos dois. Uma
forma que a API não conhece **não vira nada** — ela não vira um agrupamento por
conveniência, porque virar seria a gramática crescer sem ninguém decidir. A
árvore tem teto de fundura; uma mais funda do que isso é cortada.

```js
await SeeleUI.regiao({
  forma: "linha",
  dentro: [
    { forma: "titulo", dentro: "FICHA" },
    { forma: "texto", dentro: `pontos: ${pontos}` },
    { forma: "lista", dentro: nomes.map((n) => ({ forma: "item", dentro: n })) },
  ],
});
```

Texto vira nó de texto, sempre. **Não há forma que aceite HTML**, e isso é
deliberado: o conteúdo vem de código de terceiro, e interpretá-lo como marcação
devolveria a página inteira ao MOD por outro caminho.

A região sai inteira quando o MOD é descarregado. Não sobra nó solto nem regra
de CSS.

### `SeeleUI.tema(valores)`

Quatro nomes, e só valores `#rrggbb`:

```js
await SeeleUI.tema({ acento: "#6BFFB6", fundo: "#050403", texto: "#EAE3CF", borda: "#241F19" });
```

O tema é aplicado **ao contêiner da sessão**, e não ao documento. Sair do
servidor o tira junto, e a cor de quem usa reaparece sozinha. Isso conserta o
defeito que o ESTILO tinha por construção: a cor dele ficava na tela de entrada
e no launcher depois de sair, porque não havia onde escrevê-la que não fosse
global.

Quatro recusas, todas pelo nome, todas como `Error`:

- um nome que a API não conhece;
- um valor que não é `#rrggbb`;
- **um token que outro MOD já pediu nesta sessão.** O produto não tem como
  saber qual dos dois quem usa quis, e escolher em silêncio é escolher errado
  metade das vezes;
- um par texto/fundo abaixo de **4,5:1** de contraste. Quem paga por um tema
  ilegível é quem está lendo a conversa, e não pediu tema nenhum.

## Como migrar, passo a passo

1. **Suba o `api` do manifesto para 3.** Sem isso, nada mais importa: o pacote é
   recusado na leitura.
2. **Separe o que era desenho do que era lógica.** A lógica vai inteira para o
   worker. O desenho vira uma função que devolve a árvore de `SeeleUI.regiao`.
3. **Apague o `seele-mod-unload`.** O ouvinte, a função de limpeza, o
   `clearInterval`, o `observer.disconnect()`, o `painel.remove()` e a restauração
   do tema — tudo isso deixou de ser seu trabalho. Deixar o código não quebra
   nada; ele simplesmente nunca é chamado.
4. **Troque cada escrita no DOM por uma declaração.** Um `textContent` vira
   `{ forma: "texto", dentro: ... }`; um `<ul>` montado à mão vira `lista` e
   `item`.
5. **Troque a folha de estilo por `SeeleUI.tema`.** Se o seu MOD pintava mais do
   que os quatro tokens, o que passa disso não tem substituto hoje — ampliar o
   alcance é uma mudança da API, com ADR, e não uma descoberta.
6. **Teste com o pacote exato que você vai distribuir.**

## O que não tem substituto

Vale escrever em voz alta, porque um guia que só lista o que existe deixa quem
lê descobrindo o resto sozinho:

- **decorar a lista de pessoas e a lista de canais.** A API 3 desenha numa
  região própria, e não dentro dos painéis do produto. O ADR 0049 registra
  «apresentação de canais e pessoas» como alcance a abrir, e abrir é decisão
  escrita, revisada e versionada;
- **reagir a cliques dentro da região.** O que a região desenha hoje é leitura.
  Um controle que responde é a extensão seguinte, e ela tem de decidir como um
  evento atravessa de volta para o worker;
- **imagem e mídia dentro da região.** Não há forma para isso na gramática;
- **qualquer coisa que dependa de um seletor do produto.** Eles nunca foram
  contrato, e agora não são nem alcançáveis.

## O vetor de referência

`apps/seele-app/testes/mod-de-referencia/` é um MOD mínimo que exercita as
quatro funções de ponta a ponta, e a bateria deste repositório o executa:
`crates/seele-conformance/tests/mod_de_referencia.rs` roda a metade de servidor
no mesmo anfitrião QuickJS do produto e cobra que cada `SeeleMods.x` /
`SeeleUI.x` e cada `forma` que a metade de janela usa exista do outro lado.

É o guarda que este documento não consegue ser: se a API mudar de forma e este
texto não mudar junto, o vetor reprova antes de alguém reescrever um MOD contra
uma promessa vencida.
