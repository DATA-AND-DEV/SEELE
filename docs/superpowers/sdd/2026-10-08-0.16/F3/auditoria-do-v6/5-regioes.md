# Auditoria do `api/v6.json` — seção `regioes` (frente 5)

Ramo `0.16/api-6-corte`, ponta `48b6a1ba`. Só leitura; nenhuma bancada de navegador foi
rodada aqui (não há Playwright nesta máquina). Rodei `contribuicoes-e-camadas.cjs` (Node): verde.

## Contagem

`regioes` tem **59 folhas** (chaves e itens de lista). O «~96» do pedido conta também os nós
internos (8 regiões + 8 `modos` + 8 `produto` + 6 `pontos` + 8 `celular` = 38).

| Estado | Quantas |
|---|---|
| PROVADA | 32 |
| INCERTA | 11 |
| SEM PROVA | 13 |
| SEM CÓDIGO | 3 |
| NÃO SE APLICA | 0 |

## O que vale para a tabela inteira

- **Onde cada prova roda.** `regioes-do-produto.cjs`, `capa.cjs`, `nucleo.cjs`, `ajustes-v013.cjs`,
  `avatares-do-mod.cjs` e `diagnostico-de-mods.cjs` são bancadas de navegador: estão no job
  `bancadas` do `ci.yml` (linhas 306–385). **O `ci.yml` é só `workflow_dispatch`** (ci.yml:44-45,
  com o comentário em :3-37): nenhuma delas roda num push, nem no `validar` do `release.yml`, nem na
  bateria do `publicar.sh`. Uma release pode sair com elas vermelhas. Conto «roda» como pedido (estar
  num `run:` do `ci.yml`), mas cada PROVADA abaixo que depende delas só vale de verdade se alguém
  disparar o workflow.
- **`cargo xtask check-api`** (ci.yml:78, e no `validar`) cobra a v6: `pontos_publicados` +
  `pontos_e_tipos` (xtask/src/check_api.rs:524-645) exigem que os `modos` de cada `regioes.<r>` sejam
  **iguais, na ordem**, aos de `PONTOS_DE_CONTRIBUICAO` (mods-contribuicoes.js:76-105), nos dois
  sentidos. Conferi à mão com Python: as oito batem hoje. Isso prova a **tabela**; a **entrega na
  tela** de cada modo é outra prova (a bancada).
- **O celular não tem prova que rode.** Tudo que é específico do celular só é medido por
  `tools/bancada-do-celular.py` (casos `regioes-do-celular`, `operador-barra`, `capa-do-operador`,
  :2017-2150, :2246, :2699-2705). Ela **não está em nenhum workflow** nem no `publicar.sh` (busquei
  `bancada-do-celular` em todo o repositório fora de `docs/`: só aparece em comentários), e o guarda
  `toda_bancada_da_pasta_roda_em_algum_lugar` não a vê, porque ela mora em `tools/` e não em
  `apps/seele-app/bancada/`. `tests/celular.rs` (roda no `cargo test`) só lê texto: nenhum caso sobre
  regiões, 44 px ou 16 px. A única bancada de CI que sobe a casca do celular de verdade é
  `identidade-no-navegador.cjs` (ci.yml:536, caso 9, :591-611), e ela só confere que o `CELULAR`
  existe — nunca `cascaDoRetrato().regioes`.
- **«Uma substituição de região inteira não leva os pontos de dentro»** (rascunho §3): nenhum teste
  que roda registra um ponto de dentro junto com a substituição da região dele (`pessoas.lista`,
  `servidor.navegacao`, `conversa.mensagem`). O código preserva por construção (`emGrupos` reordena
  os mesmos nós do produto; a calha do avatar da mensagem é anexada antes do miolo), mas nada reprova
  se isso mudar. Vale como nota em todas as entradas de `pontos`.

## A tabela

Abreviações: `mc` = `apps/seele-app/ui/mods-contribuicoes.js`; `ts` = `ui/tela-sessao.js`;
`b` = `ui/base.js`; `cc` = `ui/celular-casca.js`; `cs` = `ui/celular-sessao.css`;
`cb` = `ui/celular-base.css`; `rp` = `bancada/regioes-do-produto.cjs`; `bc` =
`tools/bancada-do-celular.py`. «CI(nav)» = job `bancadas` do `ci.yml` (manual); «CI(regras)» =
`cargo xtask check-api` (ci.yml:78 e `validar`).

| Entrada (caminho no JSON) | Promessa (curta) | Código (arquivo:linha) | Prova (arquivo: caso/teste; onde roda) | Estado | Nota |
|---|---|---|---|---|---|
| `regioes.redes.modos[0]` | `redes` aceita `adicionar` (e só ele) | mc:94; ts:728 (`aplicarARegiao("redes", #trilha-mods)`); ts:700-710 | rp: lista `REGIOES` (:37-42) — o nó dentro de `#trilha-mods`, em nenhum outro lugar, revogar devolve; CI(nav). Tabela: check-api, CI(regras) | PROVADA | «e só ele»: `substituir` recusado como `modo-recusado` (mc:267-288), garantido pela igualdade da tabela no check-api |
| `regioes.redes.produto[0]` | o servidor atual continua do produto | index.html:1052 (`#trilha-server`) | rp `REGIOES.redes.doProduto` + `teto.aVista/descobertos` (marcação, tamanho, à vista e sem MOD por cima); CI(nav) | PROVADA | |
| `regioes.redes.produto[1]` | «conectar a outro» continua do produto | index.html:1090 (`#trilha-adicionar`, «Conectar a outro servidor») | rp, idem; CI(nav) | PROVADA | |
| `regioes.redes.celular.existe` | `redes` existe no celular | cc:55-58 (`CELULAR.regioes`); cs:387-400 (a trilha só à vista na aba SERVIDOR); b:3433 | bc `regioes-do-celular` (SONDA à vista na aba SERVIDOR) — **não roda** | SEM PROVA | Reverter: tirar `"redes"` de cc:56 → nada que roda reprova |
| `regioes.redes.celular.forma` | faixa deitada no alto, **itens de 56 × 44 px** | cs:387-400 (deitada no alto: ok); cs:412-418 + cb:23 (`width/height: var(--cel-alvo)` = **44 × 44**) | nenhuma (bc não mede o 56) | SEM CÓDIGO | Os itens da trilha no celular são 44 × 44, e não 56 × 44. O número veio das âncoras do `arquivo/mobile-ios` (`celular.css:220-224`) que a F8 substituiu; entrou no rascunho em `4833a55f` e no v6.json em `49283905`. A spec da F8 não tem 56 × 44. Consertar o texto (é contrato congelado: vai à próxima versão) ou a casca |
| `regioes.servidor.navegacao.modos[0]` | aceita `substituir` (a ordem e os grupos, `{ salas?, canais? }`) | mc:89; mc:199-212 (forma, recusa nomeada); ts:901-909 (`arranjoDe`); ts:1083-1092, :1141 (`emGrupos`) | rp «a navegação, substituída» (:710-785): ordem, OUTROS CANAIS, ninguém some, recusa da outra forma, não vira entrada DOS MODS, DIAGNÓSTICO conta 3; CI(nav). Tabela: check-api | PROVADA | |
| `regioes.servidor.navegacao.modos[1]` | aceita `adicionar` (entradas DOS MODS) | mc:89; ts:2852-2890 (`redesenharAsEntradasDeMod`, `#lista-mods-navegacao`) | `ajustes-v013.cjs`:1750-1759 (duas entradas, uma com `listarNaBarra:false`, `#lista-mods-navegacao li` = 1); CI(nav) | PROVADA | |
| `regioes.servidor.navegacao.pontos[0]` | `canal.item` mora na navegação | ts:1113 (dentro do `button[data-linha]` de `#lista-linhas`, em `.painel-canais`) | `diagnostico-de-mods.cjs`:953-994 registra `canal.item` e confere 2 contornos em `#lista-linhas button[data-linha]` — só existência do contêiner; CI(nav) | INCERTA | Reverter: mover o `conteudoDasContribuicoes("canal.item", …)` de ts:1113 para fora da linha (ou não chamá-lo) → prevejo que o diagnóstico passa (ele mede o contêiner, não o conteúdo). Nada mede `canal.item` com a navegação substituída |
| `regioes.servidor.navegacao.pontos[1]` | `sala.acoes` mora na navegação | ts:2381-2390 (`grupoDeSala`: no cabeçalho `.roster-sala` de `#lista-roster`, dentro de `.painel-pessoas`); camada-mods.js:1518 (`"sala.acoes": ".roster-sala"`) | nenhuma | SEM CÓDIGO | **O código contradiz**: `sala.acoes` mora na lista de pessoas (`pessoas.lista`), não em `servidor.navegacao` — no desktop e no celular (aba PESSOAS). Está assim desde `26ad0c2b`, antes da API 6; o rascunho §3 repete o erro. E `pessoas.lista.pontos` não o lista. Consertar o texto na próxima versão |
| `regioes.servidor.navegacao.produto[0]` | ENTRAR NA SALA continua do produto | ts:1065-1078 (`.voice_room-entrar`, «ENTRAR NA SALA») | rp navegação: só conta `li.voice_room` (= 1) e lê `.voice_room-nome`; CI(nav) | INCERTA | Reverter: com `arranjo` presente, tirar o `ir` do `item.append` (ts:1078) → prevejo que rp passa. No modo `adicionar` nada confere a lista de salas |
| `regioes.servidor.navegacao.produto[1]` | a moderação de salas e canais (renomear, apagar) continua | ts:1001-1022, :1052-1053, :1129-1138 | rp navegação: não olha `.voice_room-apagar`, `.linha-controles` nem o renomear (o retrato tem `may_manage_voice_rooms: true`); CI(nav) | INCERTA | Reverter: com `arranjo`, sem `botaoDeApagarLinha/Renomear` → rp passa. No celular a casca esconde tudo isso de propósito (cs:436-438), sem MOD nenhum — o contrato não diz |
| `regioes.servidor.navegacao.celular.existe` | existe no celular | cc:56; cs:61-71 (aba SERVIDOR mostra `.painel-canais`) | bc `regioes-do-celular` — **não roda** | SEM PROVA | |
| `regioes.servidor.navegacao.celular.forma` | aba SERVIDOR (salas, canais, DOS MODS, troca de servidor), célula inteira, controles 44 × 44 | cs:61-71, :383-406; cb:99-103 (todo botão ≥ 44 × 44) | bc (44 de alvo em tudo; SONDA na aba) — **não roda** | SEM PROVA | |
| `regioes.operador.barra.modos[0]` | aceita `adicionar` (e só ele) | mc:95; ts:1199; index.html:1190 (`#operador-mods`, acima do núcleo); tela-sessao.css:918-923 (96 px, rola) | rp `REGIOES` (:43-51) com teto; `rp` atualizar (:579-647, mesmo nó); CI(nav). Tabela: check-api | PROVADA | |
| `regioes.operador.barra.pontos[0]` | `pessoa.capa` (de quem usa) mora na barra | ts:1213-1214 (`vestirCapa(.operador, me)`) | `capa.cjs`:187-223 (a barra vestida, a camada cobre a barra inteira; contraste de cada texto e controle sobre 3 capas); CI(nav) | PROVADA | |
| `regioes.operador.barra.produto[0]` | SAIR continua do produto | index.html:1288 (`#operador-sair`) | rp `doProduto/aVista/descobertos`; `nucleo.cjs`:191-215 (guarda 11: fora de todo contêiner de MOD, sem cobertura com o desenho mirado); CI(nav) | PROVADA | |
| `regioes.operador.barra.produto[1]` | microfone continua do produto | index.html:1223 (`#botao-mudo`) | rp e nucleo.cjs, idem; CI(nav) | PROVADA | |
| `regioes.operador.barra.produto[2]` | «no ar» continua do produto | index.html:1243 (`#voz-estado`) | rp (`doProduto`: marcação e tamanho) e nucleo.cjs (`noAr` mirado e descoberto); CI(nav) | PROVADA | |
| `regioes.operador.barra.celular.existe` | existe no celular | cc:56; celular-ajustes.js:115 | bc `operador-barra` — **não roda** | SEM PROVA | |
| `regioes.operador.barra.celular.forma` | no alto de AJUSTES, abaixo de SEU PERFIL; largura da tela; 44 × 44; altura limitada com rolagem | celular-ajustes.js:110-115; cs:1057-1079 (`max-height: 96px`, `overflow-y: auto`); cb:99-103 | bc `operador-barra` (:2017-2078: abaixo de SEU PERFIL, 96 rolando, gestão de MODs à vista e recebendo o toque) — **não roda** | SEM PROVA | O caso existe e é bom; falta rodar. «o número sai da M3» não é promessa |
| `regioes.operador.barra.celular.contribuicao` | aplicada; a capa do operador vai no SEU PERFIL; MUDO/SURDO/SAIR na faixa de voz | celular-ajustes.js:242 (`vestirCapa(.cel-perfil-placa)`); celular-voz.js:74-103 | bc `capa-do-operador` (:2246) — **não roda**. (`capa.cjs` caso 5 mede o SEU PERFIL **do desktop**, `#perfil .perfil-cabeca`, não a placa de AJUSTES) | SEM PROVA | |
| `regioes.canal.cabecalho.modos[0]` | aceita `substituir` (o miolo entre o nome e a lupa) | mc:86; ts:1345-1358 (`#canal-cabecalho-miolo`) | rp `REGIOES` canal.cabecalho, `modo: "substituir"` (:86-101); CI(nav). Tabela: check-api; frontend.rs:17546 | PROVADA | |
| `regioes.canal.cabecalho.modos[1]` | aceita `adicionar` | mc:86; ts:1360-1372 (`#canal-cabecalho-mods`) | Registro: R3 de `contribuicoes-e-camadas.cjs`:1849-1899 (Node; check-runtime) e check-api. **Na tela, nenhum teste que roda** | INCERTA | Reverter: tirar `conteudoDasContribuicoes("canal.cabecalho", …)` de ts:1366 → prevejo que nada reprova: rp só mede o `substituir`, e o guarda `todo_ponto_..._tem_quem_o_aplique` (frontend.rs:17373) passa pelo `escolherSubstituicao("canal.cabecalho"` de ts:1349. Só bc mede o `adicionar` (não roda) |
| `regioes.canal.cabecalho.produto[0]` | o nome do canal continua do produto | index.html:1568 (`#linha-nome`) | rp `doProduto/aVista/descobertos`, com o miolo substituído e conteúdo grande mirado; CI(nav) | PROVADA | Medido sob `substituir`; o `adicionar` não é medido |
| `regioes.canal.cabecalho.produto[1]` | a busca continua do produto | index.html:1598 (`#botao-buscar`) | rp, idem; CI(nav) | PROVADA | idem |
| `regioes.canal.cabecalho.celular.existe` | existe no celular | cc:56; cs:598-614 | bc `regioes-do-celular` — **não roda** | SEM PROVA | |
| `regioes.canal.cabecalho.celular.forma` | compacta, botões de 44 px | cs:598-614 (`min-height: 52px`, `#botao-buscar` 44 × 44); cb:99-103 | bc — **não roda** | SEM PROVA | |
| `regioes.conversa.mensagem.modos[0]` | aceita `substituir` (o corpo, por mensagem) | mc:99; ts:1667-1685 (`.mensagem-miolo`) | rp «a mensagem, substituída» (:475-570): corpo do MOD, cabeça, REMOVER, vizinha intacta, busca devolve o corpo nativo, pendente fora; CI(nav). Tabela: check-api | PROVADA | |
| `regioes.conversa.mensagem.modos[1]` | aceita `adicionar` | mc:99; ts:1686 | rp mensagem: `acrescida` em alvo 11 → `m.acrescimo`; CI(nav) | PROVADA | |
| `regioes.conversa.mensagem.porMensagem` | por mensagem (`alvo` = a mensagem) | mc:99 (`porAlvo: true`); ts:1667 (alvo = `mensagem.id`, nunca a pendente) | rp mensagem: `vizinha` (a substituição da 11 não alcança a 12) e `pendente` (sem alvo, a pendente fica nativa); CI(nav) | PROVADA | Fora da seção, mas ligado: o código põe `canal.cabecalho` e `conversa.mensagem` **por alvo** (fora do teto de 128, no de 256 alvos), e `contribuicoes.teto-o-que-e` só lista `pessoa.*, canal.item, sala.acoes` como por alvo — para a frente de `contribuicoes` |
| `regioes.conversa.mensagem.pontos[0]` | `pessoa.avatar` mora na mensagem | ts:1625-1630 (`vestirAvatar(.mensagem-avatar, author)`) | `avatares-do-mod.cjs`:59-66 (`.mensagem-avatar[data-pessoa-do-avatar="1"]` com a foto do MOD); CI(nav) | PROVADA | Com a mensagem substituída, a calha fica (ts:1625, antes do miolo), mas nada mede |
| `regioes.conversa.mensagem.produto[0]` | o autor é a chave (agrupamento pela chave) | ts:1848-1851 (`anterior.author === mensagem.author`) | rp mensagem: duas Lias de autores diferentes, cada uma com a cabeça (`cabecas[1] === "Lia"`); CI(nav) | PROVADA | O plano diz que reverter para o apelido reprova |
| `regioes.conversa.mensagem.produto[1]` | o apelido verificado à vista | ts:1636-1639 (`.mensagem-autor` = `author_nickname` do servidor) | rp mensagem: com o corpo do MOD, `autor === "Lia"` e o REMOVER presente; CI(nav) | PROVADA | O rascunho diz «lido pela chave»; o código usa o `author_nickname` que o servidor grava com a mensagem — também fora do alcance do MOD |
| `regioes.conversa.mensagem.celular.existe` | existe no celular | cc:56 | bc `regioes-do-celular` — **não roda** | SEM PROVA | |
| `regioes.compositor.modos[0]` | aceita `adicionar` (e só ele) | mc:96; ts:1376 (`#compositor-mods`; vazio sem canal aberto) | rp `REGIOES` compositor (:52-58); CI(nav). Tabela: check-api | PROVADA | |
| `regioes.compositor.pontos[0]` | `compositor.ferramentas` mora no compositor | index.html:1777 (`#compositor-ferramentas` dentro de `#form-mensagem`); ts:1384-1395 | `diagnostico-de-mods.cjs`:991 (o contorno aparece) e :478-493 (a frase da aba) — só existência; CI(nav) | INCERTA | Nenhum teste que roda põe uma ferramenta na tela e a procura dentro do compositor. Reverter: tirar `acoesDasContribuicoes("compositor.ferramentas"…)` de ts:1389 → prevejo que nada reprova (o contorno é do contêiner vazio; a aba lê o registro) |
| `regioes.compositor.produto[0]` | ENVIAR continua do produto | index.html:1778 (`#botao-enviar`) | rp `doProduto/aVista/descobertos`; CI(nav) | PROVADA | |
| `regioes.compositor.produto[1]` | o anexo continua do produto | index.html:1735 (`#botao-anexar`) | rp `doProduto` (marcação e tamanho); CI(nav) | PROVADA | Não está em `descobertos`. No celular o `#botao-anexar` é escondido (cs:700-703) e o anexo é o `.cel-anexar` |
| `regioes.compositor.celular.existe` | existe no celular | cc:57 | bc `regioes-do-celular` — **não roda** | SEM PROVA | |
| `regioes.compositor.celular.forma` | campos de 16 px, botões de 44 × 44 | cb:24, :33, :99-110 (todo `input/textarea` ≥ 16 px, todo botão ≥ 44 × 44); cs:719-736 | bc (44 de alvo; não achei medida do 16) — **não roda** | SEM PROVA | |
| `regioes.pessoas.lista.modos[0]` | aceita `substituir` (`{ grupos }` dentro de cada sala) | mc:98; mc:193-198; ts:2196-2203 | rp «a lista de pessoas, substituída» (:408-473): seções do produto, OUTRAS PESSOAS, recusa da outra forma, DIAGNÓSTICO conta o ignorado; CI(nav). Tabela: check-api | PROVADA | |
| `regioes.pessoas.lista.modos[1]` | aceita `adicionar` | mc:98; ts:2196 (`#roster-mods`) | rp `REGIOES` pessoas.lista (:75-85); CI(nav) | PROVADA | |
| `regioes.pessoas.lista.pontos[0]` | `pessoa.identidade` mora na lista | ts:2678-2707 (`.pessoa-identidade` na linha) | `diagnostico-de-mods.cjs`:991 (o contorno aparece) — só existência; CI(nav) | INCERTA | Reverter: não chamar o `conteudoDasContribuicoes("pessoa.identidade"…)` de ts:2707 → prevejo que o diagnóstico passa |
| `regioes.pessoas.lista.pontos[1]` | `pessoa.cartao` mora na lista | ts:2471-2520, :2566-2600 (`li.pessoa`, `linhaSubstituida`) | rp «a linha de uma pessoa» (:324-406) e «atualizar» (:579-647, vigia em `#lista-roster`); CI(nav) | PROVADA | |
| `regioes.pessoas.lista.pontos[2]` | `pessoa.detalhes` mora na lista | ts:2599-2607 (`.pessoa-nativo`, no «⋯») | só o orçamento de rp (:794-835), que monta fora do documento (`montarContribuicao`) | SEM PROVA | Nada põe um `pessoa.detalhes` na linha e o procura |
| `regioes.pessoas.lista.pontos[3]` | `pessoa.acoes` mora na lista | ts:2718-2760 (`.pessoa-rodape`) | `diagnostico-de-mods.cjs`:991 — só existência; CI(nav) | INCERTA | Reverter: não chamar `acoesDasContribuicoes("pessoa.acoes"…)` de ts:2757 → prevejo que passa |
| `regioes.pessoas.lista.pontos[4]` | `pessoa.avatar` mora na lista | só por citação no cartão: mods-regiao.js:1967-1968, :2019-2023 → b:1534-1547 (`imagemCitada`). A linha nativa **não** veste avatar (`vestirAvatar` só em ts:1207, :1629, tela-chamada.js:606) | `capa.cjs`:394-438 cita `retrato({ daPessoa: 'avatar' })` **sem avatar registrado** e espera `inicial`; CI(nav) | INCERTA | Reverter: `imagemCitada("avatar", …)` devolvendo sempre `null` → `capa.cjs` continua verde. Falta o caso com avatar registrado |
| `regioes.pessoas.lista.pontos[5]` | `pessoa.capa` mora na lista | só por citação: mods-regiao.js:1842-1849 (`fundoDeMidia.daPessoa`) → b:1534-1547 | `capa.cjs`:394-438 (o miolo de `li.pessoa-apresentada` com a capa de fundo, sem pedido novo); CI(nav) | PROVADA | Só pela citação (como o rascunho §4 diz). Ver `celular.forma` abaixo: ela **não** fica parada |
| `regioes.pessoas.lista.produto[0]` | o menu de moderação continua do produto | ts:2650 (`.pessoa-nome-porta`, `data-moderar-pessoa`); index.html:4495 (`#moderar`) | rp «a linha de uma pessoa» (moderação a um clique, fora do «⋯», com `pessoa.cartao` substituído); `nucleo.cjs`:203-215 (`#moderar` fora de todo contêiner de MOD); CI(nav). **O caso de `pessoas.lista` substituída não olha a moderação** | INCERTA | Reverter: no braço do `arranjo` de ts:2203, linhas sem a porta `data-moderar-pessoa` → prevejo que rp passa (o caso da lista só lê `.pessoa-nome`). No celular a moderação não existe na lista (celular-pessoas.js:6-8) — o contrato não diz |
| `regioes.pessoas.lista.produto[1]` | o apelido verificado continua do produto | ts:2577-2600 (`.pessoa-nucleo`), :2650 | rp «a linha de uma pessoa» (o cartão escreve o nome de outra pessoa presente; o núcleo diz o verdadeiro, à vista, descoberto, fora da raiz do MOD) e «a lista substituída» (os nomes de cada linha); CI(nav) | PROVADA | |
| `regioes.pessoas.lista.celular.existe` | existe no celular | cc:57; cs:61-71 (aba PESSOAS) | bc `regioes-do-celular` — **não roda** | SEM PROVA | |
| `regioes.pessoas.lista.celular.forma` | gaveta PESSOAS, célula inteira; **a capa da linha fica parada** | cs:61-71 (célula inteira: ok; é uma **aba**, não gaveta); b:1541-1544 (`parada` **só com `prefers-reduced-motion`**) | nenhuma | SEM CÓDIGO | A capa citada no cartão da linha usa `estado.uri` (a animada) sempre que não há movimento reduzido, e não conta no teto de 6 capas animadas (`escolherAsCapasAnimadas`, ts:5228-5239, só vê `[data-pessoa-da-capa]`). Nada no celular a para. Ou o texto cai, ou `imagemCitada` passa a devolver o `estatico` para a linha |
| `regioes.status.barra.modos[0]` | aceita `adicionar` (e só ele) | mc:97; ts:2898 (`#telemetria-mods`) | rp `REGIOES` status.barra (:59-74), com o `#tel-local` à vista; CI(nav). Tabela: check-api | PROVADA | |
| `regioes.status.barra.produto[0]` | atraso continua do produto | index.html:1936 (`#tel-rtt`) | rp `doProduto/aVista/descobertos`; CI(nav) | PROVADA | |
| `regioes.status.barra.produto[1]` | jitter continua do produto | index.html:1937 (`#tel-jit`) | rp `doProduto` (marcação e tamanho); CI(nav) | PROVADA | Fora de `aVista/descobertos` |
| `regioes.status.barra.produto[2]` | distúrbio continua do produto | index.html:1938 (`#tel-loss`) | rp `doProduto`; CI(nav) | PROVADA | idem |
| `regioes.status.barra.produto[3]` | codec continua do produto | index.html:1939 (`#tel-codec`) | rp `doProduto/aVista/descobertos`; CI(nav) | PROVADA | |
| `regioes.status.barra.celular.existe` | **não** existe no celular | cc:47-58 (o `Set` sem `status.barra`); cs:17-24 (o rodapé escondido); b:3433-3438, :3464-3486 | rp «a região que a casca não tem» (:649-708) — com um `CELULAR` **simulado** sem `status.barra`: não desenha, um aviso só, fora do retrato, DIAGNÓSTICO conta, volta quando a região volta; CI(nav). bc `regioes-do-celular` mede a casca de verdade — **não roda** | INCERTA | O mecanismo está provado; a declaração da casca, não. Reverter: pôr `"status.barra"` no `Set` de cc:55-58 → prevejo que nada que roda reprova. O jeito barato: `identidade-no-navegador.cjs` caso 9 já sobe a casca do celular de verdade no CI — conferir `cascaDoRetrato().regioes` ali |
| `regioes.status.barra.celular.forma` | a casca tira o rodapé; quem contribui recebe `regiao-ausente`; no PC no navegador existe | cc:47-58; cs:20; b:3464-3486 (evento `contribuicao`, `motivo: 'regiao-ausente'`, linha no `seele.log`); sem `CELULAR`, todas (b:3433-3438) | rp «a região que a casca não tem» (simulado); `identidade-no-navegador.cjs`:591-611 (a casca larga não tem `CELULAR`) — CI(nav)/bancadas-de-mod | INCERTA | Mesma reversão de cima. «No PC no navegador» só se prova indiretamente: nenhuma bancada registra em `status.barra` na casca larga do navegador |

## As que não são PROVADA, em uma linha

SEM CÓDIGO (3):
- `servidor.navegacao.pontos[1]` (`sala.acoes`): mora em `pessoas.lista` (ts:2381-2390; camada-mods.js:1518), não na navegação. Corrigir o texto (próxima versão) — e `pessoas.lista.pontos` não o tem.
- `redes.celular.forma`: os itens são 44 × 44 (cs:412-418, `--cel-alvo`), não 56 × 44. Número herdado do `arquivo/mobile-ios`.
- `pessoas.lista.celular.forma`: «a capa da linha fica parada» — a citação é animada salvo com movimento reduzido (b:1541-1544).

INCERTA (11): `canal.cabecalho.modos[1]`; `servidor.navegacao.produto[0]` e `[1]`;
`servidor.navegacao.pontos[0]`; `compositor.pontos[0]`; `pessoas.lista.pontos[0]`, `[3]`, `[4]`;
`pessoas.lista.produto[0]`; `status.barra.celular.existe` e `.forma`.

SEM PROVA (13): `pessoas.lista.pontos[2]` (`pessoa.detalhes`), e as 12 do celular que só a
`bancada-do-celular.py` mede (`*.celular.existe` das sete regiões, as formas de
`servidor.navegacao`, `operador.barra`, `canal.cabecalho`, `compositor`, e
`operador.barra.celular.contribuicao`).
