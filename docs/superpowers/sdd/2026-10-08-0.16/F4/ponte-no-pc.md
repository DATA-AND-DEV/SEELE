# A ponte no PC: as recusas da matriz revistas para a casca larga (D-web-3)

**De onde vem.** A orquestradora decidiu em 09/10 (D-web-3): o web app também
serve no PC, **com a casca de desktop**, para quem quer entrar sem instalar
nada e não vai hospedar. Isso revê a decisão anterior, que dava a casca do
celular ao notebook.

No navegador largo, a `ui/` de hoje roda como está, e a ponte a atende inteira.
O escopo é «tudo menos hospedar», e não só consumo.

**O que esta página faz.** Revê, para o PC, as 68 recusas da matriz da F8
(`F8/paridade.md`, §8, `e7ba0dd0`): 51 com nome e 17 com frase. A matriz
continua valendo para o celular.

**O critério da orquestradora:**
- **ficam recusadas, com frase:**
  - hospedar, e o que só existe para quem hospeda;
  - ligar pelo código (sem UDP);
  - as versões e a linha legada;
  - o atualizador e o que é do sistema;
- **passam a ser atendidas:** a moderação, para quem tem o papel. É protocolo,
  e a identidade do navegador recebe papéis como qualquer outra;
- **a tecla de falar** vale só com a página em foco.

**Como o lado foi decidido, comando a comando.** Pelo `main.rs`:
- o que vai por `session.connection` é protocolo, e alguém com o papel o manda
  de longe;
- o que mexe na `hospedagem` local (`portaria::*`, `persistence_hospedada`,
  `seele_server::admissao`, `seele_server::mods`) só existe na máquina de quem
  hospeda.

## Passam a ser atendidas no PC

| comando | por quê | nota |
|---|---|---|
| `banir_pessoa`, `expulsar_pessoa`, `mover_pessoa` | moderação, pela conexão | o servidor confere o papel |
| `criar_linha`, `apagar_linha`, `renomear_linha`, `peso_da_linha` | administração de canais, pela conexão | estendido da «moderação», pela mesma natureza (protocolo com papel); **confirmado pela orquestradora em 09/10**, e é o §2.8 do spec da F8 |
| `criar_voice_room`, `apagar_voice_room`, `renomear_voice_room` | idem, salas de voz | idem |
| `renomear_server`, `aplicar_icone_do_server`, `escolher_icone_do_server`, `tirar_icone_do_server` | idem, nome e ícone | idem |
| `preparar_icone_do_server` | escolhe a imagem | no desktop é o seletor nativo (`escolher_e_encolher`). No PC, `<input type=file>` e a mesma redução, feita na página |
| `escolher_tecla_de_falar` | a tecla de falar | guardada no armazenamento da origem; **vale só com a página em foco** |
| `tela_cheia` | o palco em tela cheia | a Fullscreen API do navegador, no lugar da janela |
| `ajustar_reducao_de_ruido` | a supressão de ruído | a restrição `noiseSuppression` do `getUserMedia` |
| `saidas`, `escolher_saida`, `saida_escolhida` | a saída de som | `setSinkId` onde houver. Onde não houver, a lista é só «a do sistema», e não uma recusa: hoje a recusa de `saidas` derruba a lista de entrada (achado 4 da matriz) |
| `descrever_arquivo`, `escolher_arquivo`, `guardar_colado`, `enviar_anexo` | o arquivo, pelas três entradas (arraste, seletor e colar) | **por alça** (combinado com a F8 em 09/10). Ver «O arquivo sem caminho», abaixo |
| `compartilhar_tela`, `parar_de_compartilhar`, `fontes_de_tela`, `permissao_de_tela`, `pedir_permissao_de_tela`, `ajustar_limites_da_tela`, `exclusao_do_som_da_captura` | compartilhar a tela pelo PC | **tarefa nova (3.10), depende de medida.** O `getDisplayMedia` escolhe a fonte e pede a permissão ele mesmo |
| `modulo_de_video_a_baixar` | — | responde «nenhum»: quem codifica é o navegador |
| `abrir_teste_de_microfone`, `estado_do_teste_de_microfone`, `monitorar_o_microfone` | o teste do microfone fora da sala | **decidido pela orquestradora em 09/10:** no PC ele existe, pelo `getUserMedia`; no celular continua recusa com frase (D-52). Ver «O teste do microfone», abaixo |

## Mexem na hospedagem e não são recusadas

Comandos que, no `main.rs`, mexem na hospedagem local, e que a casca chama
mesmo assim. Aprovado pela orquestradora em 09/10.

**`NEUTROS_DE_QUEM_NAO_HOSPEDA`**, uma lista explícita na ponte, com a resposta
e o motivo ao lado de cada um:

| comando | a resposta, sempre esta | por quê |
|---|---|---|
| `estado_da_porta` | «não hospeda» | chamado sem gesto, na sessão (matriz da F8) |
| `estou_hospedando` | `false` | a casca pergunta antes de mostrar o que é de quem hospeda |
| `conjunto_exigido_agora` | `""` | o desktop responde assim a quem não hospeda |

**`disconnect` é atendido, e não neutro.** Sair da sessão é do navegador também.
No desktop, ele também derruba a hospedagem; no navegador, não há hospedagem
para derrubar. Se ele caísse nos neutros, sair da sessão passaria a fazer
nada, e isso seria defeito. Ele fica numa segunda lista explícita,
`ATENDIDOS_APESAR_DA_HOSPEDAGEM`, com esse motivo.

## Ficam recusadas no PC, com frase

| comando | por quê |
|---|---|
| `hospedar`, `criar_servidor`, `apagar_servidor`, `servidores_guardados`, `servidor_hospedado`, `linhas_para_hospedar` | hospedar |
| `abrir_a_porta_no_firewall`, `caminho_entre_pares`, `consentir_no_caminho_entre_pares` | do anfitrião e da rede dele (o navegador não aceita conexão de entrada) |
| `criar_convite_do_server`, `decidir_pedido`, `definir_senha_do_server`, `ligar_portaria`, `pedidos_da_portaria`, `revogar_admissao` | a portaria do próprio servidor, na `hospedagem` local |
| `aplicar_conjunto_de_mods`, `catalogo_de_mods`, `instalar_mod`, `pacotes_no_cache`, `apagar_pacote_do_cache` | instalar e habilitar MODs, na máquina de quem hospeda |
| `abrir_versao`, `baixar_versao`, `remover_versao`, `ocupacao_do_deposito` | as versões e a linha legada |
| `instalar_atualizacao`, `procurar_atualizacao` | o atualizador: a versão é a da página |
| `abrir_ajustes_do_microfone`, `abrir_ajustes_de_acessibilidade` | do sistema |
| `baixar_modulo_de_video` | não há módulo a baixar no navegador |
| `mod_nativo_*` (5) | não existem no web: o executor é o `executorNoNavegador`, e estes nunca são chamados |

## O arquivo sem caminho (combinado com a F8 em 09/10)

No navegador não há caminho de disco. **A forma das chamadas não muda**, e o
«caminho» vira uma alça opaca que a ponte cria (`web-arquivo:N`) e guarda num
mapa com o `File` ou o `Blob`:
- **o arraste:** a ponte ouve o `drop` do documento (e o `dragover`, para
  permiti-lo), guarda cada `File` sob uma alça e emite o mesmo
  `tauri://drag-drop` com `{ paths: [alça] }` que a casca já ouve
  (`tela-sessao.js:4388`);
- **o seletor:** `escolher_arquivo` abre um `<input type=file>` e devolve a
  alça. Desistir devolve `null`, como no desktop;
- **o colar:** `guardar_colado({ tipo, base64 })` guarda o `Blob` sob uma alça;
- **o envio:** `descrever_arquivo({ caminho: alça })` descreve, e
  `enviar_anexo({ caminho: alça, … })` lê o `File` do mapa e o sobe pelo
  WebTransport. O `tela-sessao.js` não muda em nenhuma das duas cascas.

**As condições, que são da ponte e têm teste:**
1. **o mapa tem teto e solta o que guarda**: depois do envio, ao descartar o
   anexo e no fim da sessão. Um `Blob` colado está todo na memória;
2. **uma alça desconhecida, ou já solta, é recusada.** A casca já diz «NÃO
   CONSEGUI LER ESSE ARQUIVO»;
3. **o `escolher_arquivo` abre o seletor sem nenhum `await` antes do
   `input.click()`.** O navegador só abre o seletor dentro do gesto, e um
   `await` no meio pode gastar a ativação;
4. **desistir do seletor devolve `null`** (o evento `cancel` do
   `<input type=file>`), e não deixa uma promessa pendurada.

A F5 só entra se a forma do que o `descrever_arquivo` devolve tiver de mudar.

## O teste do microfone (decidido em 09/10)

**No PC, o teste do microfone fora da sala existe na 0.16; no celular continua
fora** (a orquestradora, avisada pela F8). A ponte atende as três chamadas pelo
`getUserMedia` na casca larga, e na casca do celular elas continuam no
`RECUSAS_NOMEADAS`, com frase. **As listas da ponte passam a depender da
casca**, e o guarda «todo `invoke` está na ponte ou numa recusa nomeada» confere
as duas.

- **o `fechar_teste_de_microfone` é atendido sempre**, nas duas cascas e com ou
  sem o teste aberto. O `pararOTesteDeMicrofone` o chama a cada saída de ÁUDIO
  (`tela-server.js:150`, `:684`), e uma recusa sujaria o console a cada troca.
- **fechar solta o aparelho de verdade:** as faixas do `getUserMedia` param
  (`track.stop()`), e o navegador apaga o indicador do microfone. **O teste:**
  abrir e fechar o teste na bancada não deixa faixa viva (`readyState` de cada
  faixa aberta passa a `ended`). Reversão: fechar sem parar as faixas reprova.

## Como se prova

**A paridade do desktop no navegador se prova pelos 28 roteiros de
`tools/roteiros/`**, rodados contra a casca larga com a ponte. A bancada da 3.1
que anota todo `invoke` sem gesto vale para as duas cascas.

## O guarda do critério (pedido pela orquestradora em 09/10)

**Comando a comando, lendo o `main.rs`:** todo comando cujo corpo mexe na
hospedagem local (`hospedagem`, `persistence_hospedada`, `portaria::`,
`persistence_do_mod`) tem de estar em uma de três listas da ponte, e nunca solto
no `COMANDOS_DA_PONTE`:
1. `RECUSAS_NOMEADAS`;
2. `NEUTROS_DE_QUEM_NAO_HOSPEDA`, com a resposta e o motivo;
3. `ATENDIDOS_APESAR_DA_HOSPEDAGEM`, com o motivo (hoje, só o `disconnect`).

Um comando novo que mexa na hospedagem reprova o guarda até alguém decidir de
que lado ele fica.

**O guarda confere também a resposta de cada neutro**, e não só o nome. Na
bancada, cada neutro é chamado pela ponte, e a resposta tem de ser a escrita na
tabela acima. Se alguém trocar a resposta de um neutro por um atendimento de
verdade, o guarda reprova.

**As reversões:**
- tirar um dos recusados da recusa reprova o guarda;
- trocar a resposta do `estou_hospedando` por outra reprova o guarda;
- pôr o `disconnect` nos neutros faz a bancada reprovar, porque sair da sessão
  deixa de sair.

**Medido em 09/10, sobre a `versao/0.16` de `f7fb8ab9`:** dos 156 comandos, 13
mexem na hospedagem local:
- 9 ficam recusados: `apagar_servidor`, `aplicar_conjunto_de_mods`,
  `criar_convite_do_server`, `decidir_pedido`, `definir_senha_do_server`,
  `hospedar`, `ligar_portaria`, `pedidos_da_portaria` e `revogar_admissao`;
- 3 são neutros;
- 1 é atendido apesar da hospedagem (o `disconnect`).

**O guarda já pegou um erro desta página:** o `conjunto_exigido_agora` estava
na lista de atendidas, sem dizer que mexe na persistência de MODs de quem
hospeda. Ele passou para os neutros, com o motivo.
