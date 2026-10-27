# 0064 — Cargos e permissões: a soma do Discord, a restrição que já existia e um dono que não nasce da rede

Status: aceito · implementado na 0.17 (seele-proto `cargos.rs`, seele-server `cargos/`, migração 18); conformidade de ponta a ponta pendente do transporte novo
Contexto: o usuário pediu que «uma pessoa pode ter cargo pra fazer tal coisa, mas não pra outra», inclusive nas ações de MOD, «usando como base o sistema de cargos do Discord». Até a 0.16 havia quatro papéis fixos (Comandante, Operador, Pessoa, Observador), nenhum verbo de gestão no fio, a regra «negada vence concedida», e o assento do Comandante dado a quem chegasse primeiro — com uma escalada medida: revogado o Comandante de todos, a conta NOVA seguinte o tomava, porque `seat_the_arrival` não olhava a marca.
Decisão: cargos com posição, um `@everyone`, a soma das concessões menos as restrições, a hierarquia por posição, o Administrador, o proprietário declarado pela máquina que hospeda, um guarda do GANHO EFETIVO sobre toda gestão, a revisão do conjunto de cargos, e as ações de MOD autorizadas no despacho por uma chave estruturada que o MOD declarou. Detalhe abaixo.
Alternativas: copiar o Discord inteiro (sobrescritas por canal, negação só no canal) — rejeitado porque a migração daria privilégio a quem tinha Pessoa + Observador; manter os quatro papéis fixos — rejeitado pelo pedido; o primeiro que chega vira dono — rejeitado (revisão do Codex): é autoridade pela ordem da rede, e uma corrida num servidor exposto.
Consequências: a autorização tem um lugar só (`seele_server::cargos`), e a regra pura mora no `seele-proto`, onde o cliente a usa para explicar «por quê» sem decidir; o fio 10 ganha oito verbos e três mensagens no fim; o `seeled` sem `--proprietario` sobe sem dono e diz como resolver; os testes que esperavam «a primeira conta vira Comandante» passaram a declarar o dono ou dar o cargo à mão.

---

## Referências e procedência

- Documentação técnica do Discord, <https://docs.discord.com/developers/topics/permissions> — lida
  diretamente pelo cargos-backend em 24/10/2026: «Permission Hierarchy», «Permission Overwrites», o
  pseudocódigo `compute_base_permissions`/`compute_overwrites`, «Implicit Permissions» e o Timeout.
- Artigo de suporte, <https://support.discord.com/hc/en-us/articles/214836687-Discord-Roles-and-Permissions>
  (atualizado em 18/07/2025) — lido pelo coordenador Codex, que confirmou: ordem de cima para baixo;
  agir só sobre quem está abaixo do seu cargo mais alto; Gerenciar cargos cria e edita só abaixo de si e
  concede só o que tem; `@everyone` é a base; Administrador concede tudo e ignora as restrições de canal.
- A API de MOD 7 (as permissões declaradas e o `AcaoDeMod`): `api/README.md`, seção «A API 7», e
  `api/v7.json`. O nome com que a janela de um MOD pede a ação entra numa emenda da v7.
- O código: a regra pura em `crates/seele-proto/src/cargos.rs`; a autorização do servidor em
  `crates/seele-server/src/cargos/`; o banco em `crates/seele-server/src/persistence/cargos.rs` e na
  migração 18; o cliente em `crates/seele-sessao/src/cargos.rs`, e os comandos em
  `apps/seele-app/src/cargos.rs`; a conformidade em `crates/seele-conformance/tests/cargos.rs`.

## A regra

```
efetivas(p):  proprietário → todas | banida → nenhuma
  C = concede(@everyone) ∪ concede(cada cargo de p)     R = restringe(@everyone) ∪ restringe(cada cargo)
  Administrador ∈ C − R → C = todas (do produto e as de MOD declaradas)
  efetivas = C − R
```

A posição não entra na conta: só delimita quem administra quem. Toda autorização é lida na hora, do
banco; erro de banco é negação.

## Decisões SEELE × referência

| tema | Discord (referência) | SEELE 0.17 | por quê |
|---|---|---|---|
| combinação | OR de `@everyone` e dos cargos | **igual** | — |
| negação | só em sobrescrita de canal, e lá a permissão de um cargo vence a negação de outro | **restrição de cargo**, no servidor inteiro, e ela VENCE qualquer concessão | é a «negada vence» de antes, com nome: preserva o efeito de cada combinação legada (Pessoa + Observador continua calada) e serve a ação real «silenciar». O análogo do Discord é o Timeout, que também subtrai por cima da soma |
| Administrador | tudo, e ignora as sobrescritas | tudo, inclusive as de MOD, e passa pelo cargo exigido do canal; **não** passa pela restrição de cargo nem pela hierarquia | sem isso, Comandante + Observador voltaria a falar depois da migração |
| hierarquia | atribuir, editar, ordenar só abaixo do mais alto; expulsar/banir só quem está abaixo | **igual**, mais: (a) atribuir um cargo exige ter tudo o que ele concede; (b) o alvo de um verbo de cargo tem de estar abaixo (ou ser o autor); (c) quem não administra não age sobre quem administra (a regra do `moderavel` de antes) | (a) fecha «atribuo a mim um cargo de baixo que bane»; (c) garante que nenhuma combinação legada passe a moderar quem não podia |
| ganho efetivo | não existe como regra explícita | **toda** gestão é conferida pelo efeito: quem muda tem de estar abaixo do autor, e tudo o que alguém GANHA tem de estar no efetivo do autor | revisão do Codex: tirar uma restrição, revogar de si ou apagar um cargo que restringe, ou conceder Administrador estando restrito, dariam o que o autor não tem. Os cinco casos e a propriedade reprovam com o guarda revertido (medido) |
| dono | `owner_id` do servidor, tudo | a **impressão da chave** declarada pela máquina que hospeda (o app no `hospedar`, a sala pessoal pela chave dela, o `seeled` por `--proprietario <impressão>`, que é o que ele lê: não há arquivo de configuração); tudo, acima da hierarquia e de toda restrição; ninguém age sobre ele | o dono não nasce da ordem de rede. Sem declaração, não há dono, e o `seeled` registra o erro acionável. Chegar, reconectar ou repetir um apelido nunca dá autoridade |
| sobrescritas por canal | oito passos por canal | **não** na 0.17; fica o «cargo exigido do canal» de antes, que nada grava hoje, e que o Administrador e o dono atravessam | poucos canais planos, sem categoria; a restrição cobre o caso real de tirar; o fio as aceita depois como acréscimo |
| catálogo | dezenas de permissões | as ações REAIS: as treze de antes, **Transmitir tela** (separada de Falar; a migração espelha) e **Administrador**; nada sem ação por trás (mute de outros, menção a todos…) | uma permissão sem ação é decoração |
| MODs | — | a ação só é autorizável se o MOD a declarou no `mod.json` (API 7); a chave é `{mod_id, acao}`; o servidor autoriza NO DESPACHO do `AcaoDeMod`, antes de chamar a metade de servidor; o `ModRequest` da API 6 segue a regra de hoje (ler o canal; `admin`/`write`), com `contexto.acao = null`, e um MOD da 6 não tem ação autorizável | «uma ação de MOD só é autorizável se o MOD a declarou» e nunca por nome arbitrário; a lista no contexto é informativa, o bloqueio é o portão |
| concorrência | — | o conjunto de cargos tem uma **revisão**; todo verbo diz a que viu, e uma velha é recusada (`RevisaoVelha`), sem gravar; ler, decidir e gravar numa `BEGIN IMMEDIATE` | revisão do Codex: a edição velha de um gestor não desfaz em silêncio a restrição de outro |

## A migração 18 (sem perder dado, sem aumento)

`roles` ganha `position`; `@everyone` nasce no id 0 **vazio**; cargos feitos à mão vão para baixo;
`ShareScreen` espelha `Speak` (concedido e negado); `Administrator` vai a quem concedia as treze;
`person_roles` não muda; nascem `mod_permissoes`, `revisao_dos_cargos` e o gatilho do maior id.
Provado: para as 16 combinações dos quatro papéis, com e sem banimento, o efetivo depois é igual ao da
função legada copiada, e Transmitir tela é igual a Falar de antes; em 64 bancos com cargos feitos à mão,
ninguém ganha nada. O único aumento intencional é o dono declarado, que o contrato exige.

## O fio (linha 2, acréscimos no fim)

Cliente 37–44: `CriarCargo`, `EditarCargo`, `MoverCargo`, `ApagarCargo`, `AtribuirCargo`,
`RevogarCargo` (todos com `revisao`), `PedirCargos`, `AcaoDeMod`. Servidor 48–50: `QuadroDeCargos`,
`SuasPermissoes`, `CargoRecusado` (desde 10). `Permission` += `ShareScreen`, `Administrator`. O
`fio_2_0.txt` foi regerado só com acréscimos (e a `Telemetry` com `Option`, decisão à parte do
coordenador).

## O efeito imediato, e o gancho de segurança

Um verbo aceito devolve o quadro novo a todas as sessões e o `SuasPermissoes` a cada pessoa atingida.
Para que um cliente que não lê o canal de controle não atrase uma revogação, a transação chama, logo
depois do commit e antes de qualquer notificação, um GANCHO DE EFEITO DE SEGURANÇA
(`EfeitoDeSeguranca { pode_falar, pode_transmitir, pode_entrar, pode_escrever }` por pessoa), que a
sessão e a sala de voz aplicam no servidor: param de encaminhar a voz e param a tela. A autorização é
barata (lê só a pessoa) e se faz no ponto do efeito: um `AcaoDeMod` na fila atrás de um MOD lento é
reautorizado depois de obter o turno.

## Medido

Contra o servidor e a `Connection` reais (`seele-conformance/tests/cargos.rs`): o dono declarado
manda e quem chegou primeiro não; o quadro novo chega a todas as sessões; o atingido perde Falar na hora;
a revisão velha, a autoconcessão do Comandante e a concessão do que não se tem são recusadas pelo
servidor; tudo atravessa o reinício; dois servidores e duas sessões da mesma pessoa não se misturam. Não
medido aqui: os datagramas de quem não lê o canal de controle (o teste é do servidor, no gancho acima).
