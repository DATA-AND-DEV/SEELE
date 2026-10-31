# Os MODs oficiais na casca do navegador

Medida de 29/10/2026, no Mac, na branch `desenho/api-8-multiplayer`, no commit
`cf9ac7b6` (`cf9ac7b658f15fa6d9fd4e7211c3ecd4fc056756`). Ela existe para decidir,
depois, se algum MOD oficial deve declarar `plataformas`. **Nenhum MOD oficial
ganhou `plataformas` neste plano.**

## Já havia uma medida registrada?

Não. Nada em `docs/evidencias/` cita a bancada, e o histórico dela
(`git log -- apps/seele-app/bancada/mods-oficiais-na-ui.cjs`) tem um commit só,
`fec5e0c4` (27/10/2026, «Prepare 0.17 web source…»), que a cria. O cabeçalho dela
fala dos MODs 4.0.0 (API 6); os que existem hoje são 5.x (API 7). Não há
resultado antigo a citar, e esta é a primeira medida.

## O que se mediu

- **A casca**: `ui/` deste commit, o `seele_web.wasm` e o `seele_motor_web.wasm`
  de `cargo xtask pacote-web` (em `target/pacote-web/`), o servidor de
  `cargo build -p seele-server --example servidor_da_bancada`.
  O `pacote-web` precisa do wasi-sdk conferido; ele foi copiado de outro
  `target/` da mesma máquina (`cp -c -R`), que é o caminho que o próprio xtask
  indica, e o xtask o conferiu pelo hash. Nada foi baixado.
- **Os MODs**, nas pastas vizinhas, no `HEAD` de cada uma:

  | MOD | repositório | commit | versão | `api` |
  |---|---|---|---|---|
  | ESTILO (aparência) | `SEELE-MOD-ESTILO` | `13a79ee` (27/10/2026) | 5.0.0 | 7 |
  | PERFIS | `SEELE-MOD-PERFIS` | `11fb8d8` (27/10/2026) | 5.0.0 | 7 |
  | MESA | `SEELE-MOD-MESA` | `43bd55b` (27/10/2026) | 5.0.1 | 7 |

  Os três são da API 7: **sem `plataformas`**, portanto «as duas».
- **O navegador**: Chromium do Playwright, sem `--webkit`. Não se mediu o WebKit
  nem uma tela de celular.
- **O que a bancada faz**: sobe um servidor real que exige o MOD, entra pela UI
  de verdade (COLAR UM LINK, aceitar), espera o MOD ativo no executor do
  navegador, faz os gestos do roteiro de cada MOD (`mods-oficiais-na-ui/*.cjs`)
  e confere o DOM e o console. O catálogo é simulado (o pacote entra direto no
  armazém da página); o resto é real.

## O comando

```sh
export PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright
cargo xtask pacote-web
cargo build -p seele-server --example servidor_da_bancada
for m in ESTILO PERFIS MESA; do
  node apps/seele-app/bancada/mods-oficiais-na-ui.cjs \
    target/pacote-web/seele_web.wasm \
    target/debug/examples/servidor_da_bancada \
    target/pacote-web/seele_motor_web.wasm \
    /Users/dev-alexandre/SEELE-MOD-$m
done
```

## A saída, sem retoque

Os três terminaram com **código 1**. A bancada só imprime o que reprovou:

```
=== ESTILO
mods-oficiais-na-ui (seele/estilo): 1 falha(s)
- estilo: erros no console: ["error: Access to fetch at 'https://rendzvous.seele.sh/chamada/1' from origin 'http://localhost:49482' has been blocked by CORS policy: No 'Access-Control-Allow-Origin' header is present on the requested resource. @ http://localhost:49482/index.html:0"]
=== PERFIS
mods-oficiais-na-ui (seele/perfis): 2 falha(s)
- perfis: o apelido do servidor sumiu do cartão: FORA DE SALA
D
Dona de Teste
⋯
- perfis: erros no console: ["error: Access to fetch at 'https://rendzvous.seele.sh/chamada/1' from origin 'http://localhost:49506' has been blocked by CORS policy: No 'Access-Control-Allow-Origin' header is present on the requested resource. @ http://localhost:49506/index.html:0"]
=== MESA
mods-oficiais-na-ui (seele/mesa): 1 falha(s)
- mesa: erros no console: ["error: Access to fetch at 'https://rendzvous.seele.sh/chamada/1' from origin 'http://localhost:49544' has been blocked by CORS policy: No 'Access-Control-Allow-Origin' header is present on the requested resource. @ http://localhost:49544/index.html:0"]
```

A falha de console é igual nos três e **não é de MOD**: é a página, no
`localhost`, chamando o rendezvous de produção (`https://rendzvous.seele.sh`,
que não responde CORS a essa origem). A bancada não a filtra porque foi escrita
para a origem de produção. (A saída acima é a íntegra; nada foi encurtado. O `⋯` nas linhas do PERFIS é texto da própria saída da bancada, não elisão minha: é o `innerText` do cartão da lista de pessoas (`#lista-roster`), e o `⋯` está no log bruto da rodada; é um glifo que a UI põe no cartão (provavelmente o botão de ações, o que não verifiquei). O
`ruins` da bancada imprime só o que reprovou, e o PERFIS tem as duas linhas.)

### A segunda rodada, com o filtro

Para ver o que sobra sem esse erro, rodei de novo uma cópia **descartável** da
bancada (`zz-tmp-medida.cjs`, na mesma pasta, apagada depois; o arquivo do
repositório não mudou). A mudança, em duas linhas:

```diff
--- a/apps/seele-app/bancada/mods-oficiais-na-ui.cjs
+++ b/apps/seele-app/bancada/mods-oficiais-na-ui.cjs
@@ -282,9 +282,10 @@
       // subindo): `modsQueOuvemATela` (ui/base.js) lê `instancia.api` de uma entrada nula.
       const dasInstancias = doConsole.filter((l) => /reading 'api'|evaluating 'instancia\.api'/.test(l));
       defeito(curto, dasInstancias.length === 0, `o desenho do retrato estoura em \`modsQueOuvemATela\` com um MOD ainda subindo (\`instancia.api\` de uma entrada nula em modsCarregados, ui/base.js:3677; ${dasInstancias.length}x): ${dasInstancias[0]?.slice(0, 120)}`);
-      const ruins = doConsole.filter((l) => !dasInstancias.includes(l)).filter((l) => !/Failed to load resource|favicon|net::ERR|permissions policy|sem janela do Tauri|RecusaDaPonte/i.test(l) &&
+      const ruins = doConsole.filter((l) => !dasInstancias.includes(l)).filter((l) => !/Failed to load resource|favicon|net::ERR|permissions policy|sem janela do Tauri|RecusaDaPonte|blocked by CORS policy/i.test(l) &&
         // O `screenshot` do Playwright no WebKit injeta uma folha que a CSP da origem recusa: linha 0 do documento, e sem `--capturas` ela não aparece: não é do produto.
         !/Refused to apply a stylesheet.* @ \S*(index|executor-de-mod)\.html:0$/.test(l));
+      console.log("CORS-FILTRADOS:", JSON.stringify(doConsole.filter((l) => /blocked by CORS policy/.test(l))));
       confere(curto, ruins.length === 0, `erros no console: ${JSON.stringify(ruins.slice(0, 6))}`);
     } finally {
       await navegador.close();
```

(o diff é `diff -u` íntegro, aplicável com `patch -p1`; conferi com `patch --dry-run`; a linha `+console.log` só imprime o que o filtro derruba; a rodada anterior, que deu os
mesmos resultados, tinha só a primeira). Esta rodada foi refeita para registrar
isso, com os mesmos binários:

| MOD | código de saída | linha final |
|---|---|---|
| ESTILO | 0 | `mods-oficiais-na-ui (seele/estilo, chromium): ok` |
| PERFIS | 1 | `mods-oficiais-na-ui (seele/perfis): 1 falha(s)` e o apelido do cartão |
| MESA | 0 | `mods-oficiais-na-ui (seele/mesa, chromium): ok` |

**O filtro derrubou um erro de CORS por rodada, sempre o mesmo**: o `fetch` a
`https://rendzvous.seele.sh/chamada/1`, e nenhum outro CORS (`CORS-FILTRADOS`
tinha exatamente um item nos três). O texto: `Access to fetch at
'https://rendzvous.seele.sh/chamada/1' from origin 'http://localhost:<porta>'
has been blocked by CORS policy: No 'Access-Control-Allow-Origin' header is
present on the requested resource.`

**Fica para depois**: a bancada do repositório deveria ignorar esse erro do
rendezvous em `localhost` (hoje ela sai com 1 para todo mundo que a roda).

## O resultado por MOD

| MOD | carregou no navegador | roteiro |
|---|---|---|
| ESTILO | sim: subiu no executor do navegador, estado ativo | passou inteiro (com o filtro do CORS da página, acima) |
| MESA | sim: idem | passou inteiro (idem) |
| PERFIS | sim: idem, e a gravação voltou à tela (o diretório e o aviso «Perfil gravado» apareceram) | **uma conferência reprovou**: «o apelido do servidor sumiu do cartão». O cartão da lista de pessoas mostrou `D` / `Dona de Teste` e não trazia o apelido `dona` |

**Sobre o PERFIS.** A conferência que falhou é a última do roteiro
(`mods-oficiais-na-ui/perfis.cjs`, linha 51), escrita para o PERFIS 4.0.0. **Não
medi** se o PERFIS 5.0.0 deixou de pôr o apelido no cartão da lista (muda o MOD,
ou o roteiro está velho) ou se a casca do navegador o perde. A medida mostra que
o MOD carrega e grava no navegador; não diz qual das duas coisas é.

## O que não se mediu

- WebKit (`--webkit`) e a tela do celular. A bancada aceita `--webkit`; não rodei.
- O caminho do catálogo (download do pacote assinado): a bancada o simula.
  `sessao-no-navegador.cjs` (caso 14) é a que o exercita.
- O que cada MOD faz **além** do roteiro. «Passou» quer dizer o que o roteiro
  confere, e só isso.
- A 8 em si: estes MODs são da API 7, e nenhum declara `plataformas`.

## O que isto decide

Nada, sozinho. Pelo que se mediu, ESTILO e MESA funcionam no navegador no que o
roteiro cobre, e o PERFIS carrega e grava, com um ponto a esclarecer. Declarar
`plataformas: ["app"]` num deles seria, por esta medida, esconder um MOD que
roda no navegador. Quem decide é o dono, com o ponto do PERFIS esclarecido.
