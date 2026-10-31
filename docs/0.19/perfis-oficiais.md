# PERFIS oficial na 0.19 — especificação

Decisões do dono (31/10/2026): **migrar automaticamente** os perfis do MOD,
**escopo inteiro** do MOD, e **o app antigo vê só o apelido**. O MOD fica
descontinuado no catálogo e é recusado pelos servidores 0.19.

Status: **proposta, aguardando aprovação**. As perguntas abertas estão na §9.

## 1. O que o MOD faz hoje

**Dados** (`SEELE-MOD-PERFIS/servidor/main.js`):
- Ficam no quintal do MOD (`mod_data`, `schema.rs:511`), na chave `profiles`: `{<pessoa>: {revision, displayName, pronouns, bio, status, accent:'#rrggbb', effect, avatar, banner}}`. O padrão é `blank()` (`:12`), com cor `#f2521f` e efeito `none`.
- Chaves transitórias: `garbage` (`:58-85`), `serie` (`:94`) e `upload-<pessoa>` (`:159`).
- Limites, em code points: nome 40, pronomes 30, status 60, bio 280 (`:9`, `:27`).
- Efeitos: `none|aurora|sparkle|pulse` (`:5`). Até 128 perfis (`:4`, `:137`).
- Caracteres recusados: controles C0/C1, U+061C, LRM/RLM, U+202A–202E e U+2066–2069. A bio aceita quebra de linha (`:19-20`).
- Concorrência pela revisão (`:163`). Só `ctx.person` grava, e com `ctx.write` (`:135-136`). Ler é livre (`:100-133`).

**Imagens**, em `RaizesDosMods.dados_de("seele/perfis")` (`seele-server/src/lib.rs:149`), em três formatos:
1. **Volume** (formato atual): `volume:images/<pessoa>-<token>`, bytes crus em `<dados>/seele/perfis/volume/images/…`. PNG, JPEG, WebP ou GIF, até 10 MiB (`main.js:154-157`).
2. **Fragmentos** (1.2 em diante): `uploads/<pessoa>-<token>.parts`, com o índice `{format:1,parts,length}` e os pedaços `….parts.<n>` de 6000 caracteres que formam um data-URL base64 (`git show 3128927:servidor/main.js:88-116`; leitura em `main.js:39-51`).
3. **Data-URL num arquivo só** (1.0/1.1): `main.js:41`.

**Onde ele desenha** (`ferramentas/perfis.js`):
- `pessoa.cartao` (`:241`), `pessoa.avatar` e `pessoa.capa` (`:798`).
- A entrada «Perfis» (`:995`), que abre o diretório (`:653`).
- A folha de outra pessoa (`:676-683`).
- O editor, um diálogo de 820px com prévia (`:550-617`, `:685`).
- A moldura de efeito (`:85-96`).

A casca já pinta foto e capa vindas do MOD em `tela-chamada.js:601`, `tela-sessao.js:1372`, `camada-nomear.js:234`, `celular-ajustes.js:260`, `tela-sessao.js:6031-6046` e `base.js:1611-1625`.

**O perfil nativo de hoje:**
- Apelido com tag (`schema.rs:789-796`).
- Ícone PNG de até 8 KiB e 256px em `people.icon` (`schema.rs:484`, `persistence/mod.rs:451-500`).
- Mensagens `SetPersonIcon`/`PersonIconChanged` (`control.rs:1726`, `:2515`; `session.rs:3215-3243`), com o acordo `person_icons_revision` (`seele-ffi/src/types.rs:852`).

## 2. Modelo de dados no servidor

Uma migração SQL nova, a seguinte à 19 (`schema.rs:789`):

```sql
CREATE TABLE perfis (
  pessoa INTEGER PRIMARY KEY REFERENCES people(id),
  revisao INTEGER NOT NULL,
  nome_exibido TEXT NOT NULL DEFAULT '', pronomes TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT '', bio TEXT NOT NULL DEFAULT '',
  cor TEXT NOT NULL DEFAULT '#f2521f',
  efeito TEXT NOT NULL DEFAULT 'nenhum' CHECK (efeito IN ('nenhum','aurora','brilho','pulso')),
  foto_hash BLOB, foto_tipo TEXT, foto_bytes INTEGER,
  capa_hash BLOB, capa_tipo TEXT, capa_bytes INTEGER);
CREATE TABLE perfis_migracao (
  pessoa INTEGER PRIMARY KEY, revisao_no_mod INTEGER NOT NULL,
  foto TEXT NOT NULL, capa TEXT NOT NULL,   -- 'trazida' | 'nenhuma' | 'falhou:<motivo>'
  avisada INTEGER NOT NULL DEFAULT 0);
```

- **Escopo:** cada servidor tem o próprio banco, então a chave é só a pessoa. Sem o teto de 128 do MOD.
- **Imagens em disco:** endereçadas pelo SHA-256, em `perfis/<hex>` ao lado de `seele.db`, no mesmo padrão de `diretorio_de_anexos` (`lib.rs:422`).
  - A gravação passa por `.em-obras-*` e termina num `rename`.
  - A imagem antiga só é apagada depois do commit; na subida, uma varredura remove os arquivos órfãos.
  - Com o banco em memória não há pasta para as imagens, e o envio é recusado com o motivo `SemPastaDePerfis`.
- **Teto de imagem de 10 MiB, e não 100:**
  - toda pessoa que desenha a lista ou a chamada baixa a imagem;
  - o bolso de mídia de cada lugar é 10 MiB (`mods-regiao.js:77`);
  - o prazo de cada fluxo bidirecional é 60 s (`session.rs:2217`);
  - a migração nunca encontra imagem maior, porque o MOD já limitava a 10 MiB.

  Fica na constante `perfil::TETO_DA_IMAGEM`. O tipo é conferido pelos bytes (`imagem.rs`), e HEIC volta com o motivo `Heic`.
- **Revisão:** `GravarPerfil` leva a revisão esperada; se ela não bater, volta `RevisaoVelha`. Trocar ou tirar uma imagem não confere a revisão, mas sobe o número.
- **Validação num lugar só:** `seele_proto::perfil::validar`, usada pelo servidor e pelo cliente. Mede em code points, aplica `trim` e recusa o mesmo conjunto de caracteres que o MOD. Quebra de linha só na bio, cor `#rrggbb`, efeito por enum, e o erro volta por campo.

## 3. Protocolo

**`crates/seele-proto/src/perfil.rs`** (novo):
- `PerfilPublico { pessoa, revisao, nome_exibido, pronomes, status, bio, cor:[u8;3], efeito, foto: Option<ImagemDoPerfil>, capa: Option<ImagemDoPerfil> }`
- `ImagemDoPerfil { hash:[u8;32], tipo, bytes:u32 }`
- `Efeito { Nenhum, Aurora, Brilho, Pulso }`
- `LadoDoPerfil { Foto, Capa }`
- `RecusaDePerfil { RevisaoVelha, Campos, Heic, TipoRecusado, GrandeDemais, Incompleto, SemPastaDePerfis }`
- `MAX_PERFIS_POR_PARTE = 8`: o pior caso fica em ~1,8 KB por perfil, contra os 16 KiB do quadro, e um guarda prende essa conta.

**Variantes novas, no fim das listas:**
- `ClientMessage::GravarPerfil { revisao, nome_exibido, pronomes, status, bio, cor, efeito }`
- `ClientMessage::TirarImagemDoPerfil { lado }`
- `ServerMessage::Perfis { parte, ultima }`: vai depois de `Session`, em partes, no padrão de `Membros` (`control.rs:2695`).
- `ServerMessage::PerfilMudou(PerfilPublico)`: difundido a todos depois do commit, por um `Event` no barramento.
- `ServerMessage::PerfilRecusado { motivo, campos }`: só para quem pediu.
- `ServerMessage::PendenciaDoPerfil { faltou }`: só para a própria pessoa, enquanto a migração dela tiver uma falha não avisada.

**Permissões:** gravar segue a regra de `SetNickname`/`SetPersonIcon`. O pedido não tem campo de pessoa: o servidor grava a linha de `session.person`. Ler é livre para quem está conectado. A taxa é o orçamento de quadros, mais um envio de imagem por vez por pessoa.

**Imagens pelo fluxo bidirecional que já existe** (`session.rs:2200-2240` → `mods/volume.rs:920`). `PedidoDeImagem` (`volume.rs:179`) ganha dois braços:
- `EnviarAoPerfil { lado, bytes }`: confere o teto, grava em blocos de 64 KiB, confere o tipo, faz o commit, difunde e responde `RespostaDeImagem`.
- `LerDoPerfil { pessoa, lado, hash }`: se o hash não é mais o atual, responde `erro: "mudou"`.

Por que esse caminho: o fluxo bidirecional já tem autenticação, fila, prioridade e prazo, e o nativo e o web já o falam. O anexo, ao contrário, é preso a uma mensagem. Como a imagem é endereçada pelo hash, o cliente pode guardá-la em cache para sempre. O despacho mora em `seele-server/src/perfis/imagens.rs`.

**Versão e portão:**
- `PROTOCOL_VERSION` vai a 12. `vocabulario::desde` dá 12 às quatro variantes do servidor, e `entende_a_mensagem` as cala para quem fala 11.
- `vocabulario::desde_do_cliente` (o mesmo nome que a API 8 usa) dá 12 às duas variantes do cliente.
- O cliente 0.18 continua vendo apelido e ícone nativo.

## 4. Migração

**Onde:** `seele-server/src/perfis/migracao.rs`, chamada em `Server::start` **antes** do portão `ligados_de_outra_api` (`lib.rs:786-800`). Na ordem inversa, um PERFIS 4.0.0 (da API 6) ligado num banco antigo impediria a subida.

**Quando:** se `mod_data` tem a chave `profiles` com `mod_id='seele/perfis'` (`persistence/mods.rs:271`), com o MOD ligado ou não. As imagens vêm de `config.mods_dir.dados_de("seele/perfis")`.

| MOD | Oficial |
|---|---|
| `displayName`, `pronouns`, `status`, `bio` | os mesmos campos, revalidados; um campo inválido vira `''` e entra no relatório |
| `accent` | `cor` em minúsculas; uma cor inválida volta ao padrão e entra no relatório |
| `effect` none/aurora/sparkle/pulse | nenhum/aurora/brilho/pulso |
| `revision` | `revisao`, também guardada em `perfis_migracao.revisao_no_mod` |
| `avatar`/`banner` `volume:…` | bytes copiados para `perfis/<sha>` |
| fragmentos `.parts` | índice + pedaços → base64 → conferência do tipo |
| data-URL num arquivo só | base64 → conferência do tipo |
| `upload-*`, `garbage`, `serie` | ignorados (estado transitório) |

**Idempotência:**
- Uma transação por pessoa, gravando `perfis` e `perfis_migracao` juntas.
- Quem já está em `perfis_migracao` é pulado. A exceção é uma imagem `falhou:` cujo arquivo de origem apareceu depois.
- Nunca sobrescreve um perfil cuja revisão já passou da migrada.
- **Copiar, nunca mover**: o quintal e o volume do MOD ficam intactos, o que permite voltar à 0.18.
- Uma pessoa que não existe em `people` entra no relatório e não é criada.

**Falha parcial, dita e nunca calada:**
1. O texto é migrado mesmo que a imagem falhe. A imagem fica `falhou:<motivo>`, com o motivo `nao-encontrada`, `fragmento-faltando`, `tipo-recusado` ou `grande-demais`.
2. Um `tracing::error!` registra a contagem e cada pessoa.
3. Um `RelatorioDaMigracaoDePerfis` é gravado em `config` e mostrado a quem hospeda em `camada-hospedagem.js`, até ser dispensado.
4. A pessoa recebe `PendenciaDoPerfil`, e o editor diz: «Sua capa do PERFIS não pôde ser trazida. Escolha de novo.»
5. Se o próprio banco falhar, o servidor sobe com o MOD desligado e o relatório «migração incompleta», e tenta de novo na próxima subida.

**Desligar e recusar:**
- Com todo mundo tentado, o servidor desliga o MOD (`persistence::mods::disable("seele/perfis")`, `persistence/mods.rs:164`).
- `seele-server/src/mods/incorporados.rs` lista `INCORPORADOS = [("seele/perfis", "0.19")]`. A lista é conferida na subida (religado → desliga de novo) e em `aplicar_conjunto_de_mods` (`apps/seele-app/src/main.rs:10655`), que devolve o erro tipado `ModIncorporado`.
- Quem hospeda lê: «Perfis agora faz parte do SEELE (0.19). Os perfis deste servidor foram trazidos.» O botão de ligar fica desabilitado.
- Os membros não veem diálogo: o conjunto de MODs muda pelo caminho normal, e o perfil continua igual.

## 5. Cliente

### 5.1 Core, FFI e web
- **`seele-core`:** `HashMap<PersonId, PerfilPublico>` no fold.
- **`Snapshot`:** ganha `perfis_revision` (o acordo de `person_icons_revision`) e `pendencia_do_perfil`.
- **`Connection`:** `perfis()`, `gravar_perfil(...)`, `tirar_imagem_do_perfil(lado)`, `enviar_imagem_do_perfil(lado, …)`, que confere tamanho e tipo antes de enviar, e `imagem_do_perfil(pessoa, lado, hash)`, pela ponte de mídia.
- **`seele-web`:** os dois pedidos novos. O navegador também envia imagem, e a frase «foto e capa se trocam pelo computador» sai.
- **`seele-sessao/catalogo.rs`:** lê o `descontinuado` (§6).

### 5.2 Precedência
- **Avatar:** contribuição de outro MOD (escolha de quem hospeda) → perfil oficial → ícone nativo → iniciais. Implementado em `aplicarAvatar` (`tela-sessao.js:6031`).
- **Capa:** a mesma ordem, em `aplicarCapa` (`:6004`). Continuam o teto de 6 capas animadas e o quadro parado com movimento reduzido.
- **Moldura de efeito:** vira CSS do produto, em `perfis.css`, e para com `prefers-reduced-motion`.

### 5.3 Onde cada coisa aparece
| Lugar | Desktop / navegador | Celular |
|---|---|---|
| Linha da pessoa | Foto no quadrado, capa parada ao fundo (recortada na área da linha), **nome exibido** e o apelido pequeno sempre visível. Ao expandir: pronomes · status e «Ver perfil». | Linha igual. A folha ganha «VER PERFIL» acima de MODERAR. |
| Chamada | Cartão com capa recortada, foto e moldura de efeito | `celular-voz.js`, mesmas regras |
| Mensagens | Avatar = foto | idem |
| Barra do operador | Foto e capa **recortada na área do cartão**; a imagem nunca dita o tamanho | A capa vai para a placa do Seu perfil |
| Diretório «Perfis» | Entrada fixa na coluna do servidor, acima de «DOS MODS». Abre uma grade de cartões na coluna central. | Entrada em AJUSTES, abrindo uma página |
| Perfil de outra pessoa | Folha à direita, no lugar do painel PESSOAS: capa, foto, nome exibido, apelido#tag, pronomes, status e bio | Folha que sobe da borda |

Arquivos novos: `ui/perfis.js` (store, diretório, folha e editor) e `ui/perfis.css`.

### 5.4 Editor = «Seu perfil», um lugar só

O diálogo «Editar meu perfil» do MOD e o `#perfil` (`index.html:4403`, `camada-nomear.js:191`) viram um só, na gramática da 0.19:
- painel de vidro;
- Inter, com inicial maiúscula (fim do mono em caixa alta);
- campos de 48px com raio de 16px;
- botões em pílula.

Ordem, de cima para baixo:
1. «Seu perfil», **sem FECHAR**.
2. Prévia viva do cartão, em tamanho real.
3. Foto (96px) e Capa (16:9), cada uma com as pílulas ESCOLHER e REMOVER e a nota de formato. A recusa de HEIC e o progresso do envio aparecem no próprio bloco. Nenhum link solto do tipo «TIRAR FOTO».
4. Apelido (com a tag) e Nome exibido.
5. Pronomes | Status.
6. Sobre você, com contador.
7. Cor: oito amostras + campo hex.
8. Efeito: pílulas segmentadas.
9. Ligações diretas: código e agenda.
10. Rodapé com CANCELAR (vidro) e **SALVAR** (laranja, a única ação primária).

Comportamento:
- **Uma saída só:** CANCELAR, Esc e clique fora fazem a mesma coisa. Com alterações, o rodapé vira «Descartar alterações?» [CONTINUAR EDITANDO] [DESCARTAR].
- **SALVAR manda tudo:** `SetNickname` (se o apelido mudou), `GravarPerfil` e os envios de imagem.
- **Recusas:** cada uma volta ao campo dela. `RevisaoVelha` diz «Seu perfil mudou em outra janela» e oferece recarregar.

Por casca:
- **Desktop:** diálogo de 720px, com prévia e rodapé fixos.
- **Janela estreita (<760px):** tela cheia.
- **Celular:** folha de altura total, com alça e rodapé acima do teclado.

## 6. Catálogo (SEELE-MODS-INDEXER)

**Não é revogação:** `revogacoes.toml` bloqueia o download (`seele-sessao/catalogo.rs:325-333`), e isso tiraria do ar quem ainda está num servidor 0.18 com o PERFIS.

O que muda:
- `avaliacoes/seele/perfis.toml` ganha `descontinuado = { desde = <unix>, motivo = "incorporado-ao-produto", produto = "0.19" }`.
- `ferramentas/catalogo.py` passa o campo adiante e mantém o `conferir_append_only` (`:138`).
- `listas.json` ganha `descontinuacoes`.
- O site do catálogo ganha a frase e o selo «Descontinuado: agora faz parte do SEELE 0.19».
- O README do MOD ganha o aviso.

## 7. Testes, cada um com a reversão que ele pega

**Proto:**
- Vetores das 6 variantes e dos 2 pedidos em `fio_2_0.txt`.
- `um_par_onze_nao_recebe_perfis`: reverter o braço `=> 12` faz o teste falhar.
- `uma_parte_de_perfis_cheia_cabe_num_quadro`.
- `perfil::validar` caractere por caractere, com `\n` aceito só na bio.

**Servidor** (`tests/perfis.rs`):
- Revisão velha é recusada.
- A linha alterada é a da sessão.
- `PerfilMudou` chega a todos, menos a um par 11.
- 10 MiB + 1 é recusado; HEIC volta com `Heic`; envio truncado volta com `Incompleto`; trocar a imagem apaga a anterior depois do commit.

**Migração com dados reais** (`tests/migracao_de_perfis.rs`):
- **Fixture:** `fixtures/perfis-5.0.0/`, gerada rodando o `servidor/main.js` **real** pelo `Despachante`, mais uma imagem em fragmentos (1.2) e um data-URL (1.0).
- **O teste afirma:**
  - todos os campos migrados;
  - bytes idênticos pelo SHA;
  - rodar duas vezes não muda nada;
  - um fragmento apagado vira `falhou:fragmento-faltando`, no relatório e em `PendenciaDoPerfil`, e o texto daquela pessoa é migrado assim mesmo;
  - o MOD sai desligado;
  - o quintal original fica intacto;
  - um PERFIS da API 6 ligado não impede a subida.
- **Reversões que o teste pega:** tirar o `disable` faz o teste falhar, e trocar «copiar» por «mover» também.

**Recusa:**
- `aplicar_conjunto_de_mods` com o PERFIS devolve `ModIncorporado`.
- Religado no banco, o MOD sai desligado na subida.

**UI:**
- O editor tem um único `button.primario`, nenhum FECHAR no cabeçalho e nenhum `label` em caixa alta.
- CANCELAR, Esc e clique fora passam por uma única função de saída.
- A precedência de avatar e capa é conferida com a ponte simulada.

## 8. Fases, donos e a fusão com a API 8

| Fase | Tam. | Arquivos (dono exclusivo) | Depende |
|---|---|---|---|
| F0 Protocolo | M | `seele-proto/src/{perfil.rs, control.rs, vocabulario.rs, version.rs, volume.rs, lib.rs}`, `tests/fio_2_0.*` | — |
| F1 Persistência | M | `seele-server/src/persistence/{schema.rs, perfis.rs, mod.rs}` | F0 |
| F2 Sessão + imagens | M | `seele-server/src/{session.rs, server.rs, perfis/imagens.rs, mods/volume.rs (2 braços)}`, `tests/perfis.rs` | F1 |
| F3 Migração + recusa | M | `seele-server/src/{perfis/migracao.rs, mods/incorporados.rs, lib.rs}`, fixtures + teste | F1 (paralela a F2) |
| F4 Core/FFI/web/app | M | `seele-core/src/{client.rs, enlace.rs}`, `seele-ffi/src/*`, `seele-web/src/*`, `seele-sessao/src/catalogo.rs`, `apps/seele-app/src/main.rs` | F0 (paralela a F1–F3) |
| F5 UI desktop/navegador | L | `ui/{perfis.js, perfis.css, camada-nomear.*, index.html, tela-sessao.js, tela-chamada.js, base.js, frases.js, camada-hospedagem.js, camada-mods.js, ponte-web.js, sessao-web.js}` | F4 |
| F6 UI celular | M | `ui/celular-{folhas,pessoas,ajustes,voz}.js`, `celular-*.css` | F5 |
| F7 Catálogo + README do MOD | S | SEELE-MODS-INDEXER, SEELE-MOD-PERFIS | — (publica na saída da 0.19) |

**Choques com a API 8** (`desenho/api-8-partes`, que já está no 12):

| Arquivo | Choque | Resolução |
|---|---|---|
| `version.rs:154` | `PROTOCOL_VERSION = 12` nos dois lados | Valores iguais. Fica o comentário da API 8, com um parágrafo sobre os perfis. |
| `control.rs` | Os dois acrescentam variantes no fim das listas | As variantes da API 8 vão primeiro e as do PERFIS depois; as linhas do PERFIS em `fio_2_0.txt` são regeradas. Nenhum teste fixa ordinal à mão. |
| `vocabulario.rs` | A API 8 cria `desde_do_cliente` | A F0 copia a função com a mesma assinatura; a fusão vira a união dos braços. |
| `volume.rs:197-240` | A API 8 troca o teto do fio por `TETO_DO_PACK` | Fica o `TETO_DO_PACK`, mais o braço `EnviarAoPerfil` com `TETO_DA_IMAGEM`. |
| `schema.rs` | A API 8 usa a migração 20 | O PERFIS usa a 20 na `entrega/0.19`; na fusão, a da API 8 vira 21 (só bancos de desenvolvimento da branch dela têm a 20). |
| `session.rs` | Braços novos nos dois lados | Conflito só de texto. |

## 9. Perguntas para o dono (com o padrão proposto)

1. **Ícone para quem ainda está na 0.18:** ao salvar uma foto, o cliente 0.19 também gera o ícone nativo (PNG de 256px e até 8 KiB) e manda `SetPersonIcon`. *Padrão: sim.*
2. **Outro MOD que desenha avatar ou capa vence o perfil oficial?** *Padrão: sim, porque foi escolha de quem hospeda.*
3. **Dados antigos do MOD:** quando apagar? *Padrão: manter até a 0.20, com um botão «Apagar dados antigos do PERFIS» para quem hospeda.*
4. **«Redefinir perfil»:** *Padrão: fora do escopo; REMOVER nas imagens e campos vazios cobrem o caso.*
5. **Entrada do diretório «Perfis»:** *Padrão: na coluna do servidor, acima de «DOS MODS».*
