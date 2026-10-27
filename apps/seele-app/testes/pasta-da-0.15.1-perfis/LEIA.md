# A pasta de quem hospedou na 0.15.1 com o Perfis 3.3.1 (API 5)

A prova da travessia de um MOD da API 5 para a API 6 pelo fluxo estagiado do P1
(25/10). O teste é `travessia_api5_testes`, em `apps/seele-app/src/`.

## O que tem aqui

- `pasta/` — **a gravação**, como a 0.15.1 a deixou numa máquina sem servidor
  guardado: o banco em `seele.sql` (esquema 13) e o pacote no cache por conteúdo,
  `mod-packages/<hash>/`. Não se edita à mão: regrava-se com o `gerar.sh`.
- `perfis-3.3.1/` — o `seele/perfis` 3.3.1 **de mentira** que a gravação usou,
  com o id, a versão e a API (5) do oficial, mas **não** os bytes nem o hash
  dele: o pacote publicado não está neste repositório. A metade de servidor
  guarda o perfil de cada pessoa no quintal, sob `perfil:<pessoa>`.
- `gerar.rs` e `gerar.sh` — o gerador, derivado do `../pasta-da-0.15.1/` (o do
  ESTILO), com o pedido do perfil no lugar do tema.

## Como a gravação foi feita

**Pelo código da v0.15.1** (`54f542f94`), que é a linha que roda a API 5: o
banco é aberto e migrado pela `Persistence` de lá, a conta de quem hospeda é a
do `register_or_find`, o MOD é ligado pelo `mods::enable`, e o perfil entra no
quintal pelo `mods::pedidos::executar`, com o `aoPedir` rodando no QuickJS de
lá. A saída do gerador, medida em 25/10:

```text
hash d3f49d8addc6ee4ef438620af18e3981dc1ef5899b8b5b157564e8ead4902b2d
save {"ok":true,"versao":"3.3.1","revision":1,"apelido":"dona","bio":"hospedo desde a 0.15"}
```

O `seele.sql` desta pasta saiu de um despejo em Python (na ordem de criação,
como o `sqlite3 .dump`), porque o `sqlite3` com redirecionamento não estava
disponível na máquina que o gerou; o `gerar.sh` usa o `sqlite3`. Os dois são
SQL que o `execute_batch` carrega.

```sh
sh apps/seele-app/testes/pasta-da-0.15.1-perfis/gerar.sh <pasta-de-trabalho>
```
