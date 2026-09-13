# Anúncio e aceite de MODs: o contrato

> Etapa 5 da ordem de entrega do desenho de MODs — «verbos de protocolo,
> anúncio por hash, tela de aceite, "não entra sem"» — na parte que atravessa o
> fio. A instalação, o download e o launcher **não** estão aqui.
>
> Base: ADR 0045, e `docs/superpowers/specs/2026-09-05-mods-design.md`.

## A frase que este documento implementa

ADR 0045, sobre o que quem entra vê:

> Na **primeira vez** que entra num servidor com MODs, a pessoa vê o que vai
> baixar: nome, autor, versão, repositório, e o `reach` declarado — em
> particular se o MOD lê o que ela escreve. Aceitou uma vez, entra direto nas
> próximas. **Um servidor que troca de MOD pergunta de novo.** **Quem não
> aceita, não entra.**

## As três palavras que não são sinônimas

Elas se confundem com facilidade, e confundi-las é o que produz «desabilitei e
continua exigindo» ou «aceitei e ele pergunta de novo».

| palavra | onde mora | o que quer dizer |
|---|---|---|
| **instalado** | disco de quem hospeda, `mods/<autor>/<nome>/` | os bytes estão lá. Não obriga ninguém a nada. |
| **habilitado** | linha `enabled = 1` na tabela `mods` | este servidor **exige** este MOD de quem entra. |
| **aceito** | arquivo `aceites` na máquina de quem entra | esta pessoa leu **um conjunto** e disse sim a ele. |

Instalado e habilitado são de quem hospeda; aceito é de quem entra. Desabilitar
preserva os dados do MOD (ADR 0045) e **muda o conjunto**, portanto invalida os
aceites — tirar é uma mudança tanto quanto pôr.

## A identidade do conjunto

O aceite não é por MOD: é pelo **conjunto**. Um SHA-256 sobre a lista ordenada
de `(id, version, hash)`, com cada tamanho contado antes dos bytes dele — as
mesmas três regras de `content_hash`, pelo mesmo motivo: duas máquinas têm de
chegar ao mesmo número.

`repo`, `reach` e a metade de servidor **não** entram na conta. Eles saem do
`mod.json`, e o `mod.json` está dentro do `hash` de cada MOD: contá-los seria
contar a mesma coisa duas vezes, e deixá-los de fora não abre folga nenhuma.

O que isso compra, e é a peça inteira: **acrescentar, tirar, subir de versão ou
trocar os bytes de qualquer MOD muda a identidade**, e todo aceite guardado
deixa de bater sozinho. Não há lista de invalidação a manter.

## O contrato do fio

Três variantes novas, no fim de cada lista:

```
Servidor → cliente   ModsExigidos  { mods: [ModAnunciado], conjunto: hex }
Cliente  → servidor  AceitarMods   { conjunto: hex }
Cliente  → servidor  RecusarMods
```

`ModAnunciado` carrega **identidade** (`id`, `version`, `hash`) e o que a tela
de aceite precisa dizer antes de qualquer byte ser baixado (`repo`, `reach`,
`no_servidor`).

Três motivos de desconexão novos: `ModsRecusados`, `ModsMudaram`,
`ModsIndisponiveis`.

### Onde o anúncio cai no aperto de mão

```
Cliente                                   Servidor
   │── Hello ────────────────────────────────▶│
   │◀── Challenge ────────────────────────────│
   │── Response ─────────────────────────────▶│
   │                             (assinatura conferida, portaria decidida)
   │◀── ModsExigidos ─────────────────────────│   ← só se houver MOD habilitado
   │── AceitarMods | RecusarMods ────────────▶│
   │◀── Session ──────────────────────────────│   ← só para quem aceitou
```

As duas pontas do lugar são decisões:

- **depois da assinatura e da portaria**, porque a lista de MODs é configuração
  de quem hospeda, e quem varre a internet não tem por que recebê-la só por
  abrir uma conexão;
- **antes da `Session`**, porque a `Session` é o fluxo protegido — é dela que
  saem as salas, os canais, os papéis e as permissões. «Não entra» tem de
  querer dizer «não recebe nada».

Não há prazo próprio: o aperto de mão inteiro já corre dentro do
`HANDSHAKE_TIMEOUT`, e quem não responde encontra `HandshakeTimeout`, que é o
que de fato aconteceu.

**Uma consequência do lugar, dita para não ser descoberta depois:** a conta já
existe quando o anúncio sai, porque a etapa vem depois de `register_or_find`.
Quem recusa deixa o apelido reservado naquele servidor, como quem bate na
portaria e não é decidido já deixa. Mover o anúncio para antes da conta trocaria
isso por outra coisa pior — um servidor contaria a sua lista de MODs a qualquer
um que provasse uma chave gerada na hora.

**Um servidor sem MOD habilitado não manda este quadro e não espera resposta
nenhuma.** É o que mantém todo servidor de hoje trocando exatamente os quadros
que trocava.

### O que o servidor recusa, e em que ordem

1. **Um servidor que não consegue se descrever** — uma linha habilitada com
   hash que não é um hash de conteúdo, ou MODs demais para um quadro de
   controle: ninguém entra (`ModsIndisponiveis`), e o log nomeia a linha.
   Admitir mesmo assim seria exigir na tabela e não exigir no fio, que é a
   instalação parcial silenciosa que o ADR 0029 nomeia como o modo de falha a
   evitar.
2. **Um par que não alcança a versão do anúncio** — `Incompatible`, sem mandar
   o quadro. Mandá-lo mataria o fluxo de controle dele sem uma palavra.
3. **Quem diz não**, e **quem diz sim para outro conjunto** — os dois
   `ModsRecusados`. Um sim dado a outra lista não é um sim a esta.

**E o que ele não recusa enquanto o anúncio não sai.** Os três itens acima
pressupõem que exista alguém capaz de aceitar. Enquanto `PROTOCOL_VERSION` não
alcança `VERSAO_DO_ANUNCIO` não existe, e aí os três viram uma recusa de cem por
cento em troca de nada — inclusive o primeiro, que trancaria o servidor por uma
linha de banco com hash vazio. Nesse estado o portão fica **dormente**: o
servidor admite como admitia antes desta entrega e avisa quem hospeda pelo log.
Ver `mods::anuncio::o_anuncio_alcanca_alguem`, que é onde a decisão mora.

### Mudança durante a sessão

Habilitar ou desabilitar um MOD com gente dentro **acaba com a sessão de quem
aceitou outro conjunto**, com `ModsMudaram`. Inclui quem aceitou o conjunto
vazio — ou seja, todo mundo que entrou num servidor sem MOD antes de o primeiro
ser ligado.

Perguntar no meio da sessão criaria um terceiro estado — dentro, sem ter aceito
o que vale agora — e ninguém sabe o que esse estado enxerga: se enxerga tudo, o
aceite não vale nada; se não enxerga nada, é uma sessão encerrada com outro
nome.

O aviso nasce na tabela, não numa tela: `enable` e `disable` são as duas únicas
funções que a tocam, e as duas avisam. É o que faz a **desabilitação
automática** de um MOD que lançou exceção (ADR 0045, «falha isolada») também
valer para quem está dentro.

## O contrato da interface

O que a casca recebe, e o que ela tem de dizer.

### Ao tentar entrar

`ConnectionError::ModsNaoAceitos { mods, conjunto }` **não é uma falha: é uma
pergunta.** Cada `ModExigido` traz:

| campo | o que a tela faz com ele |
|---|---|
| `id` | `autor/nome`, como a pessoa o encontra no indexador |
| `version` | a versão que o autor declara |
| `hash` | o que se compara a olho com o que o indexador publica |
| `repo` | o repositório público — o link que a pessoa abre para ler o que vai rodar |
| `reach` | o alcance declarado, item por item |
| `no_servidor` | **a linha que não pode faltar** — ver abaixo |

O caminho é um só: a tela mostra, a pessoa aceita, a casca guarda a identidade
em `seele_core::aceites`, e conecta de novo. Um consentimento que não se retira
não é consentimento, então `esquecer` existe e a entrada seguinte volta a
perguntar.

### Os três verbos da janela

Os mesmos três, registrados como comando do Tauri para que a tela tenha o que
chamar quando existir:

| comando | o que faz |
|---|---|
| `aceite_de_mods(alvo)` | o conjunto que esta máquina já aceitou para aquele servidor, ou nada |
| `aceitar_mods(alvo, conjunto)` | guarda o sim; a entrada seguinte naquele servidor passa direto |
| `esquecer_aceite_de_mods(alvo)` | retira o sim; a entrada seguinte volta a perguntar |

`alvo` é o endereço **como a pessoa o digitou** — a forma canônica sob a qual
ele é arquivado é decidida uma vez só, em `seele_ffi::chave_do_servidor`, e é a
mesma que `build_destino` procura na conexão seguinte. A casca não a reproduz, e
é de propósito: errar a chave não dá erro nenhum, só faz o produto perguntar de
novo a quem já respondeu — e é o pin do certificado que mora sob a mesma chave,
então a regra tinha de continuar sendo uma.

**A tela ainda não existe**, e está fora desta etapa junto com o download dos
bytes. O que existe é a superfície, e ela é declarada como espera no
`AGUARDANDO_TELA` de `apps/seele-app/tests/frontend.rs` — a lista que obriga
quem ligar a tela a vir aqui tirar o nome.

### O que `no_servidor` significa, em português de tela

Um MOD com metade de servidor **roda na máquina de quem hospeda** e alcança o
bloco `world` do `api/v1.json`:

- **`buscar`** — abre conexões de saída a partir da máquina de quem hospeda,
  para qualquer endereço `http`/`https`;
- **`agora`** — o relógio daquela máquina;
- **`registrar`** — escreve no log daquela máquina.

É onde a «liberdade total» do ADR 0045 mora, e é a diferença entre um MOD que
repinta uma janela e um que abre conexões a partir da casa de outra pessoa. O
plano do runtime já a nomeava como «a parte que a tela de aceite tem de dizer em
voz alta»; aqui ela chega à interface como um campo, e não como uma frase que a
casca teria de inferir.

**O que o produto não promete:** o `reach` é **declarado pelo autor**, não
medido. Um MOD de servidor alcança o `world` inteiro tendo declarado o que
quiser — a defesa é o repositório público e a revisão de código de cada versão
(ADR 0045), e ela não é técnica. `no_servidor` é o único dos três campos que o
produto sabe por construção, porque ele sai de haver ou não uma metade
`servidor/`.

### Ao ser desconectado

Três motivos novos em `EndReason`, com frase própria em `frases.js`:
`ModsRecusados`, `ModsMudaram`, `ModsIndisponiveis`. Nenhum deles fala em
credencial, e a diferença não é cosmética: o primeiro instinto de quem lê
«credencial recusada» é conferir convite e senha, e aqui os dois estão certos.

### Ao habilitar um MOD

`mods_instalados` passa a devolver `repo`, `reach` e `server` por MOD — os
mesmos três campos, do lado de quem hospeda. A tela de habilitar tem de dizer o
que a de aceitar diria: quem hospeda está decidindo pela sala inteira.

`mods_instalados` também devolve `exigencia_vale_na_rede`, um booleano igual em
toda a lista — não é uma propriedade do MOD, é uma propriedade do momento.
`enabled` diz o que está gravado no banco; este campo diz o que esse registro
**faz** na rede hoje, e as duas coisas podem discordar enquanto
`docs/pendencias.md` #34 estiver aberta: com `PROTOCOL_VERSION = 4` e
`VERSAO_DO_ANUNCIO = 5`, um MOD pode estar `enabled: true` sem que
`exigencia_vale_na_rede` acompanhe — o portão está dormente, e quem entra entra
sem ler nada. A tela de habilitar precisa dizer isso ao lado do interruptor;
sem o campo, ela leria "exigido" onde o produto sabe e não conta que ainda não
tranca ninguém.

## O contrato da integração conjunta com a malha

**`PROTOCOL_VERSION` não subiu nesta entrega, e isso é deliberado.**

O postcard indexa variante por posição. Esta entrega e a da malha acrescentam
variantes ao mesmo par de listas ao mesmo tempo; se cada uma subisse a versão
global por conta própria, as duas chamariam «5» a vocabulários diferentes — que
é exatamente o defeito que o guarda dos ordinais existe para pegar, e que já
custou uma tela preta sem mensagem nenhuma.

Então o anúncio tem uma constante própria, `seele_proto::mods::VERSAO_DO_ANUNCIO
= 5`, e todo envio passa por ela.

**O que a integração conjunta precisa fazer, e é curto:**

1. subir `PROTOCOL_VERSION` para 5 **uma vez**, com as variantes das duas
   entregas já na lista;
2. conferir os ordinais que o guarda `o_ultimo_verbo_de_cada_lista_esta_onde_esta_versao_o_deixou`
   prende — eles mudam quando as duas listas se juntam;
3. deixar `VERSAO_DO_ANUNCIO` em 5, e o gate passa a abrir sozinho.

Nada além disso: o padrão de `ServerConfig::versao_do_anuncio` já é
`VERSAO_DO_ANUNCIO`, então no dia em que a versão global o alcançar o portão
liga sem uma linha a mais. O teste `hoje_o_anuncio_ainda_nao_alcanca_par_nenhum`
falha nesse dia, de propósito: é o ponto em que alguém confere que ligou.

**O que vale enquanto isso não acontece.** O portão fica **dormente**: um
servidor com MOD habilitado admite quem entra, exatamente como admitia antes
desta entrega, e escreve no log de quem hospeda que a exigência ainda não vale
no fio. Uma troca de MOD com gente dentro também não derruba ninguém.

Esta é uma correção do desenho original, e vale dizer por quê. A primeira versão
recusava todo cliente com `Incompatible`, pelo raciocínio de que é o
comportamento certo para quem não sabe ler o anúncio. O raciocínio está certo e
a conclusão não: enquanto **nenhum** par pode aceitar, «recusar quem não
alcança» e «recusar todo mundo» são a mesma coisa, e um portão que ninguém pode
atravessar não protege — só fecha uma casa que funcionava, sem dar a ninguém a
chance de aceitar. Um servidor sem MOD habilitado, que é a maioria, nunca mudou
em nada nas duas versões.

### A costura que deixa o caminho verdadeiro ser testado

`ServerConfig::versao_do_anuncio` é o limiar a partir do qual o anúncio sai, e
existe por uma razão só, com data de validade: sem ele, o caminho verdadeiro —
`seeled` anunciando, `seele_core::Client` aceitando — não roda em lugar nenhum
enquanto as versões não se juntam, e cada metade ficaria provada contra um dublê
da outra.

Baixá-lo é perigoso fora de um teste: mandar o anúncio a um par que não conhece
a variante mata o fluxo de controle dele. Quem o baixa está declarando que as
duas pontas são a mesma build — verdade num teste do workspace, e só nele. O
arranque avisa no log quando o valor está abaixo do padrão, e não há como
configurá-lo de fora: `ServerConfig` não é desserializada de arquivo nenhum.

## O que esta entrega deliberadamente não faz

- **Não baixa MOD nenhum.** O anúncio diz o que conferir; buscar os bytes em
  `mods.seele.app.br` e conferir o hash contra eles é a etapa do download, e
  duplicá-la aqui seria escrever duas vezes a mesma conferência.
- **Não desenha a tela de aceite.** O que ela precisa está na fronteira, campo
  por campo, e o texto é do `frases.js`.
- **Não mexe no launcher nem em multi-versão.** É a etapa 6.
- **Não confere o disco contra a linha da tabela.** O `hash` anunciado é o que
  quem hospeda gravou ao habilitar, calculado sobre os bytes daquele momento.
  Reconferir a pasta é outro guarda, de quem habilita, e ele não existe ainda.
- **Não administra MOD pelo fio.** Habilitar e desabilitar com
  `AdministerServer` continuam sendo de outra etapa; aqui eles são locais, como
  já eram.
