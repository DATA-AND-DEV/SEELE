# A 1.0: ligar pelo código, e a tela inteira para MODs

> **Escopo.** Duas partes, que se tocam no núcleo de confiança.
>
> **Parte I, a sala pessoal.** Cada pessoa passa a ter um **código**, como um
> número de telefone. Quem tem o código cola em CONECTAR e liga. O app de quem
> recebe toca na hora, e quem recebe atende ou recusa. Pode aceitar mais gente
> na mesma ligação, até 6 pessoas. Ninguém precisa estar num servidor: a sala
> roda no app de quem é o código, enquanto ele estiver aberto.
>
> **Parte II, os MODs na 1.0.** A lista fechada de 11 pontos de contribuição
> vira um **mapa de regiões nomeadas** que cobre a janela inteira (API 6). Só
> um **núcleo de confiança**, escrito como contrato, fica fora do alcance de um
> MOD, e a ligação da Parte I entra nele. É a migração de MODs da 1.0.

## A frase que este documento implementa

*«Ligar direto pra alguém. Tô pensando em fazer um código interno fora do
servidor, e as pessoas conseguem conversar numa sala sem estar num server.»*

*«Código de usuário. Ele tem um código e envia para um amigo. Ao colocar pra
conectar, entra na sala.»*

## Decisões do dono (23/09/2026)

| pergunta | resposta |
|---|---|
| O que é o código | um código **de usuário**, que ele manda para um amigo |
| Quando a sala está disponível | **como um telefone**: enquanto o SEELE estiver aberto, quem tem o código liga e o app toca |
| Tamanho | **pequeno grupo**: quem recebe pode aceitar mais pessoas na mesma ligação |
| Arquitetura | **abordagem A**: segunda hospedagem do app, só em memória, com o código derivado da chave de identidade |
| Toque | **instantâneo já na 1.0**: a conexão fica segurada enquanto toca, com uma mensagem nova no protocolo congelado |
| Recusa | quem liga lê **«RECUSOU»**, diferente de «NÃO ATENDEU» |

Decisões anteriores que este desenho supõe: hospedar o próprio servidor e
visitar outros (sub-projeto 3); o protocolo da 1.0 congelado depois de uma
última subida (sub-projeto 4); sem Mac Intel, sem Linux e sem assinatura de
sistema operacional na 1.0. O contexto está em
`docs/analise-para-a-1.0-2026-09-22.md`.

---

## O que existe hoje, e o que obriga a mudar

O mapa do código foi feito no HEAD `e2fac4d` (v0.15.0) por sete leitores em
paralelo. Os pontos de carga, lidos no código:

**Reaproveitado quase inteiro:**

- **A hospedagem.** Não há estado global de produção no `seele-server`: cada
  `Daemon` é dono do próprio socket, endpoint, banco e tarefas
  (`crates/seele-server/src/lib.rs:472-713`).
  `Hospedagem::iniciar(porta, Location::Memory, …)` já sobe um servidor em
  memória e sem MODs; só os testes percorrem esse caminho
  (`hospedagem.rs:385-626`).
- **A escada de alcance** (UPnP, PCP e furo de NAT) e o quarto do ponto de
  encontro (`MORO`/`QUEM`), que está no ar em produção.
- **A marca do ponto de encontro.** Ela aceita até 32 caracteres
  alfanuméricos. Um código cabe sem mudar o SEELE-ENC/1
  (`crates/seele-proto/src/encontro.rs:133`, `:161-169`).
- **Um verificador TLS que recusa antes do `Hello`**: `ConfereImpressao`, em
  `crates/seele-core/src/par.rs:385-487`, ligado em produção e testado com QUIC
  real.
- **A impressão que a portaria já grava.** É o SHA-256 da chave crua de quem
  entra (`seele-proto/src/transport.rs:318-340`), então o código de quem liga
  sai dela sem mudar o protocolo.
- **A faixa de batida da portaria** (`apps/seele-app/ui/camada-portaria.js:507-532`):
  fica fora das telas, nunca chama `focus()` e se anuncia uma vez.
- **O diálogo PERFIL, único, com e sem sessão**, e `copiarLink`, que já
  contorna a recusa da área de transferência no WKWebView.

**Obriga a mudar:**

1. **A identidade TLS não se injeta.** `Daemon::bind` só sabe
   `Identity::load_or_create` (`lib.rs:516-519`). Em memória, nasce uma
   P-256 nova a cada abertura (`tls.rs:38-47`).
2. **A assinatura do desafio é um oráculo.** O cliente assina qualquer nonce
   de até 256 bytes (`client.rs:1700`; `control.rs:273`, `:2719`). O conteúdo
   do CertificateVerify do TLS 1.3 tem 130 ou 146 bytes. Enquanto isso valer,
   apresentar a chave de identidade no TLS equivale a deixar qualquer servidor
   visitado atender as suas ligações. **O conserto (S2) é pré-requisito.**
3. **As marcas do quarto estão presas à impressão do certificado e à marca
   fixa `anfitriao`** (`alcance/encontro.rs:359-376`, `:447`). Somam-se os três
   defeitos do link (§2.1 da análise): o QUEM sai sem porta, a marca da escuta
   está errada, e sem impressão esperada não há `LEVE`.
4. **O degrau 4 só roda sem IPv4 global** (`alcance.rs:1021`), e o primeiro
   `MORO` sai 15 s depois da subida (`alcance/encontro.rs:790-791`).
5. **A portaria foi desenhada para «nada espera».** Ela derruba com
   `AdmissionPending` (`portaria.rs:30-41`), e a aprovação e a recusa são
   permanentes, travadas por teste (`portaria.rs:591-625`). O ADR 0030 recusou
   segurar a conexão. **Um toque segurado precisa de módulo próprio e de ADR
   próprio.**
6. **Os prazos.** São 4 s por candidato (`enlace.rs:697`) e 10 s para o aperto
   de mão, nas duas pontas (`client.rs:571-583`; `session.rs:327-331`;
   `transport.rs:45`).
7. **O app tem uma vaga só de hospedagem**, e `disconnect` a encerra
   (`apps/seele-app/src/main.rs:145`, `:2004-2019`).
8. **O UPnP foi escrito para um servidor só.** Ele recua para a 8383 e roubaria
   a regra do servidor do grupo; o aviso de «porta não canônica» é falso para
   8385; `fechar` apaga pelo número da porta externa; e todas as regras têm a
   mesma descrição (`alcance/porta.rs:108-123`, `:268-278`, `:299-303`,
   `:432-491`).
9. **Largar uma `Hospedagem` sem `encerrar`** deixa a tarefa do degrau 4 viva,
   com o socket e a porta presos: `Encontro` não tem `Drop`
   (`alcance/encontro.rs:379-424`).
10. **O toque da casca vem de um `setInterval` de 5 s**, que a webview
    minimizada estrangula e até suspende. O app também não tem som de interface
    nenhum.
11. **A identidade é por `SEELE_HOME`.** O lançador dá uma pasta por versão
    (`apps/seele-app/src/versoes.rs:218-223`), então o código mudaria com a
    versão aberta.

---

## As peças

| peça | onde | o que faz |
|---|---|---|
| **`Codigo`** | `crates/seele-proto`, reexportado pelo `seele-core` | O **único** lugar que deriva, formata, lê e confere um código, e que dá as marcas do quarto. Mora no `seele-proto` porque o servidor também precisa dele (ADR 0002) |
| **Identidade da sala** | `crates/seele-server/src/tls.rs` e `ServerConfig` | Certificado Ed25519 feito da chave de identidade, estável byte a byte entre aberturas |
| **`ConfereCodigo`** | `crates/seele-core`, ao lado de `par.rs` | Verificador TLS: recusa se a chave não bate com o código, antes do `Hello` |
| **`Esperada`** | `crates/seele-core/src/enlace.rs` e `client.rs` | `Certificado(fp) \| Codigo(Codigo)`, escolhido por conexão. Substitui `impressao_esperada: Option<String>`. O mesmo corte conserta o S2b e o terceiro defeito do link |
| **Marcas explícitas e SEELE-ENC/2** | `alcance/encontro.rs`, `seele-core/src/encontro.rs`, `seele-proto/src/encontro.rs`, `crates/seele-encontro` | `Convocacao` recebe as marcas prontas. A sala faz um registro assinado pela chave, que também leva dicas de rede local |
| **Campainha** | `crates/seele-server/src/campainha.rs` (novo), irmão de `portaria.rs` | Estado de cada ligação, em memória: tocando, atendida, recusada, não atendida ou cancelada. Deduplica por impressão, aprova por ligação, aplica a espera depois de recusa e o teto, e **empurra** o toque |
| **Aperto de mão em três fases** | `crates/seele-server/src/session.rs` | A fase do toque segura a conexão sem segurar a trava do banco |
| **Hospedagens** | `apps/seele-app/src/main.rs` | `{guardado, pessoal}` mais **uma** conexão. É a base compartilhada com o sub-projeto 3 |
| **Quem liga** | `seele-core` (chegada, enlace) e FFI | Leitura do código, LOCALIZANDO, `Tocando`, CANCELAR, razões terminais |
| **Casca** | `apps/seele-app/ui/` | Faixa de toque, espera CHAMANDO, "SEU CÓDIGO" no PERFIL, agenda, frases |
| **Som do toque** | `crates/seele-audio` (novo abridor só de saída) até a FFI | Toque tocado pelo Rust, na saída escolhida, sem abrir o microfone |
| **Agenda** | `crates/seele-core/src/agenda.rs` (novo) | Código → nome que eu dei, apelido visto, última ligação, direção e bloqueio |

---

## O código

### Forma

- **Derivação:** `SHA-256(chave pública Ed25519, 32 bytes crus)`. Os primeiros
  **75 bits** viram 15 símbolos de base32 Crockford.
- **Verificador:** um 16º símbolo calculado por **Luhn mod 32** sobre os 15.
  Ele pega toda troca de um símbolo e toda inversão de dois vizinhos, exceto a
  do par de valores 0 e 31, que é a fraqueza conhecida do Luhn mod N. É ele
  que permite dizer «erro de digitação» sem consultar a rede.
- **Exibição:** `XXXX-XXXX-XXXX-XXXX`.
- **Leitura:**
  - maiúsculas, ignorando hífen e espaço;
  - O → 0, e I ou L → 1;
  - U é recusado;
  - exatamente 16 símbolos.
- **Marcas do quarto**, só alfanuméricas e sem hífen. O `p` separa o espaço
  de marcas das marcas `fp16` dos servidores:
  - aviso (o que o `LEVE` traz): `p` + os 15 símbolos de dado;
  - escuta de avisos: `p` + 15 símbolos + `e`;
  - socket do servidor: `p` + 15 símbolos + `s`.
- **Formato congelado para sempre.** Derivação, verificador, alfabeto e marcas
  viram contrato público: mudar qualquer um invalida todo código já
  distribuído. Os vetores de teste ficam congelados no molde de
  `vetores-de-hash.json`.

### Como um texto colado é classificado

Uma função só no núcleo, `ler_entrada(texto) -> Endereco | Convite | Codigo |
CodigoComErro`. A casca manda **todo** texto ao Rust, e o `startsWith` de
`camada-servidores.js:169` sai.

1. `seele://@…` é código.
2. `seele://…` é convite.
3. Texto sem `.`, `:`, `[`, `]` nem `/` que normaliza para 16 símbolos Crockford:
   - com o verificador certo, é **código**;
   - com o verificador errado, é **erro de digitação**.
4. O resto é endereço.

Um nome de máquina de uma palavra só, com exatamente 16 símbolos válidos, passa
a exigir `:porta`. Nomes NetBIOS têm no máximo 15 caracteres, e o Bonjour exige
`.local`.

### Força

- Achar uma chave com o mesmo código custa cerca de 2^75 gerações de chave.
  Contra qualquer um de N códigos no ar, custa 2^75/N.
- **Depois da primeira ligação completa, a chave inteira fica pinada** sob
  `@CÓDIGO` no arquivo de pins, e dali em diante a conferência usa 256 bits.
  Chave diferente com o mesmo código dá «ALGUÉM TENTOU SE PASSAR POR …», e a
  conexão é recusada.

### Uma chave para todas as versões

A `identity.key` passa a ser lida da pasta principal de configuração, e não da
pasta de cada versão que o lançador cria. O código é da **máquina**, e é o
mesmo em qualquer versão aberta. Isso muda o ADR 0017 e o ADR 0046, e o custo
fica registrado neles.

---

## Identidade da sala e o conserto S2

### O certificado

1. A semente de 32 bytes da `identity.key` vira PKCS#8 v1 com um prefixo fixo
   de 16 bytes. Não há crate `pkcs8` no lock, e o formato do arquivo não muda,
   porque o ADR 0017 marca como alto o custo de mudá-lo.
2. `rcgen::KeyPair::from_pkcs8_der_and_sign_algo(&PKCS_ED25519)` gera o par. O
   `rcgen` 0.14.8 e o `ring` já estão na árvore.
3. `CertificateParams` com SAN fixo `seele-sala`, número de série fixo e
   validade fixa. A assinatura Ed25519 é determinística, então o DER sai igual
   a cada abertura, e o dono nunca bate em «A CHAVE MUDOU» ao entrar em
   `127.0.0.1:8385`.
4. Um campo novo, `ServerConfig.identidade: Option<tls::Identity>`, é usado no
   lugar de `load_or_create` quando presente. O `Debug` esconde a chave.
5. A semente sai da FFI, passa ao `seele-server` dentro do mesmo processo Rust
   e **nunca atravessa a webview**.

### S2: a prova de identidade amarrada ao canal

- A prova passa a ser
  `Sign("SEELE/1 prova-de-cliente\0" ‖ exportador ‖ nonce)`:
  - o exportador são 32 bytes de `export_keying_material`, com rótulo
    próprio, que os dois lados calculam;
  - o nonce tem **exatamente 32 bytes**, e o cliente recusa qualquer outro
    tamanho;
  - o primeiro byte do rótulo é diferente de `0x20`, o que separa este espaço
    de mensagens do CertificateVerify do TLS 1.3, que começa com 64 × `0x20`.
- **O cliente nunca assina bytes vindos do fio.** A mensagem é montada por um
  helper único no `seele-proto`, usado pelos dois lados.
- Isso entra na subida da 1.0 (sub-projeto 4). Clientes 0.15 deixam de se
  autenticar em servidores 1.0, e isso já é esperado.

**Risco residual aceito:** um build até a 0.15 rodando com a **mesma**
`identity.key` continua assinando nonce cru. Isso só acontece reinstalando uma
versão antiga por cima, porque o lançador usa outra pasta. Fica escrito nas
notas da 1.0.

---

## Quem liga confere o código dentro do TLS

- `Esperada::Codigo(c)` escolhe o `ConfereCodigo`, cópia do `ConfereImpressao`
  com `Relato`:
  1. `ParsedCertificate` → SPKI, exigindo o OID de Ed25519;
  2. `Codigo::confere_chave(32 bytes)`, e depois o pino `@CÓDIGO`, quando
     existir;
  3. `verify_tls13_signature` delegado ao provedor `ring`, porque é ele que
     prova a posse.
- **O `PinStore` por `host:porta` não é usado** para código.
- Falha vira `ConnectError::CodigoNaoBate { esperado, veio }`, lida do `Relato`
  antes do `classify_connection_error`. Nunca vira `TlsRefused` genérico.
- O `server_name` é a constante `seele-sala`. **Nunca o código**: o SNI viaja
  legível no Initial do QUIC e contaria para quem se está ligando.
- `Esperada::Certificado(fp)` confere o `fp` do convite **dentro do TLS**,
  antes do `Hello`, o que conserta o S2b. Na volta pela trilha, a esperada
  passa a ser a impressão guardada, o que conserta o terceiro defeito do link e
  o S3.

---

## Achar a pessoa

### Consertos compartilhados com o link persistente (etapa 0)

- `Convocacao` recebe as marcas prontas `{aviso, escuta, servidor}`:
  - servidor guardado: `fp16`, `fp16+e` e `fp16+s`;
  - sala pessoal: as marcas do código.

  Como a marca da escuta difere da marca do aviso, o eco do próprio `MORO`
  nunca passa no filtro.
- O primeiro registro sai **na subida**.
- A consulta do cliente resolve o ponto com `Bilhete::ponto()`, que aplica a
  porta 8384. `PONTO_PADRAO` sai do `seele-server` para o `seele-proto`, para
  o `seele-core` alcançá-lo sem violar o ADR 0002.
- A consulta sai em **3 tentativas em ~1,5 s**, e uma conferência `ONDE` corre
  em paralelo para separar «ponto fora do ar» de «pessoa não está».
- `impl Drop for Encontro` aborta a tarefa. Um teste larga uma `Hospedagem`
  sem `encerrar` e religa na mesma porta.

### A sala sempre se registra

A sala pessoal só é achável pelo quarto. Por isso ela ignora a condição de não
ter IPv4 global e registra sempre.

- No UPnP, a sala usa porta canônica `None`: vai direto a qualquer porta,
  **nunca recua para a 8383**, e o aviso de porta só fala da própria âncora.
- A descrição da regra passa a ser por papel: «SEELE servidor» e «SEELE sala».
- `fechar` passa a conferir o cliente interno antes de apagar.

### SEELE-ENC/2: o registro assinado

- **Pedido `MORO2`:**
  - `marca`;
  - `chave` (32 bytes);
  - `carimbo` (segundos, u64);
  - até 2 `endereços locais`;
  - `assinatura` (64 bytes) sobre `"SEELE-ENC/2 moro\0" ‖ marca ‖ chave ‖ carimbo ‖ endereços`.
- **O que o ponto confere:**
  - que a marca é a derivada da chave (`p` + código da chave + sufixo);
  - que |carimbo − agora| ≤ 300 s;
  - que a assinatura vale.
- **Regras do quarto para marcas assinadas:**
  - o registro assinado vence o não assinado;
  - para a mesma chave, vence o carimbo mais novo, então uma troca de IP vale
    em até 15 s, sem o bloqueio de 60 s de hoje;
  - ninguém mais consegue ocupar a marca.
- **`QUEM2`** devolve o endereço público visto e as dicas de rede local. As
  dicas entram na corrida de candidatos (ADR 0037), e é isso que permite duas
  pessoas se ligarem na mesma casa sem hairpin no roteador.
- **Tamanho:** até 512 bytes por datagrama. A MAGIA distingue a versão, e o
  ponto fala as versões 1 e 2 ao mesmo tempo. Os servidores guardados continuam
  na versão 1 por enquanto.
- **Operação do ponto:** limite de taxa por `/64` (e por `/32` no IPv4) e o
  teto de marcas acima de 4096. Hoje, cada app aberto ocupa 2 marcas. Isso é
  **reimplantação na VPS, que é do dono**.

---

## A campainha

Módulo `crates/seele-server/src/campainha.rs`. O tempo entra por parâmetro, no
molde de `taxa.rs`. Só existe numa hospedagem marcada como sala pessoal.

| parâmetro | valor |
|---|---|
| prazo do toque | 45 s |
| toques pendentes ao mesmo tempo | até 3; além disso, `RateLimited` («muita gente ligando agora; tente de novo») |
| teto de pessoas na ligação | 6, contando quem recebe (`voice_room_limit` e `ServerFull` na entrada) |
| espera depois de uma recusa, por chave | 60 s (`RateLimited`) |
| tolerância de reconexão de quem já foi atendido | 60 s, sem tocar de novo |

- **Quem recebe é sempre admitido na própria sala**, e fica semeada **antes** do laço de aceitação.
  Isso fecha a janela em que a primeira conta vira Comandante e em que o
  servidor fica aberto enquanto a escada sobe.
- **Deduplicação:** a mesma impressão na fase do toque duas vezes faz a mais
  nova substituir a mais velha, que é fechada. Os N candidatos da corrida não
  viram N toques.
- **Bloqueados:** a lista vem da agenda do app na subida e é atualizada ao
  vivo. Quem está bloqueado recebe `ChamadaRecusada` na hora, sem tocar.
- **Aprovação por ligação.** Tudo se apaga quando quem recebe desliga ou quando
  a sala esvazia.
- **Empurrão:** a `Hospedagem` expõe um `broadcast` de `Tocou { impressao,
  codigo, apelido }` e de `ParouDeTocar { impressao, motivo }`. O app assina o
  canal numa tarefa Rust.

### O aperto de mão em três fases

Em `session.rs`, só na sala pessoal:

1. **`Hello` → `Challenge` → `Response`**, em 10 s como hoje, com o S2.
2. **O toque.** O servidor manda `ServerMessage::Tocando { prazo_s: 45 }` e
   espera em `select!` pela decisão da campainha, pelo prazo ou por
   `connection.closed()`. **A trava do banco não é segurada** nessa espera.
   - Decisão de atender: segue para a fase 3.
   - Recusa: `Disconnecting { ChamadaRecusada }`.
   - Prazo vencido: `Disconnecting { NinguemAtendeu }`.
   - Conexão fechada: `ParouDeTocar(cancelada)`.
3. **Conta e sessão**, em 10 s.

---

## Protocolo: o que entra na subida da 1.0

Tudo apensado **no fim** das listas, com guarda de ordinal para os enums
aninhados, que hoje não têm guarda nenhum:

- S2 (acima), com o nonce de `Challenge` fixo em 32 bytes;
- `ServerMessage::Tocando { prazo_s: u16 }`;
- `DisconnectReason::ChamadaEncerrada`, `NinguemAtendeu` e `ChamadaRecusada`.
  As três são **terminais** no cliente (`enlace.rs:289-294`), e o `match` do
  teste em `enlace.rs:5120-5180` fica vermelho sozinho se uma delas cair como
  recuperável.

Reaproveitado sem mudança: `ServerFull` (existe e nunca foi emitido), e
`RateLimited`. O ponto de encontro ganha o SEELE-ENC/2 (acima), que é outro
protocolo.

---

## Do lado de quem liga

1. **Leitura** por `ler_entrada`. Código com verificador errado: «ESSE CÓDIGO
   TEM UM ERRO DE DIGITAÇÃO». O próprio código: «ESSE É O SEU CÓDIGO».
2. **Já numa sessão:** passa por `pedirTrocaDeServidor`, com a frase «ligar
   sai de Y». O servidor que a pessoa hospeda continua no ar (sub-projeto 3).
   O alvo canônico é `@<código>`, que se basta sozinho e sobrevive ao `ejetar`,
   que zera `Session.convite`.
3. **LOCALIZANDO**: uma etapa nova da `Chegada`, que aparece na trilha e na
   tela. É onde saem o `QUEM2` das duas marcas e a conferência `ONDE`.
4. **Corrida** de candidatos: dicas de rede local mais o endereço público, com
   `LEVE` para a escuta, usando a marca do aviso. `Esperada::Codigo` em cada
   candidato.
5. **`Tocando`**: o candidato vence, os outros são cancelados, e o prazo passa
   a ser `prazo_s + 5 s`. A FFI emite o progresso e a casca mostra «CHAMANDO
   ALICE…» com CANCELAR. Um comando novo, `cancelar_ligacao`, aborta a tarefa
   e fecha o QUIC.
6. **Na ligação**, a bateria de reconexão é limitada a 60 s e volta ao
   `IP:porta` que atendeu. Se o NAT de quem recebe mudar no meio da ligação, a
   reconexão falha e aparece «A LIGAÇÃO CAIU»: limite conhecido da 1.0.

---

## Do lado de quem recebe

### Hospedagens (compartilhado com o sub-projeto 3)

- `Session.hospedagens: Mutex<Hospedagens>`, com:
  - `Hospedagens { guardado: Option<Hospedado>, pessoal: EstadoDaSala }`;
  - `EstadoDaSala { Desligada, Subindo, NoAr(Hospedado), Falhou(motivo) }`.
- `onde_estou() -> Nenhum | Guardado | Pessoal | Outro` passa a ser a
  **única** resposta a "estou no meu servidor?". Hoje são três critérios
  diferentes.
- `disconnect` desmonta **só o cliente**. Um comando novo,
  `parar_de_hospedar(qual)`, desce a hospedagem escolhida.
- **A sala sobe em segundo plano no `setup`**, porque a escada custa até ~4 s.
  O estado chega à casca por evento.
- **Saída do app:** `prevent_exit`, depois `encerrar` nas duas hospedagens (a
  escada desce e os convidados recebem `ChamadaEncerrada`), e só então `exit`.
  A confirmação ao fechar vale só para o servidor guardado.
- **"Não receber ligações"**, no PERFIL, desliga a sala pessoal inteira. Sem
  registro no quarto, quem liga lê «NÃO ESTÁ DISPONÍVEL».
- **A porta 8385 ocupada**, por exemplo por outra instância, vira
  `Falhou(PortaOcupada)` com frase na linha de estado. A instância única é do
  sub-projeto da URL clicável.

### O toque

- Uma tarefa Rust assina o canal da campainha. A cada `Tocou`, ela:
  - emite `seele://toque { apelido, codigo, nome_na_agenda }`;
  - chama `request_user_attention`: `Informational` repetido no macOS e
    `Critical` no Windows, cancelado ao atender ou ao desistirem;
  - inicia o som.
- **Nunca** chama `set_focus` nem `unminimize` por conta própria.
- **Som:** o `seele-audio` ganha um abridor só de saída. Um tom sintetizado
  toca em laço na saída escolhida no SEELE, sem arquivo, sem mudar a CSP e sem
  abrir o microfone. Ele para em `ParouDeTocar`, ao atender ou ao recusar.
- **Faixa `#toque`** (`ui/camada-toque.js` e `.css`), ao lado de
  `#portaria-batendo`, com o mesmo contrato: sem `role=alert`, sem
  `autofocus`, **nunca** `focus()`, e anúncio uma vez por aparição.
  - Texto: «"Rafa" · K9P2-… está ligando», com o nome da agenda quando
    existir. O apelido aparece entre aspas, porque é o que a pessoa diz, e só
    o código é prova.
  - Botões ATENDER e RECUSAR. Se `onde_estou` for `Outro`: «atender sai de Y».
  - A faixa é **núcleo de confiança** (Parte II): nenhuma região de MOD a
    cobre, e nenhum evento de MOD fica sabendo que ela apareceu.

### Atender, nesta ordem

1. Decidir na campainha que a ligação foi atendida.
2. Se `onde_estou` já for `Pessoal`, parar aqui: é o grupo crescendo.
3. Senão:
   - parar qualquer espera em curso e aguardar um `conectar` que esteja em voo
     (a corrida de hoje em `main.rs:898`/`:1198`);
   - encerrar o ambiente dos MODs;
   - desmontar só o cliente;
   - `conectar("127.0.0.1:8385")` como dono do código;
   - `enter_voice_room` na sala única.
4. Se quem atendeu não conseguir entrar: «VOCÊ ATENDEU, MAS NÃO CONSEGUIU ENTRAR NA
   SUA SALA», com TENTAR DE NOVO. Na segunda falha, `ChamadaEncerrada` para
   todos, e ninguém fica sozinho lá dentro.

### Desligar

DESLIGAR, ou quem recebe sair da sala, manda `ChamadaEncerrada` a todos e apaga as
aprovações. Um convidado que sai não derruba os outros.

### "SEU CÓDIGO" e a agenda

- **PERFIL** ganha o bloco «SEU CÓDIGO», com `<input readonly>` e COPIAR, e a
  chave «não receber ligações». Tudo vem do comando `meu_codigo`, ao lado de
  `impressao_desta_maquina`.
- **Agenda** (`seele-core/src/agenda.rs`), chaveada pelo código:
  - nome que eu dei;
  - apelido visto;
  - `visto_em`;
  - direção: liguei, recebi ou perdida;
  - bloqueado.

  A agenda é conveniência. A chave inteira mora no arquivo de pins, que é
  segurança. A agenda não usa `conhecidos`, que é chaveado por endereço e se
  declara apagável.

---

## Erros e frases

**Quem liga:**

| situação | como o produto sabe | frase |
|---|---|---|
| verificador errado | leitura local | «ESSE CÓDIGO TEM UM ERRO DE DIGITAÇÃO» |
| o próprio código | comparação com a chave desta máquina | «ESSE É O SEU CÓDIGO» |
| ponto calado depois de 3 tentativas | `ONDE` também calou | «O PONTO DE ENCONTRO NÃO RESPONDEU: sem ele, código não funciona; link com endereço continua funcionando» |
| ponto responde, marca desconhecida | quarto vazio | «ALICE NÃO ESTÁ DISPONÍVEL: o SEELE do outro lado está fechado ou não está recebendo ligações» |
| no ar, mas nenhum candidato responde | quarto respondeu e a corrida falhou | «ALICE ESTÁ NO AR, MAS O CAMINHO NÃO ABRIU», com o degrau na trilha |
| chave ≠ código | `ConfereCodigo` | «QUEM ATENDEU NÃO É O DONO DESTE CÓDIGO». Nada foi enviado |
| chave ≠ pino `@CÓDIGO` | pino | «ALGUÉM TENTOU SE PASSAR POR ALICE» |
| versões incompatíveis | recusa legível da 1.0 | «ALICE USA UMA VERSÃO QUE NÃO CONVERSA COM A SUA» |
| `Tocando` | — | «CHAMANDO ALICE…» e CANCELAR |
| `NinguemAtendeu` | — | «ALICE NÃO ATENDEU» |
| `ChamadaRecusada`, inclusive quando bloqueado | — | «ALICE RECUSOU» |
| `RateLimited` depois de uma recusa | — | «ESPERE UM POUCO PARA LIGAR DE NOVO PARA ALICE» |
| `ServerFull` | — | «A LIGAÇÃO DE ALICE ESTÁ CHEIA (6 PESSOAS)» |
| `ChamadaEncerrada` | razão terminal | «ALICE ENCERROU A LIGAÇÃO» |
| queda no meio da ligação | bateria de reconexão | faixa de reconexão por até 60 s, depois «A LIGAÇÃO CAIU» |

**Quem recebe** (na linha de estado e no PERFIL):

| situação | frase |
|---|---|
| a porta 8385 está em uso | «NINGUÉM CONSEGUE TE LIGAR AGORA: a porta 8385 está em uso» |
| nenhum degrau de alcance funcionou | «SEU CÓDIGO NÃO ESTÁ ACHÁVEL AGORA», e o motivo |
| "não receber ligações" ligado | «VOCÊ NÃO ESTÁ RECEBENDO LIGAÇÕES» |
| atendeu e não entrou | «VOCÊ ATENDEU, MAS NÃO CONSEGUIU ENTRAR NA SUA SALA» |
| chamada perdida | marca na agenda |

**Diagnóstico.** Cada transição da campainha vai ao log com o prefixo do
código. Os contadores de toques recebidos, recusados por taxa e perdidos entram
no COPIAR DIAGNÓSTICO da Fase 1.

---

## Privacidade: o que fica escrito, na tela e na página do ponto

- Quem tem o seu código consegue ver, sem ligar, **se você está com o SEELE
  aberto e qual é o seu IP público**. As dicas de rede local também vão. É a
  natureza de ser encontrável. A alavanca é «não receber ligações».
- O ponto de encontro aprende quem está online, em qual IP, e quem procura
  quem. Isso vai para a página de privacidade (LGPD) que a análise já pedia.
- **Quem recebe hospeda a ligação**, então não há terceiro no meio da mídia.
  Todo participante é alguém que quem recebe aceitou.

---

## Parte II · MODs na 1.0: a tela inteira como superfície

A origem é a proposta [«API de MODs: a tela inteira como
superfície»](https://claude.ai/artifact/SSCqCwFAQ6PUF2mXhZxG24), de 28/09/2026,
com as decisões de 29/09. Com elas tomadas, a proposta vira o **ADR 0056**.
Esta parte é a «migração dos MODs» que a 1.0 exige.

### Decisões do dono (29/09/2026)

| pergunta | resposta |
|---|---|
| Núcleo de confiança | os 5 itens da proposta, **mais a ligação e a conexão segura**. O indicador de quem está falando fica fora |
| `decorar` | **fase própria, depois da 1.0**: entra como API 7, somada. A API 6 congela sem ele |
| Onde o banner (`pessoa.faixa`) aparece | **nos dois lugares**: no cartão da chamada e no fundo da barra do operador, além do cabeçalho do perfil |
| Ordem das fases | **diagnóstico primeiro**, numa 0.15.x, sem API nova. Depois a medição de custo, e só então a API 6 |

### O ponto de partida, conferido no HEAD

- **A API é a 5**, e o app aceita `[5, 4, 3]` (`crates/seele-proto/src/mods.rs:77`).
  Um MOD roda no QuickJS nativo (`apps/seele-app/src/executor.rs`), sem acesso
  ao documento, e desenha por declaração: o produto monta tudo o que aparece.
- **Os 11 pontos de contribuição** estão em
  `apps/seele-app/ui/mods-contribuicoes.js:76-88`, só nos modos `adicionar` e
  `substituir`.
- **`decorar` está suspenso** desde 20/09 (ADR 0052), preso pelo guarda
  `decorar_nao_volta_a_tabela_sem_quem_o_aplique`
  (`apps/seele-app/tests/frontend.rs:13844`).
- **O executor nativo não define `console`.** `executor.rs` registra só `seele`
  e globais internas, então o `console.warn` de um MOD vira `ReferenceError`
  justamente no caminho de erro.
- **Falha de mídia vai só para o MOD**, e nada chega ao `seele.log`.
- **O som de MOD** (papel `som`, API 5) provavelmente não toca no app, porque a
  CSP bloqueia `data:` em `media-src`. Isso foi medido no Chromium com a CSP
  exata, e falta medir no WKWebView.
- **Não há guarda de congelamento da API.** O mapeamento de 23/09 achou
  `api/v3.json` e `api/v4.json` editados depois de publicados.
- **A API v1 prometeu 21 momentos, e só 5 são entregues**
  (`crates/seele-server/src/mods/despacho.rs:463-507`).

### O mapa de regiões da API 6

Toda área da janela vira uma **região nomeada e estável**. O MOD continua
declarando o que quer ver, e o produto continua montando. Muda o alcance: da
lista de convites para a janela inteira. Com `decorar` adiado, a API 6 tem só
`substituir` e `adicionar`.

| região | área | modos na API 6 | o que continua sendo do produto |
|---|---|---|---|
| `redes` | coluna da trilha de servidores | adicionar | — |
| `servidor.navegacao` | salas, canais e entradas | substituir, adicionar | — |
| `operador.barra` | barra de quem está conectado | adicionar | sair, microfone e «no ar» |
| `canal.cabecalho` | topo do canal | substituir, adicionar | — |
| `conversa.mensagem` | cada mensagem | substituir, adicionar, por mensagem | o autor continua sendo a chave |
| `compositor` | caixa de escrever | adicionar | — |
| `pessoas.lista` | faixa de pessoas, com `pessoa.cartao`, `pessoa.avatar` e `pessoa.faixa` | substituir, adicionar | o menu de moderação |
| `status.barra` | barra inferior | adicionar | atraso, jitter e codec |

Quatro camadas ficam sobre a tela:

| camada | o que é |
|---|---|
| `chamada.palco` | camada sobre a chamada, com a grade de vídeo do produto por baixo. Modos `substituir` e `adicionar` |
| `dialogo` | modal. A saída é do produto. Ganha **formulários declarados**: campos, validação e botões primário e secundário |
| `folha` (nova) | coluna lateral que empilha, para edição e detalhes |
| `aviso` | fila de quatro, como hoje |

Os 11 pontos de hoje **continuam com o mesmo nome** e passam a morar dentro
das regiões: os cinco `pessoa.*` em `pessoas.lista`, `canal.item` e
`sala.acoes` em `servidor.navegacao`, e `compositor.ferramentas` em
`compositor`. `canal.cabecalho` e `servidor.navegacao` já são regiões, e
`servidor.aparencia` continua como está. Nenhum MOD publicado quebra.

**`pessoa.faixa`**: o banner de uma pessoa por id, como `pessoa.avatar`. Ele é
aplicado ao cartão na chamada, ao fundo da barra do operador e ao cabeçalho do
perfil. O PERFIS 3.4.0 é quem leva o banner a esses lugares.

**Eventos da tela**: o MOD assina mudanças de sala, de canal, de quem fala e de
quem entra, em vez de consultar o snapshot em laço.

### O que não muda

Três decisões medidas continuam valendo, e a tela personalizável depende delas
para valer só dentro do servidor:

- **O MOD não volta para a janela** (ADR 0049). JavaScript de terceiro na página
  principal alcança o Tauri, deixa `setInterval` e CSS vivos depois de sair, e
  escreve tema no documento inteiro. Personalizar tudo não exige acesso ao
  DOM: exige regiões que cubram tudo.
- **Sem HTML livre e sem seletor de CSS.** A declaração é montada pelo produto,
  com `textContent` e `createElement`. Uma região é endereçada por nome, que é
  um contrato versionado. Um seletor é um contrato que ninguém escreveu, e
  quebra todo MOD no dia em que uma classe muda (ADR 0052).
- **Identidade antes de apresentação.** Trocar o que se desenha nunca troca a
  chave: o PERFIS pode escrever outro nome no cartão, e a moderação continua
  agindo sobre a pessoa certa.

### O núcleo de confiança

São sete itens, cada um com o motivo escrito. **Nenhum deles mora dentro de
uma região, nem aceita contribuição.** Fora deles, a tela é do MOD.

| item | por que é do produto |
|---|---|
| **Sair**: do servidor, de uma página, de um diálogo | o texto e o comando precisam funcionar mesmo com um MOD quebrado |
| **Microfone e «no ar»** | quem está transmitindo precisa saber, sem depender de um MOD |
| **Moderação por identidade** | o menu age sobre a pessoa pela chave, qualquer que seja o nome desenhado |
| **Atribuição** | cada superfície diz de qual MOD ela é. Uma tela de MOD igual às do SEELE tornaria uma tela de confiança falsificável |
| **Aceite** | a tela que pede o «sim» para o conjunto de MODs de um servidor |
| **A ligação** (Parte I) | a faixa «está ligando», com o código de quem liga, ATENDER e RECUSAR; a espera CHAMANDO e CANCELAR; o SEU CÓDIGO; «não receber ligações»; e a faixa de quem bate à porta. Um MOD de servidor que desenhasse um toque falso levaria a pessoa a atender o que não devia |
| **Conexão segura** | o veredito de confiança: PRIMEIRO CONTATO, «o convite confirmou», CONEXÃO SEGURA e «QUEM ATENDEU NÃO É O DONO DESTE CÓDIGO». Um MOD que o pinte falsifica uma tela de confiança |

Por decisão de 29/09, o indicador de quem está falando **não** entra no núcleo.
Ele é região, e um MOD pode redesenhá-lo.

### Onde as duas partes se tocam

- **Os eventos da tela nunca carregam a ligação.** O MOD do servidor em que a
  pessoa está não fica sabendo que ela recebe uma ligação, de quem, nem qual é
  o código de ninguém. Toque, chamada, código e agenda não entram nos eventos
  nem no snapshot que um MOD lê.
- **Na sala pessoal não há MODs** (`mods_dir: None`). `chamada.palco` e as
  outras regiões não valem lá na 1.0, e a tela da ligação é só do produto. Ao
  atender, o ambiente de MODs do servidor visitado é encerrado (Parte I,
  «Atender, nesta ordem»).
- **As faixas de toque e de batida ficam fora das telas por construção**, então
  nenhuma região as cobre. Um guarda prova isso (ver «Testes»).

### Diagnóstico para quem escreve MOD (fase 1)

Quanto mais tela o MOD alcança, mais caro fica ele falhar em silêncio. Em
23/09, descobrir que o avatar do PERFIS carregava levou uma hora de medição e
um reinício com `RUST_LOG=debug`. Isto entra antes das regiões, sem API nova,
numa 0.15.x junto com a etapa 0:

1. **`console` no executor nativo**, escrevendo no `seele.log` com o id do MOD e
   o nível.
2. **Falhas de mídia e de montagem no log.** Imagem recusada, declaração com nós
   demais e contribuição recusada passam a sair como WARN, com o motivo.
3. **«Quem pinta esta região»** na gestão de MODs: o MOD que vale, os que
   perderam a disputa, a preferência de quem administra e o estado da mídia
   (carregando, pronta, recusada).
4. **Modo de desenvolvedor**: um contorno com o nome de cada região sobre a tela,
   ligado nas configurações.
5. **O som de MOD.** Primeiro medir no WKWebView. Se estiver bloqueado, tocar os
   bytes do som por WebAudio (`decodeAudioData` sobre os bytes entregues por
   `invoke`), que não passa por `media-src`. **A CSP não afrouxa**, e o guarda
   «`media-src` sem `data:`» continua valendo.

### Medir o custo antes da API 6

O ADR 0052 pediu uma medição de custo sob carga, e ninguém a fez. Com a tela
inteira aberta a MODs, ela deixa de ser opcional. Antes da API 6: N MODs
declarando em todas as regiões ao mesmo tempo, medindo CPU, memória e tempo de
quadro da janela, com os números escritos no ADR 0056.

### Congelar a API de MODs junto com o protocolo

- **A API 6 entra somada.** As aceitas passam a `[6, 5, 4, 3]`, e os pacotes 3,
  4 e 5 continuam funcionando. O ESTILO e o MESA (API 4) não precisam de nada; o
  PERFIS vai para a 3.4.0 na API 6, pelo banner.
- **App primeiro, pacote depois**, pela ordem do runbook do indexador. Desde a
  v0.14.2, a tela de MODs escolhe a versão do pacote pela API, então quem não
  atualizou recebe a versão anterior em vez de uma falha.
- **Guarda de congelamento.** Uma lista com o SHA-256 de cada `api/vN.json`
  publicado. Editar um reprova o build.
- **`api/v6.json` só promete o que é entregue.** O `check-api` passa a cobrir os
  momentos e os eventos: cada um listado precisa ter quem o despache. Os 16
  momentos da v1 que nunca foram entregues não entram na v6.
- **O nome de uma região publicada não muda.** Ele fica congelado em
  `api/v6.json`, como os de hoje.
- **`decorar` é a API 7**, depois da 1.0, somada ao conjunto. O guarda
  `decorar_nao_volta_a_tabela_sem_quem_o_aplique` só sai quando houver quem
  aplique, com o contrato de estilo testado contra o núcleo de confiança.

---

## Fora do escopo da 1.0

- Notificação do sistema operacional, fica para a 1.1. Na 1.0 o toque é
  faixa, atenção da janela e som.
- Ligação que sobrevive a uma troca de rede de quem recebe.
- MODs na sala pessoal: ela sobe com `mods_dir: None`.
- Gravação e E2EE. Quem recebe hospeda a mídia e é participante, então a ligação já
  é "só entre nós".
- Passar a sala para outro participante quando quem recebe sai.
- Mais de 6 pessoas.
- Toque segurado para os servidores guardados, que continuam com a portaria do
  ADR 0030.
- Código que carrega um ponto de encontro próprio. Com `$SEELE_ENCONTRO`, as
  duas pontas precisam do mesmo ponto.
- Trocar ou revogar o código, que exige identidade nova.
- `decorar`, que vira a API 7, depois da 1.0.
- O indicador de quem está falando no núcleo de confiança (decisão de 29/09).
- MOD dentro da janela, HTML livre ou seletor de CSS: continuam recusados
  (ADRs 0049 e 0052).

---

## Testes

Cada guarda só conta depois de ser provado revertendo o conserto e vendo o
teste ficar vermelho.

1. **`Codigo`**: vetores congelados.
   - A derivação é estável.
   - O verificador pega toda troca de um símbolo e toda inversão de vizinhos,
     exceto a do par 0 e 31, e o teste afirma essa exceção explicitamente.
   - A leitura normaliza.
   - `ler_entrada` classifica endereço, nome, convite, código e código com
     erro.
2. **Certificado da sala**:
   - a mesma semente dá o mesmo DER;
   - a chave do certificado é igual à `verifying_key` da identidade;
   - **num aperto de mão QUIC real com Ed25519**, o código certo passa, e o
     errado falha com `CodigoNaoBate` sem que o servidor veja `accept_bi`.
     Ed25519 nunca rodou no TLS deste repositório, então este teste vem
     primeiro.
3. **S2**:
   - o cliente recusa nonce que não tenha 32 bytes;
   - o cliente nunca produz uma assinatura válida sobre um conteúdo de
     CertificateVerify do TLS 1.3;
   - uma assinatura repassada de um servidor para outro não confere.
4. **Quarto**:
   - o ponto é montado como o link o carrega, sem porta;
   - a escada real vai até `onde_mora_hoje`, e os dois endereços chegam;
   - no SEELE-ENC/2, quem não tem a chave não ocupa a marca, e um carimbo
     velho é recusado;
   - para a mesma chave, o carimbo novo vence imediatamente.
5. **Campainha**, com o tempo injetado:
   - cada desfecho;
   - a deduplicação;
   - o teto e os 3 toques pendentes;
   - a espera depois de recusar;
   - a tolerância que não toca de novo;
   - quando quem recebe desliga, todos recebem `ChamadaEncerrada`;
   - o bloqueado nunca toca.
6. **Duas hospedagens num processo** (8383 em disco e 8385 em memória): entrar
   e sair de uma não afeta a outra. Um guarda fica vermelho se `disconnect`
   voltar a tocar nas hospedagens.
7. **Cliente**:
   - `Tocando` encerra a corrida e estende o prazo;
   - `cancelar_ligacao` fecha a conexão, e o servidor vê `ParouDeTocar(cancelada)`;
   - as três razões novas não disparam a bateria de reconexão.
8. **Casca**, em `tests/frontend.rs`:
   - a faixa `#toque` nunca chama `focus()`;
   - o toque chega por evento, não por `setInterval`;
   - o apelido é citado como alegação;
   - «atender sai de Y» aparece quando há conexão em outro lugar;
   - `media-src` continua sem `data:`.
9. **Campo**: duas casas, com quem recebe atrás de CGNAT; a mesma casa, sem
   hairpin; uma troca de Wi-Fi de quem recebe fora de ligação; e um grupo de 3.

Da Parte II:

10. **Diagnóstico de MOD**:
    - um MOD que chama `console.warn` escreve no `seele.log` com o id e o
      nível, e o teste reprova com `ReferenceError` se o `console` sair;
    - uma imagem recusada sai como WARN, com o motivo.
11. **Núcleo de confiança**: para cada um dos sete itens, um guarda prova que
    ele não está dentro de nenhuma região e que nenhuma contribuição o cobre ou
    o substitui. As faixas `#toque` e `#portaria-batendo` ficam fora de toda
    região. O menu de moderação age pela chave mesmo com o nome redesenhado.
12. **Eventos que não vazam**: nenhum evento de tela e nenhum campo do snapshot
    que um MOD lê carrega toque, chamada, código ou agenda.
13. **API congelada**:
    - editar um `api/vN.json` publicado reprova o build;
    - todo momento e todo evento listados em `api/v6.json` têm quem os
      despache;
    - um pacote de cada API aceita (3, 4, 5 e 6) monta.
14. **Som de MOD**: um MOD com papel `som` toca no app de verdade (WKWebView e
    WebView2), e a CSP continua sem `data:` em `media-src`.
15. **Custo sob carga**: a medição do ADR 0052 roda e os números entram no ADR
    0056 antes de a API 6 ser publicada.

---

## Ordem de construção

| etapa | o quê | depende de | tamanho (estimativa) |
|---|---|---|---|
| 0 | Consertos da Fase 1 usados aqui: marcas explícitas e os três defeitos do link, `Esperada` com o `fp` no TLS, primeiro registro na subida, `Drop` de `Encontro`. Sai numa 0.15.x | — | M |
| 1 | `Codigo` no `seele-proto`, com vetores | — | P |
| 2 | Hospedagens no app (sub-projeto 3). Pode ser construída antes da etapa 3, mas **só sai junto com ela**: com o seu servidor no ar enquanto você visita outro, a prova de identidade sem o S2 deixa o servidor visitado entrar no seu como você, com papel de Comandante | 0 para construir; 3 para sair | M–G |
| 3 | Protocolo da 1.0 (sub-projeto 4): S2, `Tocando`, as três razões, carimbo negociado, tolerância a mensagem desconhecida, bytes-ouro, API de MODs congelada | 1 | G |
| 4 | Servidor da sala: identidade injetada, campainha, três fases, teto, empurrão, UPnP por papel | 1, 3 | M–G |
| 5 | SEELE-ENC/2 no servidor, no anfitrião e no cliente, **e a reimplantação na VPS** | 1 | M |
| 6 | Quem liga: `ler_entrada`, LOCALIZANDO, `ConfereCodigo`, `Tocando`, CANCELAR | 3, 4, 5 | M–G |
| 7 | Casca: faixa, som pelo Rust, "SEU CÓDIGO", agenda, frases | 4, 6 | M–G |
| 8 | Validação de campo | tudo | — |

Da Parte II:

| etapa | o quê | depende de | tamanho (estimativa) |
|---|---|---|---|
| M1 | Diagnóstico de MOD: `console`, falhas no log, «quem pinta esta região», modo de desenvolvedor, som de MOD. Sai na mesma 0.15.x da etapa 0 | — | M |
| M2 | Guarda de congelamento da API e `check-api` cobrindo momentos e eventos | — | P |
| M3 | Medição de custo sob carga (ADR 0052), com os números no ADR 0056 | M1 | M |
| M4 | API 6: regiões, `pessoa.faixa`, `folha`, `chamada.palco`, formulários no `dialogo`, eventos da tela e o núcleo de confiança como contrato testado. `api/v6.json` congelado e aceitas `[6, 5, 4, 3]` | M2, M3; o item «a ligação» do núcleo depende da etapa 7 | G |
| M5 | MODs oficiais: PERFIS 3.4.0 com o banner na chamada e na barra do operador, publicado depois do app | M4 | M |

A estimativa é inferência, não medida: de 4 a 6 semanas para a sala pessoal,
contando as bases, e de 2 a 4 semanas para a Parte II. As etapas 0 a 3 servem
também aos outros sub-projetos. As duas partes correm em paralelo até se
encontrarem no núcleo de confiança (M4 depois da etapa 7).

## ADRs a escrever

- **A sala pessoal segura a conexão enquanto toca.** Restringe o ADR 0030 à
  sala pessoal e diz por que «NÃO ATENDEU» ali é resposta sobre a qual se age.
- **O código de uma pessoa.** Forma congelada, verificador, marcas, força e
  pino.
- **A identidade é uma por máquina, e não por versão.** Muda o ADR 0017 e o
  ADR 0046.
- **SEELE-ENC/2: o registro assinado no ponto de encontro.** Revê o §2.4 do
  ADR 0047, que recusou o registro assinado quando o link não dependia do
  quarto. Um número de telefone depende.
- **ADR 0056: MODs, a tela inteira como superfície.** É a proposta de 28/09 com
  as decisões de 29/09: mapa de regiões, núcleo de confiança com sete itens,
  `decorar` na API 7, e os números da medição de custo.

## Custo de reverter

- **O formato do código não reverte.** Depois de distribuído, mudar invalida
  todos os códigos. Por isso ele nasce congelado e com vetores.
- **As mensagens do protocolo** seguem a política «1.x nunca quebra o fio».
  Entram na subida da 1.0 e não saem.
- **O SEELE-ENC/2** reverte desligando o `/2` no ponto: os clientes caem para
  "não disponível".
- **Hospedagens, campainha e casca** revertem com `git revert`, com risco de
  casca médio, porque mexem em 7 guardas existentes que prendem «sair derruba».
- **O nome de uma região publicada não reverte**, como o formato do código:
  depois de `api/v6.json` publicado, renomear quebra os MODs. Por isso ele
  nasce congelado.
- **O diagnóstico de MOD** (fase M1) reverte com `git revert` e sem risco para
  os pacotes, porque não muda a API.
