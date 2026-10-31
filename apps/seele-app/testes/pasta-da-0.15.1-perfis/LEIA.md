# A pasta de quem hospedou na 0.15.1 com o Perfis 3.3.1 (API 5)

A prova de que, na 0.19, o PERFIS da API 5 que a 0.15 deixou ligado é dito
como incorporado ao produto no ANTES DE HOSPEDAR (sem ATUALIZAR para o 4.0.0),
e de que os perfis dele chegam às tabelas do produto na subida. Os testes:

- `perfis_incorporado_testes`, em `apps/seele-app/src/` (o fluxo do app: a
  recusa, a escolha, o cancelar, a cópia e a subida);
- `o_perfis_3_3_1_da_0_15_1_vira_perfil_do_produto`, em
  `crates/seele-server/tests/migracao_de_perfis.rs` (a migração, campo a campo).

## O que tem aqui

- `pasta/` — **a gravação**, como a 0.15.1 a deixou numa máquina sem servidor
  guardado: o banco em `seele.sql` (esquema 13), o pacote no cache por conteúdo,
  `mod-packages/<hash>/`, e a pasta de arquivos do MOD, `mod-data-legado/`
  (`raizes_dos_mods(config, None)` nas duas versões). Não se edita à mão:
  regrava-se com o `gerar.sh`.
- `perfis-3.3.1/` — os três arquivos do pacote do `seele/perfis` 3.3.1
  **de verdade** (`mod.json`, `cliente/main.js`, `servidor/main.js`, os que o
  `ferramentas/package.mjs` do MOD empacota), tirados do
  `SEELE-MOD-PERFIS@077477486af2`. O hash do pacote montado aqui pode não ser
  o do publicado no catálogo.
- `perfis-4.0.0/` — um Perfis 4.0.0 **de mentira**, da API 6: o que o
  ATUALIZAR de antes oferecia. O teste o põe no cache para provar que, mesmo
  com ele lá, a saída é recusada.
- `gerar.rs` e `gerar.sh` — o gerador, derivado do `../pasta-da-0.15.1/` (o do
  ESTILO), com os pedidos do PERFIS no lugar do tema.

## Como a gravação foi feita

**Pelo código da v0.15.1** (`54f542f94`), que é a linha que roda a API 5: o
banco é aberto e migrado pela `Persistence` de lá, as contas são as do
`register_or_find`, o MOD é ligado pelo `mods::enable`, e cada pedido passa
pelo `mods::pedidos::executar`, com o `aoPedir` do 3.3.1 rodando no QuickJS de
lá:

1. a dona salva o perfil (`save`): nome, pronomes, status, bio, cor e efeito;
2. a dona envia a foto pelo volume (`upload-start` com `transporte: "volume"`,
   `upload-finish`) — **o único passo simulado**: os bytes são postos onde o
   fluxo de volume da 0.15.1 os deixaria, porque o gerador não abre um fluxo
   de rede;
3. a dona envia a capa pelos fragmentos (`upload-start` sem transporte, dois
   `upload-part` de até 6000 caracteres), o formato do 1.2 ao 3.1;
4. a convidada salva só o texto.

A saída do gerador, medida em 31/10:

```text
hash 23827c3d73f62e0205f4e0c2fd67da258f51d7eb4591e05bb7e6498859d9c29f
dona 1 convidada 2
foto 82 bytes, capa 5070 bytes
```

O quintal fica numa chave só, `profiles`, a mesma que o PERFIS usa do 1.0 ao
5.0 — a que `seele-server/src/perfis/migracao.rs` lê.

```sh
sh apps/seele-app/testes/pasta-da-0.15.1-perfis/gerar.sh <pasta-de-trabalho>
```
