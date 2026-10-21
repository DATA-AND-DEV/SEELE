# Relatório do lote 58-1 (F5, 0.16)

As recusas de instalar um MOD e de apagar do cache: os itens 8, 9, 10 e 11 da
pendência 58. Elas mostravam um código cru, culpavam o catálogo por uma falha do
disco, ou sumiam sem linha no `seele.log`.

- Worktree: `/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a4b03b0faf88b8140`
- Branch: `0.16/lotes-58-1`, aberto em `bfad7594`.
- Status: DONE_WITH_CONCERNS. As ressalvas estão no fim, e nenhuma bloqueia.
- O `docs/pendencias.md` não mudou: quem fecha os itens é a F5, no fim.

## Os itens e os commits

| Item | Commit | Arquivos |
|---|---|---|
| 8 · `ManifestoRecusado` com o código cru | `676d56c1` | `ui/frases.js` (1 linha), a bancada |
| 9 · a recusa local de um MOD do catálogo | `630ef5be` | `src/catalogo.rs`, `src/mods.rs`, `ui/frases.js` (3 linhas), a bancada |
| 10 · apagar do cache sem frase, e o erro do disco calado | `d359f0ea` | `ui/frases.js` (4 linhas), `src/mods.rs`, a bancada |
| 11 · a migração do layout antigo que pula calada | `c041a256` | `src/mods.rs` |

**A bancada não é nova.** As frases são provadas numa seção 5 da
`apps/seele-app/bancada/hospedar-com-mod-antigo.cjs`, que já carrega o `frases.js`
de verdade num contexto de VM e já está no `BANCADAS` do
`xtask/src/check_runtime.rs`. Por isso o `ci.yml`, o `publicar.sh` e o
`check_runtime.rs` não mudaram. O `main.rs`, o `frontend.rs` e o `assento.rs` também
não.

## O que mudou, medido

As frases, com o `fraseDeErro` do arquivo rodado no `node`:

| Erro | Antes | Agora |
|---|---|---|
| `{"ManifestoRecusado": "api-too-old"}` | `ESTE SEELE RECUSOU O «mod.json» DESTA PASTA: api-too-old.` | `… DESTA PASTA: ele é de uma API que roda no Servidor 0.15, e não neste app; procure a versão dele para a API 6, ou use o Servidor 0.15 pelo launcher.`<br>`Nada foi instalado.` |
| `{"NaoInstalou": {"NaoCopiei": "No space left on device (os error 28)"}}` (antes `NaoEUmCatalogo`) | `O CATÁLOGO VEIO QUEBRADO. … Avise quem publica.` | `NÃO CONSEGUI COPIAR ESTE MOD PARA O SEELE: No space left on device (os error 28).`<br>`Nada foi instalado. Confira o espaço e as permissões do disco, e tente de novo.` |
| a passagem que não se escreve (antes `NaoRespondeu`) | `O CATÁLOGO NÃO RESPONDEU. Pode ser a sua rede…` | a mesma de `NaoCopiei`, acima |
| `{"Recusado": {"motivo": "nao-esta-aqui"}}` | `nao-esta-aqui` | `Este pacote já não estava no cache.` |
| `{"Recusado": {"motivo": "nao-apaguei"}}` | `nao-apaguei` | `O disco não deixou apagar este pacote, e o motivo está no seele.log. Feche o que usa o MOD e tente de novo.` |
| `{"Recusado": {"motivo": "hash-torto"}}` | `hash-torto` | `Este pedido não nomeia um pacote do cache, e nada foi apagado.` |

As linhas novas do `seele.log`, lidas do rastro capturado nos testes:

```
WARN seele_app::catalogo: não instalei o MOD do catálogo; a pasta de passagem já foi apagada mod_id=alguem/rpg versao="1.0.0" recusa=ManifestoRecusado("api-too-new") passagem="/var/folders/…/T/seele-mod-78596-1792615059329356000"
WARN seele_app::catalogo: não instalei o MOD do catálogo; a pasta de passagem já foi apagada mod_id=alguem/rpg versao="1.0.0" recusa=NaoCopiei("Is a directory (os error 21)") passagem="…"
WARN seele_app::catalogo: não instalei o MOD do catálogo; a pasta de passagem já foi apagada mod_id=alguem/rpg versao="1.0.0" recusa=NaoCopiei("Permission denied (os error 13)") passagem="…"
WARN seele_app::mods: o disco não deixou apagar este pacote do cache hash=aaaa… erro=Permission denied (os error 13)
WARN seele_app::mods: não consegui levar este pacote: o mod.json foi recusado mod_id=seele/velho recusa=api-too-new pasta="…/config/mods/seele/velho"
WARN seele_app::mods: não consegui levar este pacote: ele tem um atalho mod_id=seele/exemplo atalho="…/config/mods/seele/exemplo/atalho.txt"
WARN seele_app::mods: não consegui levar este pacote: a pasta do cache não se cria mod_id=seele/exemplo erro=File exists (os error 17)
```

### Item 9, em detalhe

- `FalhaNoCatalogo` ganha `NaoInstalou(FalhaAoInstalarMod)`. O `frases.js` a manda
  ao `fraseDeErro` da recusa de dentro, e a frase é a de instalar que já existia.
- O trecho depois de `conferir_conjunto` virou `instalar_os_baixados`, síncrona, para
  a recusa local se provar sob `rastro_de_teste::capturar`.
- **Ela publica por `mods::publicar_de`, e não por `instalar_de`.** Com `instalar_de`,
  a recusa ganhava duas linhas: a de lá, com `origem=` na passagem («não instalei o
  MOD desta pasta», a pasta que a pessoa não escolheu), e a nova. Agora é uma só, e a
  passagem aparece só como `passagem=`, já apagada. Para isso o `publicar_de` virou
  `pub(crate)`, e o comentário dele diz os dois que o chamam.
- As falhas de escrever a passagem são `NaoCopiei`. **E a passagem passa a ser
  apagada também quando a escrita falha**: antes, o `?` saía antes do
  `remove_dir_all`, e ela ficava em `temp_dir()`.
- `mod_id` vai por `id_no_registro`, e `versao` pelo `Debug`. Os dois chegam da janela,
  e só são lidos depois de o `planejar` conferi-los contra o catálogo assinado; o
  escape é para não depender disso.

**Quem mais faz `match` em `FalhaNoCatalogo`** (o brief a chama de `ErroDoCatalogo`,
e o nome no código é este): só o `frases.js`, que mudou. O `frontend.rs` não a lê (o
guarda das recusas confere `FalhaAoHospedar`, `FalhaAoEscolher`, `FalhaNaPortaria`,
`FalhaAoInstalarMod` e `FalhaAoResolverModAntigo`). A ponte web tem
`instalar_mod_do_catalogo` em `AINDA_A_FAZER` (`ponte-web.js`), sem corpo, e o
celular passa por ela. O `main.rs` só constrói `NaoAplicou`. Os roteiros conferem
`O CATÁLOGO NÃO É O NOSSO.`, que não mudou. Nenhum deles foi tocado.

### Item 11, em detalhe

Os três `continue` calados ganharam linha:

- o `mod.json` recusado. O `mod_id` sai das pastas, porque não há manifesto lido: o
  layout antigo era `mods/<autor>/<nome>/`. Vai por `id_no_registro`, com a recusa e
  a pasta;
- a cópia que falha. O atalho vai por `caminho_no_registro`; uma falha do disco vai
  com o erro e a pasta. O caso do disco não estava no esboço da triagem, e ganhou
  teste próprio;
- a pasta do cache que não se cria, com o erro.

A metade de `into_path` do item fica, como a triagem decidiu: o diálogo de pasta
sempre devolve caminho, e ela mora no `main.rs`.

## A prova: o vermelho antes, e cada reversão

Os testes de Rust rodaram com HOME falsa, por
`scratchpad/lote-58-1/falsa.sh`. As reversões, por
`scratchpad/lote-58-1/reverter.py`: ele desfaz o trecho, roda a prova, restaura o
arquivo com `copyfile` (a data de agora, para o cargo não reaproveitar o binário da
reversão) e confere com `cmp`. Todas voltaram idênticas.

| Item | O vermelho antes do conserto | A reversão | Reprova em |
|---|---|---|---|
| 8 | os nove códigos de `RECUSAS_DE_PACOTE` saem crus | a linha de antes | 5a: `malformed, schema-too-new, api-too-new, api-too-old, malformed-id, empty, tema-sem-metade-de-servidor, arquivo-invalido, arquivos-demais` |
| 9 · Rust | `Err(NaoEUmCatalogo("ManifestoRecusado(\"api-too-new\")"))` e `Err(NaoEUmCatalogo("NaoCopiei(\"Permission denied…\")"))` | (a) `NaoEUmCatalogo` no lugar da variante | os 3 testes de `o_catalogo`, pela variante |
| | | (b) sem a linha WARN | os 3, pela linha |
| | | (c) `instalar_de` no lugar de `publicar_de` | os 2 que chegam a publicar, por `origem=` |
| | | (d) o código inteiro de antes | os 3; a passagem sai `Err(NaoRespondeu("Is a directory (os error 21)"))` |
| 9 · frase | `ALGO FALHOU E ESTE APP NÃO SABE EXPLICAR O QUÊ` | sem o ramo `NaoInstalou` | 5b |
| 10 · frase | os três motivos crus | sem as três frases | 5c: `nao-esta-aqui, nao-apaguei, hash-torto` |
| 10 · linha | sem linha WARN, com `Permission denied (os error 13)` | sem a linha | o teste `cfg(unix)` do disco |
| 11 | rastro vazio nos dois testes | uma reversão por linha (4) | cada uma no seu teste, e só nele |

Os testes novos:

- em `catalogo.rs` (`o_catalogo`): `o_mod_json_recusado_de_um_mod_do_catalogo_nao_culpa_o_catalogo`,
  `a_passagem_que_nao_se_escreve_nao_culpa_a_rede` e, em `cfg(unix)`,
  `o_disco_que_nao_deixa_instalar_um_mod_do_catalogo_nao_culpa_o_catalogo`;
- em `mods.rs` (`instalar`): `o_disco_que_nao_deixa_apagar_um_pacote_fica_no_registro_com_o_erro`
  (`cfg(unix)`), `a_migracao_que_pula_um_pacote_deixa_linha_com_o_mod_e_a_recusa` e, em
  `cfg(unix)`, `a_migracao_que_pula_um_pacote_com_atalho_diz_o_atalho` e
  `a_migracao_que_pula_um_pacote_que_nao_se_copia_diz_o_erro`.

Os de permissão conferem antes que a tranca vale, e reprovam com frase se o teste
rodar como root.

## Os guardas do fim

Sobre `c041a256`, por `scratchpad/lote-58-1/guardas.sh`, com os logs em
`scratchpad/lote-58-1/guardas-1/`:

- `cargo fmt -p seele-app -- --check`: limpo;
- `cargo --offline test -p seele-app --no-fail-fast`, com HOME falsa: **803 passaram,
  0 falharam**, em 11 binários (log de 70 259 bytes). Os sete testes novos estão entre
  eles. Nenhuma HOME falsa ganhou `.config/seele`;
- `cargo --offline clippy -p seele-app --all-targets -- -D warnings`: limpo;
- `cargo --offline xtask check-runtime`: as 14 bancadas passam, a
  `hospedar-com-mod-antigo.cjs` com a linha nova da seção 5;
- as 20 bancadas do job `bancadas` do `ci.yml`, lidas dele, com o
  `PLAYWRIGHT` das regras: as 20 passam;
- os 46 roteiros de `tools/roteiros/`, numa chamada da `carga-da-casca.py`, lidos do
  `ci.yml` (46 na chamada, 46 na pasta): «os 46 passaram».

O único log vazio é o do `fmt`, que não escreve nada quando passa.

## Ressalvas

- **A frase de instalar diz «DESTA PASTA» também para um MOD do catálogo.** É a frase
  que já existia, como o brief pede, mas quem instalou pelo catálogo não escolheu
  pasta nenhuma. Trocar pede uma frase própria para o catálogo, e o brief manda
  reaproveitar.
- **A recusa de `api-too-old` vira uma linha WARN a cada abertura.** A migração roda
  no `setup`, e um pacote do layout antigo de antes da 0.12 é recusado pelo
  `ler_pasta` antes do `destino.exists()`, mesmo que uma versão dele já tenha sido
  levada. Quem veio da 0.11 com os três MODs oficiais lá ganha três linhas por
  abertura, até apagar `mods/`. A linha é verdadeira, e foi o que a triagem pediu; o
  custo é ruído.
- **A frase de `api-too-old` tem 199 caracteres**, porque interpola a frase do
  dicionário. Nenhum guarda de tamanho a mede (o `LIMITE_DE_FRASE`, 180, vale para as
  frases do dicionário), e a de `PacoteNaoServe` já é assim.
- **O target foi semeado de `/Users/dev-alexandre/SEELE/target`.** A pasta que a
  versão das regras que li primeiro apontava, a do worktree
  `eloquent-blackburn-a0bf2c`, não tinha `target`. A regra foi corrigida depois para
  este mesmo caminho. O `Cargo.lock` dos dois é igual.
- **As primeiras reversões restauraram com `copy2`**, que guarda a data velha, antes
  de eu ler a regra nova sobre isso. Nenhum resultado dependeu disso: cada reversão
  reescreve o arquivo e recompila, e depois da última delas eu dei `touch` e rodei de
  novo, recompilando, antes de qualquer conclusão. Daí em diante o `reverter.py`
  restaura com `copyfile`.

## Rodada 1

A revisão aprovou com ressalvas: 0 Críticos, 1 Importante e 4 Menores, e os cinco
entraram. Cada um num commit isolado, com o vermelho antes e a reversão conferida
com `cmp`. Os fontes foram restaurados por escrita nova (`copyfile`), sem a data velha.

| Achado | Commit | O que mudou | O vermelho, e a reversão |
|---|---|---|---|
| I1 · «DESTA PASTA» numa instalação pelo catálogo | `bcf1e8ab` | No ramo `NaoInstalou` do `fraseDeErro`, um `ManifestoRecusado` diz «ESTE SEELE RECUSOU O «mod.json» DESTE MOD: <frase do dicionário>.\nNada foi instalado.». Os outros casos seguem pela frase de instalar. O 5b da bancada exige, nesse caso, nenhuma «PASTA», o «DESTE MOD», a frase do dicionário e «Nada foi instalado»; nos outros, a igualdade de antes. | Com o `frases.js` de antes, o 5b reprova: «fala de uma pasta que ninguém escolheu». |
| M1 · a passagem apagada, sem prova | `cfbddb57` | O `exige_a_recusa_local` lê da linha WARN a pasta em `passagem="…"` (desfazendo o `\\` do `Debug`), confere que ela é uma passagem em `temp_dir()` (a conferência não fica vazia) e exige que não exista. A linha deixa de afirmar «já foi apagada»: o resultado do `remove_dir_all` vai em `passagem_apagada=`. | Vermelho: os três testes, sem `passagem_apagada=true` na linha. Reversão: `let apagada = true;`, sem o `remove_dir_all`, e os três reprovam com «a pasta de passagem ficou em `temp_dir()`». As três passagens que essa reversão deixou foram apagadas. |
| M3 · a forma serde de `NaoInstalou` | `879576cf` | `a_recusa_local_chega_a_janela_na_forma_que_a_frase_le`, em `o_catalogo`: o `serde_json::to_value` de `NaoCopiei`, `ManifestoRecusado`, `TemAtalho` e `SemManifesto` é a forma que a bancada monta. | Dois mutantes reprovam: `#[serde(rename = "FalhaLocal")]` na variante, e `#[serde(tag = "tipo", content = "detalhe")]` na `FalhaAoInstalarMod`. |
| M2 · «[object Object]» na busca automática | `a905b3f1` (só o `base.js`, 1 linha) e `38adb725` (a bancada) | `buscarOQueFaltaSeJaFoiAceito` anota a falha pelo recuo do `atender-mod`: `typeof fraseDeErro === "function" ? fraseDeErro(falha) : String(falha?.message ?? falha)`. O texto «reconecte para tentar de novo» do `camada-mods.js` não mudou. A prova é a 5d da `hospedar-com-mod-antigo.cjs`: ela recorta a função do `base.js` e a roda com o `frases.js` de verdade e um `invoke` que rejeita com `{NaoInstalou: {NaoCopiei: …}}`. | Com a linha de antes, a 5d reprova: «a linha do MOD que não veio diz «[object Object]»». |
| M4 · «roda no Servidor 0.15» para uma API de 0 a 2 | `954d9a4d` | `RECUSAS_DE_PACOTE["api-too-old"]` passa a «ele é de uma API mais velha que a 6, a deste app; procure a versão dele para a API 6, ou use o Servidor 0.15 pelo launcher, que roda as APIs 3 a 5». A seção 3 da bancada lê `APIS_POR_LINHA` em `crates/seele-proto/src/mods.rs` e exige da frase as APIs da linha 0.15 (seguidas, «3 a 5») e a da linha deste build («6»). | Vermelho, e reversão: a frase de antes reprova («uma API 2 não roda»). E a tabela com a 0.15 em `[5, 4]` reprova («não diz que o Servidor 0.15 roda as APIs 4 a 5»): a bancada lê a tabela de fato. |

### As frases, depois da rodada

| Erro | Frase | Caracteres |
|---|---|---|
| `{"NaoInstalou": {"ManifestoRecusado": "api-too-new"}}` | `ESTE SEELE RECUSOU O «mod.json» DESTE MOD: ele é de uma API mais nova que a deste app; atualize o aplicativo.`<br>`Nada foi instalado.` | 129 |
| `{"NaoInstalou": {"ManifestoRecusado": "api-too-old"}}` | `ESTE SEELE RECUSOU O «mod.json» DESTE MOD: ele é de uma API mais velha que a 6, a deste app; procure a versão dele para a API 6, ou use o Servidor 0.15 pelo launcher, que roda as APIs 3 a 5.`<br>`Nada foi instalado.` | 210 |
| `{"ManifestoRecusado": "api-too-old"}` (de uma pasta) | a mesma, com «DESTA PASTA» | 212 |
| `{"PacoteNaoServe": "api-too-old"}` | `A VERSÃO NOVA DESTE MOD NÃO SERVE: <a mesma frase>.`<br>`O MOD continua como estava.` | 210 |

### Os guardas do `frontend.rs`, conferidos e sem mudança

O `frontend.rs` não foi tocado, e nenhum guarda dele precisou mudar.

- **`todo_pacote_recusado_e_dito_com_uma_frase`** cobra só as chaves do
  `RECUSAS_DE_PACOTE` contra o `refusal_name`, e o `RECUSAS_DE_PACOTE[mod.refused]` da
  gestão. O texto do `api-too-old` não entra nele.
- **O `LIMITE_DE_FRASE` (180)** não mede o `RECUSAS_DE_PACOTE`, que não está nos
  `DICIONARIOS`, nem as frases compostas a partir dele. A composta do `api-too-old`
  passou de 199 para 210 e 212 caracteres. A revisão mediu a de 199 nos quatro lugares
  em que ela aparece, e cabia; a de 212 eu não medi no `index.html`.
- **`so_a_atualizacao_pela_gestao_reaplica_o_mod_no_servidor_hospedado` passa por
  coincidência com a linha do M2.** Ele colhe como «fase» toda literal minúscula dentro
  de uma chamada a `anotarEstadoDoMod(…)`, e a linha nova traz o `"function"` do
  `typeof`. Emulei a leitura do guarda: as fases eram 10, e passam a 11, com
  `function`. Ele passa porque o `camada-mods.js` contém `"function"` (5 vezes), e não
  porque a fase tem frase. Não mexi no guarda (o `frontend.rs` é do lote 58-4). Para a
  58-4: o guarda pode pular a literal que segue um `typeof … ===`.

### O que a rodada achou e não consertou

- **A mesma frase falsa em mais dois lugares,** fora do que o M4 pediu:
  - `camada-mods.js` (a linha do pacote no cache): «este pacote foi feito para a API
    ${dele}, que roda no Servidor 0.15». Um pacote da API 1 ou 2, como os do layout
    antigo que o item 11 agora registra, lê isso, e é falso;
  - `frases.js`, `ModsDeApiAntiga`: «${id}: feito para a API ${api}, que roda no Servidor
    0.15.». Ele é falso para um MOD ligado num banco antigo com a API 1 ou 2.
  Os dois têm o número da API na mão, e o conserto é dizer a frase só quando ela está
  em `APIS_POR_LINHA`. Ficam para quem decide o texto.
- **Os números do `api-too-old` estão escritos no `frases.js`.** Não há de onde lê-los
  ali: o `frases.js` é um dicionário estático, e o `seele-app` vê só o `seele-ffi`, que
  não exporta `APIS_POR_LINHA`. A ligação com a tabela é a da bancada, pelo texto do
  `seele-proto`.
- **Uma passagem de outro processo em `temp_dir()`** (`seele-mod-93534-…`), de antes desta
  rodada e não minha. Não a apaguei.

### Os guardas do fim da rodada

Sobre `954d9a4d`, pelo mesmo `scratchpad/lote-58-1/guardas.sh`, com os logs em
`scratchpad/lote-58-1/guardas-r1/`:

- `cargo fmt -p seele-app -- --check`: limpo (o único log vazio, porque ele não escreve
  nada quando passa);
- `cargo --offline test -p seele-app --no-fail-fast`, com HOME falsa: **804 passaram, 0
  falharam**, em 11 binários. São 803 mais o teste do M3. Nenhuma HOME falsa ganhou
  `.config/seele`;
- `cargo --offline clippy -p seele-app --all-targets -- -D warnings`: limpo;
- `cargo --offline xtask check-runtime`: as 14 bancadas passam. A
  `hospedar-com-mod-antigo.cjs` passa com a 5d («busca automática: o MOD que não veio
  diz a frase da recusa…»);
- as 20 bancadas do job `bancadas`: as 20 passam;
- os 46 roteiros, numa chamada da `carga-da-casca.py`: «os 46 passaram».
