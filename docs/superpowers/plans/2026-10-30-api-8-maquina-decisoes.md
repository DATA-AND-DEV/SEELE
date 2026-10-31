# API 8, parte 3 (a metade de máquina e o aceite) — decisões e pendências

Registro das decisões tomadas ao executar `2026-10-30-api-8-maquina.md`
(commits `8d7bb672..bb408841`, sobre `92d678ba`) e do que ficou para depois.
Cada decisão diz o custo, se estiver errada.

## Decisões

- **M0. O protocolo continua 12.** Conferido em 30/10 pelo `curl` do CLAUDE.md:
  v0.18.1 (`ae0f2715`), v0.18.2 (`13918258`) e v0.18.3 (`17554c28`) saem com
  `PROTOCOL_VERSION = 11`, e nenhuma carrega `api/v8.json`. *Custo:* uma release
  com o 12 fora obrigaria a subir para 13.
- **M1. O resumo prende o hash do pacote**, e não prende `pedido_por`: uma
  versão nova do MOD pede aceite de novo; outro administrador repedir não.
  *Custo:* toda atualização do MOD pergunta de novo à máquina; quem aceitou
  para Fulano segue servindo quando Beltrano repede.
- **M2. R9 substituído.** `ServerConfig::servir_tuneis` saiu (Task 7). Servir
  túnel exige que a pessoa, **pela conexão que aceitou**, seja a máquina aceita
  de um servidor de jogo daquele MOD cuja oferta tem o túnel com o mesmo nome,
  protocolo e porta (`RecusaDeTunel::ForaDoAceite`). Isso fecha o «primeiro a
  pedir leva o nome». *Custo:* nenhum túnel serve sem a metade de servidor ter
  pedido e o dono ter aceitado.
- **M3. `MudancaDeMaquina::Recusou.motivo` é `&'static str`**, para caber o
  `nao-esta-aqui` local do servidor. *Custo:* um campo de texto em vez de enum.
- **M4. Um pedido pendente que some avisa quem foi chamado**
  (`MaquinaDispensada { OutroPedido | ModDesligado }`). *Custo:* uma mensagem a
  mais por pedido trocado.
- **M5. `pedido_por = PersonId(0)` quando o pedido nasce de um momento**; o app
  mostra «alguém deste servidor». *Custo:* a tela não nomeia quem pediu nesse caso.
- **M6. O laço «recusou → o momento pede de novo» é limitado por taxa**: depois
  de uma recusa do servidor ou de uma recusa automática (`sem-tela`,
  `so-no-app`), um pedido nascido de momento para o mesmo (MOD, servidor,
  pessoa) em 5 s (`JANELA_DA_RECUSA_REPETIDA`) não sai; é logado e contado
  (`recusas_juntadas`). Medido: sem o guarda, 2305 momentos em 1 s (recusa do
  servidor) e 60 em 1 s (recusa automática). *Custo:* um pedido de verdade feito
  por momento logo depois de a pessoa abrir o app espera a janela.
- **M7. A recusa automática é da conexão, e não da pessoa.** Uma sessão de
  navegador (`SoNoApp`) ou sem tela (`SemTela`) só se tira do pedido; ele cai
  quando todas recusaram sozinhas, ou na hora com `PeloDono`. *Custo:* uma
  conexão que recusou sozinha não é perguntada de novo até reconectar.
- **M8. `este_processo_tem_tela(true)`, global do processo**, chamado uma vez no
  `main` do app (guardado por `frontend.rs`), além do `aceitar_pedidos_de_maquina`
  por enlace. O servidor reenvia os pendentes logo depois da `Session`, antes de
  um comando por conexão poder chegar. *Custo:* um processo que o liga aceita
  pedidos em toda conexão; só o app pode chamá-lo, e os testes nunca.
- **M9. O registro do aceite** (`<config>/maquinas`, uma linha JSON por aceite)
  é trocado sob `conhecidos::travar` e escrito 0600 por `trocar_privado`; a
  FFI recusa guardar um aceite cujo resumo não é o da oferta
  (`ResumoNaoConfere`). A atomicidade do `rename` não é provável por reversão
  (não há como matar a escrita no meio). *Custo:* uma queda no meio de uma
  escrita pode perder aceites (o dono aceita de novo).
- **M10. A pasta do servidor de jogo** é
  `<config>/jogos/<alvo>/<mod-id>/<servidor>/`, cada parte escapada (nomes
  reservados do Windows inclusos), chaveada pelo `alvo` como os aceites de MOD.
  *Custo:* o mesmo servidor alcançado por outro endereço ganha um segundo aceite
  e, na parte 4, uma segunda pasta. A parte 4 decide se chaveia pela impressão.
- **M11. O teto de canais repassados por pessoa** é 16 vivos
  (`MAX_CANAIS_REPASSADOS_POR_PESSOA`); acima, `CanaisEsgotados` a quem joga,
  log e `canais_acima_do_teto`. A prova de ponta a ponta usa canais `j`: medido
  que ~16 canais `t` parados já esgotam o crédito do enlace
  (`MAX_CREDITO_DA_CONEXAO` 8 MiB / janelas de 1 MiB), e um teste com `t`
  mediria o enlace, e não o teto. *Custo:* o corte de `repassar_t` não tem prova
  de ponta a ponta.
- **M12. As medidas do túnel na tela do MOD** (R12): entram do lado da máquina
  (`maquina.porta.quedas(nome)`); do lado de quem joga ficam para a parte 5, que
  decide a frequência na tela do MOD. *Custo:* quem joga não vê latência na tela
  do MOD ainda (só na faixa do produto).
- **M13. Duas regras novas no manifesto:** `poderes` sem `maquina` →
  `poderes-sem-maquina`; `maquina` sem `server` → `maquina-sem-servidor`.
- **M14. `rodar` continua um comando só** (`Option<ComandoOferecido>`). Forge e
  NeoForge pedem um instalador antes do servidor; esse passo pode vir como uma
  mensagem nova no fim da lista (acréscimo, que o ADR 0058 permite), e a
  `Politica` do ramo `desenho/api-8-programa` também tem um `comando`.
  *Custo:* a parte 6 acrescenta uma mensagem.
- **M15. `ServerMessage::PedidoDeMaquina.oferta` vai em `Box`** (o postcard
  codifica igual; o ouro não mudou), e saiu o `allow(large_enum_variant)` que
  deixava o clippy do CI vermelho por `Event::CargosMudaram`.
- **M16. Um aceite inválido agora tem resposta**: `MaquinaDispensada{SemAceite}`
  só à conexão que aceitou (só 12); no app, `SemAceite` para só a metade que
  roda, e nunca fecha um diálogo pendente. *Custo:* um `SemAceite` atrasado,
  resposta a um aceite já trocado, pode parar uma metade mais nova da mesma
  chave (falha fechada; a faixa diz `sem-aceite`).
- **M17. O app numera os pedidos na chegada** (sob a trava do estado), e uma
  dispensa que chega depois de um pedido não é ultrapassada por ele. Ainda há
  uma thread por pedido (sem teto); a ordem não depende disso.
- **M18. Troca de geração do enlace para as metades daquela sessão**
  (`ServidorSaiu`); um pedido igual depois as religa sem tela.
- **M19. Desvios aceitos nas tarefas:** `MotivoDeAceiteInvalido::MaquinasDemais`
  (o teto também vale no aceite); o vigia do MOD dispensa também quando o hash
  do pacote muda (coerente com M1), e agora só as entradas de hash velho;
  os comandos Tauri recebem `modId` (camelCase, como o guarda da janela pede);
  o código da metade de máquina é lido no clique, antes de guardar o aceite,
  do mesmo `read` que conferiu o hash (`read_with_files`); `mod_nome` = id;
  os comandos `maquina_*` ficam em `SEM_CARIMBO_DA_VISTA` (a máquina é do
  computador, não da vista); `outro-pedido` não fecha um diálogo que não tem o
  mesmo `resumo`.
- **M20. A Task 7 herdou trabalho não commitado** de um implementador que caiu;
  foi lido contra o brief, mantido, e revisado como os outros.
- **M21. A re-revisão da 2ª rodada do Task 10** (13 linhas de doc) foi feita
  pelo controlador lendo o diff.

## Provas por reversão (CLAUDE.md, «Existir não é funcionar»)

Cada guarda abaixo foi desfeito, o teste caiu com a mensagem dita, e o código
voltou (conferido com `cmp`):

- **Resumo:** sem `disco_maximo` no que se codifica →
  `o_resumo_muda_com_cada_coisa_mostrada` cai no caso do disco.
- **Validação do pedido:** `oferta.validate()` trocado por `Ok(())` →
  `o_pedido_de_maquina_recusa_hash_ruim_e_oferta_invalida` («oferta inválida passou»).
- **Manifesto:** sem `PoderesSemMaquina` ou `MaquinaSemServidor` → o teste das
  regras cai; `"tunel"`→`"tunel2"` na v8 → `as_capacidades_por_poder_da_v8_sao_as_do_codigo` cai com o nome.
- **Livro das máquinas:**
  - `tunel_permitido` sem a porta → o teste de unidade cai;
  - `aceitar` sem conferir `quem` → `so_quem_foi_chamado_aceita…` e
    `outra_pessoa_nao_toma_o_nome` caem;
  - `so_desta` só pelo nome → `reaceitar_com_outra_porta_para_o_tunel_da_porta_velha` (left [] right ["mc"]);
  - sem o vigia do MOD → `desligar_o_mod_dispensa_a_maquina_dele`.
- **Teto por pessoa:** sem o decremento no `Drop` → a vaga não volta; sem o
  corte → o 17º não recebe `CanaisEsgotados`.
- **Metade de servidor:**
  - `PedidoDaMaquina` sem `maquina_de` → `quem_nao_e_a_maquina_nao_fala_como_maquina`;
  - momentos a todos os MODs → `os_momentos_da_maquina_vao_so_ao_mod_dela`;
  - sem a janela da recusa → 2305 momentos em 1 s; sem a da recusa automática → 60;
  - recusa automática final → `a_recusa_automatica_de_uma_sessao_nao_derruba_o_pedido_da_outra`;
  - sem reenvio à sessão que chega → `so_quando_todas_recusam_sozinhas_o_pedido_cai`.
- **Núcleo:** `aceitar_pedidos_de_maquina` padrão `true` → `sem_tela_o_pedido_e_recusado_sozinho`;
  sem a trava do registro → a corrida de 200 voltas cai 3 de 3 (196/200 ressurreições);
  sem conferir o resumo ao guardar → o teste de `ResumoNaoConfere` cai.
- **Ponta a ponta:** `atender_servir` sem `conferir_tunel` → `sem_aceite_a_maquina_nao_serve`
  e `a_maquina_que_cai_e_volta…`; `parar` sem parar túneis → `parar_a_maquina…`
  (relatórios com `encerrado: None`); sem `tuneis().saiu` ou sem `Parou{saiu}` →
  `a_maquina_que_cai_e_volta…`.
- **App:** `decidir` só pelo `servidor` → `um_resumo_diferente_nao_se_aceita_sozinho`;
  sem `conferir_servir` → `a_bomba_recusa_servir_fora_da_oferta`; sem
  `daMaquina` no prelúdio → `a_metade_de_maquina_nao_tem_tela`; sem a
  reconferência do `servir` tardio → `um_servir_que_volta_depois_do_fim_e_parado`;
  sem `este_processo_tem_tela` no `main` → o guarda de `frontend.rs`.
- **Casca:** ACEITAR sempre ligado → bancada caso 2; sem `suaConta` → caso 1 e o
  guarda; faixa de volta dentro de `tela-sessao` → caso 13; casar só pela chave
  → caso 12b; sem a linha no `publicar.sh`/`ci.yml` → o guarda das bancadas.
- **Onda final:** a resposta `SemAceite` ao aceite inválido; `SemAceite` que
  preserva o pendente; motivo desconhecido; ordem dos pedidos; reconexão; nome
  de arquivo do artefato; dispensa que preserva o hash novo — cada um caiu com o
  conserto desfeito (`final-fix-report.md`).

**Provado só por raciocínio:** a trava `vez` serializando DESLIGAR/REVOGAR/RECUSAR
com a decisão (o teste pede um `AppHandle` vivo); o registro da metade antes de
o aceite sair; o «ligada» anunciado sob a trava; a geração passada pela `Bridge`;
o fechamento do TOCTOU de `codigo_da_maquina`; o `expect` do resumo (não falha).

## A suíte inteira

Linha de base em `8d7bb672`: 33 nomes em 17 alvos. Antes da onda final
(`958a30dc`): 36 nomes; os três novos eram dois guardas que a parte 3 quebrou
(`a_metade_de_janela_do_vetor_so_chama_o_que_a_api_expoe`,
`nenhum_manifesto_de_teste_crava_a_versao_da_api`), consertados na onda final,
e `servidores::a_lista_vem_do_mais_recente_para_o_mais_antigo`, arquivo que a
parte 3 não toca, que passa 3 de 3 sozinho (relógio sob carga). Depois da onda
final, por instrução, só os crates tocados: sem falha nova (seele-server só as
da base; proto, core, ffi, web limpos; as cinco conformâncias da máquina e do
túnel; app 504; frontend só as 4 da base; `check-api` e clippy do CI limpos).
A suíte inteira fica para quem coordena, no fim de todas as partes.

## Pendências menores (não bloqueiam)

- **T1:** o ajudante `crua` de `control.rs` é difícil de ler; o parágrafo do 12
  em `version.rs` está colado.
- **T2:** o teste das capacidades não pega chave sobrando em
  `capacidades-por-poder`/`-da-maquina`; mistura `MOD_API_VERSION` com
  `capacidades_de(8)`; «chave ausente = vazio» do `Programa` sem comentário.
- **T3:** o ramo «túnel igual em outro aceite continua coberto» de `so_desta`
  sem teste direto; `so_desta` olha só a conexão, `Tuneis::parar` a pessoa;
  o log de `ForaDoAceite` sem `servidor=` (o `ServirTunel` não o carrega).
- **T4:** o corte de `repassar_t` sem teste; `primeiro_da_maquina` enrolado;
  o `# Errors` de `repassar` desatualizado; `reservar_ou_corta` toma `tuneis()`
  duas vezes; **medir com um jogo de verdade** se os canais `Controle` contando
  no 16 e o crédito do enlace apertam.
- **T5:** nomes de campo de `oferta_do_json`; o caminho do momento lê todos os
  quintais; o formato do log do avisador mudou; logs sem `servidor=`; um
  pendente com `esperando` vazio fica até o teto; `servidor` variando passa pelo
  guarda do laço.
- **T6:** os nomes kebab→enum da FFI escritos à mão; sem teste do reenvio depois
  da resposta, do portão abaixo do 12, e do `ModReply` tardio.
- **T7:** as leituras com prazo de 500 ms em `tunel_no_servidor`; `PACIENCIA`
  repetida; o derrubar-e-servir não espera o servidor ver a queda.
- **T8:** uma thread por pedido/parada sem teto; a cola Tauri (registro,
  comandos, eventos) sem teste; reemitir `maquina-pedido` enquanto pende.
- **T9:** DESLIGAR fica desligado se o Rust responde Ok e `encerrar` devolve
  false; a faixa cai no `mod_id` para uma máquina não vista; a faixa fixa cobre
  até 30dvh; o título em maiúsculas por CSS; o caminho real `app.emit` → janela
  não foi rodado no app (a bancada usa uma ponte de mentira).
- **Final:** um `SemAceite` atrasado pode parar uma metade mais nova da mesma
  chave (M16); `ligar` para a metade anterior antes de o aceite novo ser
  validado; `numerar` antes do descarte da fila cheia de chegados cedo; `ultimos`
  chaveado só pela `Chave` (outra sessão viva da mesma chave ultrapassa).
- **Para a parte 4:** a tela não mostra `oferta.pack` nem cada artefato (só
  número, total e origens) — quando a parte 4 preencher, a tela tem de mostrar;
  decidir a chave da pasta (M10).
- **O indexador** (`SEELE-MODS-INDEXER`) precisa aceitar `maquina` e as regras
  novas antes de um MOD da 8 com elas ser publicado.
- **Não medido:** a tela e o túnel entre duas casas reais.
