# Lote K4 da F5: relatório

Base `2112c389` (`0.16/lotes`). Branch `0.16/lotes-k4`, no worktree
`/Users/dev-alexandre/SEELE/.claude/worktrees/agent-a8c98c0bf885caa60`, com o
`target/debug` semeado por `cp -c` de `abertos-da-1.0` e tudo rodado com
`CARGO_TARGET_DIR` desligado. Dois commits de código e este relatório, todos
locais: sem push, tag, release ou Actions.

| commit | item | o que muda |
|---|---|---|
| `e8a485cc` | **P49-7b** (fecha) | `midia_do_mod` confere o tipo por `ler_tipo` e devolve ao som do pacote o papel e o tamanho, sem `uri`; só a imagem passa por `ler_midia`. `MidiaDoMod.uri` vira `Option<String>`, fora da resposta só nesse caso |
| `9b43de1d` | **P49-7a** (fecha) | `bytes_decodificados_minimos`, um piso lido à mão do cabeçalho (WAV, Ogg, MP3); acima de 64 MiB, a recusa `som-grande-demais-decodificado` com os números sai antes de servir, nas três portas que entregam som à janela; a janela a diz como o teto do som (`recusarPeloTetoDoSom`), na montagem e no pedido de tocar; o guarda dos números repetidos |

Arquivos mudados (`2112c389..9b43de1d`), só os quatro da lista do lote:
- `apps/seele-app/src/main.rs`
- `apps/seele-app/ui/base.js` (só comentário: o doc de `bytesDoSom`)
- `apps/seele-app/ui/mods-regiao.js`
- `apps/seele-app/bancada/regiao-do-mod.cjs`

Nada no fio (`crates/seele-proto`), no SEELE-ENC/1, na F6, em
`apps/seele-app/src/executor.rs` nem no `crates/seele-motor`. `api/v1..v5.json`
e `api/README.md` intocados; o comando continua `midia_do_mod`, que
`"midia-do-pacote"` mapeia. Nenhuma crate de áudio, nenhuma dependência nova,
nada baixado; a CSP não muda.

## 1. P49-7b: o som do pacote atravessa a ponte uma vez só (`e8a485cc`)

**Produto (`main.rs`).**
- `fn midia_declarada_do_mod(` passa pelas mesmas conferências de antes
  (`bytes_declarados_do_mod`: o pacote pelo hash, o hash deste MOD, o arquivo
  declarado, o teto de arquivo) e confere o tipo por
  `seele_ffi::mods::ler_tipo`, a pergunta de oito bytes. Com o papel `som`,
  devolve `MidiaDoMod { uri: None, papel, bytes }`; com o papel `imagem`,
  `ler_midia`, que monta o `data:`; sem tipo, `formato-desconhecido`, como
  antes.
- O doc de `midia_do_mod` deixa de citar `recursosDoMod` (que não existe) e
  aponta para onde o total mora: `LIMITES_DA_REGIAO.bytesDeMidia` e o bolso dos
  cartões, em `mods-regiao.js`. Ganha o parágrafo «O som sai sem `uri`».
- Saem os dois «atravessa duas vezes»: o doc de `bytesDoSom:` (`base.js`) e o
  de `montarSom` (`mods-regiao.js`).

**`Option<String>`, e não um tipo próprio.** `MidiaDoMod` é a resposta de
três comandos — `midia_do_mod`, `midia_em_bytes` e `ler_imagem_mod` (por
`imagem_lida_dita`, que o brief não listava) — e a janela lê os três pelos
mesmos leitores (`montarMidia`, `montarRetrato`, os fundos). Um tipo próprio para
o som do pacote repetiria os três campos e a serialização para mudar um, e
daria à ponte duas formas para a mesma pergunta («que mídia é esta?»). Com
`#[serde(skip_serializing_if = "Option::is_none")]`, a forma que atravessa é
a mesma, menos o `uri` no único caso em que ninguém o lê. O preço é o tipo não
dizer que a mídia do servidor sempre traz o `uri`; o doc do campo diz, e os
três construtores que não são o do pacote escrevem `Some(…)`. Os dois testes
que liam `midia.uri` como `String` passaram a comparar com `Some(…)` e a
usar `is_some_and`, cada um com a mensagem do que quebra.

As quatro portas de imagem da janela conferem `midia.papel !== "imagem"` antes
de ler o `uri` (fundo, retrato, fundo de tela, avatar contribuído), e
`montarMidia` passa `{ caminho }` ao som do pacote e `{ uri }` só ao do
servidor: nenhum leitor do `uri` do som do pacote existia.

**Prova.** `o_som_do_mod_sai_em_bytes::o_som_do_pacote_sai_sem_uri_e_a_imagem_com`
publica um pacote com `som/toque.wav` (o WAV mínimo do `pacote_de_teste`) e
`img/rosto.png`, chama `midia_declarada_do_mod` e mede **o que atravessa a
ponte**, a resposta serializada (`serde_json::to_value`): o som sem a chave
`uri`, com `papel: "som"` e `bytes: 16`; a imagem com
`uri` começando em `data:image/png;base64,`.
- **Vermelho** (antes do conserto): `o som do pacote voltou a atravessar a
  ponte em base64, no uri de midia_do_mod, além de cru por som_do_mod: o
  arquivo é lido duas vezes, e ninguém lê esse data: — {"bytes": Number(16),
  "papel": String("som"), "uri": String("data:audio/wav;base64,UklGRgAAAABXQVZFZm10IA==")}`.
- **Verde** depois, junto com os dois testes ajustados e os módulos
  `a_midia_recusada_e_dita_no_registro` e `a_supervisao_dos_mods_nativos` (21
  testes).
- **Reversão**: o braço do som com `uri: seele_ffi::mods::ler_midia(&bytes).map(|lida| lida.uri)`
  — o `uri` de volta — deixa o teste vermelho com a mesma frase e o mesmo
  `data:`. Conserto de volta, `cmp` com a cópia: igual.

**Na bancada.** `oSomTocaPorWebAudioENuncaPorUmElementoDeMidia` passa a
responder ao som do pacote como o Rust agora responde, `{ papel: "som",
bytes: 12 }`, sem `uri`. Provado pelo avesso: com `montarMidia` passando
`{ uri: midia.uri }` também ao som do pacote, o caso fica vermelho («os bytes
do som não foram pedidos pelo caminho declarado», «o som decodificado não
ficou pronto: falhou», «o botão do produto que toca o som não foi montado»).
De volta, `cmp`: igual.

## 2. P49-7a: o som grande demais é recusado antes de decodificar (`9b43de1d`)

### 2.1 O piso (`main.rs`)

Junto de `som_declarado_do_mod`:
- `const TETO_DO_SOM_DECODIFICADO: u64 = 64 * 1024 * 1024`,
  `const TAXA_DA_DECODIFICACAO: u64 = 48_000` e
  `const SOM_GRANDE_DEMAIS_DECODIFICADO: &str = "som-grande-demais-decodificado"`;
- `fn bytes_decodificados_minimos(bytes: &[u8]) -> Option<u64>`, a função pura:
  despacha pelo `ler_tipo` (`audio/wav`, `audio/ogg`, `audio/mpeg`) para o
  módulo `cabecalho_de_som`, que devolve `{ amostras, taxa, canais }`; recusa
  taxa fora de 3 000–768 000 Hz e canais fora de 1–32 (fora disso o WebAudio
  não decodifica, e o piso não teria do que ser piso); converte os quadros para
  48 kHz em `u128`, tira **um quadro** pelo arredondamento em ponto flutuante
  da conversão de taxa do navegador, e multiplica por canais e por 4;
- `fn som_cabe_decodificado(bytes) -> Result<(), FalhaNoMod>`: acima do teto,
  `Recusado { motivo: "som-grande-demais-decodificado: decodificado a 48000 Hz,
  o som ocupa ao menos N bytes, acima do teto de 67108864 bytes de som" }`.

O parse é à mão, sem crate e sem índice que entre em pânico (todo acesso por
`get`):
- **WAV, exato**: os pedaços do RIFF em ordem, o `fmt ` antes do `data`. Só os
  formatos em que um quadro é um bloco (PCM, float, A-law, µ-law, direto ou no
  `WAVE_FORMAT_EXTENSIBLE`); os comprimidos dão `None`. O quadro conta pelo
  maior entre o bloco declarado e canais × bytes da amostra — um cabeçalho que
  discorde de si mesmo dá menos quadros, e não mais. **O `data` é limitado aos
  bytes que estão no arquivo**: um WAV gravado aos poucos leva ali
  `0xFFFFFFFF`, e um cortado, mais do que tem.
- **Ogg**: as páginas seguidas pelo tamanho de cada uma desde a primeira, e não
  por uma busca de `OggS` (que acharia a marca dentro dos dados). A primeira
  página, BOS, tem de ser a identificação Vorbis (`\x01vorbis`, versão 0) ou
  Opus (`OpusHead`, versão de cima 0). O piso é a granule da última página do
  fluxo menos a da primeira de dados — um fluxo que não começa do zero (um
  pedaço de transmissão) decodifica menos do que a última granule diz —; no
  Opus, menos também o pre-skip, e a taxa é 48 kHz (a do cabeçalho é a da
  entrada). O CRC das páginas não é conferido.
- **MP3**: pula as etiquetas ID3v2 do começo; o primeiro quadro tem de estar
  ali (formato livre, camada ou versão reservada: `None`). Dele em diante,
  cada quadro inteiro com a mesma versão, camada, taxa e canais conta; a
  cadeia para no primeiro que não for um. O quadro Xing/Info, quando o
  primeiro quadro traz um com a contagem, **limita** a conta (o menor dos
  dois). Sai o que o decodificador pode cortar: o próprio quadro Xing/Info, o
  atraso e o enchimento da etiqueta do LAME (até 4095 cada) e o atraso do
  decodificador (529).

**O WAV mínimo do pacote de teste** (`b"RIFF\0\0\0\0WAVEfmt "`) dá `None`:
`um_som_declarado_sai_com_os_bytes_exatos` continua verde, e um teste o
confere pelo nome.

### 2.2 Onde a recusa vale

- `som_declarado_do_mod`: o braço `Some(tipo) if tipo.papel == "som"` passa
  por `som_cabe_decodificado`, e a recusa sai por `recusa_de_midia_dita`, com
  os baldes e o instante, como as outras.
- `ler_midia_do_servidor`: quando `ler_tipo` diz som, antes de `ler_midia` —
  recusar antes do `data:` é não codificar em base64 o arquivo que ninguém vai
  ler. A recusa sai por `midia_do_servidor_dita` → `recusa_de_midia_dita`,
  como as outras dessa porta.
- **`imagem_lida_dita`, a leitura por volume (`ler_imagem_mod`), além do
  brief.** O brief nomeia duas portas; a terceira tem o mesmo pico pelo mesmo
  motivo que ele dá para a segunda: `carregarMidiaDoServidor` com
  `transporte: "volume"` chama `ler_imagem_mod`, que devolve `MidiaDoMod` por
  `ler_midia` — que aceita som —, e `montarMidia` leva esse `uri` a
  `bufferDaUri`. Um guarda em duas de três portas é o «existir não é
  funcionar» do `CLAUDE.md`. A recusa sai por `recusa_de_midia_dita` com a
  origem `volume`, e o doc da função passou de «duas recusas» a três.

`midia_declarada_do_mod` (o papel e o tamanho do som do pacote, na montagem)
**não** recusa pelo piso, como o brief pede: a recusa do som do pacote chega
pelos bytes (`som_do_mod`), e é isso que a faz valer nos dois momentos que o
K5 deixou.

### 2.3 A janela (`mods-regiao.js`)

- `SOM_GRANDE_DEMAIS_DECODIFICADO` e `recusaDoSomGrandeDemais(falha)`: o
  motivo da recusa do Rust, reconhecido pelo nome no começo de
  `falha.Recusado.motivo` — só a recusa do Rust, pelo formato com que chega; um
  `Error` da janela com as mesmas palavras não é ela.
- `recusarPeloTetoDoSom(elem, plano, estado, porque)`: **o caminho único do
  teto do som** — figura «cheia», `som.recusa = { figura: "cheia", motivo,
  teto }` (que `herdarOSom` passa adiante numa contribuição), a anotação com a
  chave `bytes de som:<teto do bolso>`, `recusada` ao MOD com o porquê, e
  «recusada» no registro. O teto do som decodificado conferido em
  `carregarSom` depois de decodificar passou a chamá-lo também (as mesmas
  linhas, que estavam escritas ali), para que as duas recusas não possam
  divergir de palavra.
- O `catch` dos bytes em `carregarSom` (que cobre `bytesDoSom` e o pedido de
  novo ao servidor) manda a recusa do Rust para `recusarPeloTetoDoSom`, e não
  para `falhou`. `carregarSom` roda na montagem para o que não é contribuição e
  no pedido de tocar (`pedirParaTocar`) para a contribuição: **os dois
  momentos do K5 passam pelo mesmo `catch`**.
- O `.catch` de `montarMidia` faz o mesmo para a resposta do servidor
  (`midia_em_bytes`, ou o volume), que chega recusada antes de o papel ser
  conhecido.
- Docs: `LIMITES_DA_REGIAO.bytesDeSomDecodificado` («O teto vale para o que
  fica de pé.») perde «não há como saber a duração sem decodificar» e ganha
  «O pico é cortado antes, no Rust, quando o cabeçalho permite»; o doc de
  `montarSom` diz a recusa do Rust e os dois momentos; `bytesDoSom` (`base.js`)
  diz a forma com que a rejeição chega.

**A deduplicação do K3.** A recusa nova entra em `contarAMidia` pela chave do
teto do som, `bytes de som:67108864` — a mesma que o teto da janela grava —, e
ocupa a mesma vaga. Medido na bancada com a regra de verdade, recortada de
`base.js` (`const vagasDosTetos` até o fim de `function contarAMidia(`): duas
recusas do Rust e uma da janela, na mesma região, dão `recusada: 3` e **um**
motivo, terminado em «(e mais 2 recusas pelo mesmo teto)». Antes do conserto,
eram dois motivos: o das recusas do Rust, sem chave, à parte.

### 2.4 Os testes em `main.rs`

Módulo `o_som_grande_demais_decodificado_e_recusado_antes`, com bytes montados
no teste (WAV PCM; Ogg de três páginas com a identificação Vorbis ou Opus;
quadros MPEG-1 camada III a 48 kHz e 320 kbps, de 960 bytes e 1152 amostras,
com o Xing quando pedido). As recusas passam por `som_declarado_do_mod` com o
`pacote_de_teste` e conferem o motivo, o teto no motivo e o WARN no registro.

| teste | o caso grande, recusado | o que passa |
|---|---|---|
| `um_wav_que_passa_do_teto_decodificado_e_recusado_antes` | 8 bits, mono, 8 kHz, do tamanho do teto de arquivo: piso exato `251657180` (≈ 250 MB) | um segundo (piso `191996`, servido com os bytes exatos); um `data` que declara `0xFFFFFFFF` conta os bytes que estão; o WAV mínimo dá `None` |
| `um_ogg_que_passa_do_teto_decodificado_e_recusado_antes` | Vorbis estéreo 44,1 kHz com granule de três minutos (≈ 69 MB); Opus estéreo de três minutos mais o pre-skip | um minuto de Vorbis estéreo e de Opus mono ficam entre 59 e 60 s a 48 kHz |
| `um_mp3_que_passa_do_teto_decodificado_e_recusado_antes` | Xing de 7600 quadros estéreo (≈ 70 MB); estéreo sem Xing do tamanho do teto (≈ 100 MB, pela cadeia) | mono sem Xing do tamanho do teto (≈ 50 MB); um Xing de 7600 num arquivo com cem quadros fica nos cem |
| `o_som_do_servidor_que_passa_do_teto_decodificado_e_recusado_antes_de_servir` | o WAV grande por `ler_midia_do_servidor` e por `imagem_lida_dita` (com o WARN de origem `volume`) | o WAV curto do servidor sai com papel `som` e `uri` |
| `os_numeros_do_som_decodificado_sao_os_da_janela` | — | o guarda: lê `mods-regiao.js` e compara `TAXA_DA_DECODIFICACAO` e `bytesDeSomDecodificado` de `LIMITES_DA_REGIAO` e `LIMITES_DO_CARTAO` com as constantes do Rust |

**Vermelho** (com `bytes_decodificados_minimos` ainda um esboço que devolvia
`None`, e as três portas já ligadas a ele): quatro dos cinco vermelhos, pela
razão certa — «`som/vorbis.ogg`, que decodificado passa do teto do som, foi
servido: a janela o decodifica inteiro antes de poder recusá-lo… — o piso foi
None», o mesmo para `som/xing.mp3`, «o piso do WAV não é o exato que o `fmt ` e
o `data` dizem (left: None, right: Some(251657180))», e «o som do servidor que
passa do teto decodificado foi servido no `data:`». O guarda já nasceu verde
(os números já eram iguais); a prova dele é a reversão abaixo.

**Verde**: os cinco, e o filtro `som` inteiro (14).

### 2.5 As reversões, cada uma desfeita e conferida com `cmp`

Por braço, como o brief pede — devolver `None` naquele formato:

| desfeito | o que ficou vermelho |
|---|---|
| `"audio/wav" => return None` | o WAV: «`som/grande.wav`, que decodificado passa do teto do som, foi servido… — o piso foi None»; e o do servidor: «… foi servido no `data:`» |
| `"audio/ogg" => return None` | o Ogg: «`som/vorbis.ogg` … foi servido … None»; os outros verdes |
| `"audio/mpeg" => return None` | o MP3: «`som/xing.mp3` … foi servido … None»; os outros verdes |

Na primeira rodada do braço WAV, a asserção do piso exato vinha antes da
recusa, e o vermelho dizia «o piso não é o exato» em vez de «foi servido». A
ordem foi trocada no teste antes do commit, e a reversão refeita mostrou o
caso grande passando.

Os sub-braços:

| desfeito | o que ficou vermelho |
|---|---|
| MP3 confiando só no Xing (`quadros = contados`) | «o piso do MP3 cortado contou os quadros que o Xing alega, e não os que estão no arquivo: um download interrompido seria recusado pelo que não tem — Some(69962624)» |
| WAV sem o limite do `data` (`tamanho / por_quadro`) | «o piso do WAV contou o tamanho que o `data` alega, e não os bytes que estão no arquivo… left: Some(103079215076), right: Some(191996)» |
| a porta do servidor (`let _ = som_cabe_decodificado(&bytes);`) | «o som do servidor que passa do teto decodificado foi servido no `data:`» |
| a porta do volume (`.ok()` no lugar do `?`) | «o som lido por volume que passa do teto decodificado foi servido» |
| `LIMITES_DA_REGIAO.bytesDeSomDecodificado` em `65 * 1024 * 1024` | o guarda: «`LIMITES_DA_REGIAO.bytesDeSomDecodificado` não é o teto do Rust… left: Some(68157440), right: Some(67108864)» |
| o mesmo em `LIMITES_DO_CARTAO` | o guarda, com `LIMITES_DO_CARTAO` |
| `TAXA_DA_DECODIFICACAO = 44100` | o guarda: «`TAXA_DA_DECODIFICACAO` de `mods-regiao.js` não é a do Rust… left: Some(44100), right: Some(48000)» |

Na janela (`regiao-do-mod.cjs`, seção 19,
`oSomGrandeDemaisDecodificadoEOTetoDoSom`, com o molde do brief —
`bytesDoSom` rejeitando com `{ Recusado: { motivo: "som-grande-demais-decodificado: …" } }`):
- **Vermelho** antes do conserto, nos quatro cenários: a figura «falhou», e não
  «cheia»; o MOD ouviu `falhou`, com o motivo no `porque`; o registro dizia
  «falhou ao carregar»; a anotação sem a chave do teto; na aba, dois motivos.
- **Verde** depois: as 45 provas.
- Reversão do mapeamento em `carregarSom`: vermelhos o som de uma região, a
  contribuição no pedido de tocar e a vaga única na aba; o do servidor, verde
  (ele passa por `montarMidia`).
- Reversão do mapeamento em `montarMidia`: vermelho o som do servidor; os
  outros, verdes.

Os quatro cenários: o som de uma região na montagem (e nada decodificado, nem
botão); o som do servidor na montagem; a contribuição de dois destinos, nada
na montagem com `tocando: false` e a recusa uma vez, no destino que segura o
som, depois do `tocando: true`; e a vaga única na aba.

## 3. O que rodou

Sobre `9b43de1d`:
- `cargo test -p seele-app --bin seele-app som`: 14 passaram;
- `cargo test -p seele-app --bin seele-app` inteiro: 274 passaram (o binário que
  o lote mexe; nenhum teste dele usa a 8384);
- `cargo test -p seele-app --test frontend`: 279 passaram;
- `cargo xtask check-runtime`: as sete bancadas de Node, saída 0;
  `regiao-do-mod.cjs`: «as 45 provas … passam»;
- `cargo xtask check-api`: verde, cinco versões;
- `cargo fmt --all -- --check`: limpo (a tabela de taxas de bits do MP3 com
  `#[rustfmt::skip]`, para ficar legível linha a linha);
- `cargo clippy -p seele-app --all-targets --all-features -- -D warnings`:
  limpo; só o aviso ambiental do linker (`ld: duplicate -rpath`);
- a mais, `diagnostico-de-mods.cjs` (Playwright, no Chromium com a CSP do
  produto): verde. Ela finge `midia_do_mod` respondendo `uri` ao som do
  pacote, o que a janela não lê; não está na lista do lote, e ficou como
  estava.

Também depois do `e8a485cc`: o filtro `som` (9), `--test frontend` (279),
`check-runtime` e `check-api`, fmt e clippy, todos verdes.

## 4. O que não rodou

- **Conformidade**: o lote não a precisou, e não rodou (e estava pausada pela
  orquestradora durante o lote).
- **WebView2 e WKWebView**: a janela de verdade não se testa aqui. A recusa do
  Rust não depende de motor; o mapeamento da janela roda na bancada de Node e
  no Chromium da `diagnostico-de-mods.cjs`.
- **Windows e Linux**: nenhum código com `cfg` de plataforma neste lote.
- **Áudio de verdade**: os bytes dos testes são cabeçalhos sintéticos com
  silêncio; nenhum WAV, Ogg ou MP3 de um codificador real passou pelo piso.
  Os formatos estão escritos pelas especificações (RIFF/WAVE, Ogg/Vorbis I,
  RFC 7845, ISO 11172-3 e o quadro Xing/LAME).

## 5. Para a v6

Nada da promessa pública mudou: o `api/README.md` já diz «recusada, com os
números no `porque`», e é o que o MOD ouve. O que poderia virar texto da v6:
- o som cujo cabeçalho já mostra que ele passa do teto é recusado antes de
  decodificar, e o `porque` começa com `som-grande-demais-decodificado`;
- um som do servidor (pela resposta ou por volume) que passa do teto chega na
  montagem como `recusada`, mesmo numa contribuição, e não no pedido de tocar:
  é a resposta do servidor que vem recusada.

## 6. Preocupações

1. **O MP3 sem Xing não usa o piso «pela taxa máxima» do brief.** Usa a cadeia
   de quadros: contar bytes pela taxa mais alta conta como som o que vem
   depois dos quadros — uma etiqueta APE com a capa, um ID3v1, Lyrics3 —, e
   aí o piso passa do real. A cadeia conta só quadros, é exata para um arquivo
   honesto (CBR ou VBR) e nunca passa do real; o mono do tamanho do teto que o
   brief pede passa (≈ 50 MB). O Xing, quando há, limita a conta (o menor dos
   dois), em vez de ser a conta: sozinho, ele recusaria um download
   interrompido pelo que não está no arquivo — e o teste disso ficou vermelho
   quando o limite foi desfeito.
2. **A terceira porta, `imagem_lida_dita`**, está fora da lista do brief e
   dentro do mesmo motivo dele. Se a leitura por volume só pode trazer imagem
   por decisão de produto, o certo é recusar som ali pelo nome, e não pelo
   piso; não mudei isso.
3. **O piso confia no cabeçalho** na granule do Ogg e no Xing do MP3 (limitado
   pela cadeia). Um cabeçalho mentiroso só faz recusar o arquivo do próprio
   MOD, e nunca faz servir algo maior que o teto que a janela confere de
   qualquer jeito.
4. **Ogg sem CRC conferido**: a leitura segue as páginas pelo tamanho e para
   na primeira que não for uma página; uma página corrompida encurta o piso,
   não o aumenta.
5. A ponte interna mudou (o `uri` some do som do pacote): duas bancadas fora da
   lista (`diagnostico-de-mods.cjs` e os moldes de outras provas da própria
   `regiao-do-mod.cjs`) ainda fingem o `uri` no som do pacote. É inofensivo — a
   janela não o lê, e a prova principal do som agora responde sem ele —, mas
   o molde não é mais o que o Rust manda.

## 7. Rodada 1 de conserto (FIX_BASE `deba6aff`)

A revisão deu o lote como conforme e aceitou os desvios, com medida contra o
`decodeAudioData` real: 300 mil entradas mutadas sem pânico, e o piso abaixo
do real no Chromium e no WebKit. Esta rodada fecha o Importante I1 e os
Menores M1 a M6, um commit por item. Brief da rodada:
`.superpowers/sdd/lotes/lote-k4-rodada1.md`.

| commit | item | o que muda |
|---|---|---|
| `f9ef3d23` | **I1** | três testes, um por margem do piso: o bloco do WAV, o pre-skip do Opus e o corte do MP3 |
| `a9a47a8b` | **M1** | um WAV com dois `fmt ` fica sem piso (`None`) |
| `0b9029ec` | **M2** | o `data` do WAV é limitado também ao tamanho do RIFF, salvo `0xFFFFFFFF` |
| `de6c6774` | **M3** | doc: o «nunca passa do real» do MP3 vale num fluxo bem formado; o corte absorve uns oito quadros corrompidos |
| `a21c2c04` | **M4** | doc: numa contribuição, a recusa do som do pacote chega no pedido de tocar, e a do servidor na montagem, uma vez por destino |
| `29b58efc` | **M5** | o fundo, o retrato, o fundo da tela e o avatar contribuído recebem o som recusado pelo teto como o som que era (`soImagem`), e o MOD ouve deles o que ouvia |
| `62790259` | **M6** | os dez moldes apontados respondem ao som do pacote sem `uri` |

### I1 — as margens que fazem do valor um piso (`f9ef3d23`)

Três testes no módulo `o_som_grande_demais_decodificado_e_recusado_antes`, com
bytes sintéticos e o valor exato do piso. A margem já estava no código; o que
faltava era o teste que a prende, e cada um nasceu verde e foi provado
tirando a margem dele — o arquivo conferido com `cmp` depois de cada uma.

| teste | o caso | sem a margem (vermelho) |
|---|---|---|
| `o_bloco_menor_que_a_amostra_nao_multiplica_o_piso_do_wav` | WAV estéreo de 16 bits a 8 kHz, um segundo, com o bloco declarado 1 em vez de 4: o piso é `383992` | `bloco.max(…)` só com o bloco declarado: «o piso do WAV contou os quadros pelo bloco que o cabeçalho declara, menor que a amostra: quatro vezes o real…», `Some(1535992)` contra `Some(383992)` |
| `o_pre_skip_maior_que_a_primeira_pagina_sai_do_piso_do_opus` | Opus estéreo com pre-skip 3840 e a primeira página de dados em 1024: o piso é um segundo, `383992` | `primeira?.max(pre_skip)` sem o pre-skip: «o piso do Opus tirou do começo só a granule da primeira página…», `Some(406520)` contra `Some(383992)` |
| `o_corte_do_decodificador_sai_do_piso_do_mp3` | cem quadros MPEG-1 camada III estéreo: 115 200 amostras, menos 9871 do corte, menos um da conversão, vezes oito: `842624` | `corte = 0`: «o piso do MP3 contou as amostras que o decodificador corta…», `Some(921592)` contra `Some(842624)` |

### M1 — um segundo `fmt ` (`a9a47a8b`)

`um_segundo_fmt_deixa_o_wav_sem_piso`: um WAV com dois `fmt ` (o primeiro
estéreo, 16 bits, 48 kHz; o segundo mono, 8 bits, 8 kHz) e o RIFF acertado.
- **Vermelho** antes: `Some(4607996)` contra `None` — o piso pelo último
  `fmt `, doze vezes o segundo real do primeiro.
- **Conserto**: em `cabecalho_de_som::wav`, um `fmt ` com um formato já lido
  devolve `None`. O doc diz por quê: qual dos dois vale é pergunta de motor.
- **Verde**; **reversão** (`if false && formato.is_some()`): o mesmo vermelho;
  `cmp` igual.

### M2 — o `data` limitado ao RIFF (`0b9029ec`)

`o_data_do_wav_nao_passa_do_tamanho_do_riff`: um segundo de WAV com o `data`
em `0xFFFFFFFF`, seguido de um pedaço `LIST` de 4000 bytes fora do tamanho que
o RIFF declara.
- **Vermelho** antes: `Some(288188)` contra `Some(191996)` — o `LIST` contado
  como som.
- **Conserto**: `wav` corta os bytes no fim que o RIFF declara (`8 + riff`),
  salvo quando o RIFF também é `0xFFFFFFFF`. Um RIFF menor que o próprio
  cabeçalho deixa o arquivo sem piso, o que não recusa nada.
- **Verde** (e os WAV de antes, cujo RIFF cobre o arquivo); **reversão**
  (`if true || riff == u32::MAX`): o mesmo vermelho; `cmp` igual.

### M3 e M4 — os docs (`de6c6774`, `a21c2c04`)

- **M3**: o doc de `cabecalho_de_som::mp3` diz que o «nunca passa do real» vale
  para um fluxo bem formado — um quadro com cabeçalho certo e conteúdo
  corrompido entra na cadeia, o Chromium o descarta e o WebKit recusa o
  arquivo —, que o corte absorve uns oito quadros de 1152 amostras, e que com
  mais que isso o piso passa do que o Chromium entrega (a revisão mediu 14 KB
  com dez). O resumo de `bytes_decodificados_minimos` diz «num arquivo bem
  formado» e ganha o limite do RIFF do M2, que o commit do M2 não tinha posto
  ali.
- **M4**: o doc de `recusarPeloTetoDoSom` e o de `montarSom` dizem os dois
  momentos de uma contribuição: o som do pacote recusado nos bytes, no pedido
  de tocar, uma vez, no destino que segura os sons; o do servidor recusado na
  resposta, na montagem, **uma vez por destino**, como o `falhou` de antes —
  o MOD ouve `recusada` de cada destino, e o registro ganha uma linha por
  destino. Sem deduplicar, como o brief pede.

### M5 — quem só mostra imagem ouve o que ouvia (`29b58efc`)

**A decisão** (do controlador): para o fundo, o retrato, o fundo da tela e o
avatar contribuído vale o comportamento de antes do lote — o «precisa ser
imagem» vem antes da recusa do som, porque o que o MOD ouve é contrato.

**Produto.** `soImagem(vindo)`, em `mods-regiao.js`, ao lado de
`recusaDoSomGrandeDemais`: troca a rejeição `som-grande-demais-decodificado`
por `{ papel: "som" }`, e deixa passar as outras rejeições como vieram. Os
quatro lugares embrulham o pedido da mídia nela, e cada um segue o caminho
que já tinha para um som — o papel é tudo o que cada um lê antes de recusar.
Quem toca (`montarMidia`, `carregarSom`) continua dizendo o teto do som.

**`base.js` mudou em código**, uma linha: o avatar contribuído
(`avatarContribuido`) embrulha `dono.carregarMidiaDoServidor(…)` em
`soImagem`. O brief da rodada previa isso como possibilidade. A bancada das
contribuições (`contribuicoes-e-camadas.cjs`, fora da lista do lote) recorta
`avatarContribuido` de `base.js` e traz de `mods-regiao.js` o que ele chama;
sem trazer `soImagem`, ela reprovou no `check-runtime` com
`ReferenceError: soImagem is not defined` — o que mostrou a dependência.
Ela passou a trazer `SOM_GRANDE_DEMAIS_DECODIFICADO`, `recusaDoSomGrandeDemais`
e `soImagem`, e ganhou o caso 15 do R4e: a recusa do som pelo teto chega ao
MOD como «não é imagem».

**Provas**, cada uma comparando o que se ouve com a resposta de antes do lote
(o som chegando com o papel) e com a de agora (a recusa do Rust) — o evento ao
MOD, as linhas do registro e as anotações da aba:
- `regiao-do-mod.cjs`, seção 20, `quemSoMostraImagemOuveOQueOuviaDoSomRecusado`:
  o fundo, o retrato e o fundo da tela. **Vermelho** antes, nos três: com a
  recusa, o fundo dizia `falhou` ao MOD e «falhou no fundo» no registro, e o
  retrato e o fundo da tela anotavam o motivo do som em vez do «precisa ser
  imagem».
- `diagnostico-de-mods.cjs`, `oAvatarOuveDoSomRecusadoOQueOuvia`, no Chromium
  com a página de verdade. **Vermelho** antes: o MOD ouvia `falhou` com
  `som-grande-demais-decodificado…`, e não com «o avatar não é imagem ou excede
  o limite de mídia».
- **Verde** depois, nas três bancadas.
- **Reversões**, cada uma desfeita e conferida com `cmp`: `soImagem` como
  passagem (`return vindo;`) — os três lugares da região e o avatar
  vermelhos; cada embrulho tirado sozinho — o fundo, o retrato, o fundo da
  tela, cada um vermelho no seu caso; o embrulho do avatar tirado — vermelhos
  o `oAvatarOuveDoSomRecusadoOQueOuvia` e o caso 15 do R4e («o MOD não recebeu
  «midia»/«falhou» do avatar que não montou (não é imagem)»).

### M6 — os moldes sem `uri` (`62790259`)

Os dez moldes apontados — `diagnostico-de-mods.cjs:1428` e `:1597`,
`regiao-do-mod.cjs:1158`, `1248`, `1286`, `1403`, `1468`, `1528`, `2346` e
`2392` — respondem ao som do pacote com `{ papel, bytes }`. Conferi cada um
antes: todos respondem a um nó com `fonte` (ou a `midia_do_mod`). As duas
bancadas continuam verdes.

### O que rodou, sobre `62790259`

- `cargo test -p seele-app --bin seele-app som`: 19 passaram;
- `cargo test -p seele-app --test frontend`: 279 passaram;
- `cargo xtask check-runtime`: as sete bancadas, saída 0 (`regiao-do-mod.cjs`:
  46 provas);
- `cargo xtask check-api`: verde;
- `diagnostico-de-mods.cjs` (Playwright): verde, dez provas;
- fmt limpo, e o clippy do `seele-app` só com o aviso ambiental do linker.

Sem conformidade, sem push, sem subagentes.

### Preocupações da rodada

1. **Há mais moldes com `uri` no som do pacote do que os dez.** Uma leitura
   por proximidade (`fonte: "som/…"` sem `doServidor` na mesma prova) acha uns
   vinte em `regiao-do-mod.cjs` — por exemplo as linhas 1744, 1852, 1899,
   3056, 3083, 3197, 3433 e 4958 de hoje. Outros servem às duas origens na
   mesma prova (o `midiaDeSom` da seção 17, com «do pacote» e «do servidor do
   MOD»), e esses precisam do `uri`. Fiquei nos dez da lista: mudar os outros
   é a mesma troca, prova por prova, e a janela não lê esse `uri` em nenhum
   deles.
2. **O M5 depende de `soImagem` estar no contexto de quem roda `base.js`.**
   Na página, `mods-regiao.js` carrega antes; numa bancada que recorta o
   avatar, ela tem de trazer as três peças, como a das contribuições agora
   traz.
3. Sem mudança: o som do servidor numa contribuição segue recusado uma vez por
   destino (M4), como o `falhou` de antes; a deduplicação ficou para depois,
   como o brief pede.
