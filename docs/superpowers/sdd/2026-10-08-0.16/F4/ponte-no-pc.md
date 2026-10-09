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
| `descrever_arquivo` | o arraste de arquivo | **com adaptação:** no navegador o arraste dá um `File`, e não um caminho de disco. A forma da chamada é a combinar com a F8 e a F5 |
| `compartilhar_tela`, `parar_de_compartilhar`, `fontes_de_tela`, `permissao_de_tela`, `pedir_permissao_de_tela`, `ajustar_limites_da_tela`, `exclusao_do_som_da_captura` | compartilhar a tela pelo PC | **tarefa nova (3.10), depende de medida.** O `getDisplayMedia` escolhe a fonte e pede a permissão ele mesmo |
| `modulo_de_video_a_baixar` | — | responde «nenhum»: quem codifica é o navegador |

## Atendidas com a resposta de quem não hospeda

Comandos que mexem na hospedagem local, e que a casca chama mesmo assim. No
navegador, a resposta é a de quem não hospeda, e não uma recusa:

| comando | a resposta no navegador | por quê |
|---|---|---|
| `estado_da_porta` | «não hospeda» | chamado sem gesto, na sessão (matriz da F8) |
| `estou_hospedando` | `false` | a casca pergunta antes de mostrar o que é de quem hospeda |
| `conjunto_exigido_agora` | `""` | o desktop responde assim a quem não hospeda (`main.rs`, sem `persistence_do_mod`) |
| `disconnect` | sai da sessão | no desktop ele também derruba a hospedagem; no navegador só há a sessão |

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

## A decidir com a F8

- `abrir_teste_de_microfone`, `estado_do_teste_de_microfone`,
  `monitorar_o_microfone`: a matriz os põe fora por causa da D-52 (o áudio abre
  na sala). Se a casca de desktop mantém o teste fora da sala, o PC o atende com
  o `getUserMedia`. Se não mantém, ficam como estão.

## Como se prova

**A paridade do desktop no navegador se prova pelos 28 roteiros de
`tools/roteiros/`**, rodados contra a casca larga com a ponte. A bancada da 3.1
que anota todo `invoke` sem gesto vale para as duas cascas.

## O guarda do critério (pedido pela orquestradora em 09/10)

**Comando a comando, lendo o `main.rs`:** todo comando cujo corpo mexe na
hospedagem local (`hospedagem`, `persistence_hospedada`, `portaria::`,
`persistence_do_mod`) tem de estar no `RECUSAS_NOMEADAS` da ponte, ou numa
lista explícita de neutros de quem não hospeda, com o motivo. Nunca no
`COMANDOS_DA_PONTE` sem estar nela. Um comando novo que mexa na hospedagem
reprova o guarda até alguém decidir de que lado ele fica.

**Medido em 09/10, sobre a `versao/0.16` de `f7fb8ab9`:** dos 156 comandos, 13
mexem na hospedagem local. Nove ficam recusados: `apagar_servidor`,
`aplicar_conjunto_de_mods`, `criar_convite_do_server`, `decidir_pedido`,
`definir_senha_do_server`, `hospedar`, `ligar_portaria`, `pedidos_da_portaria`
e `revogar_admissao`. Os outros quatro são os neutros da tabela acima.

**O guarda já pegou um erro desta página:** o `conjunto_exigido_agora` estava
na lista de atendidas, sem dizer que mexe na persistência de MODs de quem
hospeda. Ele passou para os neutros, com o motivo.

