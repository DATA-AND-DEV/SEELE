# A API 8 sobre a 0.19 — decisões da fusão

`desenho/api-8-partes` (c9e908a4, partes 1 a 6) fundida em
`desenho/api-8-sobre-0.19`, que nasceu de `entrega/0.19` em 01ac840f. Uma linha
por decisão, com o custo se estiver errada.

Conferido em 31/10/2026 no release (`curl` do CLAUDE.md): a mais nova é a
v0.18.3, protocolo 11. Nem o protocolo 12 nem a API 8 saíram; as duas linhas
podiam se juntar num 12 só, e a v8 segue emendável.

## O fio

- **Um 12 só, e as variantes da API 8 antes das do perfil**, em `ClientMessage`
  e em `ServerMessage`, como o §8 de `docs/0.19/perfis-oficiais.md` decidiu (a
  API 8 chegou ao 12 primeiro). Os bytes-ouro da API 8 em `tests/fio_2_0.txt`
  ficaram iguais aos da `desenho/api-8-partes`, byte a byte; os do perfil
  (`GravarPerfil` 0x36, `TirarImagemDoPerfil` 0x37, `Perfis` 0x3e …
  `PendenciaDoPerfil` 0x41) foram regerados pelo modo congelado, que só
  acrescenta. Se errada: um build de desenvolvimento da `entrega/0.19` fala o
  perfil com outros ordinais e não conversa com este; nenhum foi publicado.
- **`desde_do_cliente` é uma função só**, a união dos braços que os dois lados
  escreveram com a mesma assinatura. `CONHECIDAS` passa a 56 (cliente) e 66
  (servidor).
- **A migração 20 é a do perfil, e a das plataformas virou a 21** (o §8 também):
  só bancos de desenvolvimento da `desenho/api-8-partes` chegaram à 20 de
  plataformas. Se errada: um desses bancos, aberto por este build, acha a 20
  aplicada e pula a dos perfis — apague-o.

## Os tetos do volume (o choque semântico)

- **A imagem segue a 0.19: até 100 MiB por item** (`TETO_DE_ARQUIVO`,
  5c904d79), e o `.mrpack` segue em 64 MiB (`TETO_DO_PACK`). A parte 6 tinha
  imagem e som em 10 MiB; isso caiu.
- **O fio leva o maior dos dois** (`volume::TETO_DO_FIO_DE_VOLUME`, calculado em
  `const`, hoje 100 MiB): o `PedidoDeImagem::Enviar`, a `RespostaDeImagem` e a
  leitura do cliente (`read_to_end`). O §8 dizia «fica o `TETO_DO_PACK`», mas
  isso foi escrito antes dos 100 MiB: com ele, uma imagem de 80 MiB passaria
  pela espera e morreria no fio.
- **O teto continua por tipo, antes do primeiro byte.** Antes do sniff vale o da
  espera: o maior tipo que ela aceita (100 MiB com imagem, 64 só com
  `mrpack`), contra o total declarado e no corte. **Lidos os 12 bytes do sniff,
  vale o do tipo**: o total declarado acima dele morre ali, antes de o primeiro
  byte ir ao disco (`AcimaDoTeto`, com o tamanho e o teto). Para isso o sniff do
  fluxo da 0.19 (dc75be7e) reconhece, além das quatro imagens, o começo de um
  zip (`PK\x03\x04`) quando a espera aceita `mrpack`; o índice é conferido no
  fim, como na parte 6. Se errada: um `.mrpack` com dados antes do primeiro
  cabeçalho local (um zip autoextraível) é recusado como formato desconhecido.
- **O seletor faz o mesmo**: `teto_do_pedido` é 64 MiB quando o MOD pede só o
  papel `pack`, e 100 quando pede o pack e outro papel; `tipo_do_escolhido_para`
  confere o teto do tipo nos bytes, o do pack inclusive.
- **A `api/v8.json` passa a dizê-lo** (emenda em `congeladas.sha256`), e o guia
  do autor também.
