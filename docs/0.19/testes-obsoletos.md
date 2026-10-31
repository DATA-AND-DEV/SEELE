# 0.19 — Testes obsoletos: levantamento

Levantamento só de leitura. Nada foi apagado ainda.

**Resumo:** quase nenhum teste guarda código que já não existe. Os candidatos
reais são (1) os da API 6, que saem com o corte, (2) meia dúzia que só conferem
texto de documento e (3) dois quase duplicados. **A suíte fica mais rápida
cortando esperas e navegador, não apagando testes.**

## 1. Só conferem texto de documento («existir não é funcionar»)

| teste | o que faz | ação | confiança |
|---|---|---|---|
| frontend.rs:14576 `a_excecao_do_tema_de_servidor_esta_escrita_na_estetica` | procura palavras em `specs/07-estetica.md` | remover | alta |
| frontend.rs:~15014 (fim do teste do tamanho da API de tema) | procura uma frase no ADR 0049 | remover só esse trecho | média |
| frontend.rs:~17619 (fim do teste de recusas nomeadas) | confere nomes no rascunho da API 6 | remover só esse trecho | média |
| frontend.rs:12002 `a_nota_que_promete_o_enter_tem_quem_a_cumpra` | par frase/código | manter | baixa |

## 2. API 6 — saem ou mudam com o corte

Hoje `APIS_ACEITAS = [7, 6]` e `MOD_API_MINIMA = 6` (`crates/seele-proto/src/mods.rs`).

**Remover:**
- `mods.rs:1340` `a_api_6_continua_como_saiu`
- `crates/seele-proto/tests/manifesto_da_0_16.rs` inteiro (réplica do leitor da 0.16; o `:154` ainda roda `git show`). Depende de a 0.16 deixar de receber catálogo.
- `apps/seele-app/src/travessia_api5_testes.rs` (API 5 → 4.0.0 da API 6) + a fixture `apps/seele-app/testes/pasta-da-0.15.1-perfis/` + o `mod travessia_api5_testes;` em `main.rs:71`.

**Reescrever** (o assunto continua, a API 6 sai):
- `mods.rs:1375`, `:1397`, `:1414`, `:1231` (+ fundir com `:1251`), `:1267`
- `cargos/testes.rs:986`: a API 6 só está no nome
- `mod_de_referencia.rs:402`, `anuncio.rs:339`, `persistence/mods.rs:477`, `aceite_dos_mods.rs:1073/1143`
- mais de 40 fixtures `api: 6` em testes comuns (ponte_de_pedidos.rs ×7, dois_produtores_de_recusa.rs, fio_2_0.rs, reach_da_linha_2.rs, vetores/mesa-4.0.0, control.rs, imagens_de_mod.rs, session.rs, despacho.rs ×6, mods/mod.rs, persistence/tema.rs, conferir_catalogo.rs): trocar para 7
- bancadas: contribuicoes-e-camadas.cjs, hospedar-com-mod-antigo.cjs, saidas-de-quem-hospeda.cjs, copia-antes-de-converter.cjs; e `ponte-web.js:348` (`apis_de_mod_aceitas: [7, 6]`)

**Não remover:** `a_api_publicada_nao_se_edita.rs`. A lista `api/congeladas.sha256` muda junto com a saída de `api/v6.json`, que hoje `aviso.rs:295` e `despacho.rs:642` carregam.

> ⚠ O agente da API 8 está mexendo em `mods.rs`, `version.rs`, `api/` e nesses mesmos testes. O corte da API 6 vai junto da integração da API 8, para não gerar dois conflitos nos mesmos arquivos.

## 3. Migrações antigas — incertas

Só saem se a 0.19 deixar de abrir os dados da 0.15.x/0.16:
`reproducao_0151.rs` (server e app), `mods/mod.rs:1391/1457/1499` (o 1499 quase repete `reproducao_0151.rs:162`), `cargos/testes.rs:123/190/255` (papéis → cargos da 0.17), `manifesto_hibrido.rs:157/293/391`, `servidores.rs:826/850`, `diagnostico-0151.cjs`.

Candidato claro: `apps/seele-app/tests/reproducao_0151.rs:352` `o_guarda_acusa_a_forma_da_0_16`, que só testa uma função auxiliar do próprio teste.

Remover uma bancada exige tirá-la da lista `BANCADAS` (`xtask/src/check_runtime.rs`).

## 4. Onde está o tempo

- `crates/seele-conformance/tests/tela_por_um_par.rs`: 20 testes, 21 `sleep`, prazos de até 30 s
- `a_recusa_nao_tira_da_sala.rs`, `bateria_interna.rs`: esperas de 30 s; `rajada_de_muitos.rs`: 60 s; `queda_recuperavel.rs`: 7 sleeps
- `soak_audio.rs`: 3 testes sem `#[ignore]`; pelo nome, longos
- cerca de 60 bancadas com Chromium; as mais pesadas: `vista-pessoal-0-17.cjs` (64 `waitForTimeout`), `ajustes-v013.cjs` (58), `capa.cjs` (21)

## 5. Não rodam no Mac (só aviso)

`permissoes.rs:266`, `seele-video/src/som_capturado.rs:167`, `firewall.rs:772`, `seele-audio/src/laco_por_processo.rs`, `device.rs:1517/1529` e o instalador (`desinstalar.rs`, `registro.rs`). Esses só valem na prova do Windows.
