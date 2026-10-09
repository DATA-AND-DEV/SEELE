# Os lotes abertos e as pendências, na 0.16 (frente F5)

Reescrita de `docs/superpowers/sdd/2026-10-04-abertos/lotes.md`, que foi escrito em `3685a64`. Esta versão vale para a 0.16.

- **Spec:** `docs/superpowers/specs/2026-10-08-0.16-design.md`, §2 «F5» e §4 «A ordem». Quando este plano e o spec discordarem, vale o spec.
- **Levantamento:** `docs/superpowers/sdd/2026-10-08-0.16/levantamentos/F5-lotes-e-pendencias.md`.
- **Base:** `versao/0.16` em `d4eb6f1`. Ela contém a `main` `54f542f`, que é a v0.15.1 publicada. Conferido em 08/10 pela API de releases de SEELE-RELEASES: `v0.15.1 · 54f542f94 · 2026-10-08T04:14:07Z`.
- **Branch da frente:** `0.16/lotes`. Cada lote roda em `0.16/lotes-<id>` e entra em `0.16/lotes` por `git merge --no-ff`, depois de revisado. Quem leva a frente a `versao/0.16` é a orquestradora.
- **Andamento:** `docs/superpowers/sdd/2026-10-08-0.16/F5/progress.md`.

Cada âncora abaixo foi conferida em `d4eb6f1` por leitura do código, em 08/10. A âncora é o **texto**: o nome da função, do teste ou da constante, ou uma frase que o `grep` acha. O número entre parênteses, como `(:2401)`, é só a linha em `d4eb6f1`, e anda com qualquer commit.

## O que mudou desde o plano de 04/10

1. **Só a API 6 de MODs roda no app 0.16.** O código das APIs 3, 4 e 5 sai, e quem o tira é a F3, depois do K3 e do K4. Por isso:
   - o P49-10 (a frase dos cartões da API 3) perdeu o objeto;
   - o K2 sai sem o filtro `cartao`;
   - o P51-28 (o vetor de referência) e o K1-m3 (a frase de tela no erro que o MOD recebe) foram absorvidos pela API 6;
   - toda promessa pública nova de API (evento, frase do `api/README.md`) vai para o rascunho da v6, e não para o `api/README.md` de hoje.
2. **A 0.16 quebra o protocolo.** Os riscos de «convivência com a 0.15» perderam o objeto: um cliente 0.15 não conversa com um servidor 0.16. Mas **nenhum lote muda o fio**: quem muda é a F1.
3. **O FORA DO AR** (D-fio-4): quem não está conectado aparece num grupo próprio, com a lista de membros vinda do servidor. Por isso o lote F prova a saída da lista de **presentes**, e não só o `PersonGone`.
4. **A pendência 52 foi decidida** (D-52): o áudio abre só ao entrar numa sala de voz e fecha ao sair. Isso vai com o lote I.
5. **O G se divide.** O G-servidor entra agora. O G-casca vai para o Plano 2, reescrito contra `Session.hospedagens` e a razão `ChamadaEncerrada`.
6. **O H é a metade cliente da «recusa legível» do Plano 3**, e entra antes do lado cliente dele.
7. **O N encolheu.** O P51-22 foi feito em `a77da2f`, e o P51-28 foi para a API 6. Sobram o P51-15 e o P51-21, que entram no lote das ferramentas junto com os minors 2 a 4 do L1.
8. **O Z é o fecho das pendências 49, 50 e 51.** A regra do G1, o placeholder e o rascunho das notas já foram feitos (`f54f4a7`) ou perderam o objeto (as notas foram direto para `empacotar/notas/0.15.1.md`).
9. **Entram** as pendências 53 e 54, os minors 2 a 4 do L1 e o K1-m3, este como nota para a F3. **A pendência 55 vai para o Plano 5.**
10. **Nove ondas viram seis**, e a ordem passa a ser fixada pelos arquivos que vários lotes editam.

## Regras comuns a todo lote

**O fio e as fronteiras**
- **Nenhum lote muda o fio.** Nenhum lote toca `crates/seele-proto/src/{control,version,media,screen}.rs`, que são da F1, nem o SEELE-ENC/1, que é da F2.
  - Uma variante nova de `DisconnectReason`, uma mensagem nova ou um campo novo é da F1. Se um lote precisar de um, ele para com NEEDS_CONTEXT.
  - Tipos internos à casca e à FFI (`EndReason`, `ConnectionError`, `FalhaAo*`) não são fio, e podem ganhar variantes.
- **A F6 é dona de** `crates/seele-lancador`, `apps/seele-app/src/versoes.rs`, `apps/seele-app/ui/camada-versoes.js` e `empacotar/manifesto.py`. Nenhum lote toca neles.
- **A API de MODs.**
  - `api/v1..v5.json` ficam intocados, e `cargo xtask check-api` e o guarda de congelamento continuam verdes.
  - Nenhum lote cria promessa pública nova de API: evento novo, campo novo no retrato, frase nova no `api/README.md`. O que for promessa nova vai, no relatório do lote, numa seção «Para a v6».
  - Nenhum lote tira o código das APIs 3, 4 e 5: isso é da F3, depois do K3 e do K4. Texto novo não escreve «API 3».
- A CSP não afrouxa. Nenhuma dependência nova, e nada baixado da rede.

**O método**
- **TDD.** O teste que falha é escrito antes do conserto, e roda vermelho pelo motivo certo.
- **Prova por reversão.** Todo guarda novo é provado desfazendo o conserto: o teste fica vermelho com a frase dele; depois o conserto volta e o arquivo é conferido byte a byte.
- Toda asserção nova leva mensagem em português, e a mensagem diz **o que quebra**.
- `unwrap`/`expect` só em teste. Nada de `x[i]` nem `s[a..b]` em `src/`. `missing_docs` e `unreachable_pub` avisam, e o clippy roda com `-D warnings`.
- Código com `cfg(windows)` ou `cfg(target_os = "linux")` não compila neste Mac: escreva-o com o mesmo cuidado, prefira lógica pura testável em qualquer plataforma e diga no relatório o que não rodou.

**Os testes**
- Só os do crate, do binário ou da bancada que o lote mexe:
  - `perl -e 'alarm 600; exec @ARGV' cargo test -p <crate> …`, porque o macOS não tem `timeout`;
  - `cargo xtask check-runtime` quando mexer em `apps/seele-app/ui/` ou em `apps/seele-app/bancada/`;
  - `cargo test -p seele-app --test frontend` quando mexer em `apps/seele-app/src/main.rs`, e não só o `--bin`: os guardas de `tests/frontend.rs` leem o `main.rs` como texto. Foi assim que o lote E deixou vermelho um guarda que nenhum teste do lote rodava (achado na conferência da onda 3, consertado em `9daa0c0`);
  - bancada de navegador sem baixar nada: `PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright perl -e 'alarm 240; exec @ARGV' node apps/seele-app/bancada/<bancada>.cjs`.
- **Nunca** `cargo test --workspace`, nem a bateria inteira. Duas baterias em paralelo disputam a porta UDP 8384, e a `vaga` só serializa dentro de um binário (`crates/seele-conformance/tests/vaga/mod.rs`, `fn minha`).
- Teste de conformidade toma `let _vaga = vaga::minha();` na primeira linha.
- **Só um binário que usa a 8384 roda por vez, na máquina inteira.** Hoje é só o `quarto_pelo_link`. Na onda 1, só o M1 o roda.
- Antes de cada commit:
  - `cargo fmt --all -- --check`, depois `cargo fmt --all`;
  - `cargo clippy -p <cada crate mexido> --all-targets --all-features -- -D warnings`. Quando o lote muda um item público de um crate, entram também os crates que dependem dele. O clippy do workspace roda na integração.
  - O aviso do linker do macOS (`ld: duplicate -rpath`) é ambiental.
- A bateria inteira roda na integração, numa fila só.

**O registro**
- Commits em português, no estilo do `git log`: uma frase que diz o que mudou. A última linha é `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Nenhum lote de código edita `docs/pendencias.md` nem o índice da 1.0. O relatório do lote diz, por item, o hash do commit que o fecha, e o lote Z registra.
- **G1:** nenhum lote ganha cadeia. É a regra «Quem ganha cadeia» do plano 1B, decidida pelo dono em 07/10.
- Nada de push, tag, release, Actions, VPS, SEELE-RELEASES ou SEELE-MODS-INDEXER. Nunca `git stash` puro.

## As ondas

A ordem é fixada pelos arquivos que vários lotes editam:
- `apps/seele-app/src/main.rs`: C, D, E, F, I e K4, em série;
- `apps/seele-app/ui/base.js`: K2, K5, J, K3 e K4;
- `apps/seele-app/tests/frontend.rs`: H, P53, J, K3 e I;
- `apps/seele-app/ui/frases.js`: H, P53, J e I;
- `crates/seele-core/src/enlace.rs`: H, M2, G-servidor e F;
- `apps/seele-app/ui/mods-regiao.js`: K2, K5, K3 e K4.

| onda | em paralelo | depois de |
|---|---|---|
| 1 | C · H (+K1-m1) · K2 (+K1-m2) · M1, sem `enlace.rs` · ferramentas (P51-15, P51-21, L1-m2, L1-m3, L1-m4) · P54 | — |
| 2 | D · K5 · M2 (+ a montagem de certificado do 1B) · P53 | D depois do C · K5 depois do K2 · M2 depois do M1 e do H · P53 depois do H |
| 3 | E · J · G-servidor (com os testes que deixam de falar com o ponto de produção) | E depois do D · J depois do K5, do P53 e do P54 · G-servidor depois do M2 |
| 4 | F · K3 (sem o P49-10) | F depois do E e do G-servidor · K3 depois do J |
| 5 | I, com o áudio que só abre na sala | I depois do F e do J |
| 6 | K4 | K4 depois do I e do K3 |
| fecho | Z | depois da onda 6 |

**Os arquivos de cada lote da onda 1**, que não se cruzam:

| lote | arquivos |
|---|---|
| C | `apps/seele-app/src/main.rs`, `apps/seele-app/src/executor.rs` (docs e a visibilidade do corte), `docs/superpowers/plans/2026-09-29-1.0-plano-1d-mods-na-casca.md` |
| H | `crates/seele-core/src/client.rs`, `crates/seele-core/src/enlace.rs`, `crates/seele-ffi/src/lib.rs`, `crates/seele-ffi/src/types.rs`, `apps/seele-app/ui/frases.js`, `apps/seele-app/tests/frontend.rs`, `crates/seele-conformance/tests/bateria_interna.rs` |
| K2 | `apps/seele-app/ui/mods-regiao.js`, `apps/seele-app/ui/base.js`, `apps/seele-app/ui/camada-mods.js`, `apps/seele-app/bancada/regiao-do-mod.cjs`, `apps/seele-app/bancada/contribuicoes-e-camadas.cjs` |
| M1 | `crates/seele-core/src/encontro.rs`, `crates/seele-proto/src/uri.rs`, `crates/seele-proto/src/encontro.rs`, `crates/seele-proto/tests/sementes_do_fuzz.rs`, `crates/seele-server/src/alcance/encontro.rs`, `crates/seele-conformance/tests/{quarto,quarto_pelo_link,ocupante_da_escuta,volta_pela_trilha}.rs`, `docs/adr/0047-o-link-que-volta-a-funcionar-amanha.md`, os planos 1A e 1E |
| ferramentas | `xtask/src/check_api.rs`, `.gitattributes`, `empacotar/publicar.sh`, `.github/workflows/ci.yml`, `fuzz/Cargo.lock`, `install.sh`, `xtask/tests/empacotamento.rs` |
| P54 | `apps/seele-app/ui/tela-server.js`, `apps/seele-app/bancada/qualidade-da-voz.cjs` |

O M1 toca `crates/seele-proto/tests/sementes_do_fuzz.rs`, que é vizinho da F1: ela semeia `control_frame` no mesmo arquivo. O M1 só mexe no trecho do `datagrama_do_encontro`. É risco de conflito de merge, e não de propriedade.

**Onde cada plano encaixa** (do spec, §2 F5):
- **Plano 3:** a parte do `seele-proto` começa na onda 1. O `session.rs` só depois do G-servidor, e o lado cliente só depois do I.
- **ENC/2:** depois do M2.
- **Planos 2 e 5:** o servidor a partir da onda 4, e a casca depois da onda 6. O G-casca vai com o Plano 2.
- **API 6:** a medição, o ADR 0056 e o `v6.json` desde a onda 1; o código depois do K3 e do K4.
- **Web app:** o servidor divide o `session.rs` com o Plano 3, então os dois andam em série.

# Onda 1

## Lote C — o registro da casca: a frase da janela num campo, um teto de vazão por MOD, o id sempre escapado e a recusa de carga por geração

**Onda 1.** Itens: P51-05, P51-08, P51-10, P51-12 e a metade do app do P51-26.

**Por que juntos.** Os quatro mexem nas mesmas funções do registro de `apps/seele-app/src/main.rs`:
- `registrar_da_janela` (o 5 e o 8);
- as recusas de mídia (o 8 e o 10);
- `mod_request` (o 10 e a metade do app do 26);
- as recusas de carga (o 12).

A pendência pede o 5 junto com o 8, e o 10 junto com o 26.

**Arquivos:**
- `apps/seele-app/src/main.rs`;
- `apps/seele-app/src/executor.rs`: só os docs do corte e, se for preciso, a visibilidade dele;
- `docs/superpowers/plans/2026-09-29-1.0-plano-1d-mods-na-casca.md`: uma linha.

**Estado em `d4eb6f1`:** nada feito. O `executor.rs` não mudou desde `3685a64`. O `main.rs` só andou nos anexos.

**Ordem:** P51-05, P51-08, P51-10 com a metade do P51-26, e P51-12.

### 1. P51-05 — a frase da janela vai num campo
- **Os três braços** de `fn registrar_da_janela(` (:2415-2417) escrevem a frase como mensagem: `tracing::warn!(onde = %onde, mod_id, "{o_que}")`, e o mesmo com `error!` e `debug!`. Eles passam a `tracing::warn!(onde = %onde, mod_id, frase = ?o_que, "a janela disse")`, e o mesmo com `error!` e `debug!`.
  - O `mod_id` verdadeiro passa a vir antes da frase.
  - Um `mod_id=` forjado dentro da frase fica entre aspas e escapado.
- **O corte.** Hoje é `.take(TETO_DA_FRASE_NO_REGISTRO)` dentro de `let o_que: String = o_que.chars()…` (:2399-2403). Ele passa a ser o corte do console, **com o aviso do corte**, porque em `?` um caractere chega a ocupar dez.
  - Hoje `executor::ate_o_teto_do_registro` (`executor.rs`, :749) devolve `&str`, sem aviso.
  - O aviso mora em `cortar_linha_do_console` (:724), que é privada e devolve `LinhaCortada`, também privada.
  - Exponha o corte com o aviso sem duplicar a regra: por exemplo, as duas passam a `pub(crate)`. A regra não é reescrita em `main.rs`.
- **Os docs:**
  - o de `const TETO_DA_FRASE_NO_REGISTRO` (:2286-2304) deixa de dizer que a janela «escreve a frase sem escape»;
  - o de `fn quebra_ou_inverte_a_linha(` (:2336-2349) tira «a frase» dos campos que vão crus. A troca por espaço fica, como segunda defesa;
  - em `executor.rs`, o de `cortar_linha_do_console` («os 512 caracteres que `registrar_da_janela` já aplicava») e o de `ate_o_teto_do_registro` («Uma regra só para as três portas») passam a contar a janela como mais uma porta.
- **Plano 1D:** só a linha «Esperado: no caso A…» (:3876), que descreve a forma da linha da janela. Os greps de :157 e :3874 continuam casando.

**Testes**, em `mod o_registro_da_janela` (:11646):
- com `o_que = "x onde=forjado mod_id=vitima/mod"` e `mod_id = Some("a/b")`, o primeiro `mod_id=` da linha é `mod_id=a/b`. Hoje é `vitima/mod`;
- `registrar_da_janela` entra em `o_pior_caso_sai_com_o_mesmo_teto_pelas_tres_portas` (:12240), com `campo(…, "frase")` e o aviso do corte tirado por `strip_suffix`, como o do console;
- `o_aviso_da_janela_sai_como_warn_com_o_id_do_mod_em_campo_proprio` (:11650) passa a achar a frase dentro de `frase="…"`;
- `o_id_do_mod_sai_igual_pelas_duas_portas_do_registro` (:12191) continua verde.

**Reversões:** voltar a `"{o_que}"` deixa vermelho o teste da forja; voltar ao `.take(…)` deixa vermelho o do pior caso.

### 2. P51-08 — um teto de vazão por MOD
- **O mapa.** `struct Session` (:77, `#[derive(Default)]`) ganha `baldes_do_registro: Mutex<HashMap<String, BaldeDoConsole>>`.
  - A chave é `id_no_registro(id)`, com uma chave fixa para a linha sem id (o catálogo).
  - O mapa tem no máximo 64 entradas. Cheio, um id novo usa um balde de transbordo compartilhado, e nunca passa sem teto.
- **A função extraída:** `registrar_da_janela_em(&baldes, agora: Instant, nivel, onde, o_que, mod_id)`, com o relógio como parâmetro. O comando passa a receber `State<Session>`, sem mudar os argumentos do invoke.
- **As outras portas.** `recusa_de_midia_dita` (:5330), `midia_servida_dita` (:5516) e `som_servido_dito` (:5590) também recebem `&baldes` e `agora`.
  - `recusa_de_midia_dita` é chamada também por auxiliares: `midia_declarada_do_mod`, `som_declarado_do_mod`, `midia_do_servidor_dita` e `ler_imagem_mod`. A assinatura nova se propaga por eles e pelos testes que os chamam.
  - `midia_declarada_do_mod` e `som_declarado_do_mod` são do K4, na onda 6, que vai partir desta assinatura.
- **O balde já tem tudo.** `BaldeDoConsole` (`executor.rs`, :486) já tem `admitir(agora)`, `pendentes_se_couber(agora)` e `pendentes()`, este desde `7953a3b`. Não há nada a acrescentar a ele.
  - O campo `suprimidas` segue o molde de `fn registrar_console_do_mod(` (:4173).
- **`fn revogar(&self)`** (:326): antes de esvaziar o mapa, uma linha por MOD com a conta pendente.

**Testes**, com o relógio fixo (`RAJADA_DO_CONSOLE` = 32):
- 200 chamadas para `a/b` no mesmo instante deixam exatamente 32 linhas;
- avançando 250 ms, a linha seguinte traz `suprimidas=168`;
- `c/d` não é segurado pelo balde de `a/b`;
- com o mapa cheio, um id novo passa pelo transbordo;
- `revogar` com conta pendente escreve a linha da conta;
- o mesmo vale para `recusa_de_midia_dita`.

Os testes de `o_registro_da_janela` passam a chamar a versão `_em`.

**Reversão:** tirar o `admitir` deixa os testes vermelhos. Hoje saem 200 linhas.

### 3. P51-10, com a metade do app do P51-26 — o id sempre escapado
- **O doc de `fn motivo_no_registro`** (o parágrafo «O escape é o que importa…», :5368-5371) passa a dizer: «o formatador escapa só a mensagem, e só ANSI; um campo em `%` sai cru, e quem protege a linha são as aspas e o escape desta função».
- **Os quatro `mod_id = %…` crus**, por função:
  - `mod_request`: o debug «pedido de MOD a caminho do servidor» (:2737) e o warn «pedido de MOD recusado na saída» (:2742);
  - `midia_em_bytes`: o debug «mídia de MOD vinda do servidor» (:5658);
  - `escolher_para_o_mod`: o info «arquivo escolhido para um MOD» (:5966).
  
  Os quatro passam a `mod_id = %id_no_registro(&id)`. Os outros `mod_id = %…` já escapam, ou usam o id do manifesto já conferido, e ficam como estão: `recusa_de_carga_dita`, `recusa_de_midia_dita`, `midia_servida_dita`, `som_servido_dito`, `assentar_fala`, `registrar_console_do_mod` e `reservar_no_executor`.
- **A metade do app do P51-26.** As duas linhas de `mod_request` seguem os três braços da metade do servidor (`crates/seele-server/src/mods/pedidos.rs`, em `executar`):
  - id vazio: `catalogo = true`, sem `mod_id=`;
  - id que o filtro esvazia: `mod_id_ilegivel = true`;
  - senão: `mod_id = %legivel`.
  
  Extrair `pedido_recusado_na_saida_dito(id, request, erro)`. A linha debug passa pela mesma escolha dos três braços, numa função só que receba o nível, ou numa irmã.
- **O doc de `fn id_no_registro(`** (:2313-2327) lista as portas por nome, sem número de linha. Acrescente `mod_request`, `midia_em_bytes` e `escolher_para_o_mod`.

**Testes:**
- com `id = "a/b\nWARN seele_app: forjada"`, sai uma linha só. Hoje saem duas;
- id vazio dá `catalogo=true` sem `mod_id=`;
- um guarda de fonte exige que nenhum `mod_id = %id` use o parâmetro cru do comando em `mod_request`, `midia_em_bytes` e `escolher_para_o_mod`.

**Reversão:** voltar a `%id` deixa os testes vermelhos.

### 4. P51-12 — a recusa de carga por geração
- `type RecusasDeCarga = Mutex<std::collections::HashMap<String, String>>;` (:3715) passa a `Mutex<HashMap<String, (u64, String)>>`.
- `fn recusa_de_carga_dita(` (:3743) e `fn carga_dita<T>(` (:3776) recebem `geracao`. Os chamadores já a têm: `codigo_do_mod` (:5202) e `mod_nativo_reservar` (:3809).
- A recusa é dita quando a entrada não existe, quando a geração é outra ou quando o motivo mudou.
- Tirar a ressalva do doc, de «**E a memória é de uma sessão.**» até «…com a linha dita segundos antes no arquivo.» (:3733-3738).
- Ajustar:
  - as strings exatas de `os_dois_comandos_de_carga_passam_pela_recusa_dita` (:11611);
  - o ajudante `recusar` (:11390), que escreve o tipo por extenso;
  - as chamadas diretas do `mod a_carga_recusada_e_dita_uma_vez_por_motivo` (:11385).

**Teste novo**, ao lado de `a_mesma_recusa_numa_sessao_nova_e_dita_de_novo` (:11468): `revogar()`, depois a recusa atrasada da geração 1, depois a mesma recusa na geração 2. Têm de sair duas linhas WARN.

**Para o vermelho de verdade:** primeiro a assinatura nova, comparando só o motivo (o comportamento de hoje), e o teste fica vermelho; depois a comparação pela geração.

**Comandos:**
- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-app --bin seele-app o_registro_da_janela`;
- `… --bin seele-app recusa`;
- `… --bin seele-app`, inteiro, antes do último commit;
- fmt e `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`.

**NÃO fazer:**
- não manter o `.take(…)`;
- não usar `Instant::now()` dentro dos testes;
- não mudar os argumentos do invoke `registrar_da_janela`: a janela não muda;
- não mexer no prelúdio nem no laço do executor (lote D);
- não mexer nas conferências `!session.geracao_vale(` (lote E).

**Riscos.**
- Muda a forma das linhas no `seele.log`, que é texto e não fio.
- O grep do guia (`grep "mod_id=fulano/meu-mod"`, `docs/como-se-faz-um-mod.md`) e os do plano 1D continuam casando.
- Segurar linhas pode esconder um diagnóstico. Por isso a conta (`suprimidas`, mais a do `revogar`) é obrigatória e tem teste.
- O D (P51-14) escreve testes sobre `registrar_da_janela_em`, e eles devem procurar campos, e não a forma inteira da linha.
- A F3 vai mexer em `ler_codigo_do_mod` e na carga por API. O guarda `os_dois_comandos_de_carga_passam_pela_recusa_dita` compara o corpo desses comandos, e vai avisá-la.

## Lote H — a recusa legível, metade cliente: a volta recusada diz o motivo de verdade, e o impostor que não prova a chave deixa de parecer «versão incompatível»

**Onda 1.** Itens: P51-03, P51-02 e o K1-m1.

**Por que juntos.** O P51-02 sozinho trocaria «ENLACE PERDIDO» por «CREDENCIAL RECUSADA», porque com `vale_insistir = false` a volta recusada vira `Motivo::Recusado`. Por isso o P51-03 vem antes, no mesmo lote. Os dois mexem:
- em `vale_insistir` e `alguem_respondeu`;
- no braço `Motivo::Recusado` da FFI;
- nos tipos da FFI;
- nas frases.

**Na 0.16.** Este é a metade cliente da «recusa legível» do Plano 3, e entra antes do lado cliente dele. O lote não lê nada novo do fio. Ele faz o cliente parar de disfarçar as recusas que já existem. O `Disconnecting{Incompatible}` com os bytes-ouro é da F1.

**Arquivos:**
- `crates/seele-core/src/client.rs`;
- `crates/seele-core/src/enlace.rs`;
- `crates/seele-ffi/src/lib.rs`;
- `crates/seele-ffi/src/types.rs`;
- `apps/seele-app/ui/frases.js`;
- `apps/seele-app/tests/frontend.rs`;
- `crates/seele-conformance/tests/bateria_interna.rs`.

`crates/seele-core/par.rs` é só leitura.

**Estado em `d4eb6f1`:** nada feito.

**Ordem:** o P51-03 inteiro, e só depois o P51-02.

### 1. P51-03 — a volta recusada diz o motivo
- **(1) O tipo.** Em `pub enum Motivo` (`enlace.rs`, :296), `Recusado(String),` passa a `Recusado(ConnectError),`.
  - `ConnectError` (`client.rs`, :60) já deriva `Debug, Clone, PartialEq, Eq`, e `Motivo` mantém os traços que deriva.
  - O ponto de produção é `self.encerrar(Motivo::Recusado(format!("{erro:?}")))`, no braço `Err(erro) if !vale_insistir(&erro) =>` (:2931-2933).
  - O doc de `Motivo::Moderado` (:305-308), que diz que `Recusado` vira `CredentialRejected` na FFI, muda junto.
- **(2) A linha no registro.** Nesse mesmo braço, antes do `encerrar`: `tracing::warn!(?erro, "a volta da bateria foi recusada, e a sessão acaba")`. Hoje uma volta recusada acaba sem nenhuma linha no `seele.log`. Só o braço seguinte tem o `debug!` «tentativa de reconexão falhou».
- **(3) `fn fim_da_recusa(&ConnectError) -> EndReason`**, na FFI, perto de `fn classify_connect_failure` (`lib.rs`, :5097).
  - O `match` é exaustivo e não tem braço `_`, pela regra do doc «# `match` exaustivo, e sem braço `_`» (`enlace.rs`, :5430-5439):
    - `Refused { reason }` vira `(*reason).into()`;
    - `InviteMismatch` vira `EndReason::ServidorTrocouNaVolta`, que é nova;
    - `PinChanged` vira `EndReason::ChaveMudouNaVolta`, que é nova;
    - `ModsNaoAceitos` vira `ModsMudaram`, que já existe;
    - `LocalEndpoint`, `Unreachable`, `TlsRefused`, `HandshakeTimeout`, `SemResposta` e `ProtocolViolation` viram `LinkLost`, cada uma nomeada no `match`.
  - As variantes novas não levam impressões, porque `EndReason` deriva `Copy, Hash` (`types.rs`, :260-261).
  - O braço `seele_core::enlace::Motivo::Recusado(_) => EndReason::CredentialRejected` (`lib.rs`, :4184), dentro do `Aviso::Encerrado(motivo)` de `drive`, passa a chamar `fim_da_recusa`.
- **(4) As frases.** Em `const MOTIVOS` (`frases.js`, :20) entram:
  - «QUEM ATENDEU NA VOLTA NÃO É O SERVIDOR DESTA SESSÃO.\nO convite, a senha e o apelido não chegaram a ele. Confirme com quem hospeda, por outro canal, antes de entrar de novo.» (trocada na revisão: a frase de 08/10, «…SERVIDOR DO CONVITE… sem mandar nada a ele. Confirme o link…», era falsa para quem volta pela lista e pelo «nada», porque o ClientHello sai);
  - «A CHAVE DO SERVIDOR MUDOU DURANTE A SESSÃO.\nConfirme por outro canal antes de entrar de novo.».
- **(5) Os dois testes que liam o texto passam a ler o tipo.**
  - Em `a_volta_da_bateria_onde_o_pino_nao_prova_o_servidor_e_recusada_antes_do_hello` (`enlace.rs`, :6925), o `if let Aviso::Encerrado(Motivo::Recusado(texto))` com `texto.contains("InviteMismatch")` passa a `matches!(aviso, Aviso::Encerrado(Motivo::Recusado(ConnectError::InviteMismatch { .. })))`.
  - Em `a_volta_da_bateria_recusa_no_tls_o_impostor_que_a_impressao_desmente` (`bateria_interna.rs`, :509), o `Recusado(motivo)` com `motivo.contains(…)` passa a `Recusado(ConnectError::InviteMismatch { expected, offered })`, conferindo `expected == real` e `offered == ofertada`.

**Prova, com o vermelho de verdade:**
- primeiro o tipo muda, com `fim_da_recusa` devolvendo `CredentialRejected` para tudo (o comportamento de hoje). O teste novo da FFI fica vermelho nos quatro casos:
  - `InviteMismatch` dá `ServidorTrocouNaVolta`;
  - `Refused { ServerFull }` dá `ServerFull`;
  - `PinChanged` dá `ChaveMudouNaVolta`;
  - `ModsNaoAceitos` dá `ModsMudaram`;
- depois vem o mapeamento;
- um teste de rastro em `enlace.rs`, com `crate::rastro_de_teste::Rastro` (`seele-core/src/lib.rs`, :44), exige a linha WARN com a variante;
- o guarda `every_reason_a_session_can_end_with_has_a_sentence_in_the_page` (`frontend.rs`, :7634) obriga as frases novas.

**Reversões:** voltar o braço a `CredentialRejected` deixa o teste da FFI vermelho; tirar o `warn!` deixa vermelho o de rastro.

### 2. P51-02 — o impostor que não prova a chave
- **(1)** `ConnectError::ChaveNaoProvada`, em `client.rs`.
- **(2) `fn classify_connection_error`** (`client.rs`, :1709).
  - Hoje o braço junta as duas decisões e o caso sem decisão: `Some(PinDecision::FirstContact { .. } | PinDecision::Matches { .. }) | None => match error {…}` (:1723). Dentro dele, `quinn::ConnectionError::TransportError(_)` dá `warn!` e `TlsRefused` (:1730-1733).
  - Separe o braço. Com `Some(Matches | FirstContact)` e um `TransportError(e)` cujo `e.code == quinn::TransportErrorCode::crypto(51)` (decrypt_error), a função devolve `ChaveNaoProvada`.
  - `None` e os outros `TransportError` continuam `TlsRefused`.
  - O precedente é `motivo_da_recusa` (`crates/seele-core/par.rs`, :1126). Ele lê o alerta **recebido** em `ConnectionClosed(fechamento).error_code`. Aqui é o TLS **local** que falha, e o campo é `TransportError(e).code`: o molde é parecido, e a variante é outra.
- **(3)** `fn alguem_respondeu` (`enlace.rs`, :5440) dá true para `ChaveNaoProvada`, e `fn vale_insistir` (:5467) dá false. Os dois são exaustivos. `desfazer_o_pin_deste_aperto` (:4966) não muda de comportamento.
- **(4)** `fim_da_recusa(ChaveNaoProvada)` vira `ServidorTrocouNaVolta`.
- **(5)** `ConnectionError::ChaveNaoProvada`, variante unitária, em `pub enum ConnectionError` (`types.rs`, :1937), traduzida em `fn classify_connect_failure` (`lib.rs`, :5097), que é exaustiva. Hoje essa função manda `TlsRefused | ProtocolViolation` para `Refused { Incompatible }` (:5102-5106), e é por isso que o impostor aparece como «versão incompatível».
- **(6)** Em `const FRASES` (`frases.js`, :638): `ChaveNaoProvada: «QUEM ATENDEU NÃO PROVOU SER ESTE SERVIDOR.\nEle mostrou o certificado do servidor sem a chave que o assina. O convite, a senha e o apelido não chegaram a ele.»`.
- **(7) Os docs:**
  - o doc de `TlsRefused` (`client.rs`, :68-78);
  - o parágrafo do doc de `desfazer_o_pin_deste_aperto` (`enlace.rs`, :4954-4964), de «`TlsRefused` e `SemResposta` saem do próprio TLS» até «faria quem investiga achar que quem atendeu provou ter a chave».

**Prova:**
- `o_pin_que_um_impostor_fez_fixar_e_desfeito_e_o_log_nao_diz_que_o_tls_terminou` (`enlace.rs`, :6059), com o impostor `fn impostor_que_repete_o_certificado` (:7025), passa a exigir `Some(ConnectError::ChaveNaoProvada)`. Hoje exige `TlsRefused`, em :6094-6098. A asserção de :6116-6121 passa a procurar `ChaveNaoProvada` na linha do pin desfeito;
- em `client.rs`, no módulo `a_recusa_do_tls_chega_com_nome` (:1744), `sem_recusa_do_verificador_a_classificacao_e_a_de_sempre` (:1777) continua verde. Um caso novo prende que um alerta diferente de 51 (por exemplo, 120) continua `TlsRefused`, e outro que `None` com o alerta 51 continua `TlsRefused`;
- em `frontend.rs`, um guarda novo de `ConnectionError` contra `FRASES` exige `ChaveNaoProvada:` com «NÃO PROVOU». Hoje só há o de `EndReason` (:7634), o de `FalhaAo*` (:2220) e o de `FalhaAoAtualizar` (:5277).

**Reversões:** tirar o braço do alerta 51 deixa vermelho o teste do impostor; tirar a frase deixa vermelho o de `frontend.rs`.

### 3. K1-m1 — o doc de um guarda do `frontend.rs`
- O doc de `o_dono_da_regiao_leva_cada_recusa_de_midia_ao_registro` (`frontend.rs`, :8149-8167) termina em «(medido em 01/10/2026)» e diz que as seis bancadas de node ficam verdes com o dono sem `anotarRecusa`.
- Desde `4599e14`, o bloco «o elo» da `contribuicoes-e-camadas.cjs` (`confere("o elo · anotarRecusa", …)`) também reprova, com o dono sem `anotarRecusa` e com o `meu()` invertido. O doc ganha essa frase, mantendo a data como registro.
- É só doc, e não precisa de reversão.

**Comandos:**
- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-core enlace`;
- `… -p seele-core client`;
- `… -p seele-ffi fim_da_recusa`;
- `… -p seele-conformance --test bateria_interna`;
- `… -p seele-conformance --test acceptance_m5 --test estados`: eles só usam `matches!` sobre `ConnectionError`, e rodá-los confere que compilam;
- `… -p seele-app --test frontend`;
- fmt, e o clippy de `seele-core`, `seele-ffi`, `seele-conformance` e `seele-app`.

**NÃO fazer:**
- não pôr impressões dentro de `EndReason`;
- não usar braço `_` em `fim_da_recusa`;
- não classificar o caso sem decisão do verificador (`None`) como `ChaveNaoProvada`;
- não entregar o P51-02 sem o P51-03;
- não mudar o fio, nem ler razão nova de `DisconnectReason`.

**Riscos.**
- A FFI ganha variantes internas à casca.
- `EndReason` chega ao Snapshot (`ended`), que os MODs leem hoje. Os valores novos são aditivos, e nenhum `api/*.json` enumera `EndReason`. O que a API 6 projeta do Snapshot é da F3.
- A classificação depende do número do alerta que o rustls manda. Se uma versão nova mudar o mapeamento, o teste do impostor reprova alto.

## Lote K2 — a mídia provada e contada: uma imagem que não decodifica é recusada e dita, e a aba DIAGNÓSTICO conta a mídia da região e das páginas

**Onda 1.** Itens: P51-07, P49-5 e o K1-m2.

**Por que juntos.** Os dois reescrevem os mesmos caminhos de imagem e as anotações de mídia, e são provados nas mesmas duas bancadas. Feitos juntos, o fundo de tela tem um caminho só de recusa. O K1-m2 é o mesmo defeito do P51-06, no mesmo `mods-regiao.js`.

**Na 0.16.**
- **Sem o filtro `cartao`.** A soma conta tudo o que os renderers da região e das páginas anotam.
  - A mídia dos cartões de `SeeleUI.cartoes` aparece também no bloco de `pessoa.cartao`, e são duas vistas da mesma mídia. A aba não soma os dois blocos.
  - O filtro só serviria enquanto a capacidade `cartoes` existir, e ela sai com a F3.
  - É também o que a pendência 49, item 5, propõe.
- **Nenhum evento novo para o MOD.** O retrato não fala com o MOD hoje, e continua sem falar. Se o retrato passar a mandar `midia`/`falhou`, isso é promessa nova, e vai para o rascunho da v6.

**Arquivos:**
- `apps/seele-app/ui/mods-regiao.js`;
- `apps/seele-app/ui/base.js`;
- `apps/seele-app/ui/camada-mods.js`;
- `apps/seele-app/bancada/regiao-do-mod.cjs`;
- `apps/seele-app/bancada/contribuicoes-e-camadas.cjs`.

O bloco novo da aba reaproveita as classes que já existem (`mods-versao`, `mods-linha-gestao`), para não tocar `index.html` nem o CSS.

**Estado em `d4eb6f1`:** nada feito. Nenhum caminho usa `decode()`, e o único `new Image()` da `ui/` é o do fundo de tela.

### 1. P51-07 — a imagem só vale depois do `decode()`
- **Criar `provarImagem(uri)`:** `const i = new Image(); i.src = uri; return i.decode().catch(() => { throw new Error('a imagem não decodificou'); })`.
- **Os quatro caminhos.** O `decode()` é encadeado DENTRO do `then` que já existe (`return provarImagem(midia.uri).then(…)`), e assim a recusa cai no `.catch` de cada um.

  | caminho | onde | o que o `.catch` faz hoje |
  |---|---|---|
  | retrato | `montarRetrato` (`mods-regiao.js`, :1509) | anota e diz ao registro (`dizerRecusaDeMidia`), sem falar com o MOD |
  | fundo de mídia | `montarFundoDeMidia` (:1462) | anota, diz ao registro e manda `midia`/`falhou` |
  | fundo de tela | `buscarFundoDaTela` (:1931); o `img.onload` vira o `decode()` do próprio `img` | manda `tela`/`fundo-falhou` |
  | avatar contribuído | `avatarContribuido` (`base.js`, :1179) | manda `midia`/`falhou` com a chave `"avatar"` |

- Depois do `await`, confira de novo `cancelado`, `this.solta` e `podeFalar()` antes de aplicar.
- **Os bytes e o teto.** A conta continua sendo feita e reservada **antes** do `decode()`, como hoje (`this[bolso.conta] += midia.bytes` no retrato e no fundo de mídia; `estado.bytes` no avatar), e é **devolvida** quando o `decode()` recusa.
  - Somar só depois do `decode()` abriria uma corrida: duas imagens decodificando juntas passariam as duas pelo teto.
  - O comentário «Contados **agora**…» (:2237) já defende a reserva para o som.
  - O fundo de tela não tem bolso.
- **No retrato,** um ouvinte de `error` no `<img>` leva ao mesmo caminho de recusa. O molde é `naoAbriu` em `montarMidia` (:2256-2265, de `b167b9f`).

### 2. P49-5 — a aba conta a mídia da região e das páginas
- **Em `buscarFundoDaTela`:**
  - «carregando» ao pedir;
  - «pronta» depois do `decode()`;
  - «recusada» nas três saídas: o papel errado (:1958-1961), o `.catch` e a recusa do `decode()`.
  
  Tudo pela mesma `anotarMidia(elem, situacao, motivo)` (:2631).
- **`esquecerMidia(elem)`** (:2643) entra em dois lugares:
  - quando a declaração tira o fundo (`if (!declarado) return;`, :1940);
  - quando a tela é descartada, no `this.guardar(elem, \`tela ${plano.chave}\`, …)` (:1899-1921). O plano antigo não previa este.
- **Em `base.js`, a função pura `midiasDaRegiaoEDasPaginas(regioes, superficies)`.** Ela soma `midiasAnotadas()` (:2648):
  - da região de cada MOD (`regioesDosMods`, :695, um `Map` de instância para `RegiaoDeMod`, em que `regiao.id` é o id do MOD);
  - das páginas de cada MOD (`superficiesDosMods`, :1111). Cada entrada é um `SuperficiesDoMod` (`mods-superficies.js`, :695), e a soma anda por `conjunto.porChave.values()` e lê `s.renderer.midiasAnotadas()`.
  
  O resultado sai por id de MOD, no formato `{carregando, pronta, recusada, motivos}`, sem filtro de cartão.
  - Ponha-a entre `function midiasDoPonto(` (:1403) e `calarOsSonsQueSairamDaTela`, para que o recorte que a bancada já faz de `midiasDoPonto(` até o doc «Cala o som de MOD…» (`contribuicoes-e-camadas.cjs`, :1188) a leve junto.
- **A aba** (`camada-mods.js`, junto de `quemPintaCadaPonto`, :881, e `desenharQuemPinta`, :1012) ganha o bloco «a região e as páginas de cada MOD», com frases como «a região e as páginas de mod/x: 1 recusada — <motivo>».
- **O doc de `anotarMidia`** (:2611-2622) deixa de dizer «ninguém as lê» (:2619) e deixa de citar os cartões da API 3.

### 3. K1-m2 — o corte da finalidade por ponto de código
- `pedidoDeArquivo` (`mods-regiao.js`, :399) corta a finalidade por índice: `finalidade: … no.finalidade.slice(0, 200)` (:402). Ela vai a `invoke("escolher_para_o_mod", …)` (`base.js`, :905-912) e chega a `finalidade: Option<String>` (`main.rs`, :5834).
- Se a posição 200 cair num par substituto, o `serde_json` recusa o corpo inteiro (medido pelo corretor do K1), e o pedido de arquivo volta recusado.
- O conserto: `[...no.finalidade].slice(0, 200).join("")`, o modelo de `ef707c7` para a chave do campo.
- Os outros cortes por índice da mesma família (`atualizarArquivo`, :1148-1150; `textoDoRotulo`; `plano.no.descricao.slice(0, 120)`) só vão a `aria-label`/`title`, e não quebram nada: ficam fora.

### 4. Provas
- **O `Image` de mentira nas bancadas.**
  - Em `regiao-do-mod.cjs`, o `bancada()` (:423) ganha um `Image` padrão cujo `decode()` resolve. Sem ele, um `new Image()` lança, e ficam vermelhos `oRetratoDoCartaoEOMesmoNoEntreDoisRetratos`, `aImagemAcompanhaAMudancaDaFonte`, `osTetosDoCartaoSaoDoCartaoENaoDaRegiao`, `aMidiaDoServidorTemOMesmoDono`, `fundoTrocaSoltaECancela`, `oFundoEORetratoTambemDizemOEstado` e outros.
  - O mesmo vale para o contexto do avatar em `contribuicoes-e-camadas.cjs` (o recorte de `function avatarContribuido(`).
- **`regiao-do-mod.cjs`:** um `Image` cujo `decode()` recusa para um uri marcado. Com esse uri:
  - retrato, fundo de mídia e fundo de tela ficam «recusada»;
  - o fundo de mídia manda `midia`/`falhou` ao MOD, e o fundo de tela manda `tela`/`fundo-falhou`. O retrato não manda evento;
  - a linha sai em `anotarRecusa`;
  - a conta de bytes fica sem a imagem.
  
  Hoje os três ficam «pronta» e calados.
- **No laço «fundo de tela»** de `cadaMidiaRecusadaEDitaAoAnfitriao` (:1576; os casos «fundo de tela que não é imagem» e «… que o Rust recusou», :1700 e :1705), `regiao.midiasAnotadas()` tem a anotação `recusada` com o motivo. Um caso novo, em que a declaração tira o fundo, faz a anotação sumir.
- **`contribuicoes-e-camadas.cjs`:**
  - o avatar contribuído, com o `Image` que recusa no contexto;
  - com o renderer de mentira (`anotando`, :1215) e o contexto `abaDoDiagnostico` (:1177) ganhando `regioesDosMods` e `superficiesDosMods`: uma região com um som recusado, uma superfície com um fundo recusado e uma anotação `cartao: true` recusada na região. A função devolve `recusada: 3`, com os três motivos.
- **`regiao-do-mod.cjs`, para o K1-m2:** a finalidade `"a".repeat(199) + "😀"` chega inteira e bem formada ao `escolherArquivo` do dono, no molde de `aChaveDoCampoNaoPartePar` (:1479). O `dono()` de mentira ganha um `escolherArquivo` que anota os argumentos.
- **Reversões:**
  - tirar o `decode()` de um caminho deixa o caso dele vermelho;
  - tirar a devolução da reserva deixa vermelha a conta de bytes;
  - tirar o `anotarMidia` do fundo deixa vermelho o caso do laço;
  - tirar a soma da superfície dá 2;
  - voltar ao `slice` por índice deixa vermelho o caso da finalidade.

**Comandos:**
- `cargo xtask check-runtime`;
- opcional, o desenho do bloco no Chromium: `PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright perl -e 'alarm 240; exec @ARGV' node apps/seele-app/bancada/diagnostico-de-mods.cjs`.

**NÃO fazer:**
- não criar evento novo na API de MODs, nem fazer o retrato falar com o MOD;
- não afrouxar `img-src`;
- não somar bytes depois do `decode()` sem reserva;
- não filtrar as anotações de cartão;
- não tocar `index.html` nem o CSS;
- não instalar Playwright pela rede.

**Riscos.**
- Nada no fio, na API congelada nem na CSP: é o mesmo uri, sob o mesmo `img-src`.
- A imagem passa a ficar «pronta» um pouco mais tarde.
- O `decode()` com bytes corrompidos no WKWebView, no WebView2 e no WebKitGTK não se mede aqui, só com o `Image` de mentira no Node.
- Enquanto a capacidade `cartoes` existir, a mesma mídia de cartão aparece em dois blocos da aba.

## Lote M1 — a consulta ao quarto e as marcas: o que os testes do 1A e do 1B diziam provar passa a ser provado, sem `enlace.rs`

**Onda 1.** Itens: P51-17 e P51-19. A parte do 1B que mexia em `crates/seele-core/src/enlace.rs` (a montagem de certificado compartilhada no módulo de testes) foi para o M2, porque o H mexe em `enlace.rs` nesta onda.

**Por que juntos.** O T5a, o T5c e o T6f do 17 e o M2 do 19 mexem na mesma `resolver_ponto` e no mesmo `saidas == 0` de `seele-core/src/encontro.rs`, e a pendência pede que se façam juntos. O T1c do 17 e o C4 e o T5e do 19 mexem no mesmo `alcance/encontro.rs` do `seele-server`.

**Arquivos:**
- `crates/seele-core/src/encontro.rs`;
- `crates/seele-proto/src/uri.rs`;
- `crates/seele-proto/src/encontro.rs`;
- `crates/seele-proto/tests/sementes_do_fuzz.rs`: só o trecho do `datagrama_do_encontro`, pelo T1c;
- `crates/seele-server/src/alcance/encontro.rs`;
- `crates/seele-conformance/tests/quarto.rs`, `quarto_pelo_link.rs`, `ocupante_da_escuta.rs` e `volta_pela_trilha.rs`;
- `docs/adr/0047-o-link-que-volta-a-funcionar-amanha.md`;
- `docs/superpowers/plans/2026-09-29-1.0-plano-1a-o-quarto.md` e `…-1e-a-api-congelada.md`.

Não toca o `seele-ffi`, `enlace.rs`, `hospedagem.rs` nem o `lib.rs` do `seele-server`.

**Estado em `d4eb6f1`:** nada feito. Desde `3685a64`, só o `a6c2cc0` tocou estes arquivos (`Bilhete::com_porta_escrita` e `Encontro::bilhete`).

**Ordem:**
1. a consulta: T5a, T5c, M2 do 19, T6f, C1, T5d;
2. as marcas: T1a, T1b, T1c, T2c/T3c/M3;
3. a conformidade: T4a, T7a, T8b/T8d, a metade do 1B que fica aqui;
4. o P51-19: C4, T5e, T3d/T3e, T7b e os planos.

O lote é pesado. Se não fechar numa sessão, pare depois do passo 2 e deixe o 3 e o 4 para uma segunda sessão do mesmo lote, antes do M2.

### 1. A consulta (`crates/seele-core/src/encontro.rs`)
- **T5a e T5c.** De `async fn resolver_ponto(ponto: &str)` (:338), extraia uma função pura que recebe os achados e devolve o `Vec<SocketAddr>` **ordenado**, com IPv4 primeiro (o `achados.sort_by_key(|achado| u8::from(achado.is_ipv6()));` de :362), e a linha «resolveu para nenhum endereço» (:365-369) quando vazio.
  - Ela devolve a lista, e não um `Option`, porque o M2 do 19 precisa do seguinte.
  - Teste: `[[::1]:8384, 127.0.0.1:8384]` dá `127.0.0.1` primeiro; `[]` dá vazio, com a linha.
  - Reversão: apagar o `sort_by_key` ou a linha do vazio reprova.
- **M2 do 19.** Se nenhuma pergunta sair pela família do primeiro endereço, a consulta (`async fn consultar(`, :722, e `async fn consultar_por(`, :745) tenta o seguinte, dentro do mesmo `ate`.
  - Hoje «nenhuma pergunta saiu» volta como `PontoMudo`, igual ao socket que não abriu (:727-733) e ao ponto mudo. Separe esses casos por dentro, sem mudar o que `OndeMora` diz para fora.
  - Teste da função de escolha. Ele reprova com a escolha de hoje.
- **T6f.** A decisão de depois do envio vira uma função pura. O `if saidas == 0 { break; }` (:829-831) só sai do laço com `enviados == 0`.
  - Depois da volta 1, a volta recusada conta como volta: `enviados += 1;` e `proxima_volta += INTERVALO_DOS_ENVIOS;` (:832-833). Assim não vira laço quente.
  - Testes: a função pura reprova com o `break` de hoje; um teste de `consultar_por`, com o envio recusado na volta 2 e a resposta tardia à volta 1, dá `Achado` em vez de `PontoMudo`.
- **C1.** Testes de unidade de `fn teto_depois_da_resposta(` (:406), com `FOLGA_MINIMA` = 100 ms (:397):
  - 1 ms de ida e volta dá `primeira + 100 ms`;
  - 200 ms dá `primeira + 400 ms`.
  
  A segunda marca dentro do teto e o `ONDE` perdido ganham testes de `consultar_por` com um ponto falso, no molde de `um_aqui_de_outro_ip_nao_entra_na_resposta_da_consulta` (:1046).
- **NÃO mexer no doc de `onde_mora_hoje`** (:632-680): o M2 (P51-18) o reescreve com o comportamento que este lote deixar.
- **T5d** (`crates/seele-proto/src/uri.rs`). `pub fn separar_ponto(` (:569) passa a recusar `""` com `EnderecoInvalido`, com teste. Hoje devolve `Ok(Alvo { maquina: "", porta: 8384 })`.
  - `Bilhete::ler` já recusa o ponto vazio: `if ponto.is_empty() || aviso.is_empty()` (:289).
  - `fn validar_alvo(` (:631) também aceita `""`, e por isso `Bilhete::novo("", …)` dá `Ok`. **`Bilhete::novo` passa a recusar o ponto vazio junto**, como `Bilhete::ler`. Assim continuam verdadeiros o doc de `Bilhete::ponto` (:302-305, «Não falha para um bilhete que veio de [`Bilhete::novo`]») e o de `com_porta_escrita` (:333-335).
  - O `analisar` já recusa o alvo vazio (:455).
  - Outro chamador de `separar_ponto`: `crates/seele-encontro/examples/sondar.rs`, que tem de continuar compilando.

### 2. As marcas (`crates/seele-proto/src/encontro.rs`)
- **T1a.** `pub fn do_servidor(impressao_digital: &str)` (:274) só faz `impressao_digital.get(..16)?` (:275). Ela passa a conferir o texto inteiro, letra e número, como o doc já promete (:271-272, «curto demais, ou com algo que não é letra nem número»). O teste pede `None` para `0123456789abcdef!!!`, que hoje dá `Some`.
- **T1b.** Parece já coberto. `as_marcas_de_um_servidor_saem_da_impressao_digital` (:833) exige `aviso.texto() == "3cbcfb0212da738f"`, com 16 caracteres, desde `6d2eafc`. Prove pela reversão (`..15` ou `..17` deixa o teste vermelho). Se ficar vermelho, o item fecha sem teste novo, e o relatório diz isso.
- **T1c.** Os campos de `pub struct Marcas` (:250; `pub aviso`, `pub escuta`, `pub servidor`, :253-257) passam a privados, com `aviso()`, `escuta()` e `servidor()`, e sem construtor de fora. As leituras de campo fora do módulo:
  - `seele-core` (`src/encontro.rs`): 7;
  - `seele-server` (`src/alcance/encontro.rs`): 14, mais o literal `Marcas { aviso, escuta, servidor }` em `subir_atender_de_teste` (:1575), que passa a sair de `marcas_da_impressao()` (:1807) ou de `do_servidor`, porque um construtor `cfg(test)` não atravessa crates;
  - `seele-conformance`: 14, em `quarto.rs`, `ocupante_da_escuta.rs` e `quarto_pelo_link.rs`;
  - `crates/seele-proto/tests/sementes_do_fuzz.rs`: 5, no trecho do `datagrama_do_encontro` (:203-218). É teste de integração, e os campos privados o quebram;
  - `seele-ffi`: nenhuma. Ele só chama `Marcas::do_servidor`, e não muda.
- **A mensagem de cada asserção.** No laço de `a_escuta_nunca_tem_a_marca_do_aviso` (:859; laço em :863-885), as três asserções ganham `{impressao}` na mensagem.
- **T2c, T3c e M3.** Testes em `alcance/encontro.rs` que capturam os pedidos da subida e conferem a marca de cada um:
  - em `pub async fn abrir(` (:459): `&encontro::onde(&marcas.escuta), &marcas.escuta` (:512-513) e `perguntar_pelo_server(…, &marcas.escuta)` (:540);
  - o `LEVE`, que sai em `encontro::leve(para, esperada)` (:716);
  - no tique, `async fn registrar(` (:893).
  
  O T2c está em parte coberto por `o_primeiro_registro_no_quarto_sai_na_subida` (:1859), que já exige o `MORO` do servidor: falta a reversão própria. O apoio é `async fn ponto_que_anota(` (:1745) e `esperar_no_caderno` (:1782).

### 3. A conformidade
- **T4a.** Em `largar_uma_hospedagem_sem_encerrar_devolve_a_porta` (`quarto_pelo_link.rs`, :197), o laço `let segunda = loop {` (:256) guarda o último erro, que hoje os braços `Err(_)` descartam (:268-271), e o põe no `panic!` (:276-282).
- **T7a.** `um_ponto_que_nao_conhece_a_pergunta_apenas_cala` (`quarto.rs`, :104) usa um socket mudo (:113). Ou ganha um caso com um ponto que responde lixo e tem de dar `PontoMudo`, ou muda de nome para o que ele mede.
- **T8b e T8d.** Em `o_link_leva_ate_o_servidor_e_a_escuta_de_hoje` (`quarto_pelo_link.rs`, :342):
  - distinguir a escuta `None` da FFI da do anfitrião;
  - ler de volta o `ponto`/`aviso` do `enc=` (`let mut formas = vec![bilhete.ponto.clone()];`, :391);
  - **corrigir o comentário de :386-390**, falso desde `a6c2cc0`. Ele diz que as duas formas são a mesma; hoje `bilhete.ponto` é `"127.0.0.1:8384"`, diferente de `sem_porta`.
- **A metade do 1B que fica aqui** (`volta_pela_trilha.rs`):
  - `async fn ponto_que_conta()` (:88-100) passa a contar só os datagramas que `seele_proto::encontro::analisar` (`encontro.rs`, :484) lê como `Pedido::Leve` com a marca do aviso esperado. A marca sai em `Batida::preparar` (`seele-core/src/encontro.rs`, :115-120). Trocar a marca do `LEVE` ali reprova a asserção `avisos.load(Ordering::Relaxed) >= 1` (:225-230), que hoje passa;
  - `expect` com motivo no lugar de `.ok()?`, em `server_de_teste`, `ponto_que_conta`, `lembrar_de_ontem` e `server_com_banco`.

### 4. P51-19
- **T5e.** Em `async fn resolver(` de `alcance/encontro.rs` (:565), o `Bilhete::novo(texto, "0.0.0.0:0")` (:572) vira `seele_proto::uri::separar_ponto(texto).ok().map(|a| (a.maquina.to_owned(), a.porta))`, com o comportamento de antes. O comentário de :569-571 deixa de falar do `Bilhete`.
- **C4.** `fn mandar_pelo_server(` (:742) devolve um erro tipado, com `passageira` para `ErrorKind::WouldBlock`.
  - `async fn registrar(` (:893) o repassa.
  - `fn anotar(` de `RegistroNoQuarto` (:927-944) não muda de estado numa falha passageira.
  - Os outros chamadores, `perguntar_pelo_server` e `furar`, compilam com o tipo novo.
  - Teste com captura, no molde de `o_registro_que_deixa_de_sair_diz_no_log_uma_vez_e_diz_quando_volta` (:2070): `Err(passageira)` seguido de `Ok` não deixa linha nenhuma. Hoje saem duas linhas info.
- **T3d e T3e.** Constantes de teste compartilhadas:
  - o literal `let fp = "3cbcfb02…"` (:1303) passa a usar `const IMPRESSAO` (:1804);
  - o bloco do «alvo do furo de teste», repetido em :1557-1562 e :1912-1917, vira um ajudante.
- **T7b.** Um adendo no ADR 0047, no molde do ADR 0003 («## Adendo — 2026-09-29 · o pino prova…», `9f6f90e`), dizendo que `onde_mora` virou `onde_mora_hoje` e onde ele mora hoje. A linha «`seele-core/src/encontro.rs:333` (`onde_mora`)» (:83) não muda, porque :85 diz «como ele está no commit».
- **Os planos.** Uma nota no topo de cada seção citada, dizendo o que a execução trocou, sem reescrever o histórico:
  - **plano 1A:**
    - :23, nas Global Constraints;
    - :923 e :1076, na Task 4: `fn restaurar` virou `AmbienteDoEncontro` (`quarto_pelo_link.rs`);
    - :1223, na Task 5, e :2749, em Riscos: o G1 de hoje é o do índice e do plano 1B;
    - a Task 9 (:2541, «CONDICIONADA, portão G2») foi executada em `a6c2cc0`;
  - **plano 1E:** `PUBLICADAS_ANTES_DO_GUARDA` (:57, :124, :291, :543, :551) e o bloco `$tmp` (:544-551) viraram `fn linhas_que_faltam` em `crates/seele-conformance/tests/a_api_publicada_nao_se_edita.rs`, com a restauração por `git checkout -- api/congeladas.sha256`.
- O T5f fica de fora: o resto dele fechou em `27f86af`, e o `Espiao` do `seele-ffi` é opcional.

**Comandos:**
- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-core --lib encontro`;
- `… -p seele-proto`;
- `… -p seele-server --lib alcance`;
- `… -p seele-conformance --test quarto --test quarto_pelo_link --test ocupante_da_escuta --test volta_pela_trilha`. O `quarto_pelo_link` usa a 8384: confira antes que nenhum outro binário a está usando (`lsof -nP -iUDP:8384`), e não rode dois deles ao mesmo tempo;
- `cargo build -p seele-encontro --examples`;
- fmt, e o clippy de `seele-proto`, `seele-core`, `seele-server`, `seele-ffi`, `seele-conformance` e `seele-encontro`, porque o T1c muda um item público.

Rode a reversão de cada guarda novo antes do commit.

**NÃO fazer:**
- não mudar o fio: as marcas continuam saindo dos mesmos 16 caracteres, e o SEELE-ENC/1 não muda;
- não tocar `enlace.rs`: a montagem de certificado é do M2;
- não criar construtor de `Marcas` para teste entre crates;
- não reescrever a linha :83 do ADR 0047;
- não mexer no doc de `onde_mora_hoje`, em `hospedagem.rs` nem no `lib.rs` do `seele-server` (lote M2);
- não fazer o conserto da pendência 55, que é do Plano 5;
- no `sementes_do_fuzz.rs`, só o trecho do `datagrama_do_encontro`: o `control_frame` é vizinho da F1.

**Riscos.**
- T5d, T6f e T1a são código de produto, mas só mudam a aceitação local.
- O T6f muda o tempo da consulta só quando um envio é recusado depois da volta 1: em vez de `PontoMudo` na hora, ela espera a resposta tardia até a volta seguinte, o teto ou o prazo.
- O M2 do 19 muda o custo da consulta só quando a primeira família não tem rota.
- O T1c alcança cinco crates.
- O ENC/2 (F2) vem depois do M2 e edita os mesmos arquivos.

## Lote ferramentas — os guardas que provavam menos do que diziam, e o que a revisão do L1 deixou

**Onda 1.** Itens: P51-15, P51-21, L1-m2, L1-m3 e L1-m4.

**Por que juntos.** São pequenos, de ferramenta, e nenhum é código do produto. Os três do L1 são guardados no mesmo `xtask/tests/empacotamento.rs`.
- O P51-22 (o guia) foi feito em `a77da2f`.
- O P51-28 (o vetor de referência) foi absorvido pela API 6.

**Arquivos:**
- `xtask/src/check_api.rs`;
- `.gitattributes`;
- `empacotar/publicar.sh`;
- `.github/workflows/ci.yml`;
- `fuzz/Cargo.lock`;
- `install.sh`;
- `xtask/tests/empacotamento.rs`.

**Estado em `d4eb6f1`:** nada feito. Duas partes do P51-15 já fecharam, e quem as registra é o Z:
- o «panicked» no stderr saiu, em `4887565`;
- a etapa do check-api no `publicar.sh` existe, em `8b766e5`.

### 1. P51-15 — o check-api
- Em `fn conferir_api` (`check_api.rs`, :362), dois `push` não têm teste:
  - `falhas.push(format!("não deu para ler {}", caminho.display()))` (:388);
  - `falhas.push(format!("{} não é json", caminho.display()))` (:392).
- No `mod tests` (:530), com `fn pasta_com(nome, arquivos)` (:904), ao lado de `uma_api_6_no_disco_que_falha_reprova_o_check_api` (:932):
  - (a) um `v7.json` com o conteúdo `{` faz `conferir_api` voltar `Err` com uma linha que contém «não é json» e o caminho;
  - (b) `Ok(dir.join("v8.json"))`, sem o arquivo, volta `Err` com «não deu para ler».
- O braço `Err(falhas) => {` de `pub(crate) fn run() -> ExitCode` (:478-483) sai para `fn relatar(resultado, n_momentos, n_eventos, saida: &mut impl Write, erros: &mut impl Write) -> ExitCode`, com teste:
  - `Err` dá `FAILURE` e escreve cada falha com o prefixo «check-api:»;
  - `Ok` dá `SUCCESS` e escreve o resumo.
- **Reversões:** apagar o push «não é json» reprova (a); apagar o «não deu para ler» reprova (b); trocar o `FAILURE` de `relatar` por `SUCCESS` reprova o teste dele.

### 2. P51-21 — o `.gitattributes`
- `.gitattributes`, :26-28, só comentário. «A regra de cima / já bastaria hoje; esta sobrevive a alguém afrouxá-la.» contradiz o comentário de `api/*.json -text` (:68-80). Passa a dizer: «A regra de cima bastaria para os vetores que só chegam pelo checkout; para um arquivo escrito numa máquina Windows e conferido pelo hash da cópia de trabalho (`api/*.json`, abaixo), não basta. Nos dois casos, esta sobrevive a alguém afrouxá-la.».
- O guarda `todo_vetor_cujos_bytes_sao_o_contrato_fica_fora_do_conversor_de_fim_de_linha` (`apps/seele-app/src/catalogo.rs`) ignora as linhas com `#` e continua verde.

### 3. L1-m2 — o aviso dos assuntos sem classe sai antes da bateria
- O aviso «1 assunto desta faixa não diz pelo prefixo se muda o produto:» (e o irmão «é um escopo que a tabela de $0 não conhece») mora dentro de `notas_das_mudancas() {` (`publicar.sh`, :536; os avisos em :622-639).
- Ela é chamada num ponto só: `git -C "$RAIZ" log --no-merges --format='%s' "$FAIXA" | notas_das_mudancas` (:2012). Isso acontece depois:
  - da bateria (`bateria`, :1773, com o `cargo test --workspace` em :1147-1149);
  - dos builds;
  - do «--- publicando ---» (:1960);
  - e do `ANTERIOR=$(… tag_anterior …)` (:1979).
- **O que está errado.** Quem publica só lê o aviso depois da bateria e dos três builds. O `--conferir` sai (:1757-1766) sem nunca classificar. E o conselho do aviso («conte em `empacotar/notas/<versão>.md`») não se segue sem abortar, porque `conferir_arvore` exige `empacotar/` limpo.
- **O conserto.** A classificação roda também na fase de conferências, antes da bateria, e só pelos avisos: a faixa e a tag anterior são operações de git, baratas. Assim o `--conferir` mostra o aviso, e a publicação de verdade o mostra antes de compilar. O corpo continua montado no fim, como hoje.
- **Teste em `empacotamento.rs`**, no molde de `com_tudo_no_lugar_as_conferencias_passam_sem_compilar_nada` (:918, que roda `["1.2.3", "--conferir"]`):
  - `Bancada::montar` (:560) faz um commit só, sem tag. O teste cria a tag `v1.2.2` e mais um commit sem prefixo;
  - o `--conferir` tem de mostrar «não diz pelo prefixo».
  
  A ordem não se prova pela posição no texto, porque `Saida.texto` é o stdout seguido do stderr. A prova é o aviso aparecer num `--conferir`, que nunca chega à bateria.
- **Reversão:** tirar a chamada antecipada deixa o teste vermelho.
- O guarda do conteúdo, `quem_publica_fica_sabendo_quantos_assuntos_o_resumo_nao_classificou` (:1906), continua verde.

### 4. L1-m3 — o fuzz com o lock conferido, e o crash guardado
- **O lock está atrasado.** `fuzz/Cargo.lock` foi tocado pela última vez em `8df5da6` (09/08). O `serde_json` entrou no `crates/seele-proto/Cargo.toml` em `f4e3183` (05/09). No lock, o `seele-proto` tem só postcard, serde, sha2 e thiserror, e faltam o `serde_json` e as dependências dele.
- **Atualizar o lock sem rede:** `cargo metadata --offline --manifest-path fuzz/Cargo.toml --format-version 1 > /dev/null`, ou `cargo update --offline -p seele-proto --manifest-path fuzz/Cargo.toml`. As crates já estão no cache local, porque o workspace usa as mesmas versões.
  - Confira com `git diff fuzz/Cargo.lock` que só entram o `serde_json` e as dependências dele, nas versões do `Cargo.lock` do workspace.
  - Se o cache não bastar, pare com NEEDS_CONTEXT: nada se baixa.
- **No job `fuzz-curto:`** (`ci.yml`):
  - tirar a justificativa «Sem `--locked`, e de propósito: o `fuzz/Cargo.lock` está atrás do `seele-proto`…» (:423-425);
  - antes dos `cargo +nightly fuzz run`, um passo que confere o lock: `cargo +nightly metadata --locked --manifest-path fuzz/Cargo.toml --format-version 1 > /dev/null`. O `cargo fuzz run` não aceita `--locked`;
  - no fim, um passo `actions/upload-artifact@v4` com `if: failure()`, guardando `fuzz/artifacts/`, onde o libFuzzer grava o `crash-*`. O molde do repositório é `release.yml`.
- **Guardas em `empacotamento.rs`:**
  - `o_ci_roda_deny_e_fuzz` (:3089) passa a exigir o passo do `--locked` e o upload de `fuzz/artifacts` com `if: failure()`. A ajuda `comandos_do_ci` (:3047) lê só `run:` de uma linha: o upload, que é `uses:`/`with:`, precisa de uma leitura própria;
  - um guarda novo, sem rede, compara as dependências declaradas em `crates/seele-proto/Cargo.toml` com as que o pacote `seele-proto` lista no `fuzz/Cargo.lock`. Ele reprova com o lock de hoje e passa com o atualizado.
- **Reversões:** voltar o lock deixa o guarda novo vermelho; tirar o upload ou o `--locked` deixa vermelho o `o_ci_roda_deny_e_fuzz`.
- **Não mexer no gatilho do `ci.yml`**, que é do dono.

### 5. L1-m4 — o `sysctl` fora do PATH
- `install.sh`, :141: `if [ "$(sysctl -in hw.optional.arm64 2>/dev/null || true)" != 1 ]; then`. No macOS o `sysctl` mora em `/usr/sbin`. Num `curl | sh` com um PATH sem ele:
  - o comando não é achado;
  - o `2>/dev/null || true` engole o erro e devolve vazio;
  - um Mac Apple Silicon recebe «este Mac é Intel» e a frase de compilar.
- **O conserto:** acrescentar `/usr/sbin:/sbin` **no fim** do PATH dessa chamada, ou do script. Chamar `/usr/sbin/sysctl` por caminho absoluto faria o dublê do teste deixar de interceptar.
- **Prova:**
  - `Instalador::rodar_contra` (`empacotamento.rs`, :2411) põe os dublês `uname` e `sysctl` na frente do PATH. Os vizinhos são `o_instalador_de_uma_linha_instala_quando_a_soma_confere` (:2527) e `uma_maquina_sem_pacote_da_arquitetura_dela_recebe_a_frase_de_compilar` (:2559);
  - um teste comportamental, `#[cfg(all(target_os = "macos", target_arch = "aarch64"))]`, roda o `install.sh` com o dublê de `uname` (Darwin arm64) e um PATH **sem** `sysctl`. O script tem de seguir como Apple Silicon, achando o `sysctl` de verdade em `/usr/sbin`, e não pode dizer a frase do Intel. Fora de um Mac Apple Silicon ele não roda, e o relatório diz isso;
  - um guarda de texto, que roda em toda máquina: a chamada do `sysctl` no `install.sh` alcança `/usr/sbin`;
  - **reversão:** voltar a linha de hoje deixa os dois vermelhos.

**Comandos:**
- `perl -e 'alarm 600; exec @ARGV' cargo test -p xtask check_api`;
- `… -p xtask --test empacotamento`;
- `… -p seele-app --bin seele-app catalogo`;
- fmt e o clippy do `xtask`;
- opcional, sem rede: `cargo +nightly fuzz run uri fuzz/sementes/uri -- -max_total_time=10`, para conferir que o lock novo compila.

**NÃO fazer:**
- não mudar o gatilho do `ci.yml`;
- não pôr cargo-deny nem cargo-fuzz em nenhum `Cargo.toml`;
- não baixar nada;
- não mexer no vetor de referência nem no prelúdio (P51-28, API 6);
- não publicar, não dar push, não disparar Actions.

**Riscos.**
- Nada no produto.
- O passo do upload e o do `--locked` só se provam num disparo do `ci.yml`, que é do dono. Aqui fica o guarda de texto.
- O efeito do `install.sh` só chega a quem usa o curl do README depois do push do `main`.

## Lote P54 — «PADRÃO DA MÁQUINA · <aparelho>» só diz o aparelho quando o padrão é a escolha

**Onda 1.** Item: a pendência 54.

**Arquivos:**
- `apps/seele-app/ui/tela-server.js`;
- `apps/seele-app/bancada/qualidade-da-voz.cjs`.

**Estado em `d4eb6f1`:** nada feito. O defeito vem de `57fae20` (01/09).

**O defeito.** Em `function marcarUmaLista(chave, lista, aberto)` (`tela-server.js`, :270), a linha do padrão ganha o nome do aparelho aberto sem olhar a escolha:
- o trecho é `const resolvido = aberto === null ? null : nomeDe[chave].get(aberto);` e `` `PADRÃO DA MÁQUINA · ${resolvido}` `` (:302-309);
- a escolha mora em `const escolhidoDe = { captura: null, saida: null }` (:47), em que `null` é o padrão. Ele é preenchido em `desenharUmLado` (:194-219), por `microfone_escolhido`/`saida_escolhida`;
- o aparelho aberto vem do Snapshot (`capture.id`/`playback.id`), por `function marcarLinhas(snapshot)` (:262);
- o padrão do sistema chega em `dispositivo.default`, que vira `dataset.padrao` (:211-213, :236).

**O conserto.** Só escrever o nome resolvido quando `escolhidoDe[chave] === null`, ou seja, quando a linha do padrão é a escolhida. Com um aparelho escolhido à mão, a linha do padrão diz o padrão do sistema (a linha com `dataset.padrao === "sim"`), ou nada.

**A prova**, em `bancada/qualidade-da-voz.cjs`, que é Playwright sobre a janela de verdade e já abre `#tela-server`:
- uma tabela com dois aparelhos, um com `default: true`;
- `microfone_escolhido` igual ao outro;
- `snapshot.capture.id` igual ao escolhido.

A linha do padrão não pode trazer o nome do escolhido. Hoje traz.

**Reversão:** voltar a linha de hoje deixa o caso vermelho.

**O guarda de texto que já existe** continua verde: `the_screen_says_which_microphone_is_open_and_not_only_which_was_chosen` (`frontend.rs`, :2157) exige «EM USO» e «ESCOLHIDO» no corpo de `marcarUmaLista`. O P54 não edita o `frontend.rs`, que é do H nesta onda.

**Comandos:**
- `PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright perl -e 'alarm 240; exec @ARGV' node apps/seele-app/bancada/qualidade-da-voz.cjs`;
- `cargo xtask check-runtime`.

**NÃO fazer:**
- não mudar o que o Snapshot traz;
- não tocar `frontend.rs`;
- não instalar Playwright pela rede.

**Riscos.** É tela e não fio. A confirmação no WKWebView e no WebView2 é do dono.

# Onda 2

## Lote D — o console do executor: a promessa rejeitada só é dita depois da volta e com teto, a pilha e o aninhado têm prova, e a conta do balde sai sem esperar o MOD parar

**Onda 2, depois do C.** Itens: P51-01 e P51-14.

**Por que juntos.** Os dois mexem no laço de `rodar` do `executor.rs`:
- o P51-01 muda `dizer_as_rejeitadas`, chamada em `escoar_jobs` e na parada;
- o P51-14 muda a espera do laço e o `BaldeDoConsole`.

Os dois também mexem no prelúdio (`emTexto`) e nos testes do registro em `main.rs`, que dependem da forma que o C deixou.

**Arquivos:**
- `apps/seele-app/src/executor.rs`;
- `apps/seele-app/src/main.rs`: só testes;
- opcional, `docs/como-se-faz-um-mod.md`: a frase das seguradas, «a linha seguinte diz quantas foram seguradas (`suprimidas=`)».

**Na 0.16.**
- O `console` sobrevive à API 6, e não está em nenhum `api/v*.json`.
- O mesmo laço tem o `ParaODentro::Codigo { fonte, api }`, que chama `seele_ffi::mods::capacidades_da_api(api)`, e o mesmo `PRELUDIO` tem `comCapacidade`. A F3 edita os dois depois, em série com este lote.

**Estado em `d4eb6f1`:** nada feito. O `executor.rs` não mudou desde `3685a64`.

**Depois da onda 1 (o que o C deixou).**
- `fn registrar_da_janela_em(baldes: &BaldesDoRegistro, agora: Instant, nivel: String, onde: String, o_que: String, mod_id: Option<String>)`, com o teto por MOD **e por classe** (`enum Classe { Aviso, Servida, Depuracao }`). O nível «aviso» e o «erro» gastam do balde `Aviso`; o resto gasta de `Depuracao`. Os testes do T3 M2 passam um `BaldesDoRegistro` novo e um instante fixo, no molde de `mod o_teto_de_vazao_do_registro`.
- O m9 já está feito: o C pôs a porta da janela em `o_pior_caso_sai_com_o_mesmo_teto_pelas_tres_portas`. Confira e cite no relatório.
- `cortar_linha_do_console` e `LinhaCortada` ficaram `pub(crate)`, com o leitor `LinhaCortada::texto()`. O P51-01 (b) usa a mesma função; não reescreva a regra.

### 1. P51-01 — a rejeição é dita depois da volta, e com teto
- **(a)** Tirar o `avisar(&manda_rejeicao, &fila_de_rejeicao, ParaOFora::Falhou(format!("{REJEITADA_SEM_TRATAMENTO}: {texto}")))` do caminho normal do rastreador (:1163-1167).
  - Ele fica no braço `Err(_) => linha_de_console(` do `try_borrow_mut` (:1174-1179), onde a linha já sai na hora. Sem isso, a janela deixaria de saber.
  - O comentário do rastreador (:1105-1112, «A janela já recebeu o `Falhou` da primeira») acompanha.
- **(b)** `RejeicoesDaVolta::guardar` (`struct RejeicoesDaVolta`, :2016; `fn guardar`, :2028) guarda, junto da `LinhaCortada`, o texto do `Falhou` já cortado por `cortar_linha_do_console`. A conta `alem` continua só número.
- **(c)** `fn dizer_as_rejeitadas(` (:2089) recebe `fila: &Arc<Fila>`. Para cada rejeição que ninguém pegou:
  - manda a linha pelo balde;
  - e DEPOIS `avisar(manda, fila, ParaOFora::Falhou("promessa rejeitada sem tratamento: <texto cortado>"))`, por `fn avisar(` (:2113).
  
  Com `alem > 0`, sai um `Falhou` só, com a conta. Os três chamadores passam `fila`:
  - `escoar_jobs` (:1926), que já a recebe;
  - a parada (:1401);
  - o teste `a_linha_que_espera_o_fim_da_volta_ja_cabe_no_teto_e_sai_sem_novo_corte` (:4003), que precisa de uma `Fila`.
- **(d)** `tratada` continua tirando a rejeição da espera.
- **(e)** O doc de `const REJEICOES_GUARDADAS` (:1996) perde o parágrafo «**O `Falhou` que vai à janela não passa por esta espera, nem por este corte.** …» (:1987-1995).

**Testes:**
- `uma_rejeicao_pega_logo_depois_nao_vira_linha_de_erro` (`main.rs`, :11011) ganha a exigência de que nenhum `ParaOFora::Falhou` contenha «pega logo depois». Hoje ele só confere o rastro, e a exigência falha;
- `o_falhou_da_rejeicao_tem_o_teto_do_registro`: `Promise.reject(new Error('x'.repeat(100000)))` exige o `Falhou` dentro do teto (512 mais o aviso do corte);
- o teste de :4003 continua lendo o primeiro `try_recv()` como `Console`, e ganha a asserção de que o segundo é o `Falhou` cortado;
- `uma_promessa_rejeitada_sem_tratamento_nao_morre_calada` (`executor.rs`, :3908) e `uma_promessa_rejeitada_sem_tratamento_chega_ao_registro_com_o_texto` (`main.rs`, :10961) continuam verdes.

**Reversão:** devolver o `avisar` ao caminho normal deixa vermelha a primeira asserção.

### 2. P51-14
- **m8, a pilha.**
  - Exigir a pilha, com `r"\n    at "`, na linha do temporizador (`um_temporizador_que_lanca_deixa_linhas_limitadas_pelo_balde`, `main.rs`, :11288) e na do ouvinte (`um_ouvinte_de_evento_que_lanca_deixa_a_linha`, :11321). No `seele.log`, a quebra sai escapada.
  - No teste do executor que lê o texto cru (`cada_metodo_do_console_escreve_no_seu_nivel`, `executor.rs`, :2530), exigir a quebra de verdade.
  - **Reversão:** tirar `+ (valor.stack ? '\n' + String(valor.stack) : '')` de `emTexto` (:1560) reprova os três.
- **m9.** Confira que o C pôs `registrar_da_janela_em` em `o_pior_caso_sai_com_o_mesmo_teto_pelas_tres_portas`, e cite isso no relatório. Se não estiver lá, escreva o caso com `"x".repeat(2048)`, exigindo o que `ate_o_teto_do_registro` promete: o teto escapado e o aviso.
- **T3 M2.** Dois testes por `registrar_da_janela_em`:
  - nível «erro» com `Some("a/b")` exige ERROR e `mod_id=a/b`;
  - nível «outro» exige DEBUG e `mod_id=a/b`. A captura já é TRACE (`.with_max_level(tracing::Level::TRACE)`, em `mod rastro_de_teste`).
  
  Procure campos, e não a forma inteira da linha. **Reversão:** tirar `mod_id` do braço «erro» reprova.
- **T1, em `emTexto`** (`const semMentir`, :1544-1549; `const emTexto`, :1555-1575):
  - `semMentir` dá `name: message` para um Error aninhado, `[função nome]` para uma função e `String(v)` para um símbolo;
  - quando o primeiro `JSON.stringify` lança (ciclo), uma segunda passada troca por «[cíclico]» só o que é ancestral do valor de agora. A pilha de ancestrais sai do `this` do substituto. Um `WeakSet` de tudo o que já foi visto marcaria como cíclica uma referência repetida;
  - teste: `console.log({e:new Error('x'), f(){}, s:Symbol('s')})` dá «Error: x», «[função f]» e «Symbol(s)»; `o.o=o; console.log(o)` contém «cíclico» e não «[object Object]»;
  - o teste `uma_parte_que_nao_vira_texto_nao_derruba_a_linha_do_console` (:2508-2525) troca de exemplo para um objeto de protótipo nulo com um getter que lança. O comentário «**Cada parte se protege sozinha.**» (:1550-1554) acompanha.
- **T2 N1, a conta sem esperar o MOD parar.**
  - `BaldeDoConsole` (:486) ganha `ate_a_proxima_ficha(agora) -> Option<Duration>`, que só é `Some` com `suprimidas > 0`.
  - No laço de `rodar`, a espera (`Some(quando) => match recebe.recv_timeout(quando)` e `None => match recebe.recv()`, :1278-1287) passa a `recv_timeout(min(proximo_vencimento, ate_a_proxima_ficha))`. O `recv()` sem prazo só fica quando os dois são `None`. O ramo do timeout (`let Some(entrada) = entrada else {`, :1292) já chama `dizer_as_seguradas`.
  - Os docs acompanham: :476-478 (o doc do balde), :581-582 («…tem a conta dita quando para.») e :1275-1277 («sem temporizador ela dorme no canal»).
  - Teste: `for (let i=0;i<100;i++) console.log(i);`, sem temporizador e sem Encerrar, recebe em até 1 s um `Console` com `CONTA_DO_BALDE` e `suprimidas>=60`. Hoje a conta só vem no Encerrar.
  - `uma_rajada_no_console_seguida_de_silencio_tem_a_conta_dita_ao_parar` (:2937) continua verde.
  - **Risco:** `a_conta_das_seguradas_sai_na_primeira_volta_com_ficha` (:2965) continua verde, mas deixa de provar o que o nome diz. Renomeie-o, ou reescreva o doc dele, para o que ele mede agora.
- O T2 m1 já fechou em `517afaa`, e não há nada a fazer.

**Depois de mexer no prelúdio,** rode `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-conformance --test mod_de_referencia`. O guarda dele acha onde o console começa e termina no prelúdio (`sem_o_console`). `semMentir` e `emTexto` ficam antes de `globalThis.console = {`.

**Comandos:**
- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-app --bin seele-app executor`;
- `… --bin seele-app o_registro_da_janela`;
- o `mod_de_referencia` acima;
- fmt e o clippy do `seele-app`.

**NÃO fazer:**
- não mandar o `Falhou` por `manda.send` direto: ele passa por `avisar`, por causa da cota `AVISOS_NA_FILA`;
- não mandar o `Falhou` antes da linha, ou o teste de :4003 quebra pela ordem;
- não usar `WeakSet` para detectar ciclo;
- não mexer nas portas do registro (C), nas gerações (E) nem em `capacidades_da_api`/`comCapacidade` (F3).

**Riscos.**
- Nada no fio nem na API.
- O T1 muda o texto de objetos aninhados no `seele.log`.
- O T2 N1 acorda a thread do motor no máximo uma vez por ficha (≤250 ms), e só enquanto há linha segurada.
- A gestão passa a saber da rejeição no fim da volta, e não no meio dela.

## Lote K5 — o som de uma contribuição só decodifica quando o MOD manda tocar, e muda de destino quando quem o tomou sai da tela

**Onda 2, depois do K2.** Itens: P49-7c (a parte daqui), P49-8 e, opcional, a parte daqui do P51-25.

**Por que juntos.** Os dois mexem na mesma marca de sons da contribuição (`sonsDaContribuicao.tomados`, `tomouOsSons`) e em `montarSom`. O 7c adia a decodificação até o `tocando`, e o 8 devolve a marca e remonta noutro destino. Feitos juntos, há uma regra só de quem toma e de quando decodifica.

**Na 0.16.**
- A ordem inverteu: o K5 vem antes do K4. É o K4 que vai partir do `montarSom` que este lote deixar.
- A frase pública nova («numa contribuição, a recusa pelo teto chega quando o MOD pede para tocar») vai para o rascunho da v6, e **não** para o `api/README.md`.

**Arquivos:**
- `apps/seele-app/ui/mods-regiao.js`;
- `apps/seele-app/ui/base.js`;
- `apps/seele-app/bancada/regiao-do-mod.cjs`.

**Estado em `d4eb6f1`:** nada feito.

**Depois da onda 1 (o que o K2 deixou).** A bancada `regiao-do-mod.cjs` ganhou um `Image` padrão no `bancada()`, e os URIs `URI_QUE_DEMORA` e `URI_QUE_NAO_DECODIFICA`. Os caminhos de imagem passaram a `.then(aplicar, recusar)`, com o tropeço de aplicar dito por `dizerTropecoDaMidia`. O som não muda com isso: não mexa nos caminhos de imagem nem nesses dublês.

### 1. P49-7c — o som da contribuição só decodifica ao tocar
O teto por MOD fica com o dono.
- Uma contribuição declarada com `tocando: false` não decodifica até o MOD declarar `tocando: true`. Numa contribuição não há botão.
- **Em `async montarSom(`** (:2338):
  - a marca é tomada como hoje, antes de qualquer `await` (`if (this.sonsDaContribuicao && !this.tomouOsSons)`, :2342-2349);
  - o pedido a `som_do_mod` e o `decodeAudioData` ficam adiados;
  - a figura assenta, e a anotação diz «pronta» com o tamanho do arquivo, porque o Rust já conferiu o papel e o teto de arquivo.
- Quando o pedido de tocar chega, a decodificação roda, e o teto do bolso vale ali, com a mesma recusa: figura «cheia» e `recusada` com os números.
- **Os docs:**
  - `LIMITES_DA_REGIAO` («Bytes de som **decodificado** somados nesta região», :67) diz que cada superfície tem o seu bolso;
  - o doc de `bytesDeSomDecodificado` em `LIMITES_DO_CARTAO` (:132-147) diz que o som de uma contribuição só é decodificado quando o MOD o manda tocar. O K3 edita o mesmo bloco depois.
- **Prova** na `regiao-do-mod.cjs`, com os `sonsDaContribuicao` que a bancada já monta (em `oSomNumCartaoNaoTemBotao`, :2369, e em `oSomQueSaiDaTelaParaComoOAudioParava`, :3245):
  - uma contribuição com `tocando: false` não chama `bytesDoSom` nem `decodeAudioData`;
  - depois de um `tocando: true`, chama.
  
  Decodificar na montagem deixa o caso vermelho.

### 2. P49-8 — o som muda de destino
- **O caso.** O destino que tomou os sons sai da tela: o nó dele desconectado, visto em `function calarOsSonsQueSairamDaTela()` (`base.js`, :1494-1504) e em `calarSonsForaDaTela(cartoesPintam)` (`mods-regiao.js`, :2583-2597). Enquanto isso, a contribuição ainda desenha noutro destino conectado.
- **O que acontece:** o renderer que saiu:
  - solta os tocadores;
  - devolve ao bolso o que o som decodificado ocupava;
  - esquece as anotações desses sons;
  - devolve a marca (`sonsDaContribuicao.tomados = false`, `tomouOsSons = false`).
- **Redesenhar não basta.** A reconciliação reaproveita a figura que assentou sem tocador: o doc de `montarContribuicao` (`base.js`, :1259-1261) e o código `const pronta = montadas.get(chave); if (pronta) return pronta.raiz;` (:1272-1275).
  - O renderer que assentou um som sem tocador guarda esse adiamento.
  - Um destino conectado o remonta, na mesma varredura, quando a marca volta: toma a marca, e só pede os bytes quando houver `tocando` (pelo 7c).
- **A recusa `SOM_FORA_DA_TELA`** (`aplicarTocando`: `if (!elem.isConnected) { som.recusar(SOM_FORA_DA_TELA)`, :2536-2538) deixa de sair nesse caso.
- O doc «Ela não é devolvida: os destinos saem juntos» (:2325-2327) muda.
- Quando a pessoa do primeiro destino volta, o nó dele reaparece sem tocador.
- **Prova:**
  - dois renderers com o mesmo `sonsDaContribuicao`;
  - o primeiro toma o som; o nó dele sai do documento, e a varredura roda;
  - um `tocando: true` religado toca no segundo, sem recusa, e só existe um tocador;
  - num segundo passo, o nó do primeiro volta, e continua um tocador só.
- **Reversões:** não devolver a marca traz de volta a recusa de `SOM_FORA_DA_TELA`; devolver a marca sem remontar no segundo deixa o `tocando` sem tocador.

### 3. Opcional, a parte daqui do P51-25
No `try` de `bytesDoSom` (:2367-2375): quando `typeof this.dono.bytesDoSom !== 'function'`, o `falhou` diz «o dono desta região não oferece bytesDoSom», no lugar do `TypeError` genérico, com um caso na `regiao-do-mod.cjs`.

**Comando:** `cargo xtask check-runtime`.

**NÃO fazer:**
- não criar teto de som por MOD: escolher quanto som um MOD segura, e a frase pública disso, é do dono;
- não mudar a API congelada, nem o `api/README.md`;
- não decodificar na montagem.

**Riscos.**
- O primeiro toque de uma contribuição passa a ter a latência da decodificação.
- A recusa pelo teto passa a chegar no pedido de tocar, e não na montagem.
- Uma troca de tocador no meio de um som o faz recomeçar.
- O pior caso (muitas contribuições tocando ao mesmo tempo) continua sem teto por MOD, e isso fica com o dono.

## Lote M2 — o ambiente do encontro nos testes com trava, a linha do quarto com `levou=`, os textos do 1B, e a montagem de certificado

**Onda 2, depois do M1 e do H.** Itens: P51-16, P51-18, P51-20, e a montagem de certificado do 1B, que veio do M1.

**Por que juntos.**
- Os três mexem no mesmo bairro do M1, e por isso vêm depois dele.
- O P51-18 é o dono do doc de `onde_mora_hoje`, que tem de descrever o que o T6f do M1 deixou.
- A montagem de certificado e o doc do pino moram no `enlace.rs`, que o H edita na onda 1.

**Arquivos:**
- `crates/seele-server/src/lib.rs`;
- `crates/seele-server/src/hospedagem.rs`;
- `crates/seele-server/src/alcance/encontro.rs`;
- `crates/seele-core/src/encontro.rs`;
- `crates/seele-core/src/enlace.rs`: o doc do pino e o módulo de testes;
- `crates/seele-ffi/src/types.rs`, `crates/seele-proto/src/encontro.rs` e `crates/seele-encontro/src/lib.rs`: só doc;
- `docs/alcance-pela-internet.md` e `docs/ponto-de-encontro.md`;
- `crates/seele-conformance/tests/volta_pela_trilha.rs`;
- o plano 1B, só a linha :99.

**Estado em `d4eb6f1`:** nada feito. `hospedagem.rs` e o `lib.rs` do `seele-server` não mudaram desde `3685a64`.

**Depois da onda 1.**
- **O que o M1 deixou** em `seele-core/src/encontro.rs`, e que o doc de `onde_mora_hoje` (P51-18) tem de descrever:
  - o traço privado `Canal`, a costura de teste do T6f;
  - `PorUmEndereco { SemSocket, NadaSaiu, Perguntou }`: a consulta passa ao endereço seguinte só quando nada saiu, e diz isso numa linha info;
  - a volta recusada depois da volta 1 conta como volta;
  - `saiu_em` só anda quando algo saiu.
- **O que o M1 deixou no alcance:** `NaoSaiu { passageira }` e `a_falha_que_conta`.
- **O que o H deixou no `enlace.rs`.** Testes novos usam `impostor_que_repete_o_certificado`: o do pin desfeito, e `o_candidato_perdedor_que_nao_prova_a_chave_vai_ao_log_como_aviso`. A montagem compartilhada (passo 4) tem de mantê-los verdes. O H também reescreveu o doc de `EndReason::ServidorTrocouNaVolta` em `types.rs`: releia o C-M3 contra ele.

### 1. P51-16 — o ambiente do encontro com trava
- **Em `crates/seele-server/src/lib.rs`:** `#[cfg(test)] pub(crate) mod ambiente_de_teste`, com `AmbienteDoEncontro` (guardar, pedir, e um `Drop` que devolve). O molde é `quarto_pelo_link.rs` (`struct AmbienteDoEncontro`, `guardar`, `pedir`, `impl Drop`).
  - Ele segura, **como campo**, um `MutexGuard<'static, ()>` de um `static TRAVA: Mutex<()>`, tomado com `unwrap_or_else(PoisonError::into_inner)`.
  - Tem de ser campo, e não um `let _t = TRAVA.lock()` solto. O teste de `hospedagem.rs` é `#[tokio::test] async` e segura o guarda através de `.await`. O clippy do CI (`-D warnings`, `ci.yml`) reprova um guarda solto por `await_holding_lock`, e não reprova o struct (medido com o clippy 1.97).
- **Os dois testes que trocam a variável passam a usá-lo:**
  - `async fn um_ponto_de_encontro_fora_do_ar_nao_atrapalha_nada` (`hospedagem.rs`, :538-608), com `let restaurar = |valor: Option<String>|` (:546) e o `restaurar(antes);` (:572);
  - `fn desligar_o_degrau_4_e_uma_variavel_de_ambiente` (`alcance/encontro.rs`, :1297-1336). O comentário «SAFETY-de-teste: … são serializados por rodarem em sequência neste teste» (:1308-1309) deixa de prometer a serialização.
- **Teste com `catch_unwind`,** no molde de `o_ambiente_do_encontro_volta_mesmo_quando_o_teste_entra_em_panico` (`quarto_pelo_link.rs`): esvaziar o `Drop` reprova.
- **Para a corrida:** `cargo test -p seele-server --lib -- --test-threads=8`, em laço.

### 2. P51-18 — a linha do quarto com `levou=`
- Em `pub async fn onde_mora_hoje(` (`seele-core/src/encontro.rs`, :681): `let comecou = tokio::time::Instant::now();`. A linha final (`"quarto: onde o anfitrião mora hoje"`, :702) ganha `levou = ?comecou.elapsed()`.
- **O doc** (:632-680; o item errado é «leitura falha**: volta na hora, como [`OndeMora::PontoMudo`]», :666-667) é reescrito com o comportamento que o M1 deixou, o T6f inclusive: «como PontoMudo se nada respondeu antes; se o ponto já respondeu, como NinguemMora, ou Achado com o que chegou».
- **Prova:** em `um_nome_que_nao_resolve_no_prazo_diz_no_rastro_que_foi_o_prazo` (:1404), exigir uma linha com «onde o anfitrião mora hoje» e «levou=». Hoje ela reprova.

### 3. P51-20, só texto
- **C-M3.** No doc de `InviteMismatch` (`seele-ffi/src/types.rs`, «O que falhou é a promessa, do link ou da»): num alvo de LAN, a promessa pode estar certa e quem atendeu ser outro servidor no mesmo endereço. Releia o texto depois do H, que mexeu em `types.rs`.
- **C-M4.** No doc de `pub struct Marca` (`seele-proto/src/encontro.rs`, «conhecidos, que guardou a impressão de um link;»): «guardou a impressão que a conexão aceitou».
- **C-M5.** No plano 1B, :99 («o loopback conta como público.»): a prova é dos testes da FFI (`um_alvo_publico_ou_de_loopback_tem_o_pino_que_prova_o_servidor` e `num_alvo_publico_ou_de_loopback_o_pino_que_confere_passa_e_so_avisa`), e o fixture de `convite.rs` passa `o_pino_prova_o_servidor: true` à mão.
- **C-R1-m1.** «até o quarto encher e chegar uma marca nova (`Quarto::morar`)» nos cinco lugares:
  - `crates/seele-encontro/src/lib.rs` («até o quarto encher (a varredura)»);
  - `docs/alcance-pela-internet.md`: «quando o quarto enche», e a outra menção;
  - `docs/ponto-de-encontro.md`, nas duas menções.
  
  **Uma delas fica no parágrafo de abertura sobre os 60 s**, que é a cadeia do Lote C no G1 (`63113a6`). Troque só a oração do quarto, e não a afirmação dos 60 s. O G1 confere commits, e não texto. A frase fica mais precisa, e não falsa, e por isso não ganha cadeia.
- **T4.** No doc de `async fn esperar_o_servidor_fechar` (`volta_pela_trilha.rs`, :345-353), só «dezenas de / milissegundos» vira «microssegundos (medido: 4 µs)». A frase dos «20,0 s (o `IDLE_TIMEOUT`)» é do lote F.
- **O pino.** O doc de `fn desfazer_pin_orfao` (`enlace.rs`) passa a dizer o segundo resíduo e que os dois falham para o lado seguro. O segundo resíduo: dois candidatos na mesma chave, e a linha do pin órfão levando a falha do vizinho. O primeiro já está no doc de `fn desfazer_o_pin_deste_aperto` («**O que ela não distingue:**»).
- **Prova:** os blocos de grep da pendência 51 (P51-16, P51-18 e P51-20) deixam de achar as frases.

### 4. A montagem de certificado do 1B (veio do M1)
- No `mod tests` de `enlace.rs`, `fn ponta_com_certificado_proprio()` e `async fn servidor_que_aperta_a_mao_e_cai()` repetem, linha a linha, o trecho que vai de `rcgen::generate_simple_self_signed` até o `quinn::Endpoint::server`.
- Um terceiro, `fn impostor_que_repete_o_certificado()`, repete o pedaço `ServerConfig` + ALPN + `Endpoint`.
- Uma montagem compartilhada no módulo de testes serve aos três, sem mudar o que cada teste prova. Os testes do H que usam o impostor continuam verdes.

**Comandos:**
- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-server --lib`;
- `… -p seele-core --lib encontro`;
- `… -p seele-core enlace`;
- `… -p seele-conformance --test volta_pela_trilha`;
- `cargo doc -p seele-proto --no-deps`;
- fmt e o clippy dos crates mexidos.

**NÃO fazer:**
- no plano 1B, só a região de :99;
- não mexer na afirmação dos 60 s da abertura de `ponto-de-encontro.md`;
- não pôr trava nos outros testes que chamam `Hospedagem::iniciar`: fica registrado como risco.

**Riscos.**
- No `seele-server` há 11 `Hospedagem::iniciar(`, todos em `hospedagem.rs`. Dois estão no teste que troca a variável, e os outros 9 continuam sem trava. Eles leem `$SEELE_ENCONTRO` por `Convocacao::do_ambiente`, e podem pagar o prazo na janela do `BURACO_NEGRO`.
- O guarda torna o futuro do teste `!Send`, o que o `#[tokio::test]` de uma thread aceita.
- P51-18 e P51-20 são texto e uma linha INFO. Nada toca o fio.

## Lote P53 — instalar um MOD de uma pasta com atalho: a tela diz qual é o atalho, e o `seele.log` ganha a linha

**Onda 2, depois do H** (`frases.js` e `frontend.rs`). Item: a pendência 53.

**Arquivos:**
- `apps/seele-app/src/mods.rs`;
- `apps/seele-app/ui/frases.js`;
- `apps/seele-app/tests/frontend.rs`.

**Estado em `d4eb6f1`:** nada feito.

**Depois da onda 1 (o que o H deixou).** O `frontend.rs` ganhou `a_volta_recusada_so_diz_o_que_vale_para_toda_origem_e_todo_motivo` e um guarda só da variante `ConnectionError::ChaveNaoProvada` contra `FRASES`, que lê o nome pelo serde. O doc dele diz que `ConnectionError` não tem guarda de todas as variantes. O conserto de `variants_of` não pode quebrar esses dois guardas.

**O defeito.**
- `pub(crate) enum FalhaAoInstalarMod {` (`mods.rs`, :216) tem `SemManifesto`, `ManifestoRecusado(String)` e `NaoCopiei(String)`.
- A recusa do atalho nasce em `fn copiar_arvore(de, para)` (:459), em `if tipo.is_symlink() {`, como `std::io::Error` com «atalho dentro do pacote: …». Ela vira `NaoCopiei` em `pub(crate) fn instalar_de` (:401), na linha `return Err(FalhaAoInstalarMod::NaoCopiei(erro.to_string()))`.
- Outras duas falhas de cópia, `create_dir_all` e `rename`, também viram `NaoCopiei`.
- Nem `instalar_de` nem o comando `async fn instalar_mod(` (`main.rs`) escrevem no `seele.log`.
- Nenhuma das três variantes tem frase: todas caem em `desconhecida(erro)`, «ALGO FALHOU E ESTE APP NÃO SABE EXPLICAR O QUÊ», com o JSON cru.

**O conserto.**
- **Uma variante própria para o atalho**, por exemplo `FalhaAoInstalarMod::TemAtalho(String)`, com o caminho. `copiar_arvore` passa a devolver um erro tipado que separa o atalho das outras falhas de cópia. `NaoCopiei` fica para as outras.
- **A linha no registro.** `instalar_de` escreve `tracing::warn!` com o motivo e o caminho, em toda recusa. O precedente no mesmo arquivo é «não consegui levar este pacote».
- **As frases**, em `function fraseDeErro` (`frases.js`, :398), com um braço por variante com campo, como o `erro.NomeRecusado`:
  - `TemAtalho`: «ESTA PASTA TEM UM ATALHO, E O SEELE NÃO INSTALA MOD COM ATALHO: <caminho>.\nInstale a pasta do pacote, sem `node_modules`.»;
  - `NaoCopiei`, `ManifestoRecusado` e `SemManifesto`: frases que digam o que houve.
  
  Respeite o `LIMITE_DE_FRASE`.

**O guarda que não pega isto, e precisa pegar.** `every_refusal_the_bridge_writes_itself_has_a_sentence_in_the_page` (`frontend.rs`, :2220) não alcança `FalhaAoInstalarMod`, por dois motivos:
- ele lê só `src/main.rs`, e o tipo mora em `mods.rs`;
- `fn variants_of(` (:2197) só aceita linhas `Nome,` feitas só de alfanumérico, e descarta calada a variante com campo. Isso já cala hoje o `FalhaAoHospedar::NomeRecusado(…)`.

O guarda passa a ler também `src/mods.rs`, a incluir `FalhaAoInstalarMod` e a aceitar `Nome(…),`. O guarda irmão de `FalhaAoAtualizar` (:5277) usa a mesma `variants_of`, e tem de continuar verde.

**Provas:**
- em `mods.rs`, `um_atalho_dentro_do_pacote_nao_e_instalado` passa a exigir `Err(FalhaAoInstalarMod::TemAtalho(_))` com o caminho do atalho, e `uma_copia_que_falha_no_meio_nao_deixa_pacote_pela_metade` continua `NaoCopiei`;
- um teste de rastro (`crate::rastro_de_teste::capturar`) exige a linha WARN com o caminho;
- o guarda reescrito reprova sem a frase de `TemAtalho`.

**Reversões:**
- tirar a frase reprova o guarda;
- tirar a leitura de variante com campo faz o guarda passar calado, e um teste do próprio guarda precisa prender isso;
- tirar o `warn!` reprova o teste de rastro.

**Comandos:**
- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-app --bin seele-app mods`;
- `… -p seele-app --test frontend`;
- `cargo xtask check-runtime`;
- fmt e o clippy do `seele-app`.

**NÃO fazer:**
- não passar a aceitar atalho dentro do pacote: a recusa está certa;
- não mudar o fio nem a API de MODs.

**Riscos.** Nada no fio. A janela passa a receber `{"TemAtalho": "…"}`, que é interno à casca.

# Onda 3

## Lote E — todo comando de uma geração morta é contado, e `ler_imagem_mod` diz cada recusa

**Onda 3, depois do D.** Itens: P51-09 e P51-11.

**Por que juntos.** O P51-11 usa a `recusar_se_morta` que o P51-09 cria, nas duas conferências de geração de `ler_imagem_mod`, e a pendência pede os dois juntos.

**Arquivo:** `apps/seele-app/src/main.rs`, e só ele.

**Na 0.16.**
- `ler_imagem_mod` e `enviar_imagem_mod` são a capacidade `volume`, que nasceu na API 5.
- **A API 6 mantém o `volume`** (ADR 0048), confirmado pela F3 em 08/10. A capa e a foto do PERFIS, os mapas da MESA e o fundo do ESTILO dependem dela. O P51-11 tem objeto.
- No celular, a capacidade pode ficar ausente por plataforma, se a ponte web não tiver `ler_imagem_mod`. No desktop nada muda.

**Estado em `d4eb6f1`:** nada feito.

**Depois da onda 1 (o que o C deixou).**
- `fn recusa_de_midia_dita(baldes: &BaldesDoRegistro, agora: Instant, mod_id: &str, origem: &str, falha: FalhaNoMod) -> FalhaNoMod`. O motivo `sessao-encerrada` gasta da classe `Depuracao`, e o resto da classe `Aviso`.
- `ler_imagem_mod` já passa `&session.baldes_do_registro`. O guarda `os_comandos_passam_os_baldes_da_sessao` conta as chamadas por comando, e o mínimo dele para `ler_imagem_mod` muda se `imagem_lida_dita` concentrar as duas recusas. Atualize-o junto, com reversão.
- `imagem_lida_dita` recebe `baldes` e `agora`, como as outras portas.

### 1. P51-09 — toda geração morta é contada
- **Criar `Session::recusar_se_morta(&self, geracao) -> Result<(), FalhaNoMod>`**, ao lado de `fn geracao_vale` (:352) e de `fn confere_geracao` (:357). Ela:
  - confere a geração;
  - conta em `comandos_de_geracao_morta`;
  - escreve a linha DEBUG de `confere_geracao` («comando recusado: a sessão dele já acabou»);
  - devolve `sessao-encerrada`.
- **As treze conferências `if !session.geracao_vale(geracao)` passam a usá-la.** Procure pelo texto, porque as linhas andam depois do C.

  | função | conta hoje? |
  |---|---|
  | `mod_nativo_colher` | não |
  | `mod_nativo_entregar` | não |
  | `ler_codigo_do_mod` (auxiliar, não é `#[tauri::command]`) | sim, à mão |
  | `midia_do_mod` | sim, à mão |
  | `som_do_mod` | sim, à mão |
  | `midia_em_bytes` | sim, à mão |
  | `enviar_imagem_mod`, antes e depois do `await` | não |
  | `ler_imagem_mod`, antes e depois do `await` | não |
  | `escolher_para_o_mod`, a primeira | sim, à mão |
  | `escolher_para_o_mod`, a segunda, depois do seletor | não |
  | `pedaco_do_escolhido` | sim, à mão |

- **`fn liberar_reserva(`** (:4011; o `if !geracao_vale()` em :4018) recebe `mortos: &AtomicU64`, como `fn registrar_reserva(` (:3989). `mod_nativo_ativar` passa `&session.comandos_de_geracao_morta`.
- **Tirar a ressalva** do doc de `registrar_reserva`: «A de [`liberar_reserva`], logo abaixo, não é contada…». A frase da tela (`camada-mods.js`, a do contador do modo de desenvolvedor) fica a mesma.

**Testes:**
- (a) `liberar_reserva` com `geracao_vale = || false` faz `mortos` subir de 0 para 1, no molde de `a_reserva_recusada_por_geracao_morta_e_contada` (`mod a_supervisao_dos_mods_nativos`). Primeiro a assinatura muda sem o `fetch_add`, e o teste fica vermelho; depois entra o `fetch_add`.
- (b) Um guarda de fonte, sem comentários, como `os_quatro_comandos_de_midia_passam_pela_recusa_dita`: nenhum `!session.geracao_vale(` fora de `impl Session`. **Um guarda que olhe só os corpos de `#[tauri::command]` acharia 12, e não 13**: a de `ler_codigo_do_mod` escaparia. Os fechos `&|| session.geracao_vale(geracao)` não casam com o padrão, e ficam.
- (c) `Session::recusar_se_morta` com `Session::default()` e a geração morta faz o contador subir. Nenhum teste de unidade de `main.rs` monta um `State`, e por isso a prova é da função, mais o guarda (b).

**Reversões:** voltar uma conferência ao `if` cru reprova (b); tirar o `fetch_add` de `liberar_reserva` reprova (a).

### 2. P51-11 — `ler_imagem_mod` diz cada recusa
- **Extrair a parte de depois do `await`:** `fn imagem_lida_dita(id: &str, lida: Result<Vec<u8>, String>) -> Result<MidiaDoMod, FalhaNoMod>`.
  - As duas recusas, a do servidor e a de `formato-desconhecido`, passam por `recusa_de_midia_dita`, com os baldes e o instante que o C pôs nela.
  - `Connection::ler_imagem_mod` devolve `Result<Vec<u8>, String>`, e isso bate com a assinatura.
- **As duas conferências de geração** passam por `recusar_se_morta`.
  - **A ordem importa.** Hoje o `map_err` do servidor vem antes da segunda conferência, e um `Err` do servidor com a geração já morta sai como WARN.
  - Mantenha: primeiro `imagem_lida_dita` sobre o `Err` do servidor, e só depois a conferência da geração para o caminho `Ok`. O relatório diz o que escolheu.
- **A `connection()` que falha** (`.connection().map_err(|_| recusa("sessao-encerrada"))?`) passa por `recusa_de_midia_dita` com `sessao-encerrada`, na linha DEBUG «a sessão acabou». Ela só conta se a geração estiver morta (`recusar_se_morta` antes dela): uma conexão ausente com a geração de pé não é comando de sessão encerrada.
- **O guarda** `fn os_quatro_comandos_de_midia_passam_pela_recusa_dita` passa a exigir `imagem_lida_dita(` em `ler_imagem_mod`. O irmão, em `mod o_som_do_mod_sai_em_bytes`, continua verde.
- **O doc de `recusa_de_midia_dita`** perde o parágrafo «**A geração morta que os comandos conferem na entrada não passa por aqui.**».

**Teste de rastro de `imagem_lida_dita`:**
- `Err("Prazo esgotado")` dá um WARN com `mod_id=` e `motivo="Prazo esgotado"`;
- `Ok(b"so texto")` dá um WARN com `motivo=formato-desconhecido`;
- `Ok(PNG)` dá `Ok`, sem linha.

Tirar uma das duas chamadas deixa o caso dela vermelho, e é isso que o guarda de hoje não pega.

**Comandos:**
- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-app --bin seele-app geracao`;
- `… --bin seele-app imagem`;
- `… --bin seele-app`, inteiro, antes do último commit;
- fmt e o clippy do `seele-app`.

**NÃO fazer:**
- não contar como geração morta a conexão ausente com a geração de pé;
- não mudar a frase da tela;
- não mexer nas portas do registro (C) nem no som (K4).

**Riscos.**
- O contador passa a subir mais, e só aparece no modo de desenvolvedor.
- Fora do escopo: as recusas de `enviar_imagem_mod` voltam à janela sem passar por `recusa_de_midia_dita`.

## Lote J — a tecla de falar com nome, o aviso de fone e o nome pedido no primeiro uso

**Onda 3, depois do K5, do P53 e do P54.** Item: F1-tecla-fone-nome.

**Por que sozinho.** É um item só, de tela, mas espalhado por oito arquivos de `ui/` e pelo `frontend.rs`. Ele divide `frases.js`, `tela-sessao.js` e `frontend.rs` com o I, e `base.js` com os K.

**Arquivos:**
- `apps/seele-app/ui/tela-sessao.js`, `tela-server.js`, `base.js`, `frases.js`, `camada-ajuda.js`, `camada-nomear.js`, `tela-boot.js` e `index.html`;
- `apps/seele-app/tests/frontend.rs`.

**Estado em `d4eb6f1`:** nada feito.

**Na 0.16.**
- O aviso de fone aparece **ao entrar numa sala de voz**, que passa a ser também o momento em que o áudio abre (D-52, lote I). A frase não muda com isso.
- A casca de uma mão (F4) mexe em sete destes arquivos depois.

### 1. A tecla
- **`function nomeDaTecla(codigo)`** (`tela-server.js`, :1523-1545) é movida para `base.js`, que carrega antes das duas telas (`index.html`: `base.js` antes de `tela-sessao.js` e de `tela-server.js`). Ela passa a ser usada nos dois lugares.
  - O comentário «**Este é o único lugar do produto que nomeia teclas**» vai junto.
  - Hoje ela só é usada em `desenharTeclaDeFalar`.
- **As linhas de estado** (`PushToTalk: "microfone abre na tecla"`) dizem «microfone abre enquanto segura ESPAÇO», com `nomeDaTecla(teclaDeFalar)`. Elas ficam:
  - em `function desenharContextoDaSessao()` (`tela-server.js`);
  - em `function desenharOperador(snapshot)` (`tela-sessao.js`).
  
  `let teclaDeFalar = "Space";` mora em `tela-sessao.js`.
- **Na ajuda** (`index.html`, `<dt class="ajuda-termo"><span class="ajuda-tecla">ESPAÇO</span></dt>`), o span fica `<span id="ajuda-tecla-falar" class="ajuda-tecla">ESPAÇO</span>`: o id antes da classe, e o texto padrão continua «ESPAÇO».
  - O guarda `a_ajuda_so_promete_teclas_que_a_janela_atende` (`frontend.rs`) lê `class="ajuda-tecla">` até o `<`, e traduz «ESPAÇO» por `"Space"`.
  - `function abrirAjuda()` (`camada-ajuda.js`) escreve `nomeDaTecla(teclaDeFalar)` no span ao abrir.
- **A linha de baixo** (`<dd class="ajuda-diz">Segure para falar, no modo tecla.</dd>`) passa a «Segure para falar, no modo tecla. Vale com a janela do SEELE na frente e fora da caixa de mensagem.».

### 2. O fone
- Na primeira entrada numa sala de voz de cada execução, uma faixa que se fecha: «USE FONE. O SEELE ainda não cancela eco: sem fone, quem fala com você se ouve de volta.».
- A mesma frase entra na ajuda e em CONFIGURAÇÕES · MICROFONE E SOM (`data-titulo="MICROFONE E SOM"`, `index.html`).
- Se a marca de «já avisei nesta execução» for um `Map` ou um `Set` de topo em `tela-sessao.js`, ela leva um comentário dizendo que é por execução, e não por sessão. O guarda de mapas de topo do lote I vai pedir esse motivo.

### 3. O nome
- **Em `async function conectar(alvo, apelido, token)`** (`tela-boot.js`):
  - se o apelido continua vazio depois de `apelido = ((await invoke("apelido_local")) ?? "").trim();`, abrir o perfil com o título «COMO OS OUTROS VÃO TE CHAMAR?» antes de `invoke("connect", {`;
  - gravar com `escolher_apelido_local` e seguir.
- **`async function abrirPerfil()`** (`camada-nomear.js`) não tem parâmetro, e o título é fixo no HTML (`<h2 id="perfil-titulo" …>SEU PERFIL</h2>`). Ela ganha um parâmetro opcional de título. Sem ele, continua «SEU PERFIL».
- **Fechar sem nome segue como hoje**, mas dizendo: «Você vai entrar com um nome tirado da sua chave. Dá para trocar no perfil.».
  - A frase não diz o nome (`pessoa-3f2a`), porque a janela não o tem: ele é derivado em Rust, dentro de `connect`, da impressão desta máquina, e nenhum comando de UI o expõe (`tela-boot.js` diz isso por escrito).
  - Expor o nome pediria um comando novo, e o lote não mexe no Rust do apelido.
- **O guarda `the_waiting_screen_says_what_happened_what_to_do_and_what_is_useless`** (`frontend.rs`) lê o corpo de `async function conectar` e procura `levarParaAEspera(` e `erro.textContent = fraseDeErro(`. O J não pode desmontar isso.

### Provas, em `frontend.rs`
- (a) `a_linha_de_estado_nomeia_a_tecla_de_falar`: as duas tabelas chamam `nomeDaTecla(`, e não contêm «abre na tecla» sozinho;
- (b) `a_ajuda_escreve_a_tecla_de_falar_que_esta_gravada`: o `index.html` tem o span com `id="ajuda-tecla-falar"`, e `camada-ajuda.js` escreve nele com `nomeDaTecla(teclaDeFalar)`;
- (c) `ha_um_aviso_de_fone_em_algum_lugar_que_a_pessoa_le`: `frases.js` tem a frase, e a tela a desenha ao entrar numa sala;
- (d) `sem_nome_o_primeiro_conectar_pergunta_antes`: em `conectar`, a checagem de vazio e a abertura do perfil vêm antes de `invoke("connect"`.

Cada um fica vermelho quando se reverte a sua parte. `a_ajuda_so_promete_teclas_que_a_janela_atende` e `o_padrao_da_tecla_de_falar_e_o_mesmo_nos_dois_lugares` continuam verdes.

**Comandos:**
- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-app --test frontend`;
- `cargo xtask check-runtime`.

**NÃO fazer:**
- não tirar «ESPAÇO» do HTML da ajuda;
- não pedir nome a quem já tem apelido;
- não mexer no Rust do apelido.

**Riscos.**
- É UX: o diálogo de nome no primeiro uso acrescenta um passo a quem entra por link.
- A medida em tela de verdade (WKWebView e WebView2) é do dono.

## Lote G-servidor — quem hospeda fecha avisando: o servidor difunde `ServerShuttingDown` terminal antes de cair

**Onda 3, depois do M2.** Itens: a metade servidor do P50-4 e do F1-fechar, e **os testes que falam com o ponto de encontro de produção** (achado da revisão do M2, decidido pela orquestradora em 08/10).

**Na 0.16.** O G antigo se divide:
- **G-servidor (este lote):** o servidor avisa cada sessão antes de cair, e o cliente trata `ServerShuttingDown` como fim. Ele já serve ao SAIR de quem hospeda, pelo comando `disconnect` da casca, que chama `server.encerrar().await` depois de `desmontar_o_cliente`.
- **G-casca (vai para o Plano 2):** a pergunta ao fechar a janela (`CloseRequested`, `prevent_close`), o `fechar_de_vez`, o `ExitRequested` segurado e a hospedagem encerrada com aviso na saída do app. Ele é reescrito contra `Session.hospedagens` e a razão `ChamadaEncerrada`, que ainda não existem.

**Arquivos:**
- `crates/seele-server/src/server.rs`;
- `crates/seele-server/src/session.rs`;
- `crates/seele-server/src/lib.rs`;
- `crates/seele-server/src/hospedagem.rs`;
- `crates/seele-core/src/enlace.rs`;
- `crates/seele-conformance/tests/bateria_interna.rs`;
- `crates/seele-conformance/tests/quem_hospeda_fecha.rs` (novo).
- para o passo 0: `crates/seele-server/src/alcance/encontro.rs`, `crates/seele-conformance/tests/ejetar.rs` e o `Cargo.toml` de `seele-server` e de `seele-conformance`, se o guarda vier por uma feature.

**Não toca `frases.js`.** `ServerShuttingDown` também é a recusa do aperto de mão quando o banco não pode ser lido (`Refusal { reason: DisconnectReason::ServerShuttingDown, detail: "could not read the server…" }`, `session.rs`). Uma frase nova como «QUEM HOSPEDA FECHOU O SERVIDOR» seria falsa ali. A frase de hoje, «O SERVIDOR ESTÁ ENCERRANDO» (`frases.js`, `MOTIVOS`), vale para os dois casos, e fica.

**Estado em `d4eb6f1`:** nada feito.

**Depois das ondas 1 e 2 (o que a F7 e a F1 deixaram).**
- **A F7 fechou a janela do Comandante** (`29d66937`). `Hospedagem::iniciar` ganhou um quinto argumento obrigatório, `antes_de_aceitar: impl FnOnce(&mut Persistence) -> Result<()>`, que roda antes do laço de aceitação. Quem não tem nada a fazer passa `|_| Ok(())`.
  - O teste dela, `crates/seele-server/tests/janela_do_comandante.rs`, também sobe hospedagem. O passo 0 o alcança.
  - Ele é teste de integração do próprio `seele-server`, e por isso não vê o `cfg(test)` da biblioteca.
  - Hoje há 17 chamadas de `Hospedagem::iniciar` no workspace: 11 em `hospedagem.rs`, 2 em `ejetar.rs`, 2 em `quarto_pelo_link.rs` (que já usa ambiente), 1 em `janela_do_comandante.rs` e 1 no `main.rs` do app, que é a de produção.
- **A F1 pôs no fio as razões da sala** (`ChamadaEncerrada`, `NinguemAtendeu` e `ChamadaRecusada`), e `a_sessao_acabou_aqui` já as inclui. Este lote só acrescenta `ServerShuttingDown`.
- **A F1 também pôs no fio o `ModAviso` e o `TemaMudou`.** Se algum `match` sobre `ServerMessage` ou `Event` que este lote toque for exaustivo, eles aparecem nele.
- **O M2 deixou em `seele-server/src/lib.rs`** o `mod ambiente_de_teste`, com o `AmbienteDoEncontro` e a `TRAVA`, para o passo 0 usar.

### 0. Os testes não falam com o ponto de encontro de produção (primeiro)
**O defeito, medido na revisão do M2.**
- Sem `$SEELE_ENCONTRO`, `Convocacao::do_ambiente` (`alcance/encontro.rs`) cai em `PONTO_PADRAO = "encontro.seele.app.br"`. Se o DNS falhar, cai nos IPs de `REDE_DO_PADRAO`.
- `Escada::subir` chama `encontro::abrir` sempre que a máquina não tem IPv4 global.
- Quem sai assim para a rede:
  - os testes de `hospedagem.rs` que sobem sem trocar a variável (9 chamadas de `Hospedagem::iniciar`);
  - as 2 chamadas de `crates/seele-conformance/tests/ejetar.rs`;
  - o `quem_hospeda_fecha.rs` novo deste lote.
- Cada rodada registra a marca de um servidor de teste no quarto da VPS de produção e deixa lá o IP público da máquina. Nenhum teste afirma esse degrau. Um teste que depende da rede de fora é flaky, e ainda mexe no quarto de produção.

**O que a orquestradora pede:**
- todo teste aponta um ponto local, ou nenhum (`nao`);
- **um guarda reprova se um teste resolver o endereço do ponto padrão de produção**;
- a prova por reversão: com o guarda no lugar e o endereço de produção de volta num teste, esse teste reprova.

**O caminho, para o implementador medir e escolher:**
- Os testes de `hospedagem.rs` usam o `AmbienteDoEncontro` que o M2 criou em `seele-server/src/lib.rs` (`mod ambiente_de_teste`, com a trava), pedindo `nao` ou um ponto local. Outra saída é uma costura de teste que passe `convocacao = None` à `Hospedagem::iniciar` sem ler o ambiente. Diga qual, e por quê.
- O `ejetar.rs` já serializa pela `vaga`. Ele usa um ambiente com `nao`, como o `quarto_pelo_link`.
- O guarda tem de alcançar também os testes de integração do `seele-conformance`, que não veem o `cfg(test)` do `seele-server`. Uma saída é uma feature de teste do `seele-server`, ligada pela `dev-dependency` do `seele-conformance` e pelo próprio `cfg(test)`, em que resolver o `PONTO_PADRAO` (ou os IPs de `REDE_DO_PADRAO`) entra em pânico com uma frase que diz o que quebra. **Meça que um build de release do app não leva a feature**: com o resolvedor 2, a feature de uma `dev-dependency` não se une ao build normal, mas confira com `cargo tree -e features` e diga o resultado. Se a feature não servir, proponha outra forma e pare com NEEDS_CONTEXT.
- Não use o `[env]` do `.cargo/config.toml`: ele vale também para o `cargo run` de quem desenvolve.
- O `main.rs` do app (a hospedagem de verdade) não muda.

**A prova:**
- o guarda reprova um teste que sobe a hospedagem sem ambiente, com a frase dele;
- a reversão é tirar o ambiente de um dos testes de `hospedagem.rs` e do `ejetar.rs`, e ver o vermelho;
- todos os testes que sobem hospedagem passam sem rede para a VPS. Se der, meça com o DNS do ponto padrão inalcançável, sem mudar ajuste do sistema.

**A decisão da orquestradora sobre o alcance do guarda (08/10, depois da revisão).**
- **O problema.** O guarda, ligado por `cfg(test)` e por uma feature de teste, não alcança os testes de integração do próprio `seele-server` rodados sozinhos.
- **A decisão é a forma estrutural**, e não remendar o `check_deps`:
  - `Convocacao::do_ambiente` deixa de recuar para o `PONTO_PADRAO`: sem `$SEELE_ENCONTRO`, não há ponto;
  - os pontos de entrada de produção, o `hospedar` do app e o `seeled`, passam o ponto padrão de forma explícita;
  - os guardas provam por reversão as duas coisas, que `do_ambiente` sem a variável não dá ponto e que os pontos de entrada passam o padrão;
  - o `recusar_a_producao` fica como segunda parede.
- **A F2 (o ENC/2) vai construir em cima.** Ela move o `PONTO_PADRAO` para o `seele-proto` e dá um assinante à `Convocacao`.

### 1. Servidor
- **O evento.** Uma variante interna de difusão em `pub enum Event` (`server.rs`), fora do fio, **com a razão**: `Event::ServidorEncerrando { reason: DisconnectReason }`. A razão é para que o Plano 2 mande `ChamadaEncerrada` pela mesma via (recomendação do levantamento F1).
  - Toda sessão a trata como o braço da expulsão: `Event::SessionEnded { person, reason } if *person == session.person =>` (`session.rs`). Esse braço escreve `ServerMessage::Disconnecting { reason }`, faz `send.finish()` e passa por `despedir(&connection, &mut send, …)`.
  - `async fn despedir` (`session.rs`) espera até 1 s por `send.stopped()`, e o prazo de `encerrar_avisando` tem de caber nisso.
  - Acerte os `match` sobre `Event`. O único exaustivo é `fn translate` (`Event::SessionEnded { .. } | Event::PersonMoved { .. } => None,`).
- **`Daemon::encerrar_avisando(&self, reason, prazo).await`.** Difunde pelo barramento (`server.events`), espera as sessões acabarem até o prazo e só então chama `shutdown()`.
  - Não há registro de sessões vivas: `session::Registry` só tem contadores. Para esperar, use o `endpoint` (`open_connections()` do quinn) ou o `wait_idle` com prazo, e diga no relatório qual.
  - `pub fn shutdown(&self)` (`lib.rs`) continua síncrono, para o `Drop`.
  - O doc dele, que ainda diz que «specs/04… quer ManutencaoProgramada e 3 s… precisa do registro M3», é reescrito.
- **`Hospedagem::encerrar`** (`pub async fn encerrar(mut self)`, `hospedagem.rs`; o caminho do SAIR): depois de `escada.descer().await`, `encerrar_avisando(ServerShuttingDown, ~500 ms)` no lugar do `self.server.shutdown()` cru. O resto continua como está.
- **`Hospedagem::despedir(self, teto)`,** para o Plano 2 usar na saída do app:
  - `encerrar_avisando(ServerShuttingDown, teto/2)`;
  - `timeout(teto/4, wait_idle())`;
  - `timeout(teto/4, escada.descer())`.
  
  É a ordem inversa da de `encerrar`, porque na saída o aviso vale mais que o roteador. Este lote a cria e a prova. Quem a chama da casca é o Plano 2.
- A variante `ServerShuttingDown` já existe no fio (`crates/seele-proto/src/control.rs`, que é da F1). Nada muda no fio.

### 2. Cliente
- **`fn a_sessao_acabou_aqui`** (`enlace.rs`, :341-346) inclui `ServerShuttingDown`. Hoje inclui `Kicked | Banned | ModsMudaram`.
  - `ScheduledMaintenance` continua sendo o «volto já» recuperável.
  - Na FFI isso já vira `EndReason::ServerShuttingDown`, e a tela de fim já tem a frase.
- **O doc de `Motivo::Moderado`** («Um operador acabou com esta sessão») passa a valer também para quem hospeda fechou.
- O Plano 2 e o Plano 5 acrescentam depois mais três razões a `a_sessao_acabou_aqui` e ao teste exaustivo.

### 3. Provas
- **`crates/seele-conformance/tests/quem_hospeda_fecha.rs`** (novo, com `vaga::minha()`):
  - `encerrar_avisa_cada_sessao_que_o_servidor_fechou`: `Hospedagem::iniciar(0, Location::Memory, "Casa", None)`, como em `ejetar.rs`, com um `Enlace` dentro. `encerrar()` faz o cliente ler `Disconnecting { ServerShuttingDown }`, e não um erro de transporte. Remover a difusão reprova;
  - `a_despedida_avisa_mesmo_com_o_runtime_caindo_em_seguida`: a Hospedagem numa `std::thread` com runtime `current_thread`, que espera um oneshot, roda `despedir(800 ms)` e deixa o runtime cair na linha seguinte. O cliente vê o aviso, e `closed()`, em menos de 1 s. Tirar o aviso ou o `wait_idle` faz o cliente cair só pelos 20 s do `IDLE_TIMEOUT`.
- **`enlace.rs`:** `toda_despedida_do_protocolo_escolhe_um_lado` espera true para `ServerShuttingDown`.
- **`bateria_interna.rs`:** um par de `uma_despedida_recuperavel_reconecta_em_vez_de_acabar_com_a_sessao` injeta `SessionEnded { reason: ServerShuttingDown }` pelo barramento (`servidor.server().events.send(...)`) e afirma que a sessão acaba sem reconectar.

Reverter `a_sessao_acabou_aqui` deixa os dois vermelhos.

**Comandos:**
- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-server hospedagem`;
- `… -p seele-server --lib`;
- `… -p seele-conformance --test quem_hospeda_fecha --test bateria_interna`;
- `… -p seele-core enlace`;
- fmt e o clippy dos crates mexidos.

**NÃO fazer:**
- não pôr nada novo no fio;
- não tornar `ScheduledMaintenance` terminal;
- não mexer na casca (`main.rs`, `tela-sessao.js`, `tela-fim.js`, `encerramento.rs`): isso é o G-casca, do Plano 2;
- não trocar a frase de `ServerShuttingDown`;
- não tratar sinal no `seeled` avulso, que é outro item.

**Riscos.**
- O SAIR de quem hospeda passa a esperar até ~500 ms pelo aviso.
- **Fechar o app hospedando continua sem aviso** até o Plano 2: o `Drop` de `Hospedagem` só faz `shutdown()` cru. Fica registrado no Z como a parte do P50-4 que vai com o Plano 2.
- O `session.rs` é do Plano 3 depois deste lote.

# Onda 4

## Lote F — quem sai pelo app some da lista de presentes dos outros em menos de dois segundos, e a saída para os MODs e diz isso no `seele.log`

**Onda 4, depois do E e do G-servidor.** Itens: P50-1, P50-2 e P50-3. É a pendência 50.

**Por que juntos.** É o mesmo caminho de saída:
- o P50-1 faz o `Enlace::sair` esperar a despedida;
- o P50-2 faz a casca esperar a thread da conexão, no lugar da espera cega de 150 ms;
- o P50-3 põe os MODs parando antes, na mesma despedida.

Os três mudam `despedir_se` e o guarda de texto `apps/seele-app/tests/encerramento.rs`, que reprova quando o `sleep` sai.

**Na 0.16.**
- **A prova mede a lista de presentes.** Com o FORA DO AR (D-fio-4, da F1), quem sai deixa de estar entre os presentes e passa ao grupo de membros fora do ar. Por isso o lote prova a saída de `presentes`, e não só a chegada do `PersonGone`. A prova continua valendo depois que a F1 trouxer a lista de membros.
- **O P50-4 (fechar hospedando) não é daqui.** O G-servidor deu a `Hospedagem::despedir`, e o Plano 2 a chama da casca.
- **A sonda `medida_do_disconnect.rs`, que o plano antigo citava, não existe**: nunca foi commitada, e não está em lugar nenhum. O teste novo deste lote é a medida.

**Arquivos:**
- `crates/seele-core/src/enlace.rs`;
- `crates/seele-ffi/src/lib.rs`;
- `apps/seele-app/src/main.rs`;
- `apps/seele-app/tests/encerramento.rs`;
- `crates/seele-conformance/tests/saida_com_despedida.rs` (novo);
- `crates/seele-conformance/tests/tela_por_um_par.rs`;
- `crates/seele-conformance/tests/volta_pela_trilha.rs`: só o doc de `esperar_o_servidor_fechar`.

**Estado em `d4eb6f1`:** nada feito.

**Depois da onda 1 (o que o C deixou).** `Session::revogar` passou a chamar `Session::dizer_as_contas_do_registro()`, que diz a conta pendente de cada balde do registro, fora do cadeado. O `despedir_da_sessao` deste lote chama `sessao.revogar()` primeiro, e com isso a saída do app passa a dizer as contas. Prove com um teste: um balde com conta pendente, depois `despedir_da_sessao`, e a captura tem a linha da conta. Hoje a saída do app perde essa conta.

### 1. P50-1 — o `Enlace::sair` espera a despedida
- **A constante `ESPERA_DA_DESPEDIDA` (300 ms)**, com doc: o CONNECTION_CLOSE sai na primeira volta do driver, e a espera só limita o escoamento (3×PTO).
- **Os dois braços** `Some(Comando::Sair) | None => return self.encerrar(Motivo::Pedido)` (no laço com cliente e no laço sem cliente) passam a `return self.despedir().await`.
- **`async fn despedir(&mut self)` faz, nesta ordem:**
  - guarda `self.cliente.as_ref().map(Client::endpoint)`;
  - cancela todas as `TarefasDePar` (`struct TarefasDePar`, com `largar_tudo` e afins) e espera, com teto de ~100 ms, que as alças abortadas devolvam. Um `abort` não interrompe um poll que já corre noutra thread, e as baterias rodam em `multi_thread`;
  - chama `self.encerrar(Motivo::Pedido)`. `fn encerrar` chama `Client::disconnect`, que faz `close(EJECTED)`;
  - espera `tokio::time::timeout(ESPERA_DA_DESPEDIDA, ponta.wait_idle())`.
- **`pub async fn sair(&self)`** passa a `&mut self`. Ele manda o `Sair`, espera a tarefa do motor com teto de ~1 s, e aborta se vencer.
  - A alça (`tarefa: tokio::task::JoinHandle<()>`) vira `Option<JoinHandle>`, com `take()`.
  - O `impl Drop for Enlace` aborta a que sobrar.
  - Os três chamadores já têm `mut`: `client.sair().await` no fim de `drive` (FFI), `sair_encerra_sem_esperar_a_bateria` (`bateria_interna.rs`) e `uma_saida_voluntaria_derruba_o_caminho_do_par_e_a_tela_para` (`tela_por_um_par.rs`).
- `drive` (FFI) não muda.
- **O doc de `esperar_o_servidor_fechar`** (`volta_pela_trilha.rs`) deixa de dizer «20,0 s (o `IDLE_TIMEOUT`)». A frase dos «microssegundos» veio do M2.

**Provas**, em `crates/seele-conformance/tests/saida_com_despedida.rs` (novo, com `vaga::minha()`):
- (a) `quem_sai_pela_ffi_some_dos_presentes_dos_outros_em_menos_de_dois_segundos`:
  - um Daemon em memória e um observador `Enlace` B;
  - A entra por `seele_ffi::Connection::connect` (`audio: false`) em `spawn_blocking`, como o `async fn conectar` de `volta_pela_trilha.rs`;
  - depois de `a.disconnect(); drop(a)`, a lista de presentes de B perde A em menos de 2 s. B dobra o que recebe num `seele_core::state::Room` (`Room.presentes`, que o `ServerMessage::PersonGone` encolhe) e lê `presentes`;
- (b) `o_enlace_que_sai_num_runtime_que_cai_em_seguida_ainda_avisa`: o `Enlace` numa `std::thread` com runtime `current_thread`, que termina logo depois de `sair().await`. Os presentes de B caem em menos de 2 s;
- (c) em `tela_por_um_par.rs`, depois de `sair().await`, ler a fila com `proximo()` em prazo zero até o `Encerrado` e, SEM drenar, exigir que nenhum `TelaQuadro` venha depois dele. O teste de hoje drena 300 ms (`DRENAGEM_DEPOIS_DO_FIM`, `drenar_depois_do_fim`) e não prova a ordem.

**Reversões:** voltar os braços a `self.encerrar(...)` e tirar a espera de `sair` faz (a) e (b) voltarem aos ~20 s; tirar o cancelamento de antes do aviso reprova (c).

### 2. P50-2 — a casca espera a thread da conexão
- Na FFI, a thread da conexão (`std::thread::Builder::new().name("seele-connection".into())`) avisa por um `std::sync::mpsc::Sender<()>` logo depois de `runtime.block_on(drive(...))` voltar, e antes de o runtime cair.
- O `Receiver` mora na `Connection` (`pub struct Connection { commands, shared }`) num `Mutex<Option<Receiver<()>>>`.
- **`pub fn encerrar_e_esperar(&self, teto: Duration) -> bool`** chama `disconnect()` e faz `recv_timeout(teto)`. `Ok` ou `Disconnected` dão true, e só `Timeout` dá false.
- **Prova:** `encerrar_e_esperar_volta_com_o_aviso_ja_dado` roda o próprio binário como filho, no molde de `crates/seele-audio/tests/realtime_safety.rs` (`CANARY`, `current_exe`).
  - O filho conecta pela FFI ao servidor do pai, faz `assert!(a.encerrar_e_esperar(500 ms))` e `std::process::exit(0)` na linha seguinte.
  - O pai exige que os presentes percam a pessoa em menos de 2 s.
  - Uma `encerrar_e_esperar` que volte sem esperar o sinal reprova.

### 3. P50-3 — a saída para os MODs, em `main.rs`
- **`despedir_da_sessao(sessao: &Session, teto)`,** sem `AppHandle`:
  - `sessao.revogar()` primeiro, como `desmontar_o_cliente` faz, com `tracing::info!(mods = n, "saída do app: MODs nativos mandados parar")`;
  - espera com teto de ~200 ms que `encerrando` (o mapa em `mods_nativos`) esvazie, sem segurar o cadeado de `mods_nativos` entre as conferências;
  - por fim, `encerrar_e_esperar(Duration::from_millis(500))`. Se ele voltar false: `tracing::warn!("a despedida não terminou em 500 ms: quem ficou vai notar a saída pelo prazo de ociosidade, 20 s")`.
- **`fn despedir_se(handle: &tauri::AppHandle)`** passa a chamá-la. Hoje ela faz `connection.disconnect();` e depois `std::thread::sleep(std::time::Duration::from_millis(150));`. O doc dela descreve o que acontece de verdade.
- **Em `fn assentar_fala`,** o braço `executor::ParaOFora::Parou` sobe de debug para `info!`, com «MOD nativo parou», o `mod_id` (`quem`) e a instância (`numero`). A assinatura não tem a geração: use a instância, ou passe a geração, e diga qual.
- **Testes:**
  - (1) com `Session::default()` e uma instância nativa reservada (o ajudante `instancia(geracao)` de `mod a_supervisao_dos_mods_nativos`): depois de `despedir_da_sessao`, a instância está em `encerrando`, e a captura (`capturar`) tem a linha info;
  - (2) `assentar_fala` com `ParaOFora::Parou` tem INFO e «MOD nativo parou».
- **Reversões:** tirar o `revogar()` reprova (1); voltar a `debug!` reprova (2).

### 4. `apps/seele-app/tests/encerramento.rs`, o guarda de texto
- `fechar_a_janela_avisa_o_servidor` mantém `RunEvent::ExitRequested` antes de `despedir_se(handle)`, e troca a exigência de `connection.disconnect()` por `despedir_da_sessao(` e `encerrar_e_esperar(`.
- `a_despedida_espera_o_quadro_sair` troca o `sleep` por `encerrar_e_esperar(` com um teto.
- O doc do módulo diz que a prova de comportamento mora agora em `saida_com_despedida.rs`.
- **Reversão:** voltar `despedir_se` ao `disconnect()` com `sleep` reprova os dois.

**Comandos:**
- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-conformance --test saida_com_despedida -- --test-threads=1`;
- o mesmo com `--test tela_por_um_par` e com `--test bateria_interna` (`sair_encerra_sem_esperar_a_bateria`);
- `… -p seele-ffi encerrar_e_esperar`;
- `… -p seele-app --test encerramento`;
- `… -p seele-app --bin seele-app despedir`;
- fmt e o clippy dos crates mexidos.

**NÃO fazer:**
- não mudar o fio: o CONNECTION_CLOSE é do QUIC, com o `EJECTED` de `Client::disconnect`;
- não sinalizar depois do `drop(runtime)`, que espera a piscina de bloqueio sem prazo;
- não segurar o cadeado de `mods_nativos` enquanto espera;
- não apagar os guardas de `encerramento.rs`: reescreva-os;
- não mexer na hospedagem: é o G-casca, do Plano 2.

**Riscos.**
- A saída voluntária segura a thread da conexão por até ~400 ms, e o `ExitRequested` a thread principal por até ~700 ms (200 dos MODs e 500 da conexão).
- Windows e Linux não se testam aqui.
- Que a lista de presentes cai em menos de 2 s foi deduzido da leitura do quinn: quem mede é o teste novo.
- Os MODs da janela continuam morrendo com o WebView, como hoje.

## Lote K3 — a gestão de MODs diz quem desenha de verdade, o modo de desenvolvedor diz o que cortou, e o polimento do 1C e do 1D

**Onda 4, depois do J.** Itens: P49-9 e P51-23. **O P49-10 perdeu o objeto**: a frase dos cartões da API 3 sai junto com a capacidade `cartoes`, pela F3.

**Por que juntos.** Os dois mexem em `camada-mods.js`, na frase da gestão (`oQueAGestaoDiz`) e no desenho dos contornos, e são provados na mesma bancada `contribuicoes-e-camadas.cjs`.

**Arquivos:**
- `apps/seele-app/ui/mods-contribuicoes.js`;
- `apps/seele-app/ui/camada-mods.js`;
- `apps/seele-app/ui/base.js`;
- `apps/seele-app/ui/mods-regiao.js`: docs e as anotações do teto;
- `apps/seele-app/bancada/contribuicoes-e-camadas.cjs` e `diagnostico-de-mods.cjs`;
- `apps/seele-app/tests/frontend.rs`;
- `.github/workflows/ci.yml`: um comentário;
- `docs/superpowers/plans/2026-09-29-1.0-plano-1c-mods-no-log.md`.

`ui/camada-mods.css` só entra se a nota dos contornos ganhar classe nova.

**Estado em `d4eb6f1`:** nada feito. `mods-contribuicoes.js`, `camada-mods.js` e `diagnostico-de-mods.cjs` não mudaram desde `3685a64`.

**Depois da onda 1 (o que o K2 deixou).**
- A aba DIAGNÓSTICO ganhou o bloco «a região e as páginas de cada MOD» (`desenharAMidiaDosMods`, `camada-mods.js`), desenhado por `desenharQuemPinta`.
- Nenhuma prova versionada prende esse desenho. Em `diagnostico-de-mods.cjs` (que é deste lote), um caso exige que o bloco apareça com a mídia recusada de uma região e de uma página. Reversão: tirar a chamada de `desenharQuemPinta` deixa o caso vermelho.
- O K2 mexeu em `montarFundoDeMidia` e `montarRetrato`. As anotações do teto do P51-23 continuam lá, mas noutras linhas: procure pelo texto.

### 1. P49-9 — a gestão diz quem desenha
- Com a escolha automática e alguém substituindo o alvo vazio, `resumo(preferidos, estaDePe)` (`mods-contribuicoes.js`, :504-583) passa a somar `quemPinta(ponto, '', estaDePe)` (`quemPinta`, :628).
- O campo novo `tambemDesenham` são os que vencem noutro alvo, fora o escolhido. `preteridos` (:548) junta a disputa do alvo vazio com `perderam` (:659).
- `oQueAGestaoDiz` (`camada-mods.js`, :750-822) diz «automática: mod/a para todos, mod/b para quem declarou», com a nota «Também pediram este lugar e não o receberam: mod/e. Escolha abaixo qual deve desenhar.».
- **Prova** na bancada, que já roda `oQueAGestaoDiz` sobre o registro de verdade (o recorte em `contribuicoes-e-camadas.cjs`, :1191): os casos de mod/a, mod/b e mod/e, que são o comando da pendência 49.
  - Tirar `tambemDesenham` traz de volta «apresentado por mod/a (escolha automática)», e o caso fica vermelho.
  - O caso parcial («M1 · a gestão · o caso parcial») não muda.

### 2. P51-23 — o polimento
- **Os contornos.** Em `function desenharContornos(` (:1169), com `const TETO_DE_CONTORNOS = 300` (:1068): contar os nós que passam do teto em vez do `if (caixas.length >= TETO_DE_CONTORNOS) break;` (:1194). Esse `break` só sai do laço interno, e cada laço seguinte sai na primeira volta.
  - Havendo nós de fora, pôr ao lado do aviso `"contornos-dos-pontos-aviso"` a nota «N pontos não contornados (teto de 300)».
  - A nota não pode usar `focus`, `blur`, `tabIndex` nem ouvintes de clique ou de tecla, pelo guarda `o_modo_de_desenvolvedor_nao_toma_o_teclado_nem_o_clique` (`frontend.rs`).
- **O recorte vazio.** O critério é a interseção com o recorte, e não o tamanho zero da caixa, porque o contêiner vazio é contornado de propósito (o comentário de :1195-1197, e `.contorno-de-ponto { min-width: 2px; min-height: 2px }` em `camada-mods.css`).
  - Um nó com área cuja interseção com o recorte tem largura ou altura zero não ganha caixa.
  - O `r.bottom < cima` de `if (r.bottom < cima || r.right < esquerda || r.top > baixo || r.left > direita) continue;` (:1221) passa a excluir a igualdade.
- **As recusas do pedido.** As quatro `registrarNoAnfitriao("pedido-de-mod", \`${id}: …\`, "aviso", id)` de `async function pedirAoServidor(` (`base.js`, :545: a da geração, a dos oito pedidos, a da carga e a do prazo) passam por um ajudante.
  - **No mesmo commit**, o guarda `toda_recusa_de_mod_que_a_janela_registra_leva_o_id_em_campo_proprio` (`frontend.rs`, :12144) passa a conferir o ajudante (o id na quarta posição e `"aviso"` na terceira) e que `pedirAoServidor` o chama.
  - O guarda de hoje reprova com «deixou de chamar `registrarNoAnfitriao`». Ele também cobre `atenderOMod`, que a F3 reescreve: não mexa nessa parte.
- **`ci.yml`:** «… da instalação do Chromium e das quatro bancadas de navegador» (:333-334) passa a «as bancadas de navegador», sem número.
- **Plano 1C:** `fn os_tres_comandos_de_midia_passam_pela_recusa_dita()` (:1458) e o comando `…::os_tres_comandos` (:1684) ganham o nome de hoje, `os_quatro_comandos_de_midia_passam_pela_recusa_dita`.
- **Os docs de `LIMITES_DO_CARTAO`** (`mods-regiao.js`, :104): «Mídias somadas em todos os cartões deste MOD.» e «Bytes de mídia somados em todos os cartões deste MOD.» passam a dizer que, numa contribuição, os bytes valem por destino. O K5 mexeu no mesmo bloco. Não escreva «API 3» de novo.
- **Os outros subitens** seguem o texto da pendência 51, item 23:
  - as duas definições de «classe que o script aplica» (`classes_que_os_scripts_criam` e o doc «"Criam" quer dizer criar», `frontend.rs`);
  - a frase «nenhum script cria» (`frontend.rs`);
  - a anotação do teto sem os números: `"passou do teto de ${bolso.teto} bytes de mídia deste lugar"` (`mods-regiao.js`, em `montarFundoDeMidia`, `montarRetrato` e `montarMidia`);
  - os comentários da bancada e da aba, e o Docs-m5/m7.
- **Prova dos contornos e do recorte**, em `diagnostico-de-mods.cjs` (`oModoDeDesenvolvedorContornaSemTomarNada`, :840):
  - com mais de 300 contêineres, a nota diz quantos ficaram de fora;
  - um contêiner com área recortado a zero não ganha caixa;
  - um vazio dentro do recorte continua ganhando.
  
  Essa bancada é de navegador e não está no `check-runtime`. Rode-a aqui, com a reversão de cada caso, antes do commit.

**Comandos:**
- `cargo xtask check-runtime`;
- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-app --test frontend`;
- `PLAYWRIGHT=/Users/dev-alexandre/SEELE-MOD-PERFIS/node_modules/playwright perl -e 'alarm 240; exec @ARGV' node apps/seele-app/bancada/diagnostico-de-mods.cjs`.

**NÃO fazer:**
- não apagar o contorno do contêiner vazio;
- não trocar as chamadas pelo ajudante sem reescrever o guarda no mesmo commit;
- não mexer em nada do `ci.yml` além do comentário;
- não mexer nas frases da API 3 da gestão (`modsDoCaminhoAntigo`, o ramo `linha.escolhidoNaoSubstitui` com `cartoesDosMods`, as frases «(API 3)»), nem no caso da bancada que as fixa: a F3 as tira;
- não instalar Playwright pela rede.

**Riscos.**
- A nota dos contornos é frase de tela, só no modo de desenvolvedor.
- A prova dos contornos só roda no job manual `bancadas` e aqui, à mão.
- Nenhum par de MODs publicados provoca o caso do P49-9 hoje.

# Onda 5

## Lote I — o áudio só abre na sala de voz e não prende a conexão, um pânico deixa linha, o microfone mudo pelo macOS é dito, e o volume e os rascunhos não vazam entre servidores

**Onda 5, depois do F e do J.** Itens: D-52 (a pendência 52), F1-audio-panico, F1-microfone-macos e F1-vazamento.

**Por que juntos.** Os quatro mexem no `Shared` da voz no `seele-ffi` (a abertura, o prazo, os controles guardados, os volumes), no Snapshot de `types.rs`, no `main.rs` e na tela da sessão com as frases. Juntos, o Snapshot muda uma vez.

**Na 0.16: a pendência 52 muda o desenho do áudio.** O áudio abre ao entrar numa sala de voz e fecha ao sair dela. Quem fica no servidor só pela conversa de texto devolve o repouso à máquina.

**Hoje:**
- **Abre na conexão.** Em `drive` (FFI), `if config.audio {` → `Voice::start_preferring(&chosen_devices(&config), client.media(), client.sessao().ssrc)`. É uma abertura **síncrona** na thread da conexão (`open_preferring(chosen)?`, `voice.rs`), antes de `shared.running.store(true)` e de `let _ = ready.send(Ok(trust));`.
- **Fecha só no fim de `drive`**, com `*voice = None`. O `impl Drop for Voice` grava `stop`.
- **A medida da 52:** `PreventUserIdleSystemSleep` no macOS, e o pedido de driver no Windows, a sessão inteira.

**Arquivos:**
- `crates/seele-ffi/src/lib.rs`;
- `crates/seele-ffi/src/types.rs`;
- `crates/seele-core/src/voice.rs`;
- `apps/seele-app/src/main.rs`;
- `apps/seele-app/ui/frases.js`, `tela-sessao.js`, `tela-chamada.js` e `tela-fim.js`;
- `apps/seele-app/tests/frontend.rs`.

Talvez também:
- `apps/seele-app/ui/tela-server.js`, se `audio_available` mudar de sentido;
- `crates/seele-core/src/state.rs`;
- `crates/seele-audio/src/telemetry.rs`.

`crates/seele-conformance/tests/voz_na_reconexao.rs` é um guarda que tem de continuar verde.

**Estado em `d4eb6f1`:** nada feito.

**O lote é grande.** A divisão natural, se não fechar numa sessão: o passo 1 (a abertura na sala, com o prazo e o pânico) numa sessão, e os passos 2 e 3 numa segunda, antes do K4.

**Depois da onda 4 (09/10).**
- **A divisão foi adotada.** O **I-a** é o passo 1. O **I-b** são os passos 2 e 3, sobre o I-a já integrado. Os dois mexem no mesmo `Shared`, no `lib.rs` da FFI, no `voice.rs` e no `tela-sessao.js`, e por isso não rodam em paralelo. O Snapshot muda só no I-a.
- **As âncoras que mudaram de lugar:**
  - `MovedToVoiceRoom`, `VoiceRoomDeleted` e os `Alert` de recusa são aplicados no `seele-core`: em `Room::apply`, no `state.rs`, e no motor do `enlace.rs`, para o `MovedToVoiceRoom`. A FFI não os casa. Ela vê o efeito em `room.current_voice_room`.
  - `consentimento_do_microfone` mora na FFI, no `seele-core`, no `seele-audio` e em `tela-boot.js`, e não no `main.rs`.
  - «SEM ÁUDIO» só aparece escrito em `tela-sessao.js`. `tela-server.js` (`if (!snapshot || !snapshot.audio_available)`) e `tela-chamada.js` (`pintarCartao(…, snapshot.audio_available)`) leem o campo sem a frase.
- **O que a onda 4 deixou e o I mantém:**
  - na FFI, o aviso da thread da conexão antes do `drop(runtime)`, e `encerrar_e_esperar`, do F;
  - no `main.rs`, `despedir_da_sessao`, do F;
  - no `enlace.rs`, a `Esperada` da F7;
  - em `tela-sessao.js`, a faixa de fone do J, que se fecha na primeira entrada numa sala de voz.
- **O teste (e) do pânico** usa `with_default` numa thread. Com a pendência 56, ele chama o `segundo_despachante()` do app.

**Da revisão do I-a (09/10).**
- O servidor manda `VoiceRoomEntryRefused` também quando mantém a pessoa sentada:
  - numa troca de sala recusada;
  - a cada volta numa sala com senha;
  - pelo `nao_deu`, como no apelido tomado.
- Com a D-52, isso fechava o áudio de quem continuava na sala. A regra do cliente, em `seele-core` (no `Room` e no motor), é esta: uma recusa desfaz só o pedido de entrada **pendente** e devolve o assento de antes. Sem pedido pendente, ela não move ninguém. A orquestradora confirmou que a regra fica.
- O `nao_deu` passa a `AlertReason::PedidoRecusado { pedido, causa }`, que é fio e é da F1. O `pedido` é o ordinal do pedido recusado em `ClientMessage`.
  - O I-a não programa contra essa variante, porque ela ainda não está na árvore. Ele só deixa a regra num lugar só.
  - Quando o `PedidoRecusado` entrar na `versao/0.16`, o primeiro lote da F5 que vier depois o consome. Com isso, uma recusa que nomeia outro pedido não desfaz a entrada.
- **Para a F1, fora da F5:** recusada por sala cheia, a pessoa passava a ouvir a sala que a recusou. A conformidade que o prova foi entregue à orquestradora: três testes, que hoje dão 1 verde e 2 vermelhos.
- **No I-b:** a supressão e a sensibilidade salvas em disco passam a valer no connect, e o `ganhos_a_aplicar` cobre os volumes ao reabrir a voz.

### 1. D-52 e F1-audio-panico — o áudio abre na sala, numa thread própria, com prazo
- **O gatilho.**
  - Na FFI, `Connection::enter_voice_room` e `leave_voice_room` mandam `Command::EnterVoiceRoom` e `LeaveVoiceRoom`, e `run_command` os trata (`client.entrar_na_voice_room` + `room.enter_voice_room`, e `room.leave_voice_room`).
  - Além da entrada pedida, a pessoa entra numa sala por `ServerMessage::MovedToVoiceRoom` (um operador a move). Ela sai por `leave_voice_room`, por um `Alert` `VoiceRoomEntryRefused | VoiceRoomFull` e por `VoiceRoomDeleted`.
  - O áudio abre quando a pessoa passa a estar numa sala, e fecha quando deixa de estar, pelos cinco caminhos.
- **A reconexão. Meça antes de escolher o gatilho.** No braço `Aviso::Reconectado` da FFI, `room.adopt` zera `current_voice_room` (`Room::reabrir`/`adopt`, `state.rs`), enquanto o motor do enlace volta a sentar a pessoa sozinho (`restabelecer(…, self.voice_room, …)`). Nada na FFI repõe `current_voice_room`.
  - Se o gatilho for o espelho da FFI, a reconexão fecharia o áudio de quem estava numa sala.
  - Confira o que a tela mostra hoje depois de uma volta, com um teste, e escolha um gatilho que siga o assento que o motor restabelece.
  - A reabertura da reconexão (`reabrir_voz_na_reconexao`, com os testes dela) continua: quem estava na sala volta com a voz, e quem estava fora continua sem aparelho.
- **A thread própria e o prazo.**
  - Abrir o áudio numa thread própria com nome (`std::thread::Builder::new().name("seele-abre-audio")`), devolvendo por `tokio::sync::oneshot`, com `tokio::time::timeout(PRAZO_DO_AUDIO, rx)`.
  - A entrada na sala acontece dentro do laço de `drive`, que roda num runtime `current_thread`. Abrir ali de forma síncrona travaria a conexão inteira.
  - `PRAZO_DO_AUDIO` = 10 s, com um doc dizendo que ele cobre o driver são. A primeira abertura com o pedido do TCC e a do WASAPI são medidas do dono.
  - **Por que thread própria, e não `spawn_blocking`:** uma tarefa bloqueante presa no driver seguraria o `drop` do runtime.
  - Ao vencer o prazo: `tracing::warn!("o áudio não abriu em {PRAZO}s; a sala segue sem voz")`. A voz que chegar tarde é descartada na própria thread, e o `drop` fecha o aparelho.
  - A reabertura da reconexão usa a mesma espera, sem segurar o `Mutex` de `shared.voice` enquanto o aparelho abre.
- **Os controles passam a morar em `Shared`.** Com a 52, `shared.voice` é `None` todo o tempo fora de sala, e hoje vários comandos pulam ou recusam sem voz viva:
  - `set_muted`, `set_total_isolation`, `set_voice_mode`, `set_supressao_de_ruido` e `set_abertura_da_voz_dbfs` pulam;
  - `controles_da_voz` devolve `None`;
  - `set_volume` e `switch_device` (por `set_capture_device`/`set_playback_device`) devolvem `NoAudioDevice`.
  
  O que a pessoa escolhe fora de sala (mudo, modo, isolamento, supressão, abertura, aparelhos) é guardado em `Shared` e aplicado ao abrir.
  - `carregar_controles` (`voice.rs`) já carrega mudo, isolamento, modo, tecla e ganhos por SSRC. A supressão e a abertura não estão nela.
  - A casca aplica `set_voice_mode` logo depois do connect (`main.rs`), e isso tem de continuar valendo.
  - A escolha de aparelho (`chosen_devices(&config)`) mora só em `drive`, e vai para `Shared`.
- **O Snapshot.**
  - Ganha um estado da abertura, com um nome que não colida com `seele_core::EstadoDoAudio` (que já existe, em `voice.rs`) nem com `EstadoDoAparelho` (FFI). Por exemplo, `AberturaDoAudio`, com `ForaDeSala`, `Abrindo`, `Aberto`, `SemAparelho` e `NaoAbriuNoPrazo`.
  - `audio_available` continua existindo, mas fora de sala a tela **não** pode dizer «SEM ÁUDIO». Hoje ela diz isso sempre que ele é falso: `tela-sessao.js`, e também `tela-server.js` e `tela-chamada.js`.
  - Os campos novos são estado, sem dado da máquina. O que a API 6 projeta do Snapshot para os MODs é da F3.
- **A frase de quando não abriu:** «O ÁUDIO NÃO ABRIU A TEMPO. A conversa por escrito já funciona; para tentar a voz de novo, saia da sala e entre de novo.».
  - Não mande escolher o microfone: hoje `switch_device` exige uma voz viva.
  - Com a mudança deste lote, escolher o aparelho fora de sala só grava a escolha.
- **O pânico.** `instalar_gancho_de_panico()`, chamado uma vez (`Once`), logo depois do `tracing_subscriber…init()` em `fn main()`.
  - Ele guarda o gancho anterior, escreve `tracing::error!(target: "seele_app", thread = nome, local = %info.location(), "pânico: {mensagem}")` e chama o anterior.
  - Hoje não há nenhum `set_hook` em `apps/` nem em `crates/*/src`.

**Provas:**
- (a) **Fora de sala, nenhum aparelho aberto.** Pela FFI com `audio: true`, entrar no servidor não abre a voz (`shared.voice` continua `None`, e o estado é `ForaDeSala`). Entrar numa sala abre; sair fecha. Hoje a voz abre no connect.
  - Se o teste não puder depender de um aparelho de verdade, o abridor é injetável, como em `reabrir_voz_na_reconexao`, e a prova é da decisão.
- (b) **Os controles fora de sala valem ao entrar.** Mudo pedido fora de sala, depois a entrada: a voz abre muda.
- (c) **A reconexão** de quem está numa sala reabre a voz, e a de quem está fora não abre.
- (d) `esperar_o_audio(abrir: impl FnOnce() -> Result<V> + Send + 'static, prazo) -> Result<V, Atraso>`, genérica: um `abrir` que dorme 2 s, com prazo de 200 ms, volta em menos de 1 s com `Atraso`. A versão síncrona leva 2 s e reprova pela asserção de tempo, sem pendurar a bateria.
- (e) `um_panico_numa_thread_deixa_linha_no_log`: instala o gancho e cria uma `std::thread` com nome. Dentro dela, `tracing::subscriber::with_default(captura, || panic!("x"))`, porque o subscriber é por thread. Depois do `join`, exige «pânico:», a mensagem e o nome da thread.
- (f) Em `frontend.rs`: fora de sala, a tela não diz «SEM ÁUDIO»; a frase de quando não abriu existe e é desenhada a partir do estado.
- `voz_na_reconexao.rs` (`nenhuma_casca_reabre_a_voz_jogando_fora_os_controles`) continua verde.

**Reversões:**
- abrir no connect de novo reprova (a);
- tirar a aplicação dos controles guardados reprova (b);
- voltar ao gatilho do espelho da FFI reprova (c), se a medida confirmar o achado;
- a abertura síncrona reprova (d).

### 2. F1-microfone-macos, só a parte (1)
- No laço de captura (`async fn pipeline`, `voice.rs`), uma função pura conta quadros de silêncio **digital** (amostras exatamente 0.0) com o microfone aberto e não mudo.
  - Ela lê o quadro cru (`bruto`), antes da supressão.
  - Passados 3 s de zero exato, a telemetria marca `microfone_entrega_silencio_digital: bool`, por `AudioTelemetry`/`LocalTelemetry` e pelo `Telemetry` da FFI.
- **Só no macOS,** a tela diz «O MACOS PODE ESTAR BLOQUEANDO O MICROFONE.\nAjustes → Privacidade e Segurança → Microfone → ligue o SEELE, e entre de novo.». No Windows, o consentimento já é lido (`consentimento_do_microfone`).
- **Provas:**
  - (a) o detector com quadros sintéticos: 3 s de zeros exatos marcam; ruído a −90 dBFS, ou uma única amostra diferente de 0, não marcam. Reverter o detector deixa o teste vermelho;
  - (b) em `frontend.rs`: a frase existe em `frases.js`, e a tela a desenha a partir do campo.
- A medida com o TCC negado e a via AVFoundation (a parte 2) ficam com o dono.

### 3. F1-vazamento — o volume e os rascunhos não vazam entre servidores
- **(1) Em `tela-sessao.js`, `esquecerOEstadoDaSessao()`.**
  - Ela limpa os mapas de topo da sessão: `rascunhos`, `previas`, `previasFechadas` e `previasDeLink`, e `volumes` enquanto ele existir. Chama também `esquecerRetratos()`.
  - Ela é chamada em `async function ejetar()` e em `async function limparSessaoEncerrada` (`tela-fim.js`). Hoje `ejetar` só chama `esquecerRetratos()`, e `limparSessaoEncerrada` não chama nada disso.
  - O RECONECTAR da tela de fim (`async function reconectar()`) passa por `limparSessaoEncerrada`. Um rascunho não sobrevive ao fim de uma sessão, que é o «por sessão» que o doc de `rascunhos` («Só em memória, por sessão.») já diz: atualize o doc.
  - A reconexão automática da bateria não passa por aqui, e não perde nada.
- **(2) O volume vai para o Rust.**
  - `Shared` guarda `volumes: Mutex<HashMap<String, u16>>`, por apelido. `set_volume` (FFI) grava ali e aplica. Hoje ele resolve `room.ssrc_of(&nickname)` e chama `voice.set_gain`, sem guardar nada.
  - Com a 52, o volume pedido fora de sala também só grava, e vale ao abrir.
  - A função pura `ganhos_a_aplicar(volumes, room) -> Vec<(ssrc, ganho)>`, no molde de `reabrir_voz_na_reconexao`, é chamada sempre que o SSRC de um apelido muda (a reconexão inclusive) e ao abrir a voz.
  - O valor chega à janela por um comando próprio, `volumes_locais()`, registrado no `generate_handler!`, e **nunca** pelo Snapshot, que os MODs leem.
  - O mapa `volumes` da janela deixa de existir, também em `tela-chamada.js`. O comentário de `tela-chamada.js` diz que `volumes` é o mesmo mapa nas duas telas, e muda junto.
- **Provas:**
  - (a) `todo_mapa_de_topo_da_sessao_e_esquecido_ao_sair`, em `frontend.rs`:
    - ele lista todo `const|let X = new Map()/new Set()` de topo em `tela-sessao.js` e exige o `X.clear()`, ou a chamada nomeada (`esquecerRetratos()` para `retratos`, `encerrarBusca`/`limparBusca` para `casamentosPorMensagem`), dentro de `esquecerOEstadoDaSessao`;
    - exige também que ela seja chamada nos dois lugares;
    - uma exceção só entra com o motivo escrito no teste, como a marca do fone do lote J;
    - tirar qualquer `.clear()` reprova;
  - (b) `o_volume_volta_quando_o_ssrc_muda`: 150 para «rafa», depois um SSRC novo para «rafa», e a função devolve (novo, 1.5). Um segundo teste confere que o laço a chama na troca e na abertura.

**Comandos:**
- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-ffi audio`;
- `… -p seele-ffi volume`;
- `… -p seele-core voice`;
- `… -p seele-app --bin seele-app panico`;
- `… -p seele-app --test frontend`;
- `… -p seele-conformance --test voz_na_reconexao`;
- fmt e o clippy dos crates mexidos.

**NÃO fazer:**
- não pôr o volume, nem nada que identifique a máquina, nos campos novos do Snapshot;
- não usar `spawn_blocking`;
- não abrir o aparelho fora de sala;
- não abrir exceção de `unsafe` nem usar `objc2-avf-audio`;
- não rodar `tccutil`, que muda um ajuste de privacidade do sistema;
- não mudar o fio: entrar e sair da sala continuam as mensagens de hoje.

**Riscos.**
- **A entrada na sala passa a pagar a latência de abrir o aparelho,** e o primeiro quadro de quem fala logo ao entrar pode se perder. É o custo que a 52 aceitou.
- Abrir e fechar o aparelho a cada entrada pode pedir a permissão do microfone de novo no macOS. A medida é do dono.
- A thread presa no driver continua viva até o driver soltar, mas não segura nada.
- O detector pode dar falso positivo com interfaces USB que entregam zero exato com o ganho no mínimo, e por isso a frase diz «pode estar».
- Um rascunho escrito antes do fim de uma sessão some no RECONECTAR.
- O WASAPI e o WebView2 não se testam aqui.

# Onda 6

## Lote K4 — o som do pacote atravessa a ponte uma vez só, e um som grande demais é recusado antes de decodificar

**Onda 6, depois do I e do K3.** Itens: P49-7b e P49-7a.

**Por que juntos.** Os dois mexem nas mesmas funções de som do `main.rs` (`midia_declarada_do_mod`, `som_declarado_do_mod`, `ler_midia_do_servidor`) e em `montarSom`, no `mods-regiao.js`. O 7b é mais simples, e limpa o caminho que o 7a passa a conferir.

**Na 0.16.**
- **O K5 veio antes.** `montarSom` já adia a decodificação das contribuições até o `tocando`. A recusa do 7a numa contribuição chega no pedido de tocar, e o mapeamento para «cheia»/`recusada` tem de valer nos dois momentos: na montagem, para o que não é contribuição, e no tocar, para a contribuição.
- **O C mudou a assinatura** de `recusa_de_midia_dita`, com os baldes e o instante.
- A promessa pública que já existe (`api/README.md`: «recusada, com os números no `porque`») fica como está. O que for novo vai para o rascunho da v6.

**Arquivos:**
- `apps/seele-app/src/main.rs`;
- `apps/seele-app/ui/base.js`: só o comentário;
- `apps/seele-app/ui/mods-regiao.js`;
- `apps/seele-app/bancada/regiao-do-mod.cjs`.

**Estado em `d4eb6f1`:** nada feito.

### 1. P49-7b — o som do pacote atravessa uma vez
- `fn midia_declarada_do_mod(` usa `seele_ffi::mods::ler_midia` para todo papel. Ela passa a conferir o tipo por `ler_tipo` (`crates/seele-core/src/mods.rs`, reexportado pelo `seele-ffi`; `som_declarado_do_mod` já o usa) antes de montar o `data:`.
- **Com o papel `som`,** devolve o papel e o tamanho, sem `uri`, depois das mesmas conferências de pacote, de arquivo declarado e de teto. Só a imagem passa por `ler_midia`.
- **`struct MidiaDoMod` é compartilhado** com `midia_em_bytes` e `ler_midia_do_servidor`, e **o som do servidor precisa do `uri`**: `montarMidia` passa `{ uri: midia.uri }`, e `montarSom` usa `bufferDaUri`. Por isso só o som do pacote (`{ caminho }`) dispensa o `uri`.
  - Ou o campo vira `Option<String>`, omitido na serialização só nesse caso, ou o som do pacote ganha um tipo de resposta próprio. Diga qual.
  - Com `Option`, ajuste os dois testes que leem `midia.uri` como `String`: `imagem_do_perfis_em_data_uri_chega_ao_decodificador` e `midia_do_servidor_confere_bytes_e_limite_mesmo_com_cabecalho`.
- Saem os dois comentários «atravessa duas vezes»: o doc de `bytesDoSom:` (`base.js`) e o de `mods-regiao.js`.
- O doc de `midia_do_mod` deixa de citar `recursosDoMod` («ver `recursosDoMod` em `base.js`»), que não existe.
- **Teste:** com o pacote de teste (`mod pacote_de_teste`; `som/toque.wav` em `um_som_declarado_sai_com_os_bytes_exatos`), a resposta para um WAV não tem `uri`, e a de uma imagem tem. Voltar o `uri` do som deixa o teste vermelho. A `regiao-do-mod.cjs` continua verde.

### 2. P49-7a — o som grande demais é recusado antes de decodificar
- **Junto de `som_declarado_do_mod`, a função pura `bytes_decodificados_minimos(bytes) -> Option<u64>`.** Ela devolve um **piso** do que o som ocupa decodificado a 48 kHz em float32 (quadros × 48000/taxa × canais × 4), e nunca uma estimativa que possa passar do real. O parse do cabeçalho é feito à mão, sem crate:
  - WAV: exato, pelo `fmt ` e pelo `data`;
  - OGG: pela granule da última página e pela taxa e canais do cabeçalho de identificação Vorbis. No Opus a granule já está a 48 kHz, menos o pre-skip;
  - MP3: exato pelo quadro Xing/Info quando houver. Senão, o piso pela taxa máxima da versão e da camada do primeiro quadro (320 kbps no MPEG-1 camada III) e pelos canais dele.
  
  O WAV mínimo do pacote de teste (`b"RIFF\0\0\0\0WAVEfmt "`, sem `fmt` nem `data` completos) dá `None`. Senão, `um_som_declarado_sai_com_os_bytes_exatos` quebra.
- **Acima de 64 MiB, a recusa sai antes de servir**, com o motivo `som-grande-demais-decodificado` e os números, pelo mesmo `recusa_de_midia_dita`. Isso vale:
  - em `som_declarado_do_mod`;
  - para o papel `som`, em `ler_midia_do_servidor`, porque o som do servidor tem o mesmo pico (`midia_em_bytes` → `bufferDaUri`).
- **A janela diz essa recusa como o teto, e não como falha de carga.** Em `montarSom` (`bufferDoSom(await this.dono.bytesDoSom(origem.caminho))`, no `try`), esse motivo segue o caminho do teto: figura «cheia», `recusada` ao MOD com os números no `porque`, anotação e registro. É o que o comentário «A recusa é a do teto de bytes de `montarMidia`…» diz. Vale nos dois momentos que o K5 deixou.
- **Um guarda** lê `mods-regiao.js` e falha se `TAXA_DA_DECODIFICACAO` e `bytesDeSomDecodificado` (região e cartão) divergirem das constantes repetidas no Rust.
- O doc de `LIMITES_DA_REGIAO` («**O teto vale para o que fica de pé.**») passa a dizer que o pico é cortado antes, no Rust, quando o cabeçalho permite.
- **Testes em `main.rs`,** um por braço, com bytes sintéticos montados no teste:
  - um WAV de 8 bits, mono, 8 kHz, do tamanho do teto de arquivo, é recusado (piso ≈ 250 MB), e um curto passa;
  - um OGG Vorbis com granule de três minutos em estéreo é recusado;
  - um MP3 com quadro Xing de muitos quadros é recusado, e um MP3 mono sem Xing, do tamanho do teto, passa.
  
  **Reversão por braço:** devolver `None` naquele formato deixa passar o caso grande dele.
- **Na `regiao-do-mod.cjs`:** `bytesDoSom` rejeitando com `{ Recusado: { motivo: 'som-grande-demais-decodificado …' } }` faz o MOD ouvir `recusada`, e não `falhou`, e a figura fica «cheia». Isso vale para um som de região e para uma contribuição depois do `tocando`. O molde da recusa na bancada é `d.api.bytesDoSom = () => Promise.reject({ Recusado: { motivo: "formato-desconhecido" } })`. Sem o mapeamento, o MOD ouve `falhou`, e o caso fica vermelho.

**Comandos:**
- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-app --bin seele-app som`;
- `cargo xtask check-runtime`;
- fmt e o clippy do `seele-app`.

**NÃO fazer:**
- não acrescentar crate de áudio;
- não recusar por estimativa que possa passar do real: só o piso recusa;
- não mudar a promessa pública do `api/README.md`;
- não mudar o nome do comando `midia_do_mod`, que `api/v3..v5.json` mapeiam como `"midia-do-pacote"` e o `check-api` confere;
- não tirar o `uri` do som do servidor.

**Riscos.**
- Nos formatos comprimidos só há estimativa. Por isso só o piso recusa, e um som que o piso não alcança segue como hoje, decodificado e conferido na janela.
- A ponte interna da casca muda (o `uri` some do som do pacote).
- O WebView2 e o WKWebView não se testam aqui.

# Fecho

## Lote Z — o fecho das pendências 49, 50 e 51, o registro do que a frente fechou, e o conserto da pendência 56

**Depois da onda 6.** Documentos, mais o conserto da pendência 56 (o item 7), que é código só de teste. Os commits de documento têm assunto «docs: …»; os do item 7, não.

**Na 0.16.** O Z antigo encolheu:
- a regra «Quem ganha cadeia» e o placeholder do G1 foram feitos em `f54f4a7`;
- o rascunho das notas perdeu o objeto: as notas foram direto para `empacotar/notas/0.15.1.md`;
- a parte do índice «Fora desta especificação» foi reescrita em `27f8cb0`;
- a pendência 52 do plano antigo («O que a onda de 04/10 deixou com quem opera») nunca foi escrita, e o número 52 é hoje a voz que segura o aparelho. O que fica com o dono está no §6 do spec da 0.16, e não vira pendência.

**Arquivos:**
- `docs/pendencias.md`;
- `docs/superpowers/plans/2026-09-29-1.0-indice.md`;
- os do item 7, listados lá.

**A regra:** nunca escrever «fechado» sem o hash do commit que fecha. As linhas históricas das pendências não se reescrevem: acrescenta-se.

### 1. Pendência 49
- O título (hoje «Consertada em 2026-10-01, sem confirmação de campo · …») passa a dizer que a medida foi feita em 07 e 08/10, e toca nas duas plataformas (itens 1 a 3, `docs/evidencias/som-de-mod-wkwebview/registro.md`).
- Os itens 5, 7a, 7b, 8 e 9 dizem «fechado em <data>, `<hash>` (lote …)»: o K2, o K4, o K5 e o K3.
- O 7c diz o que fechou (o som adiado, no K5) e o que fica com o dono (o teto por MOD).
- O 10 diz que perdeu o objeto com a API 6: a frase dos cartões da API 3 sai junto com a capacidade `cartoes`.
- O 6 diz «cumprido em `563bc37`: o contrato de `tocando` está em `api/README.md`, e o comportamento é guardado pela `regiao-do-mod.cjs` (`oSomQueTerminouVoltaComOTocandoDeclarado`); `repetir` fica como opção, e o contrato entra no texto da v6».

### 2. Pendência 50
- O título passa a «Consertada em <data>».
- O P50-1 e o P50-2, com os hashes do F e o tempo que `saida_com_despedida.rs` mediu.
- O P50-3, com o hash do F.
- O P50-4: a metade servidor, com os hashes do G-servidor; a casca vai com o Plano 2.
- Que a sonda `medida_do_disconnect.rs` nunca foi commitada, e que o teste novo é a medida.

### 3. Pendência 51, item por item, com o commit e o lote
- **Já registrados:** 4 (`b655edf`), 6 (`ef707c7`), 13 (`4599e14`), 27 (`cd71f8e`, `380a745`, `34235fe`), e a metade servidor do 26 (`fbf2d0d`, `deed8a0`).
- **Fechados e ainda não registrados:**
  - 15, nas duas partes de antes: o «panicked» (`4887565`) e a etapa do `publicar.sh` (`8b766e5`), mais o lote das ferramentas;
  - 22: `a77da2f`.
- **Fechados por esta frente:**
  - 1 e 14 (D);
  - 2 e 3 (H);
  - 5, 8, 10, 12 e a metade do app do 26 (C);
  - 7 (K2);
  - 9 e 11 (E);
  - 16, 18 e 20 (M2);
  - 17 e 19 (M1);
  - 21 (ferramentas);
  - 23 (K3).
- **Os que fecharam por dentro antes:** o T2 m1 do 14 (`517afaa`); o T5f do 19 (`27f86af`), sobrando o `Espiao` do `seele-ffi`; a 8384 do 16, como escolha documentada.
- **O 28:** absorvido pela API 6 (F3).
- **24 e 25:** ficam, porque são de outros repositórios.

### 4. As pendências 52, 53, 54, 55 e 56
- **52:** decidida em 08/10 (D-52), e fechada com os hashes do I.
- **53:** fechada com os hashes do P53.
- **54:** fechada com o hash do P54.
- **55:** vai com o Plano 5. Corrigir o nome: a função é `Batida::preparar`, e não `Encontro::preparar`.
- **56:** fechada com os hashes do item 7.

### 5. O índice da 1.0
- **:22, a linha do Plano 6:** troca «o m-4 da pendência 49 (`repetir`, ou o contrato de `tocando` escrito na API)» por «o contrato de `tocando` (`api/README.md`, `563bc37`) entra no texto da v6; `repetir` é opção, não condição».
- **:118:** a frase «Elas terminam num bloco «[PREENCHER ANTES DE PUBLICAR …]»», que `54f542f` já trocou nas notas, passa a dizer o que as notas da 0.15.1 trazem.
- **A seção «Fora desta especificação»:** G, J e I deixam de estar abertos, e o G-casca aponta o Plano 2.

### 6. O que a onda 1 deixou para o Z
- **Os hashes da onda 1**, por item, estão em `F5/progress.md` e nos relatórios `F5/lote-<id>-report.md`. Entre eles, a pendência 54 fecha em `0d68c95`.
- **Quatro portas do registro da casca continuam sem balde**, porque ficam fora do P51-08: `pedido_a_caminho_dito` e `pedido_recusado_na_saida_dito` (de `mod_request`), `mod_nativo_colher` e `mod_nativo_entregar`. O Z decide se elas viram pendência ou ficam com um lote.
- **A frase de `ServidorTrocouNaVolta`** mudou na revisão do H. A frase do plano era falsa para quem volta pela lista, e a nova não fala em link nem em convite. Registre isso na pendência 51, item 3.
- **Os menores adiados das revisões**, para a revisão final decidir:
  - o `changelog_dos_commits` do `publicar.sh` ainda cala falha do git;
  - a conferência dos alvos do fuzz lê o `ci.yml` inteiro, e não olha `continue-on-error`;
  - as linhas que a conta de id ilegível conta não levam o rótulo dela;
  - o guarda de `drive → fim_da_sessao` é de texto;
  - o tropeço de aplicar uma imagem divide o `onde` com as recusas.

### 7. Pendência 56: os testes que leem o log perdem linhas quando rodam em paralelo
Este item entrou por pedido da orquestradora, em 08/10. A pendência foi escrita em `26cda877`, na `versao/0.16`. O mecanismo medido e a lista dos testes expostos estão em `docs/pendencias.md`, na 56. Leia-a antes.

**O mecanismo**, em uma linha: com um despachante só registrado, o interesse de uma linha de log é decidido pela thread que a toca primeiro. Um teste sem captura pode marcar como «nunca» uma linha que outro teste está capturando.

**O conserto que já existe** está no app: `rastro_de_teste::segundo_despachante()`, em `apps/seele-app/src/main.rs` (`1405210b`). É um `OnceLock<tracing::Dispatch>` com `NoSubscriber`, chamado por `capturar`. A prova de ordem forçada dele é `uma_linha_que_outra_thread_tocou_primeiro_ainda_e_capturada`.

**Um auxiliar por crate**, e não um crate novo:
- **`seele-core`:** `Rastro::a_partir_de` (`crates/seele-core/src/rastro_de_teste.rs`) passa a chamar um `segundo_despachante()`. Isso conserta num lugar só os cerca de 24 testes do `Rastro` (`enlace.rs`, `encontro.rs`, `anexo_no_disco.rs`, `par.rs`, `tofu.rs`, `client.rs`).
- **`seele-server`:** um `#[cfg(test)] pub(crate) mod rastro_de_teste` com o mesmo auxiliar, usado por:
  - `admissao.rs`;
  - `mods/pedidos.rs`;
  - `alcance/encontro.rs`, cujo `de_info` devolve um `DefaultGuard`;
  - `recepcao.rs`, se o arquivo já estiver na `versao/0.16` quando o Z começar. Ele é da F7 e, em 08/10, só existia no `0.16/sala`. Hoje o conserto dele é um truque local (`3fc72adb`), que passa a usar o auxiliar.
- **`seele-ffi`:** `o_log_da_trilha_diz_qual_candidato_deu_o_que` (`lib.rs`) chama uma cópia local.

**A prova:** cada cópia leva a prova de ordem forçada do app, adaptada ao crate. Tirar o `segundo_despachante()` tem de reprová-la.
- Rode a prova algumas vezes **sem** o conserto, para ver que ela não depende de sorte.
- Confira com `grep -rn "subscriber::with_default\|subscriber::set_default" crates/` que nenhum teste que lê o log ficou sem o auxiliar. A lista da pendência não foi medida teste a teste.

**Comandos:**
- `perl -e 'alarm 600; exec @ARGV' cargo test -p seele-core --lib`;
- o mesmo com `-p seele-server --lib` e com `-p seele-ffi --lib`;
- fmt e o clippy dos três crates.

**NÃO fazer:**
- não trocar a forma de captura dos testes, só garantir o segundo despachante;
- não mexer no auxiliar do app, que é a referência.

**NÃO fazer, no lote todo:**
- não acrescentar cadeia ao G1;
- não escrever «fechado» sem hash;
- não mexer no `release.yml`.

# Fora dos lotes

## Para a F3, o rascunho da v6

O que esta frente absorve na API 6, ou deixa para ela:
- **K1-m3.** O `catch` de `async function atenderOMod(` (`base.js`) já calcula o `motivo` e o registra, mas responde ao MOD `responder(false, { erro: … fraseDeErro(falha) … })`, com a frase de tela. Na v6, o `erro` que o MOD recebe é o motivo, e não a frase de tela. Hoje é um conserto de uma linha, mas mudaria o que MODs publicados das APIs 3 a 5 recebem. Por isso ele vai com o roteador que a F3 reescreve.
- **P51-28.** O guarda do vetor de referência (`crates/seele-conformance/tests/mod_de_referencia.rs`, a leitura de `oferecidos` por `split_once(": (")`, sem `async` e sem o membro condicional) se reescreve contra o prelúdio da v6.
- **P49-10.** A frase dos cartões da API 3 na gestão sai com a capacidade `cartoes`.
- **O retrato que não fala com o MOD.** Se o retrato deve mandar `midia`/`falhou` quando a imagem não decodifica (K2), é evento novo da v6.
- **A recusa do som de contribuição no pedido de tocar** (K5), e o «recusada, com os números» do som grande demais (K4): o texto público entra na v6.
- **O contrato de `tocando`** (P49-6, `563bc37`) entra no texto da v6.
- **O `volume` fica na API 6** (ADR 0048, confirmado pela F3 em 08/10), e o P51-11 do E tem objeto.
- **O Snapshot que os MODs leem** já leva os nomes dos aparelhos (`capture`, `playback`). A regra «nada da máquina vai aos MODs» é das leituras da v6, por lista branca.

## Vão para outros planos
- **G-casca → Plano 2:** a pergunta ao fechar a janela hospedando, o `fechar_de_vez`, o `ExitRequested` segurado (só com `code.is_none()`: no Tauri 2.11, `prevent_exit` é ignorado no `RESTART_EXIT_CODE`, e o atualizador chama `app.restart()`), e a `Hospedagem::despedir` chamada na saída do app. É reescrito contra `Session.hospedagens` e a `ChamadaEncerrada`.
- **Pendência 55 → Plano 5:** o `LEVE` que resolve de novo o ponto que acabou de não resolver. Se o conserto for entregar ao `LEVE` o endereço já resolvido, ele parte da lista ordenada que o M1 deixa em `resolver_ponto`.

## O que fica com o dono
- **As medidas em tela e em hardware:**
  - o WKWebView, o WebView2 e o WebKitGTK para o K2, o J e o P54;
  - o TCC do microfone negado (F1-microfone-macos, parte c);
  - a primeira abertura do áudio com o pedido de permissão do macOS e com o WASAPI, para calibrar o `PRAZO_DO_AUDIO` (lote I);
  - se abrir e fechar o aparelho a cada sala pede a permissão de novo (lote I, D-52).
- **A via exata do microfone** pelo `AVAudioApplication.recordPermission` (`objc2-avf-audio`): pede exceção ao `forbid(unsafe_code)` do workspace.
- **O teto de som decodificado por MOD** (P49-7c): escolhe quanto som um MOD segura, e muda a frase pública.
- **O disparo do `ci.yml`** (job `bancadas`, e o passo do `--locked` e do upload do fuzz), e o gatilho dele.
- **Os laboratórios dos MODs** (P51-25) e o guia externo (P51-24), que são outros repositórios.
- **A conferência à mão do fechar hospedando,** no Mac e no Windows, vai com o Plano 2.

## Achados da conferência de 08/10
- **O guarda dos `FalhaAo*` cala variante com campo.** `variants_of` (`frontend.rs`) só aceita linhas `Nome,`, e por isso não confere `FalhaAoHospedar::NomeRecusado(…)` nem nada parecido. O P53 o conserta.
- **O guarda do E acharia 12, e não 13.** A conferência de `ler_codigo_do_mod` não fica num `#[tauri::command]`.
- **O `pendentes()` do balde já existe** (`7953a3b`), e o C não precisa criá-lo.
- **O T1c alcança mais do que o plano antigo dizia:** 14 leituras em `seele-conformance` e 5 em `sementes_do_fuzz.rs`.
- **`Bilhete::novo("")` dá `Ok`** pelo `validar_alvo`. O M1 o fecha junto com o `separar_ponto`.
- **O comentário de `o_link_leva_ate_o_servidor_e_a_escuta_de_hoje`** está falso desde `a6c2cc0`. O M1 o corrige.
- **A reconexão zera `current_voice_room` na FFI** enquanto o motor volta a sentar a pessoa. O I mede antes de escolher o gatilho do áudio.
- **`ServerShuttingDown` também é a recusa por falha de banco.** Por isso o G-servidor não troca a frase.
- **Somar bytes só depois do `decode()` abriria uma corrida no teto.** O K2 reserva antes e devolve na recusa.
- **A frase do J com o nome derivado não tem de onde vir.** A janela não conhece o `pessoa-…`.
- **A frase antiga do I («escolha o microfone em CONFIGURAÇÕES»)** não funciona hoje: `switch_device` exige uma voz viva.
- **O `fuzz/Cargo.lock` está atrás do `seele-proto` desde 05/09.**
- **O nome da pendência 55 está errado:** é `Batida::preparar`.
