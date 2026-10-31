# 0.19 — Cibersegurança: levantamento

Revisão só de leitura (servidor, MODs, web, Tauri e chamada pessoal). Nada foi
alterado ainda. Os itens 1, 2 e 3 foram reconferidos no código depois da
revisão; os outros ainda não foram.

| # | gravidade | o quê | onde |
|---|---|---|---|
| 1 | **alta** | anexo ignora a permissão do canal | `seele-server/src/transfer.rs:172` |
| 2 | **alta** | upload de volume de MOD sem limite de tamanho | `seele-server/src/mods/volume.rs:446` |
| 3 | média | senha de sala de voz verifica Argon2 segurando o cadeado do banco | `seele-server/src/session.rs:2758` |
| 4 | média | `buscar` de MOD alcança rede interna e segue redirecionamentos | `seele-server/src/mods/mundo.rs:42` |
| 5 | baixa | `WeighChannel` revela tamanho de canal restrito | `seele-server/src/session.rs:3431` |
| 6 | baixa | `RemoveMessage` só confere a permissão global | `seele-server/src/session.rs:3286` |
| 7 | baixa | `enviar_anexo` (Tauri) aceita qualquer caminho do disco | `apps/seele-app/src/main.rs:7210` |

## 1. Anexo ignora a permissão do canal — ALTA (reconferido)

`receive` só confere `Permission::AttachFile`, que é global, e publica em
`header.channel`. Não chama `canal_existe` nem `pode_no_canal(Escrita)`, que o
`SendMessage` chama.

**Ataque:** um membro sem escrita em #anúncios manda um anexo de 1 byte com
texto qualquer apontando para #anúncios. A mensagem é publicada.

**Conserto:** conferir o canal (existência + escrita) antes do orçamento de
bytes; conferir que `replies_to` é do mesmo canal; responder com
`AttachmentRefusal::NotAllowed`.

## 2. Volume de MOD sem limite de tamanho — ALTA (reconferido)

`volume::receber` chama `receber_imagem(..., None)`. Sem tamanho declarado, o
fluxo é gravado até o fim; o tipo só é conferido depois de tudo escrito, e o
caminho não passa pelo balde de bytes por pessoa.

**Ataque:** quem tem uma espera válida (um MOD de avatar chamou
`volume.esperar`) manda dezenas de GB. O disco de quem hospeda enche e o
servidor cai.

**Conserto:** exigir o tamanho declarado; teto por espera (declarado pelo MOD,
limitado por um teto fixo); conferir o tipo nos primeiros bytes; passar pelo
balde de bytes.

## 3. Senha de sala de voz segura o banco — MÉDIA (reconferido)

`voice_room_liberado(&*server.persistence.lock().await, ...)` roda o Argon2 com
o cadeado do banco tomado, num worker do tokio. Não há limite de tentativas por
sala.

**Ataque:** algumas conexões mandando senha errada em laço travam mensagens,
histórico e cargos para todos; também permitem força bruta da senha.

**Conserto:** ler o hash, soltar o cadeado, verificar em `spawn_blocking`;
balde de tentativas por pessoa e por sala.

## 4. `buscar` de MOD alcança a rede interna — MÉDIA (decisão de produto)

`buscar` é registrado para todo MOD de servidor, seja qual for o `reach`; só
confere `http(s)://`, segue até 10 redirecionamentos e não bloqueia loopback,
rede privada, link-local nem o endereço de metadados da nuvem. O MOD também
recebe o texto de todas as mensagens.

O ADR 0045 assume essa liberdade, mas a tela de aceite não mostra que o MOD
alcança a rede interna de quem hospeda.

**Conserto proposto:** `buscar` só com `reach` declarado; recusar loopback,
privada, link-local e metadados depois de resolver o DNS; sem redirecionamento
(ou reconferir a cada salto).

## 5–7. Baixas

- **5.** `WeighChannel`: conferir `pode_ler_canal`, com a mesma recusa de canal inexistente.
- **6.** `RemoveMessage`: conferir no canal da mensagem e a hierarquia (`pode_moderar` sobre o autor).
- **7.** `enviar_anexo`/`descrever_arquivo`: aceitar só uma referência opaca devolvida por `escolher_arquivo`/`guardar_colado`, como já foi feito no `salvar_anexo`. Hoje exige um XSS que a revisão não encontrou (CSP estrita, sem `innerHTML`/`eval` na UI).

## Conferido e sólido

- **Prova do handshake:** ligada à impressão DTLS e ao nonce.
- **Canal conferido:** `FetchHistory`, `JoinChannel` e `FetchAttachment` conferem o canal.
- **Limites:** teto de 16 KiB por quadro; limites de texto no `Validate`.
- **MODs:** pastas de MOD recusam `..`, caminho absoluto e prefixo do Windows.
- **Web:** executores de MOD em `iframe` sandbox opaco; a chave Ed25519 do navegador não é exportável; nada secreto no `localStorage`.
- **Chamada pessoal:** provas conferidas pelo WASM; SDP assinado ligado ao par DTLS.

**Fora do alcance:** a revogação de convite da chamada pessoal acontece no
serviço de encontro, cujo código não está neste repositório.
