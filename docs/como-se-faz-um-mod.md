# Como se faz um MOD

> Para quem vai escrever um. A decisão de por que MODs existem e o que eles
> alcançam está no [ADR 0044](adr/0044-mods-o-produto-base-tem-regras-e-um-mod-nao.md);
> o indexador, em `indexador-de-mods.md`, no repositório `SEELE-MODS-INDEXER`.

**Um MOD é JavaScript.** Não há SDK, não há build, não há passo de compilação.
O que você escreve é o que roda, e é também o que passa pela avaliação — os três
são o mesmo arquivo, de propósito.

## O que um MOD é, em disco

```
meu-mod/
├── mod.json
├── cliente/main.js     roda na janela de quem entra
└── servidor/main.js    roda no servidor
```

As duas metades são opcionais. Um MOD de cor não tem `servidor/`; um bot de chat
não tem `cliente/`.

### `mod.json`

```json
{
  "schema": 1,
  "id": "fulano/meu-mod",
  "version": "1.0.0",
  "api": 1,
  "repo": "https://github.com/fulano/meu-mod",
  "reach": ["dom"],
  "client": "cliente/main.js",
  "server": "servidor/main.js"
}
```

| campo | o que é |
|---|---|
| `schema` | formato deste arquivo. Hoje, `1`. |
| `id` | `autor/nome`. Minúsculas, dígitos e hífen. **Tem de bater com a pasta.** |
| `version` | sua. Nunca é interpretada por nós. |
| `api` | a versão da API contra a qual você escreveu — ver `api/v1.json`. |
| `repo` | seu repositório público. Obrigatório. |
| `reach` | o que você pede alcançar. A tela de aceite mostra isto antes de baixar. |
| `client`, `server` | caminhos das metades, relativos à pasta. |

**Chave desconhecida é recusada, e é de propósito.** `vesion` com um `s` a menos
instalaria calado um MOD sem versão, e você nunca ficaria sabendo. A recusa é a
única forma de retorno que este formato te dá.

**`api` mais novo que o servidor faz o MOD não carregar, nomeando os dois
números.** O produto não roda pela metade.

## A metade de cliente

Carregada depois do primeiro desenho da página, com **acesso à janela inteira**.
Não há caixa de areia — as regras do produto protegem o produto e não te
alcançam. É a decisão do ADR 0044, e a contrapartida é a avaliação por que o seu
código passa.

```js
// cliente/main.js
document.documentElement.style.setProperty("--seele-laranja-nerv", "#00b7ff");
```

Você tem DOM, CSSOM e o que mais o navegador embutido oferecer. O que você
**não** tem é rede: a CSP da janela admite `mod:` e nada mais. Rede é do lado do
servidor.

## A metade de servidor

Uma função, e três objetos globais.

```js
// servidor/main.js
globalThis.aoAcontecer = (momento, carga) => {
  const evento = JSON.parse(carga);

  if (momento === "MessageReceived" && evento.texto.startsWith("$dado")) {
    const rolagem = 1 + Math.floor(Math.random() * 20);
    dados.ultimaRolagem = String(rolagem);
    mundo.registrar(`rolou ${rolagem}`);
  }
};
```

### `aoAcontecer(momento, carga)`

`momento` é o nome do evento — **o mesmo nome que o protocolo usa**, para que um
relatório de defeito que diga `MessageReceived` ache a mesma palavra no
protocolo, no `api/v1.json` e no seu código. A lista está em `api/v1.json`.

`carga` é JSON. Sempre. Faça `JSON.parse`.

Um MOD sem `aoAcontecer` não é erro: é um MOD cuja metade de servidor existe
para outra coisa.

### `dados` — o seu quintal

Um objeto de texto para texto. O que estiver nele ao fim da chamada é gravado; o
que estava lá chega preenchido.

```js
dados.placar = String(Number(dados.placar ?? "0") + 1);
```

Três coisas que valem saber:

- **Uma chamada é transacional.** Se o seu código lançar no meio, **nada** é
  gravado. Meia escrita é pior que nenhuma, porque você não teria como saber
  qual metade entrou.
- **Só texto.** Números viram texto; objetos, `JSON.stringify`.
- **Teto de 256 KiB**, e ele **recusa** em vez de aparar. Um quintal que
  descartasse a entrada mais velha em silêncio seria um quintal que você não
  pode acreditar.

### `arquivos` — a sua pasta

```js
arquivos.escrever("fichas/coelho.json", JSON.stringify(ficha));  // → true/false
arquivos.ler("fichas/coelho.json");                              // → texto ou null
arquivos.listar();                                               // → ["fichas/coelho.json"]
arquivos.apagar("fichas/coelho.json");                           // → true/false
```

Tudo dentro de `mods/<autor>/<nome>/dados/`, e **nada fora**. `..` é recusado,
caminho absoluto é recusado, e a recusa é a mesma resposta para todos os casos.

**Por que só a sua pasta**, num sistema em que o resto é liberdade total: ao lado
dela ficam o `identity.key`, os `pins` e o banco com todas as conversas. Disco
inteiro entregaria a chave privada de identidade de quem te hospeda, e aí o que
cai não é uma regra de produto — é a garantia de que aquela pessoa é ela mesma.

Teto de 4 MiB por arquivo.

### `mundo` — rede, relógio, registro

```js
const resposta = mundo.buscar("https://api.exemplo.com/x");  // texto ou null
const agora = mundo.agora();                                  // segundos desde 1970
mundo.registrar("o que aconteceu");                           // vai para o log de quem hospeda
```

- **`http` e `https` e mais nada.** `file://` é recusado, senão a rede faria o
  que a pasta impede no disco.
- **Teto de 2 segundos por busca**, e ele existe por um motivo que vale você
  saber: o servidor roda um momento por vez, então uma busca lenta sua atrasa
  **todos os outros MODs** e a fila de eventos atrás deles. Dois segundos é o
  pior caso que alguém aceitou pagar por você.
- **Resposta acima de 4 MiB é descartada.**
- `mundo.registrar` sempre carrega o seu `id`. Você não escolhe o prefixo — uma
  linha cuja origem você pudesse forjar seria uma linha que ninguém pode usar
  para decidir qual MOD desligar.

## Os dois tetos que te cortam

| | quanto | o que acontece |
|---|---|---|
| memória | 8 MiB por MOD | a chamada morre; **o seu contexto sobrevive** |
| tempo | ~500 consultas ≈ 225 ms | a chamada morre; o seu contexto sobrevive |

Um laço infinito não derruba a sala: ele derruba a sua chamada. Mas um MOD que
falha é **desligado**, e o desligamento é gravado — então um bug que lança em
todo evento tira o seu MOD do ar até alguém religá-lo.

Os 225 ms não são de relógio: são de trabalho. Uma máquina lenta te dá o mesmo
tanto de operações e demora mais, em vez de te cortar antes.

## Testar antes de publicar

```sh
mkdir -p ~/.config/seele/mods/fulano/meu-mod
# copie mod.json, cliente/ e servidor/ para lá
```

Abra o app, hospede, e habilite. **Erros do seu MOD aparecem no log de quem
hospeda** — `~/.config/seele/seele.log` — com o seu `id` no prefixo.

> **O que falta, e é honesto dizer:** não há `seele mod new`, não há definições
> de tipo para o seu editor, e não há como rodar um MOD sem hospedar um servidor.
> O ADR 0044 promete as definições de tipo e elas ainda não existem. Enquanto
> não existirem, o retorno que você tem é o log.

## Publicar

1. **Clone o repositório base**, faça o seu, e dê `push` no **seu** repositório
   público.
2. **Peça inclusão pelo site**, com a URL do repositório.
3. **A avaliação roda**: estrutura, e o código inteiro contra código malicioso,
   comunicação com terceiros e tentativa de invasão.
4. **O veredito**, e são três:

| | o que significa |
|---|---|
| **verificado** | passou. Entra no catálogo com assinatura de verificado. |
| **publicado com notas** | não passou limpo, mas o que ele faz é legítimo — uma integração com um terceiro conhecido, por exemplo. Entra **sem** verificação, e as notas de segurança aparecem na tela de quem instala. |
| **negado** | o que ele faz lesa quem instala. Não entra. |

**O commit avaliado é fixado no catálogo.** Um `push` depois da aprovação não
muda o que as pessoas baixam — ele exige uma solicitação nova. É o que faz a
avaliação valer alguma coisa.

## As regras que não mudam

- **Uma versão publicada nunca é editada.** Versão nova é caminho novo. É a
  mesma disciplina das migrações do servidor, e vale pelo mesmo motivo.
- **O `api` que você declara é uma versão só.** Para valer numa API mais nova,
  publique de novo — e passe pela avaliação de novo.
- **A API nunca perde nada.** O que `api/v1.json` promete hoje continua
  valendo. Renomear por dentro é problema nosso, e há um build vermelho que
  cobra isso de nós, não de você.
