# A entrega da API 3: o que está pronto, o que falta homologar, o que publicar

Três coisas diferentes, e a confusão entre elas é o que faz alguém publicar o
que não foi exercitado — ou segurar o que já está pronto. Cada seção abaixo diz
**quem faz** e **o que prova**.

## 1. Desenvolvimento — concluído

| Peça | Estado | Prova |
| --- | --- | --- |
| Executor | QuickJS, um só, sem alternativa | `bancada/ciclo-do-executor.cjs` |
| `MOD_API_VERSION` | 3, com `api/v3.json` | `cargo xtask check-api` |
| Região: 11 formas, 6 eventos, figuras | de pé | `bancada/regiao-do-mod.cjs` |
| Mídia por bytes, paginada | de pé | `bancada/continuacao-de-midia.cjs` |
| Arquivo escolhido, em pedaços | de pé | `bancada/regiao-do-mod.cjs` |
| Tema: 6 cores, densidade, fonte | de pé | `o_tema_de_um_mod_fica_dentro_da_sessao` |
| Recusa nomeada de raio e sombra | de pé | `o_que_a_marca_proibe_e_recusado_com_a_razao` |
| `SeeleUI.marcas` na lista de pessoas | de pé | `bancada/marcas-na-lista.cjs` |
| Os três MODs em 2.0.0 | 14, 31 e 41 provas | as suítes de cada repositório |
| Matriz dos três MODs | sem pendência | `matriz-dos-tres-mods.md` |
| Guia e exemplo | reescritos e regerados | 204 provas em Python, 98 em JS |

A bateria do produto fecha verde: `cargo test --workspace --all-features`,
`cargo clippy --workspace --all-features --all-targets` sem um aviso,
`cargo fmt --all --check` limpo, e as quatro bancadas de `cargo xtask
check-runtime`.

## 2. Homologação — parte feita, parte impedida

**Feito, no aplicativo nativo de verdade:** o MOD sobe no executor nativo, a
região monta inteira — a mídia é a última forma declarada, e o produto só a pede
depois de passar por todas —, os bytes são reconhecidos pelo que são, o evento
volta ao MOD, a gravação no servidor persiste, e `SeeleUI.marcas` é aceita com
uma pessoa de verdade. O vetor anota no quintal dele o que a janela viu, e isso
se lê com `sqlite3` — sem depender de automação.

**Impedido, e o impedimento é do método:** digitar, arrastar e apertar na janela.
A automação de acessibilidade do macOS recusa este processo (`osascript é um
acesso assistivo não permitido`). Com ela ficam de fora:

- a latência de interação medida na janela de verdade;
- o arraste e o desenho no WKWebView;
- a saída **durante** o carregamento e a reprodução, e a troca de servidor A→B;
- o impacto na voz sob carga.

Os quatro primeiros são cobertos pelas bancadas, que rodam o código de verdade
num DOM mínimo. **É prova mais fraca que a nativa**, e está dita como mais
fraca em todo lugar onde aparece.

**Não feito, e não impedido:** Windows e Linux. Esta máquina é uma só.

## 3. Publicação — não iniciada, e por ordem

O que está no ar hoje, conferido e não suposto:

- **aplicativo**: `v0.11.2`, do commit `e90bcc167`, publicado em 18/09. Ele
  carrega `MOD_API_VERSION = 2`;
- **catálogo**: `api_oferecida: 2`, com ESTILO 1.0.1, MESA 1.2.1 e PERFIS 1.2.2.

Ou seja: **nada desta rodada está publicado**, e são 48 commits de diferença. Os
dois lados no ar concordam entre si, que é o estado certo para ficar parado.

A ordem está em `migracao-para-api-3.md`, e ela não é preferência: um cliente só
roda MODs da própria API, por igualdade e não por «até». Resumida:

1. publicar o aplicativo com a API 3 — **antes** do catálogo;
2. regerar e assinar o catálogo com `api_oferecida: 3`;
3. publicar os três MODs em 2.0.0;
4. copiar o catálogo novo para os vetores deste repositório.

**O passo 2 é o único que precisa da chave**, e ele é de quem a tem: a chave
privada dos MODs mora em `~/.minisign/mods.key`, com senha que só a pessoa que
opera conhece, e nem a chave nem a senha entram em Cloudflare, em CI, ou nesta
sessão. Quem publica exporta `MINISIGN_PASSWORD` no próprio terminal e roda
`ferramentas/gerar.py`.

**O `publicado/` do indexador não foi tocado.** Ele é o que está no ar, e
regerá-lo antes da hora faria a árvore afirmar uma publicação que não aconteceu.
O `site/` foi regerado, e é o que vai substituir aquele quando a hora chegar.
