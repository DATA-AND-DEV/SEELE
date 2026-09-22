# O que foi entregue da revisão da v15, e o que não foi

Escrito em 22/09/2026, sobre a árvore de trabalho da branch `review-v15`, que sai
de `5340290`.

Este documento responde a `docs/review-v15-2026-09-21.md` achado por achado, e a
`docs/features-v15.md` nos dois pacotes dela. Ele diz o que está feito **e o que
não está**, porque um relatório de entrega que só lista o feito é a metade que não
serve para planejar a seguinte.

## Como ler as provas

Cada linha de «prova» nomeia um teste que **reprova quando o conserto é
revertido**. Onde escrevo «revertido e visto reprovar», a reversão aconteceu nesta
sessão: o conserto saiu, o teste ficou vermelho, o conserto voltou. Onde não
escrevo isso, o teste existe e a reversão não foi feita — e a diferença é o
«existir não é funcionar» do `CLAUDE.md`.

## Os achados de código

| # | O quê | Estado | Prova |
|---|---|---|---|
| R01 | Histórico sem conferir `ReadChannel` | **feito** | `seele-server/src/autorizacao.rs` (7 testes) e `conformance/autorizacao_de_canal.rs`: `sem_leitura_o_historico_nao_chega`, `assinar_sem_leitura_e_recusado`, `a_difusao_para_quando_a_leitura_e_revogada` |
| R02 | Troca de canal misturava conversas | **feito** | `state::uma_mensagem_de_outro_canal_nao_entra_na_conversa_aberta` — revertido e visto reprovar |
| R03 | Soltar o PTT deixava o microfone aberto | **feito** | `bancada/conversa-com-estado.cjs` R03 — revertido e visto reprovar |
| R04 | Uma mensagem inválida derrubava o lote | **feito** | `messages::uma_entrada_invalida_nao_leva_as_validas` — revertido e visto reprovar; e `um_destino_invalido_nao_leva_a_mensagem_de_outra_pessoa` no protocolo |
| R05 | O compositor apagava o texto sem confirmação | **feito** | `bancada/conversa-com-estado.cjs` R05 — revertido e visto reprovar |
| R06 | Prévias alcançavam qualquer domínio e a rede local | **feito** | ADR 0053; `uma_previa_nao_alcanca_a_maquina_nem_a_rede_de_quem_le`, `cada_salto_da_previa_e_validado_resolvido_e_fixado`, `a_previa_pergunta_antes_de_buscar`, `frontend::nenhuma_previa_sai_sem_consentimento`, e a bancada R06 — revertido e visto reprovar |
| R07 | Publicação não dependia de validação verde | **feito** | `release.yml` ganhou o job `validar` nos três sistemas, e `empacotar` depende dele; `ci.yml` ganhou macOS e o job `bancadas` |
| R08 | Revogar escrita não afetava sessão viva | **feito** | `conformance::revogar_a_escrita_morde_a_sessao_viva` |
| R09 | Reenvio idempotente anunciava no canal errado | **feito** | `messages::a_chave_repetida_devolve_o_canal_da_linha_original` |
| R10 | Teto da busca de MOD aplicado depois de alocar | **feito** | `mundo::uma_resposta_grande_e_cortada_enquanto_chega` — revertido e visto reprovar, medindo bytes empurrados |
| R11 | Pedido de MOD segurava o banco global | **feito** | O guarda longo saiu; cadeado por MOD e cache de pacote por hash |
| R12 | Snapshot copiava o histórico inteiro | **parcial** | `room.clone()` saiu do `snapshot`; **virtualização da lista de mensagens não foi feita** |
| R13 | Falha ao persistir confiança era silenciosa | **feito** | `identity::a_gravacao_troca_o_arquivo_...` e `uma_gravacao_que_falha_e_anotada` — os dois revertidos e vistos reprovar |
| R14 | Durabilidade não explicitava queda de energia | **feito** | `Durabilidade` com as duas promessas escritas; `a_escolha_de_durabilidade_chega_ao_banco` |
| R15 | Retenção e limites sem jornada | **parcial** | Retenção agendada e rotação de log feitas; **painel de armazenamento, alertas de capacidade e backup não** |

## Os seis de compartilhamento

| # | O quê | Estado | Prova |
|---|---|---|---|
| R16 | «Não ver» não cancelava operações pendentes | **feito** | `bancada/compartilhar-por-identidade.cjs` R16 — revertido e visto reprovar |
| R17 | Falta volume/mute do áudio da transmissão | **feito** | `voice::o_som_de_uma_tela_nao_e_limpo_por_outra` e `trocar_de_tela_larga_o_som_da_anterior` — os dois revertidos e vistos reprovar |
| R18 | A interface bloqueava múltiplos transmissores | **feito** | Bancada R18 — revertido e visto reprovar; e `tela_por_id` no lugar de «a primeira da sala» |
| R19 | Parar em tela cheia escondia a navegação | **feito** | Bancada R19 — revertido e visto reprovar |
| R20 | Painel reservava espaço para medições ausentes | **feito** | Os quatro campos saíram da ponte; `a_tela_em_curso_atravessa_pelo_nome_e_diz_de_qual_tela_fala` proíbe o retorno sem medida |
| R21 | Captura devolvia a conversa aos participantes | **parcial** | macOS: `excludesCurrentProcessAudio` aplicado. Windows: **não há exclusão** e o produto diz isso — `seele-video/src/som_capturado.rs`. Os três controles existem. **A validação acústica entre máquinas não aconteceu.** |

## A feature

| # | O quê | Estado | O que está lá |
|---|---|---|---|
| F01 | Ativação por voz mais sensível, alvo −60 dBFS | **parcial** | Ajuste manual de −72 a −24 dBFS, retenção de 40 ms da primeira sílaba, medidor com a marca do corte que está valendo, teste local. O limiar **mede** a sala: −60 dBFS é o alvo, e o corte é o ruído medido mais 9 dB. Falta a validação acústica. ADR 0055 |
| F02 | Filtro de microfone para priorizar a voz | **parcial** | Rust puro, sem crate novo. Em sinal sintético sobram 29% do ruído — 10,6 dB — com 95% de um tom de 300 Hz, e o residual mede −59 dBFS. Falta a validação acústica e a medida de CPU. ADR 0055 |

### A primeira versão destas duas linhas dizia «feito», e não estava

Uma auditoria independente — `docs/auditoria-f01-f02-2026-09-22.md` — mediu cinco
defeitos que os testes desta entrega não pegavam. Eles estão consertados e cada
conserto tem um teste que reprova quando ele sai:

| # | O que estava errado | Medida de antes | Medida de agora | Prova |
|---|---|---|---|---|
| A01 | `set_mode` zerava o portão a cada volta do laço, apagando a retenção e a sustentação | a primeira sílaba nunca saía | o estado atravessa a volta | `gate::a_volta_do_laco_nao_reinicia_o_portao` — revertido e visto reprovar |
| A02 | o piso da supressão não aprendia depois de silêncio digital | 100% do ruído passava | sobram 21% | `supressao::depois_de_silencio_digital_o_piso_ainda_aprende` |
| A03 | o padrão de −60 dBFS abria no ruído que sobra do filtro | **200/200 quadros** abertos sem fala | **0/200**, com o limiar medido em −50,5 dBFS | `tests/supressao_e_portao.rs` — revertido e visto reprovar com 200/200 |
| A04 | o primeiro trecho filtrado tinha 480 amostras e o codec o recusava | um trecho perdido por sessão, calado | quadro inteiro desde a primeira volta | `supressao::a_saida_e_sempre_um_quadro_inteiro_ou_nada` — revertido e visto reprovar |
| A05 | quadros retidos saíam com carimbos iguais | **duas ressincronizações** no buffer real | **zero**, e cada quadro recua 20 ms | `voice::o_relogio_de_quem_recebe_aceita_os_quadros_retidos`, que passa os carimbos pelo `JitterBuffer` de verdade e tem o controle negativo dentro; mais `os_quadros_retidos_recuam_um_quadro_cada_um` e o guarda do cabeçalho |

**O padrão que o `CLAUDE.md` nomeia aparece duas vezes aqui**, e nas duas contra
mim: A04 é «o produto sabe e não conta» — ele tinha o erro do codec na mão e o
descartava —, e A03 é «medir antes de concluir»: eu media a razão entre entrada e
saída e concluía sobre um limiar que compara dBFS absoluto.

E o terceiro é «existir não é funcionar», na forma mais direta: **os dois módulos
passavam em separado enquanto o par estava errado**. A supressão entregava a
atenuação que prometia, o portão abria onde foi mandado, e ninguém tinha um teste
com os dois juntos. Agora tem: `crates/seele-audio/tests/supressao_e_portao.rs`
roda `captura → supressão → portão` e tem três oráculos — ventilador sozinho não
abre, fala baixa abre, e depois da fala fecha de novo — mais as forças
intermediárias que a auditoria pediu por nome.

As duas correções que a auditoria pede e que **não** estão aqui continuam
pendentes, e são as mesmas de antes: gravações reais e medida de CPU. Elas estão na
lista «o que não está feito» abaixo, agora com F01 e F02 marcadas como parciais em
vez de feitas.

## O que **não** está feito, dito por inteiro

Isto é a metade que serve para planejar:

1. **Nenhuma validação acústica.** F01 e F02 pedem gravações reais de fala baixa,
   normal e distante, teclado, ventilador, headset e microfone de notebook. R21
   pede uma sessão entre duas máquinas com vídeo tocando e alguém falando,
   Windows↔Windows e Windows↔macOS. Nada disso aconteceu. O que existe é sinal
   sintético e guardas de acoplamento — e a auditoria de 22/09/2026 é a prova de
   que sinal sintético não basta: **ela achou cinco defeitos com sinal sintético
   também**, mudando só o oráculo. O que falta aqui não é mais um banco de testes,
   é ouvido humano em gravação de gente.
2. **Nenhuma medida de CPU, memória ou latência na máquina.** F02 critério 3 pede
   isso por plataforma, inclusive durante compartilhamento de tela. A latência da
   supressão está calculada — 10 ms — e não medida em carga.
3. **R21 no Windows não tem exclusão.** A API que a faria é o loopback por
   processo da WASAPI, que o `cpal` não expõe. O produto **relata** isso e oferece
   as duas saídas — compartilhar janela, ou transmitir sem áudio — em vez de
   afirmar que o eco foi resolvido.
4. **R12 pela metade.** O snapshot deixou de copiar o histórico. A virtualização da
   lista de mensagens e o teto do cache de prévias não foram feitos, e o critério
   de 10 mil mensagens não foi medido.
5. **R15 pela metade.** Retenção opt-in agendada e rotação de log estão feitas.
   Painel de armazenamento por servidor/MOD, alertas locais de capacidade e backup
   com restauração testada não estão — são jornadas de interface e de operação, e
   o review as apresenta como proposta.
6. **Nada do roteiro de produto.** PTT global, convite `seele://` clicável,
   notificação da portaria, busca no servidor, responder/editar, exportação de
   diagnóstico, backup de identidade, hospedagem persistente: tudo continua como o
   review os deixou. Eles são a tabela «Oportunidades de produto», e o próprio
   review os distribui por versões.
7. **Nenhuma divisão de `main.rs` nem tipagem da ponte.** A seção «Melhorias na
   organização do código» não foi tocada, exceto onde o conserto de um achado
   passou por ali.
8. **O CI deste repositório reprovava antes desta branch**, e o review registra
   isso. O portão de `release.yml` agora **bloqueia** a publicação enquanto os
   testes não passarem nos três sistemas — o que quer dizer que ele vai morder na
   primeira tentativa de publicar, e isso é o comportamento pedido. A saída de
   emergência existe, é um campo que diz o que faz, e aparece no registro do run.

## O protocolo subiu para 8

Duas coisas novas no vocabulário, as duas com portão em
`session::entende_a_mensagem`:

- `ServerMessage::MessageRejected` — a recusa de uma mensagem de texto, com a
  chave de quem escreveu e um `MessageRefusal`;
- `AlertReason::RetencaoApagouHistorico` — a limpeza de retenção avisando quem
  está com a tela aberta.

**Um par v7 continua entrando, conversando e escrevendo.** O que ele não recebe é
a notícia de que uma mensagem dele foi recusada — que é exatamente o que ele não
recebia antes desta versão.

O que isso custa em campo, dito como as subidas anteriores dizem: `encode` carimba
a versão global em todo quadro, então **publicar a v8 tira do ar o cliente v7
publicado**. É a mesma troca de todas as subidas deste repositório, e o seletor de
versão do ADR 0046 é o que a atenua.

## O que a verificação diz

Nesta máquina, em 22/09/2026:

- `cargo test --workspace -- --skip a_saida_desta_maquina_abre_como_entrada`:
  **2.384 passaram, zero falharam, cinco ignorados**, saída 0. Os treze a mais são
  os das cinco correções da auditoria. O teste pulado é o
  que abre a saída desta máquina como entrada, e o review registra que ele não
  conclui — a CI passou a pulá-lo por nome, com o motivo escrito.
- `cargo clippy --workspace --all-targets --all-features`: **zero aviso**. A CI
  usa `-D warnings`, então isto é o que ela vai medir. Um aviso apareceu no
  caminho — `indexing may panic`, num índice direto que o conserto de A02 tinha
  introduzido no laço de áudio — e saiu com `get_mut`.
- `cargo fmt --all -- --check`: limpo.
- `cargo xtask check-api`, `check-deps`, `check-versao`, `check-vetores`: passaram.
- As três bancadas de navegador — `compartilhar-por-identidade`,
  `conversa-com-estado` e `qualidade-da-voz` —: dez asserções, todas verdes, e
  agora rodando na CI.

Um defeito meu apareceu nessa verificação e vale registrar, porque é o padrão que
o `CLAUDE.md` nomeia: `conformance::moderacao` começou a reprovar porque a lista de
mensagens passou a carregar as pendentes, `Message::id` vale zero numa pendente, e
o teste pegava `messages()[0].id`. Ele recebia «não há tal mensagem» no lugar da
recusa de permissão que mede. O conserto foi em três lugares — o campo documentado,
um guarda em `remove_message` e o teste pedindo uma mensagem **confirmada** — e
está preso por `uma_pendente_nao_e_uma_mensagem_do_servidor`.

## O que as correções da auditoria medem, em números

Impressos pelos próprios testes, nesta máquina, e todos em sinal sintético:

| Medida | Valor |
|---|---|
| quadros abertos em 200 de ventilador puro, com filtro | **0/200** (era 200/200) |
| o mesmo com força de filtro 0,25 · 0,50 · 1,00 | **0/100** nas três, com o limiar em −38,1 · −40,7 · −50,7 dBFS |
| quadros abertos nos 100 de ventilador depois da fala | **0/100**, e a sustentação soltou em 10 quadros |
| fala baixa em sílabas | **todos** os quadros saíram, com 2 retidos na frente |
| ruído que sobra do filtro | 29%, e **−59,0 dBFS** de residual |
| ruído que sobra depois de silêncio digital | 21% (era 100%) |
| ressincronizações do buffer de quem recebe, numa abertura | **0** (eram 2) |

A linha das forças intermediárias é a que responde a pergunta que a auditoria fez
por último: o limiar acompanha o ruído **de cada força**, e não só o da força
máxima.

## Como publicar, depois desta branch

Os dois workflows passaram a rodar **só pela aba Actions** — `on:
workflow_dispatch` nos dois, e nada mais. Foi pedido por quem opera a
publicação, e as duas formas automáticas que saíram eram estas:

- **a CI em todo push e todo PR.** O que se perde está dito no cabeçalho do
  `ci.yml`: um push não é conferido por ninguém até alguém pedir. O que **não**
  se perde é o portão da publicação, que é outro arquivo;
- **o release na tag `v*`.** `git push --tags` empurra tudo o que está local,
  inclusive uma tag antiga criada para marcar um ponto e nunca destinada a
  lançar. Agora a publicação é: **Actions → Release → Run workflow**, com o
  número em `versao`.

**O portão do R07 continua inteiro**: `empacotar` depende de `validar` verde nos
três sistemas sobre o mesmo SHA, e a saída de emergência continua sendo um campo
que diz o que faz e aparece no registro do run. Quer dizer que publicar ainda
roda a verificação — o que deixou de acontecer sozinho é ela rodar **antes** de
alguém pedir.

Devolver qualquer um dos dois gatilhos é devolver duas linhas, e as duas estão
escritas nos comentários dos próprios arquivos.

## Decisões que ganharam ADR

- **0053 — Uma prévia não é um clique.** Separa a política de prévia automática da
  de abrir link, bloqueia destinos reservados, revalida cada salto com o endereço
  fixado, e põe o consentimento por domínio nas mãos de quem lê.
- **0054 — Uma transmissão tem identidade.** Tudo de compartilhamento passa a ser
  por `ScreenId`: a escolha de quem assiste, o som, os controles, a contagem de
  espectadores e a autoria.
- **0055 — O filtro é o que torna o limiar baixo defensável.** A supressão de
  ruído, e o alvo de −60 dBFS que ela compra.
