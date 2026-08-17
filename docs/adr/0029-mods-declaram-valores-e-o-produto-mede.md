# ADR 0029 — MODs: declaram valores, e o produto mede antes de aplicar

**Estado:** proposto
**Data:** 2026-08-17

## Contexto

Pedido do dono, e a definição é dele: **um MOD é aparência mais comportamento
declarado.** Temas, palhetas, sons, glifos, textos — e um manifesto que pode
acrescentar comandos, painéis ou atalhos por um **esquema fechado**. Nada de
código arbitrário. Junto vieram quatro restrições, e elas não são sugestões:
**MODs precisam ser de código aberto**; **a confiabilidade é julgada pelo
usuário**, não por nós; **haverá MODs oficiais**, lançados por nós; e **precisa
de um indexador online** que facilite achar e baixar.

Havia uma posição escrita sobre isto, e ela era o contrário desta — em dois
lugares, e o segundo é mais específico que o primeiro:

- `specs/00-visao-geral.md`, em "Não-objetivos — v1.0": *"Bots, webhooks,
  **marketplace de plugins**."*
- `apps/seele-app/ui/index.html:1427`, no Terminal Dogma: *"**TEMA** — o ADR 0014
  congela a paleta. Um segundo tema é uma segunda paleta canônica, e essa é
  decisão de ADR, não de tela."* E isso é cobrado por teste:
  `the_settings_screen_omits_what_the_product_lacks_instead_of_drawing_it_dead`
  (`apps/seele-app/tests/frontend.rs:2405`) reprova se a tela de configurações
  escrever a palavra `TEMA`.

Aquela frase não recusava a ideia: adiava. Este ADR é a decisão que ela estava
esperando, e quando ele valer, a lista daquele teste perde uma entrada.

O que existe hoje, e que condiciona todo o desenho abaixo:

- **A palheta é congelada e as garantias dela são medidas.** ADR 0014 tornou o v2
  canônico, e o argumento mais forte foi de acessibilidade: em v1 o
  `vermelho-alerta` reprovava AA a 4,14:1 — a cor que mais precisa ser legível.
  `apps/seele-app/tests/tokens.rs` refaz aritmética de contraste WCAG em Rust,
  recusa literal de cor em qualquer folha (por listagem de diretório, não por
  lista de nomes), e prova que a varredura não derruba nenhum token abaixo do
  critério **que ele já cumpria** — "os pisos são os critérios que cada token já
  atendia, não um inventado aqui".
- **Os guardas do vermelho não olham para cor nenhuma.** São quatro testes por
  superfície em `frontend.rs` — a caixa de alerta (1435), a moderação (3624), a
  faixa de veredito (845) e as regras de `.convite-alcance` (~3930) — mais
  `alert_and_accent_stay_distinct_in_sixteen_colours` em
  `crates/seele-tui/src/theme.rs:362`. Todos os quatro primeiros fazem a mesma
  pergunta: **esta regra *nomeia* `vermelho`?** Nenhum deles pergunta que cor o
  token guarda. Guardar isto na cabeça é o que faz o resto deste ADR se decidir
  sozinho.
- **A janela é embutida no binário.** `tauri.conf.json` tem
  `frontendDist: "./ui"` e `main.rs:1312` usa `tauri::generate_context!()`: cada
  `.css`, `.js` e fonte entra no executável em tempo de compilação. Não existe
  pasta em disco onde largar um arquivo. E o conjunto é fechado por teste —
  `the_page_loads_only_files_that_are_shipped` (`frontend.rs:281`) exige que todo
  arquivo em `ui/` seja carregado pela página e que todo asset carregado exista.
- **A CSP é restritiva e não tem folga:** `default-src 'self'; style-src 'self';
  script-src 'self'; img-src 'self' data:; font-src 'self'; connect-src ipc:
  http://ipc.localhost`. Sem `unsafe-inline`, sem `unsafe-eval`, sem origem
  remota. O protocolo de assets do Tauri está desligado e não há diretório
  `capabilities/`.
- **A TUI é constante compilada.** `theme.rs` são dez `pub const Ink`, com campos
  privados, sem construtor e sem `Deserialize`; o crate inteiro não tem uma
  chamada de leitura de arquivo. A degradação para 256 e para 16 cores é
  **escolhida, não calculada** — "arredondar uma palheta cuidadosa em tempo de
  execução é como ela vira lama por SSH" —, e o mapa para 16 cores é semântico e
  respeita a exclusividade do vermelho à mão.
- **Cor nunca anda sozinha, e não é o token que garante isso.** `theme.rs` tem
  `sync_mark()` devolvendo `█ ▒ ░` em toda palheta, `alert()` virando
  `REVERSED | BOLD` em monocromático, e a marca de bloco mais o número acompanham
  toda Taxa de Sincronização. `specs/05-cliente-tui.md:143` é onde a regra mora:
  *"Nenhuma informação transmitida **só** por cor"*.
- **`acessibilidade.css` corrige por token, e é a última folha da lista.** Sob
  `prefers-contrast: more` ela troca `osso-apagado` por `osso` em rótulo, título
  de painel, coordenada e valor ausente. Ela vence por ordem de fonte, com a
  mesma especificidade, e há teste que reprova se qualquer folha que pinta for
  carregada depois dela.
- **Frases são duas tabelas paralelas, e i18n não existe.** `ui/frases.js` e
  `crates/seele-tui/src/text.rs` dizem as mesmas coisas em pt-BR, em duas cascas,
  e o `text.rs` diz que o `match` chapado é de propósito. Não há
  `crates/seele-i18n`, não há `.ftl`. O ADR 0012 decidiu a **fronteira** e adiou
  o catálogo: *"A tabela de tradução nasce em M4. Até lá não há o que traduzir."*
- **Sons não existem.** O `seele-audio` é captura, reprodução, codec e mixagem. O
  produto é silencioso fora da voz, e `index.html:1433` registra: *"**AVISO
  SONORO** … não tem nada atrás."*
- **Glifos são desenho, não caractere.** `ui/glifos.js` é geometria SVG escolhida
  contra as métricas da IBM Plex Mono embarcada, porque ▸ ◂ ▼ ▶ ● ○ ⌘ não estão
  nela e cairiam numa segunda face no meio de uma grade de 8×16. No terminal, os
  mesmos papéis são caracteres de verdade.
- **O disco por máquina já tem um formato legível de propósito.** `$SEELE_HOME`,
  ou `$XDG_CONFIG_HOME/seele`, ou `~/.config/seele` (ADR 0017), com
  `identity.key`, `pins`, `conhecidos` e `preferences`. O `pins` é texto puro
  porque *"quem foi avisado de que a chave do servidor mudou precisa conseguir
  abrir o arquivo e comparar a olho"*.
- **O produto não tem licença.** `README.md:388`: *"Ainda não definida. A postura
  de direitos sobre o vocabulário de Evangelion está em aberto, e inventar uma
  licença antes dessa decisão seria pior que deixar em branco."* Isso importa
  para a exigência de código aberto, e está tratado abaixo.

E há uma frase de `specs/07-tema-evangelion.md:3` que este ADR tem de encarar de
frente, porque é a tese contrária: *"O tema é **vocabulário de produto**, não
skin. É definido aqui uma vez e aplicado em todo lugar."*

## Decisão

**Um MOD é um arquivo de valores. Ele nunca escreve um seletor, nunca traz
código, e o produto mede o que ele declara antes de aplicar — por valor, com os
pisos que a palheta congelada já cumpre.**

### "Valores, nunca seletor" é a decisão inteira

Tudo o mais aqui é consequência disto, então vale gastar o parágrafo.

Os guardas do vermelho perguntam se uma **regra** nomeia `--seele-vermelho-alerta`.
O teste de literal pergunta se uma **folha** escreve `#`. O teste de contraste da
varredura lê a opacidade que a **folha declara**. Todos os três continuam
verdadeiros com um MOD instalado, porque um MOD não é uma folha e não tem
seletor: ele é uma lista de pares nome→valor, e a aplicação é
`document.documentElement.style.setProperty("--seele-negro-painel", …)` — escrita
por CSSOM, que a CSP não cobre, sobre a lista fechada de nomes que `tokens.css`
já declara.

Isso dá quatro propriedades de uma vez, e nenhuma delas custa código novo:

1. **A CSP não afrouxa.** Nada de `unsafe-inline`, nada de folha externa, nada de
   protocolo de asset. Se aplicar um MOD exigisse mexer nessa linha, a resposta
   seria não — e fica escrito aqui como critério, não como coincidência.
2. **`the_page_loads_only_files_that_are_shipped` continua valendo**, porque
   nenhum arquivo entra em `ui/`.
3. **Os guardas do vermelho continuam valendo**, e não por sorte: um MOD que
   pudesse escrever um seletor derrubaria os quatro de uma vez, sem que nenhuma
   cor estivesse "errada". `.banner { color: var(--seele-fosforo) }` é uma queda
   de enlace pintada de verde, e nenhuma conferência de cor no mundo pega isso.
4. **`acessibilidade.css` não é modificável**, porque ela é folha e MOD não é
   folha. Ela é o piso, não o tema. O que ela corrige, ela corrige por cima de
   qualquer palheta em vigor.

E dá o motivo da recusa da alternativa 2, antes mesmo de chegar lá: a diferença
entre "declara valores" e "roda código" não é de quantidade de poder. É que a
primeira mantém **todas** as garantias que este repositório já escreveu, e a
segunda revoga todas elas de uma vez.

### O que um MOD pode repintar, e o que continua nosso

As catorze cores de interface se separam em três classes, e o critério é o que
elas carregam, não a beleza delas.

**Superfície e preenchimento — livres.** `negro-absoluto`, `negro-painel`,
`linha`, `linha-forte`, `laranja-fraco`, `vermelho-fraco`, `fosforo-fraco`,
`azul-fraco`, `laranja-carga`. Nenhuma delas diz nada sozinha; o que elas fazem é
segurar as outras. Livres para mudar — **e mudá-las remede tudo**, porque
contraste é razão e elas são o denominador.

**As seis medidas — repintáveis sob conferência.** `osso`, `osso-apagado`,
`laranja-nerv`, `vermelho-alerta`, `fosforo`, `padrao-azul`. Não é uma lista
inventada: são exatamente as seis que `design/seele-tokens.json` já anota com
`contraste`, porque são as seis que carregam significado. Um MOD pode dar valor
novo a qualquer uma, inclusive ao vermelho, e a conferência abaixo decide se ele
entra.

**O papel — nunca.** Um MOD dá **valor** a um nome. Ele não cria nome, não apaga
nome, e não reaponta nome. `vermelho-alerta` continua sendo a única cor com que
qualquer superfície de alerta ou de queda se pinta, e continua sendo exclusiva
disso, porque quem decide isso são os seletores e os seletores são nossos. As
faixas de sincronia continuam apontando para onde apontam
(`--seele-sync-critico: var(--seele-vermelho-alerta)`), e o glossário de
`specs/07` continua intacto: `Cage` continua `Cage`.

Então a resposta direta à pergunta: **o vermelho de alerta continua sendo dele.**
O papel é intransferível; o tom é negociável. Um MOD para daltonismo que precise
mover aquele vermelho tem uma razão boa e legítima, e um tema em que tudo mudou
menos o alerta pareceria quebrado. O que não se move é para que ele serve.

### A conferência, e o que acontece com um tema ruim

Duas medidas, as duas rodadas **na instalação**, sobre os valores que o próprio
MOD declara — nunca sobre os nossos.

**Contraste.** Cada uma das seis contra **as duas superfícies do MOD**
(`negro-absoluto` e `negro-painel`), pelo pior dos dois, com a mesma aritmética
WCAG 2.1 que `tokens.rs` já implementa. O piso é o critério que aquele token já
cumpria, e não um número novo: 4,5:1 para `vermelho-alerta`, `laranja-nerv`,
`fosforo`, `osso` e `padrao-azul`; 3:1 para `osso-apagado`, que já era só texto
grande e cuja correção `docs/tokens-achados.md` guarda para M4. **A varredura
entra na conta**, com a opacidade que a folha declara, porque ela é pintada por
cima e o véu cobra — a mesma composição que o teste da scanline já faz.
E a conta é feita **também** com a troca de `acessibilidade.css` aplicada, isto
é, com `osso` no lugar de `osso-apagado`: aquela folha corrige por token, então
ela herda o erro do MOD, e conferir só o par padrão deixaria passar exatamente
o tema que quebra quem pediu mais contraste.

**Distinção.** As seis, par a par, em CIELAB — o mesmo espaço que o mapa ANSI já
usa —, e o piso de cada par é a distância que esse par **já tem** na palheta
congelada. Nenhum número inventado, de novo. Vale olhar a tabela, porque ela diz
uma coisa desconfortável sobre a palheta que estamos protegendo:

| par | ΔE76 hoje |
|---|---|
| `laranja-nerv` × `vermelho-alerta` | **19,00** |
| `osso` × `osso-apagado` | 42,62 |
| `osso` × `fosforo` | 56,43 |
| … | … |
| `vermelho-alerta` × `fosforo` | 143,77 |

O par mais próximo entre as seis, por mais que o dobro, é justamente **"vá
olhar" contra "algo quebrou"** — e o vermelho fica a 90 ou mais de todas as
outras doze cores da folha. Ou seja: **a palheta congelada não se sustenta em
distância cromática. Ela se sustenta nos guardas.** O alerta e o laranja quase
nunca dividem superfície porque quatro testes proíbem, e onde eles dividem — as
faixas de sincronia, em linhas vizinhas do roster — a marca de bloco e o número
acompanham sempre. Um MOD que declara valores herda as duas defesas inteiras; um
MOD que escrevesse seletor não herdaria nenhuma. É a mesma conclusão do bloco
anterior, agora com número.

**O que acontece quando falha: recusa por token, e o resto entra.** Não é
"avisa", não é "aceita", e não é "recusa o MOD". A cor que não passa fica com o
valor nosso, o MOD instala com as outras, e a tela de instalação diz qual ficou
de fora, com o número medido e o piso. O motivo de ser por token e não por MOD é
prático: recusar um tema inteiro por causa de uma cor faz a conferência parecer o
defeito, e a próxima coisa que alguém pede é o interruptor para desligá-la.

**Não há interruptor.** Nem "aplicar assim mesmo", nem modo avançado, nem
variável de ambiente. Um escape existe para ser clicado uma vez e esquecido, e
quem clica é exatamente quem menos sabe o que está abrindo mão. Se este ADR
tivesse um interruptor, ele seria a decisão de verdade e todo o resto seria
decoração.

**E há um teto que a conferência não alcança:** o MOD pode passar em tudo e ser
pior que o nosso. Contraste e distinção são o que dá para medir. Está na seção
"O que fica sem saída", que é onde essa frase pertence.

### O esquema fechado, e o que entra na versão 1

A regra de admissão, primeiro, porque é ela que impede este ADR de virar uma
lista de desejos: **uma capacidade entra no esquema quando o produto já tem a
tabela e já tem o consumidor dela.** Uma capacidade cuja tabela precisa ser
inventada para o sistema de MODs é uma tabela congelada antes de alguém tê-la
usado uma vez — e o custo de tirar capacidade depois é quebrar MOD de terceiro,
que é a razão pela qual essa porta não fecha.

Aplicada às sete coisas nomeadas no pedido:

| capacidade | tabela existe? | v1 |
|---|---|---|
| **cor** | `tokens.css` / `seele-tokens.json`, com dois consumidores | **entra** |
| glifo | `glifos.js` é geometria SVG; no terminal são caracteres | não |
| frase | dois catálogos chapados em duas cascas; sem i18n | não |
| som | não existe subsistema nenhum | não |
| atalho | as teclas são `match KeyCode` em `selecao.rs`, não tabela | não |
| comando | `command.rs` existe no `plug`; a janela não tem linha de comando | não |
| painel | não há catálogo de leituras publicadas; `:sync` imprime uma linha | não |

E o motivo de cada recusa, porque "não tem tabela" é curto demais:

- **Glifo.** Os glifos da janela são desenhos dimensionados contra a altura de
  caixa alta da Plex Mono embarcada; os do terminal são caracteres de verdade.
  Uma capacidade `glifo` teria de querer dizer as duas coisas, e não quer.
- **Frase.** O ADR 0012 decidiu a fronteira e adiou o catálogo, e o catálogo
  continua adiado. O sistema de MODs seria o **segundo** consumidor de uma tabela
  que ainda não tem o primeiro. Quando ela nascer, esta é a capacidade mais
  valiosa da lista: uma tradução vira MOD em vez de fork. E ela nasce com uma
  regra já decidida — **acrescenta locale, não sobrescreve os que o produto
  publica** —, porque reescrever o pt-BR é renomear o produto, e `specs/07` diz
  que o tema é vocabulário e não skin.
- **Som.** Não há o que trocar. Um MOD de som construiria o subsistema, e um
  subsistema desenhado a partir de um formato de arquivo de terceiro nasce
  torto.
- **Atalho, comando, painel.** Os três exigem que as duas cascas concordem sobre
  um vocabulário que hoje elas não têm em comum. Congelar esse vocabulário agora
  é congelar a discordância.

**Sim, uma capacidade só se parece com a alternativa que este ADR acabou de
recusar, e não é ela.** A diferença não está no que v1 pinta: está em que a
alternativa recusada é um sistema que **não tem para onde crescer**, e este tem
versão de esquema, negociação, recusa nomeada, caminho de instalação, indexador e
assinatura — tudo construído com N=1. As partes caras de um sistema de MODs não
são as capacidades. São o que acontece quando um MOD está errado, quem a pessoa
está confiando, e o que o indexador aprende. Essas três estão prontas na
primeira versão, e a segunda capacidade custa uma versão de esquema e nada mais.

Este é também o ponto deste ADR que o dono tem mais motivo para derrubar, e ele
está isolado de propósito: mudar v1 de uma capacidade para três não mexe em
nenhuma outra seção.

### A forma do arquivo

JSON, um arquivo, e o formato é o espelho de `design/seele-tokens.json`:

```json
{
  "esquema": 1,
  "nome": "…", "autor": "…", "versao": "…",
  "fonte": "https://…", "licenca": "…",
  "cor": {
    "negro-painel": { "hex": "#0A0806", "ansi256": 232, "ansi16": "bright-black" }
  }
}
```

JSON porque `serde_json` já está na árvore em três crates nossos — nenhum crate
novo, nenhuma exceção nova no `deny.toml`, que é a mesma régua que o ADR 0027
usou. A validação é **escrita à mão em Rust**, e não por um validador de esquema
genérico: o esquema é pequeno e fechado, o ADR 0006 já valida uma URI assim, e um
validador escrito à mão consegue devolver **uma frase por recusa** — que é o que
`specs/02-protocolo.md` exige de toda razão e o que um validador genérico não sabe
fazer.

O esquema é **escrito por extenso** neste projeto, e não definido como "igual ao
`seele-tokens.json`". Aquele arquivo tem duas partes desatualizadas conhecidas —
`faixas_sync` ainda com quatro faixas, que o ADR 0024 reduziu a três, e
`movimento.nao_adotado` dizendo que a varredura não entrou, que o ADR 0014
reverteu em M5. Um esquema por referência herdaria as duas.

**`ansi256` e `ansi16` entram no esquema em v1, são validados, e não são lidos.**
Ver a seção do terminal, logo abaixo, para o motivo — e o motivo de estarem aí
mesmo assim: o arquivo que alguém escrever hoje já está completo no dia em que o
`plug` os ler, e acrescentar o terminal não custa versão de esquema.

**Chave desconhecida é recusada; cor ausente não é.** As duas metades importam.
Uma cor omitida mantém a nossa — é o que faz um MOD de três linhas que só levanta
o `osso-apagado` ser possível, e não é instalação parcial silenciosa porque a
tela de instalação mostra exatamente o que muda. Já um **nome** desconhecido é
erro: `vermelo-alerta` com um `l` instalaria calado um tema que mantém o nosso
vermelho, e o autor nunca ficaria sabendo. Recusar é a única forma de retorno que
um autor de MOD tem.

### Versão de esquema

Um inteiro, e não semver. Semver convida a discussão sobre o que é compatível, e
aqui não há discussão: **o esquema só cresce.**

- **MOD mais novo que o app** — recusa, inteira, nomeando a versão do arquivo,
  a versão que este app fala, e que atualizar o app é o caminho. Não se lê "o que
  der": um MOD escrito para o esquema 2 que declara uma capacidade que o esquema 1
  não conhece instalaria como um MOD **pela metade**, sem aquilo que o autor
  achava essencial. Instalação parcial silenciosa é o modo de falha de todo
  sistema de extensão que escolheu ser tolerante.
- **MOD mais velho que o app** — lê, sempre, para sempre. É a promessa, e é o
  custo: capacidade não sai. Uma capacidade que se revelar errada é
  **desencorajada no indexador e documentada**, nunca removida do leitor.
- **Quem diz o quê a quem.** Quem recusa é o **app**, na instalação, antes de
  qualquer coisa ser aplicada — não a cada abertura. O indexador diz a mesma
  coisa antes, por conveniência: ele conhece a versão de esquema de cada MOD e o
  cliente diz qual fala, então o catálogo se filtra e a pessoa não baixa o que
  vai ser recusado. Mas a palavra do app é a que vale, e o indexador pode não
  existir.

### Onde mora, como instala, o que acontece ao desinstalar

**Um arquivo.** Não um pacote, não um zip. Isso não é frugalidade: é consequência
de v1 ser só cor, que é texto — e por isso não há formato de arquivo comprimido,
não há extração, não há travessia de caminho, não há dependência nova. No dia em
que uma capacidade trouxer binário, esse dia tem o custo dele, e ele está nomeado
na seção de código aberto.

**Onde:** `mods/`, dentro do diretório do ADR 0017 — ao lado de `identity.key`,
`pins`, `conhecidos` e `preferences`. Qual MOD está ativo é campo em
`preferences`, que é onde o `seele-core` já guarda preferência de máquina.

**Instalar** é copiar o arquivo e escrever o nome no `preferences`. **Desinstalar**
é apagar as duas coisas, e é seguro por construção: **um MOD não escreve nada.**
Ele não tem estado, não toca banco, não toca histórico, não toca identidade.
Desinstalar não pode perder dado de ninguém porque não há dado de ninguém dentro
de um MOD — e essa é uma restrição de desenho, não uma observação: nenhuma
capacidade futura pode dar a um MOD um lugar para escrever.

**Um por vez.** Nada de empilhar dois temas. Duas listas de valores compostas
produzem uma terceira palheta que ninguém mediu e ninguém escreveu, e a
conferência acima passaria a medir uma coisa que nenhum autor assinou.

**Em v1, o MOD pinta a janela; o `plug` fica com a palheta congelada.** Este é o
custo mais concreto da decisão e não é pequeno — quem usa por SSH não ganha nada.
O motivo: a palheta do terminal não são catorze cores, são catorze cores vezes
quatro fidelidades, três escolhidas à mão. `theme.rs` se recusa a arredondar em
tempo de execução porque é assim que uma palheta cuidadosa vira lama por SSH, e o
mapa de 16 cores é **semântico**, escolhido para manter alerta e laranja
distintos num espaço onde não sobra folga — há teste só para isso. Um MOD que
repintasse o terminal teria de refazer à mão um trabalho de design que este
projeto fez uma vez com instrumento de medida. O caminho existe e está no
esquema: quando o MOD declarar `ansi256` e `ansi16`, o `plug` os lê; o `ansi256`
inclusive pode ser calculado na instalação pelo método já documentado — vizinho
em CIELAB restrito a 16–255 —, porque calcular uma vez ao instalar não é
arredondar a cada desenho. O `ansi16` não tem como ser calculado, e é justamente
o que `theme.rs` diz.

### Um MOD não acompanha um Dogma

Alguém entra numa sala e o tema muda: **não.** É a pergunta mais atraente do
pedido e a resposta é a mais restritiva deste ADR, então ela vem com os motivos
inteiros.

- **Toda defesa daqui repousa numa pessoa decidindo.** A conferência mede, a tela
  mostra, o arquivo é legível, a assinatura confere. Aplicar por entrar numa sala
  remove a pessoa de todo esse desenho e deixa só a conferência de pé — e a
  conferência é a metade fraca, porque ela sabe medir contraste e não sabe medir
  intenção.
- **Um Dogma não é necessariamente entre amigos.** O ADR 0021 deixa um Dogma sem
  convite e sem senha **aberto por padrão**, de propósito. Uma sala que empurra
  tema é uma sala que empurra tema onde a queda de enlace é discreta.
- **E a tela mudaria enquanto a pessoa está nela.** `specs/07` diz que movimento
  é diagnóstico, com uma exceção nomeada; a janela inteira trocando de cor porque
  alguém entrou numa sala é o movimento menos diagnóstico que este produto
  poderia ter.

**O que fica permitido é recomendar.** O ADR 0006 já estabelece que parâmetro
desconhecido no `seele://` é **ignorado em vez de recusado** — foi essa regra que
deixou o `alt=` ser compatível com clientes velhos —, então um Dogma pode
carregar no link a identidade de um MOD: nome, autor, hash e onde achar. O
cliente mostra "este Dogma sugere o MOD X" e **instalar continua sendo o mesmo
ato, na mesma tela, com as mesmas medidas na frente**. Recomendação é dado;
aplicação é consentimento.

Isto custa a coisa mais divertida que o pedido continha, e o custo está escrito
aqui em vez de descoberto depois.

### O indexador

Tem a forma exata do problema do degrau 4 do ADR 0022, e recebe o mesmo
tratamento — inclusive a parte de dizer em voz alta.

**Ele aprende metadado.** Qual instalação pediu qual MOD, e quando, e de qual
endereço. Nunca conteúdo, nunca conversa, nunca com quem a pessoa fala. Mas é
informação que hoje não existe em lugar nenhum, num produto cujo argumento é não
ter serviço no meio, e o ADR 0022 já pagou o preço de escrever isso antes de
construir em vez de depois.

**Opcional, trocável e não obrigatório**, nas três palavras que o 0022 usa para o
ponto de encontro. O app vem apontando para o nosso; o endereço é configurável; e
instalar de arquivo local funciona sem indexador nenhum. Nada em um MOD depende
de ele ter vindo de lá — não há registro, não há identificador emitido, não há
carimbo. Hospedado por nós, porque este produto é auto-hospedado por decisão e o
ADR 0022 recusou retransmissão de terceiro pelo mesmo princípio.

**Catálogo estático, e busca no cliente.** Não é uma API. É um arquivo de
catálogo assinado, servido como arquivo, mais os MODs como arquivos. O cliente
baixa o catálogo e **busca localmente**. A diferença é concreta: com API, o
indexador aprende cada termo que alguém digitou; com catálogo, ele aprende que
alguém buscou o catálogo. É a redução mais barata que existe aqui, e ela vem com
um brinde — um arquivo estático é **espelhável**, então quem não quiser que
sejamos nós a servir os bytes serve os mesmos bytes de outro lugar, e a
assinatura continua conferindo.

**O app não consulta ao abrir.** Nenhuma procura automática por atualização de
MOD. É a mesma regra do ADR 0026 — procurar não baixa, e nada roda sozinho —, e
aqui ela tem uma razão extra: um indexador consultado a cada abertura deixa de
ser opcional e vira um registro de quando esta pessoa usa este produto.

**Sem contador de download, sem estrela, sem nota.** Um contador é um número que o
indexador só produz **guardando registro de quem buscou o quê**, e ele substitui
o julgamento que a tela de instalação existe para tornar possível. Custa uma
descoberta pior; poupa a única coisa que o catálogo estático estava conseguindo
não saber.

O que continua verdade depois de tudo isso: **quem serve os bytes vê quem os
pediu.** Minimizado, espelhável, opcional — e verdade.

### "Precisa ser de código aberto" — exigido como

Há três exigências possíveis, e elas são muito diferentes: que o fonte esteja
publicado; que o artefato seja **construído** a partir dele por alguém confiável;
ou um link que ninguém confere.

A resposta aqui é que **a pergunta se dissolve em v1, e é preciso dizer por
quê**: um MOD de cor não tem forma compilada. O fonte e o artefato são os mesmos
bytes. Publicar o fonte não é uma promessa sobre o arquivo — **é o arquivo.** Não
há reprodutibilidade a exigir porque não há build a reproduzir.

Então o que o produto exige de verdade não é uma licença: é **legibilidade**, e
ela é exigida por construção. Disso sai a regra que vale mais que a exigência
original, porque é a única que sobrevive ao futuro:

> **Nenhuma capacidade entra no esquema se o artefato dela não for legível por
> uma pessoa.** No dia em que uma capacidade quiser trazer fonte, som ou imagem,
> ela traz junto o problema que v1 não tem, e é aí que "construído a partir do
> fonte por alguém confiável" passa a ser a pergunta certa. Enquanto o MOD for
> texto, a exigência de código aberto é gratuita e absoluta.

**O que a pessoa vê para julgar**, que é o outro lado do "a confiabilidade é
julgada pelo usuário":

1. **O arquivo inteiro**, legível, na tela. São alguns kilobytes. É a mesma razão
   pela qual o `pins` do ADR 0017 é texto puro: quem precisa julgar precisa
   conseguir abrir e comparar a olho.
2. **A diferença contra o que está em vigor** — quais cores mudam, de que valor
   para que valor, pintadas lado a lado. É a informação que ninguém consegue
   extrair lendo hexadecimal.
3. **As medidas**: contraste antes e depois, distância antes e depois, e qual
   token foi recusado e por quê.
4. **Autor, fonte, licença declarada**, e se é oficial — conferido por
   assinatura, offline, e não afirmado por um campo.

Duas honestidades que precisam estar na mesma seção. **O link de fonte é um link,
e ninguém o confere**; ele importa menos aqui do que importaria em qualquer outro
lugar, justamente porque o artefato é lido diretamente. E **estamos exigindo de
terceiros uma coisa que o produto ainda não fez**: o SEELE não tem licença, e a
recomendação vigente é repositório privado até M4. Por isso o **produto** exige
legibilidade, que ele consegue garantir, e o **indexador** exige um campo de
licença declarada de uma lista curta, que é política e não garantia. Exigir uma
licença específica antes de termos escolhido a nossa seria cobrar uma promessa que
não fizemos. Nota de alívio: a negação de GPL/AGPL/LGPL do `deny.toml` não
alcança isto — ela existe porque o produto é um binário estaticamente ligado, e um
MOD não é ligado a nada. É dado.

### MOD oficial

**O que o distingue é uma assinatura que o cliente confere offline, não um campo
no catálogo.** Chave `minisign` própria, embutida no app do mesmo jeito que a do
atualizador — e **uma segunda chave, separada da do ADR 0026**. Separada porque
uma chave que atesta duas coisas diferentes deixa as duas se passarem uma pela
outra, e a chave do 0026 é a que autoriza instalar programa. A do MOD autoriza
pintar.

A consequência de ser assinatura e não campo é a que importa: **se o indexador for
espelhado, oficial continua oficial; se o indexador mentir, ele não consegue
forjar oficial.** É a mesma lição do ADR 0026, alternativa 5: TLS diz de qual
servidor o arquivo veio, e não quem o produziu.

**E o que "oficial" promete é estreito, e tem de ser dito assim**, porque as
`NOTAS-DE-RELEASE.md` deste projeto já ensinaram a separação: *"O que o atestado
**não** faz é dizer que o software é bom. Ele diz de onde veio."* Um MOD oficial
não é um MOD melhor. É um MOD que **nós mantemos**: acompanha as versões de
esquema, é consertado quando quebra, e não fica para trás no dia em que a palheta
mudar. É promessa de manutenção, e é por isso que a distinção não é um selo — um
selo diz que gostamos, e isto diz que respondemos por ele.

**Nenhum MOD é isento da conferência, e os nossos menos ainda.** Se um MOD oficial
pudesse pular a medida de contraste, a medida seria mentira e o selo seria a única
coisa de pé. Um tema oficial que reprovasse teria a cor recusada na tela de quem
instala, exatamente como o de qualquer outra pessoa.

## Alternativas consideradas

1. **Só aparência, sem manifesto e sem esquema.** Seguro, e o que menos atende ao
   que foi pedido. Recusada por duas razões, e a segunda é a que decide: não
   responde a "acrescentar comandos, painéis ou atalhos", que é metade do pedido;
   e construiria **a mesma máquina inteira** de qualquer jeito — a conferência, a
   versão, a instalação, o indexador, a assinatura, a tela de julgamento. Todo o
   custo, e a recusa justamente do caso que abre a porta. É a mesma forma da
   alternativa 3 do ADR 0027, e pela mesma razão.

2. **Código de verdade em caixa de areia** (WebAssembly, Lua, um interpretador
   qualquer). Poder máximo, e o que qualquer sistema de MOD maduro faz. Recusada,
   e o motivo central é do dono: **caixa de areia limita capacidade e não julga
   intenção.** Com código de verdade, "MODs precisam ser de código aberto" deixa
   de ser boa prática e vira a **única defesa real** — e uma defesa que consiste
   em esperar que alguém tenha lido o fonte não é uma defesa, é uma esperança.
   Três custos concretos por cima: um runtime é dependência numa árvore que o ADR
   0026 contou crate a crate e que o `xtask/src/check_deps.rs` obriga a declarar
   por escrito; escape de caixa de areia vira uma classe de defeito que este
   projeto passa a ter para sempre; e um MOD que computa pode ser lento, num
   produto com orçamento de latência medido (ADR 0009) que deixaria de ser nosso.
   E o que mais dói: **todas as garantias citadas no Contexto cairiam de uma vez.**
   Código que pinta escreve seletor, e seletor derruba os quatro guardas do
   vermelho sem que nenhuma cor esteja errada.

3. **Nada — quem quiser que faça fork.** É de graça, funciona hoje, e é o que de
   fato acontece na ausência deste ADR. Recusada por três: um fork não recebe
   atualização, que é o ADR 0026 inteiro; dois forks não compõem, enquanto dois
   MODs pelo menos podem se alternar; e um fork tira a pessoa do caminho do
   binário assinado, que é a pior coisa possível num produto que já apanha do
   SmartScreen e do Gatekeeper por falta de credencial (pendência 16). "Faça
   fork" é a resposta que empurra o usuário para um executável sem procedência.

## Consequências

- **Uma decisão adiada em dois lugares fecha**, e um teste muda: a lista de
  `the_settings_screen_omits_what_the_product_lacks_instead_of_drawing_it_dead`
  perde `TEMA`, e o não-objetivo de `specs/00` precisa ser emendado — este ADR o
  desfaz pela metade, do mesmo jeito que o 0027 desfez metade da D14.
- **O ADR 0014 continua valendo inteiro**, e ganha um papel novo: a palheta
  congelada deixa de ser só a palheta do produto e passa a ser **o piso contra o
  qual toda outra é medida**. Nenhum número dele muda.
- **A aritmética de `tokens.rs` ganha um segundo consumidor**, e por isso sai de
  um teste. Contraste WCAG, composição em sRGB e distância em CIELAB passam a ser
  código de produção — em `seele-core`, que é onde os dois clientes alcançam
  (ADR 0002) e onde `identity`, `conhecidos` e `preferences` já moram. O teste
  continua existindo e passa a medir a mesma função que o produto usa, que é
  melhor do que hoje.
- **Nenhuma dependência nova, e nenhuma exceção nova no `deny.toml`.**
  `serde_json` já está declarado em três crates; `minisign-verify` entrou com o
  ADR 0026. Um crate de validação de esquema (`jsonschema`) foi considerado e não
  entra: o esquema é fechado, e validação à mão devolve frase.
- **A CSP não muda, o conjunto de arquivos de `ui/` não muda, e os quatro guardas
  do vermelho não mudam.** Se alguma das três precisasse mudar, o desenho estaria
  errado.
- **O `plug` não é modificável em v1**, e quem usa por SSH não ganha nada. Está
  na decisão, com o motivo, e o caminho de saída já está no esquema.
- **Um sistema silencioso ganha um formato de arquivo antes de ganhar som.** A
  capacidade `som` está fora e o subsistema não existe; se ele nascer, nasce pelo
  produto e não pelo esquema.
- **O indexador é infraestrutura nova para manter**, ainda que seja arquivo
  estático. É a primeira coisa que este projeto hospeda além do release no
  GitHub, e o ADR 0026 tinha orgulho de não hospedar nada.
- **Uma chave a mais para guardar e para perder.** A chave de MOD oficial tem o
  mesmo problema de custódia da chave de atualização, e a pendência 16 já mostra
  que credencial é a parte deste projeto que não é trabalho de código.

## O que fica sem saída

Cinco, e nenhuma tem resposta boa. Estão aqui porque um ADR que só louva a opção
escolhida não serve para nada daqui a um ano.

**Feio não se mede.** A conferência sabe dizer que um par tem 4,7:1 e 61 de
distância. Ela não sabe dizer que a tela ficou cansativa às três da manhã, que a
hierarquia sumiu, ou que o produto deixou de parecer o produto. Um MOD pode passar
em tudo e ser pior que o nosso em todos os aspectos que importam. A única resposta
é que a pessoa desinstala — o que só funciona porque ela consegue.

**Distinção é aos pares, e olho não é.** ΔE em CIELAB é média de população. Duas
cores podem estar longe uma da outra na tabela e serem o mesmo borrão para uma
pessoa específica, e a deficiência de visão de cor não é um deslocamento uniforme
que um piso capture. `specs/05` já registra que a palheta depende muito de
vermelho e verde, e a conferência não conserta isso — ela só impede que fique pior
do que já está.

**O esquema só cresce, e uma das capacidades vai estar errada.** É a consequência
direta de não quebrar MOD de terceiro, e não há versão desta decisão em que dê
para desfazer barato. A regra de admissão é a única mitigação, e ela é uma regra
de disciplina, não um mecanismo.

**O indexador sabe quem baixou o quê.** Catálogo estático em vez de API, busca no
cliente, nenhuma consulta automática, espelhável, opcional — e quem serve os bytes
continua vendo quem os pediu. É o mesmo saldo do degrau 4 do ADR 0022, e a mesma
honestidade.

**Julgar exige ler, e quase ninguém lê.** "A confiabilidade é julgada pelo
usuário" pressupõe um usuário que abre o arquivo. O produto pode tornar a leitura
possível — arquivo curto, texto, diferença pintada, medidas ao lado — e não pode
tornar a leitura provável. O que sobra dito, e não resolvido: uma parte das
pessoas vai instalar clicando, e para elas a defesa que resta é a conferência,
que mede contraste e não mede intenção.

## Custo de reverter

**Baixo antes do primeiro MOD instalado.** Um leitor, um validador, uma tela, um
campo no `preferences` e um diretório. Nada no protocolo, nada no banco, nada nas
folhas.

**Depois, tem uma propriedade que vale escrever, e ela não foi acidente:**
desinstalar todo MOD devolve a palheta congelada, exatamente, sempre — porque o
MOD nunca escreveu nada e a folha nunca mudou. O estado sem MOD não é um estado de
recuperação: é o estado normal, com uma linha a menos no `preferences`.

**Alto para o esquema e para o indexador, assim que houver um MOD de terceiro.**
Mudar o formato do arquivo depois de distribuído quebra o de todo mundo, e é a
mesma advertência que o ADR 0017 escreveu sobre o formato do `identity.key`. O
endereço do indexador é trocável de propósito justamente para que essa parte não
fique presa.
