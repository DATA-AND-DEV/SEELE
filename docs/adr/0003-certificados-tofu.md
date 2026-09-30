# 0003 — TOFU como padrão de certificado

Status: aceito por default
Contexto: `specs/08-seguranca.md` deixa em aberto entre TOFU com certificado auto-assinado e ACME/Let's Encrypt. É critério de aceite de M0.
Decisão: TOFU com pinning por padrão. ACME como opção documentada, não como caminho principal. O aviso de troca de chave é um `Alerta · 警告` bloqueante, impossível de ignorar.
Alternativas: ACME por padrão. Descartado porque exige domínio e portas 80/443 disponíveis, o que contradiz a simplicidade de porta UDP única de `specs/01-arquitetura.md` e o perfil de operador descrito em `08` — confiável, mas não especialista em segurança. O modelo do SSH é o que o público-alvo já tem na cabeça.
Consequências: mais fácil — auto-hospedagem sem domínio, sem renovação, sem porta extra. Mais difícil — exige UX explícita de aceite e de troca de chave, e essa UX precisa existir na TUI (M4) e no app (M5), não só no papel. Não há caminho não criptografado em nenhum dos dois modos.

Custo de reverter: **médio**. Adicionar ACME depois é aditivo; tirar TOFU depois quebra clientes que já pinaram.

## Adendo — 2026-09-29 · o pino prova o endereço que a pessoa escolheu

O pino é por endereço: prova que *este* `IP:porta` já apresentou *esta* chave.
Por isso um link que discorda do pino avisa e não recusa: o pino é a prova de
continuidade, e quem discorda dele é o link.

Isso vale no **alvo**, o endereço que a pessoa escolheu (o do link, o
digitado, ou o da entrada da lista), quando ele é de **escopo público**. Não
vale num **candidato que ninguém escolheu**, e a corrida de candidatos do
ADR 0037 tem dois:

- **A resposta do quarto** do ponto de encontro (ADR 0022). O quarto guarda o
  endereço de onde veio o registro. Um ponto hostil, ou alguém que registre a
  marca de outro servidor a partir do socket do próprio servidor, faz o quarto
  apontar para um endereço que esta máquina já fixou com a chave de quem
  atende ali.
- **O alternativo que colide na LAN.** `192.168.x.y:8383` é o mesmo endereço
  de uma casa para outra, e ali atende o servidor de sempre da casa em que a
  pessoa está.

Nos dois, um pino que confere só diz que algum servidor já atendeu naquele
endereço. Deixá-lo passar mandava o `Hello` (o convite, o apelido e a
assinatura) a um servidor que a impressão prometida desmente.

**Num candidato que ninguém escolheu, a impressão prometida vale mais que o
pino.** A que não confere recusa dentro do TLS, antes do `Hello`, mesmo com
pino que confere, e nada é fixado nem desfeito. No alvo de escopo público, a
regra de cima continua. Uma chave trocada continua recusada em todos.

**O endereço de LAN do link é o alvo, e a colisão nele é coberta.** O
primeiro endereço de um link é o da rede de casa do anfitrião, quando ele tem
uma, e é também a chave da entrada que o link deixa na lista de servidores.
Para quem visita pela internet, esse é justamente o endereço que se repete de
uma casa para outra: na rede em que a pessoa está, outro servidor pode atender
nele, já fixado. Num alvo de escopo local (privado, link-local, CGNAT, ULA), a
regra é a do candidato que ninguém escolheu, e a impressão prometida vale mais
que o pino. O porquê: numa LAN, um pino que confere com a esperada discordando
é, no caso comum, colisão, porque um servidor que trocou de chave dá `Changed`,
e não um pino que confere. Deixá-lo passar mandava o `Hello` a outro servidor e
deixava o servidor do amigo inalcançável daquela casa.

A decisão é pelo endereço resolvido, e não pelo texto: um nome como
`casa.local` que resolve para a rede de casa é um alvo de escopo local. O
loopback conta como público, porque é esta máquina em qualquer rede. O preço é
que, num alvo de LAN, um link ou uma entrada da lista que discorda da chave
fixada ali é recusado, em vez de entrar com aviso, também quando não é
colisão: a entrada que a 0.15.0 gravou com a impressão de outro servidor, que
num alvo público se cura na primeira volta, e um link de antes de o servidor
trocar de chave, quando esta máquina já fixou a nova.

A regra de onde o pino prova o servidor fica num lugar só, `build_destino`, em
`crates/seele-ffi/src/lib.rs` (o campo `seele_core::enlace::Destino::o_pino_prova_o_servidor`),
e quem a aplica é `seele_core::tofu::TofuVerifier::decide`. O teste que prende
o candidato que ninguém escolheu é
`um_candidato_que_a_pessoa_nao_escolheu_nao_toma_a_entrada_da_lista`, com o
irmão `um_candidato_que_a_pessoa_nao_escolheu_nao_bate_na_portaria_dele`, em
`crates/seele-conformance/tests/volta_pela_trilha.rs`. Os dois olham o servidor
que ninguém escolheu, e não o cliente: nenhuma conexão de pé, o convite inteiro,
a portaria vazia. O alvo de LAN é preso por
`um_alvo_de_escopo_local_nao_tem_o_pino_que_prova_o_servidor` e
`num_alvo_de_lan_o_pino_que_confere_nao_passa_por_cima_da_esperada`, em
`crates/seele-ffi/src/lib.rs`, e o que não pode ir junto (o alvo público e o
loopback) por `um_alvo_publico_ou_de_loopback_tem_o_pino_que_prova_o_servidor`.
