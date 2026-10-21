# O indexador na API 6 — o que o dono leva ao `SEELE-MODS-INDEXER`

Tarefa 13 do plano da F3, Step 6 (do dono, e não desta frente). Escrito em
15/10/2026, com o corte da API 6 feito no ramo `0.16/api-6-corte`. Lido do
`ferramentas/manifesto.py` do indexador no `589bf7f`, só para leitura.

**Nada aqui foi feito no indexador.** É o que muda lá, e por quê, para o dono
fazer e publicar. Até lá, o guarda
`o_indexador_e_este_build_oferecem_a_mesma_api_de_mods`
(`crates/seele-conformance/tests/a_api_dos_mods_nao_diverge.rs`) reprova **de
propósito**: o app da 0.16 não pode sair antes de o indexador aceitar a API
dele, e o vermelho é essa ordem sendo cobrada.

## As quatro mudanças em `ferramentas/manifesto.py`

O catálogo publicado diz hoje `"api_oferecida": 5`, `"apis_aceitas": [5, 4, 3]`.

1. **`VERSAO_DA_API = 6`**, o espelho de `seele_proto::mods::MOD_API_VERSION`.

2. **`APIS_ACEITAS = (6, 5, 4, 3)`**, que deixa de ser o espelho de
   `seele_proto::mods::APIS_ACEITAS` (esse agora é só `[6]`, o desta linha) e
   passa a ser **a união das linhas**: `seele_proto::mods::apis_de_toda_linha()`,
   tirada de `seele_proto::mods::APIS_POR_LINHA`
   (`crates/seele-proto/src/mods.rs`):

   ```rust
   pub const APIS_POR_LINHA: [(&str, &[u32]); 2] = [("1", &[6]), ("0.15", &[5, 4, 3])];
   ```

   A 6 é da linha 1 (o app 0.16, o Servidor 1.0). As 5, 4 e 3 são da linha
   0.15, que o launcher roda ao lado. No catálogo publicado hoje estão a MESA
   3.1.0 (API 4), o ESTILO 3.1.0 (API 4) e o PERFIS 3.3.0 (API 5). Os legados
   MESA 3.1.1, ESTILO 3.1.1 e PERFIS 3.3.1, que saem depois, continuam
   publicáveis para quem hospeda pelo Servidor 0.15.

   O comentário de lá passa a dizer que o conjunto é a união de
   `APIS_POR_LINHA`, e não o espelho de `APIS_ACEITAS`.

3. **A chave `"tema"`, um booleano** (a porta do tema da API 6, tarefa 17a;
   campo `tema` do `Manifest`, `#[serde(default)] pub tema: bool`). São três
   lugares, e não um:
   - em `CHAVES`;
   - em `TIPOS`, como `"tema": bool`;
   - e na conferência de tipo, onde `isinstance(valor, bool)` recusa todo
     booleano: ela foi escrita porque `true` não é um u32, e tem de valer só
     para as chaves `int`. **Sem isso, `"tema": true` é recusado** como
     «tema deveria ser bool».

   E **fora** de `OPCIONAIS`: no Rust, a chave ausente vale `false`, mas
   `"tema": null` não desserializa.

   Medido em 15/10, com o `manifesto.py` do `589bf7f` alterado só na memória e
   um manifesto de API 6 com metade de servidor:

   | o que muda | `"tema": true` dá |
   |---|---|
   | nada (hoje) | `malformed: chave desconhecida: tema` |
   | `VERSAO_DA_API` e `APIS_ACEITAS` | o mesmo |
   | e mais `CHAVES` e `TIPOS` | `malformed: tema deveria ser bool` |

   E pelo `read_manifest` do produto, no mesmo dia:
   - `true` → `Ok(true)`;
   - sem a chave → `Ok(false)`;
   - `null` → `Malformed`;
   - `1` → `Malformed`.

   O ESTILO 4.0.0 ainda não existe: a `revisao-0.16` dele está na API 4, sem a
   chave. Mas ele precisa de `"tema": true` para que `mundo.tema` o aceite.
   Com as regras de hoje, o indexador o recusaria inteiro.

4. **A recusa nova, `tema-sem-metade-de-servidor`:** `"tema": true` sem
   `"server"`, como o produto faz desde o corte
   (`Refused::TemaSemMetadeDeServidor`, Step 6b). Quem abre a porta do tema é a
   metade de servidor, por `mundo.tema`; sem ela, o tema nunca aparece. O
   indexador não deve publicar um pacote que o app recusa ao instalar.

   **A ordem é a do Rust:** `api-too-new`, `api-too-old`, `malformed-id`,
   `empty` e, só então, esta. Medido: um manifesto com `"tema": true` e sem
   `client` nem `server` dá `Empty`; um com só `client` dá
   `TemaSemMetadeDeServidor`. A conferência entra logo depois da de `empty`.

   A tabela de recusas do guia (`docs/guia-criacao-mods.md`, perto da linha
   1270) ganha a linha dela quando o guia passar à API 6.

## A ordem de publicação (spec-mestre `b9817abc`, §4)

1. O indexador com as quatro mudanças acima.
2. Os três oficiais na API 6 — PERFIS, MESA e ESTILO 4.0.0 —, **antes** do app.
3. O app 0.16.
4. Os legados das 3, 4 e 5, **depois**.

## Depois de publicar o indexador

Copiar os dois números para `apps/seele-app/testes/api-do-indexador.json`
(`"api_oferecida": 6`, `"apis_aceitas": [6, 5, 4, 3]`), a partir do
`manifesto.py` publicado, e não destes valores. Com eles, o guarda passa. Medido
em 15/10 contra o guarda do Step 5: com o vetor de hoje (5, `[5, 4, 3]`)
reprova; com `[6, 5, 4, 3]` passa; com uma 7 a mais reprova («aceita uma API
que nenhuma linha roda»); e sem a 3 reprova («faltam [3]»).
