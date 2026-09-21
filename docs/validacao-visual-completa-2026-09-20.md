# Validação visual do SEELE — 20/09/2026

Rodada interrompida pelo bloqueio da sessão macOS. Não há aprovação visual
global: 17 achados registrados, com telas observadas e lacunas discriminadas
na matriz de cobertura. A inspeção foi feita no aplicativo nativo e nos três
MODs, com um administrador e um visitante fictício.

Base local: `08b3788` com os ajustes descritos em
[ajustes-visuais-mods-2026-09-20.md](ajustes-visuais-mods-2026-09-20.md).
Release público conferido nesta rodada: v0.12.1, publicado em 20/09/2026.
Cópia SEELE QA, casa `/tmp/SEELE-QA-08b3788/home`, áudio desabilitado apenas
nessa cópia devido ao bloqueio CoreAudio já registrado. Capturas e árvores de
acessibilidade foram lidas diretamente pelo controle nativo do macOS.

## Critérios

Espaçamento de campo/grupo/seção, densidade, legibilidade, contraste percebido,
hierarquia de ações, alinhamento, cortes, rolagem, estados vazios, consistência
entre produto e MODs, foco e saídas. Contraste percebido não equivale a uma
medição certificada de razão de contraste.

## Cobertura inicial observada

- Entrada: padrão, detalhes técnicos expandidos, nome público expandido,
  tentativa de conexão e falha.
- Conectar: histórico vazio e campo de endereço.
- Perfil nativo: imagem vazia, apelido e ações.
- Hospedar: lista existente e formulário de novo servidor.
- Configurações: Microfone e som (topo e fim), Atalhos, Aparência, Identidade.
- MODs: Instalados, Catálogo vazio e preenchido, Armazenamento, Permissões
  preenchidas e vazias após revogar o consentimento da própria casa QA.
- Aplicativo: Versões sem versões paralelas, Malha, Atualização antes e depois
  da busca, com nova versão disponível. Instalação de release não executada.

## Achados registrados durante o percurso

### V01 — A navegação de Configurações ainda é densa

Mesmo com espaço amplo, notas de contexto ocupam a coluna de navegação e
empurram Atualização para baixo da dobra em 1229×768. Fora de sessão, o grupo
Este servidor mantém uma explicação e uma lista vazia. O conteúdo repete
subtítulos já presentes no cabeçalho (especialmente Identidade e MODs).

Reduzir a navegação a grupos e destinos; ocultar grupos sem destinos; manter
uma explicação curta no painel correspondente. Usar uma escala única de
espaçamento para campo, grupo e seção. Voltar deve parecer uma ação disponível,
e não um botão apagado.

### V02 — Texto secundário pequeno e fraco

Atalhos, rótulos espaçados, estados de permissões e notas da navegação são
visualmente muito pequenos e pouco contrastantes. Há títulos em caixa alta,
tracking amplo e descrições longas competindo com controles.

Aumentar a legibilidade de instruções necessárias; reservar a tipografia
mínima para metadados dispensáveis. Verificar os tokens também com captura
em janela menor e com contraste do sistema, sem presumir aprovação WCAG.

### V03 — Pacotes indistinguíveis na lista Instalada

Há três revisões locais de MESA 3.0.0 e duas dos outros MODs. A lista mostra
mesmo identificador, versão e estado, sem permitir distinguir a revisão;
o hash só aparece em Armazenamento. Além disso, a lista tem rolagem interna
no painel já rolável. LIGAR desabilitado parece visualmente utilizável.

Agrupar por MOD, mostrar a revisão selecionada e permitir expandir outras
revisões; oferecer identificação curta quando a versão se repete. Uma área
de rolagem principal por painel e aparência inequívoca de ação indisponível.

### V04 — Catálogo chama downgrade de atualização

Observado: pacote local 3.0.0, catálogo 2.0.0, aviso dessa diferença e botão
ATUALIZAR. Nenhuma instalação foi feita. O nome da ação deve distinguir
atualizar, reinstalar e usar versão anterior. As capacidades são apresentadas
como linhas emolduradas de largura inteira, aumentando a altura dos cartões.

### V05 — Entrada e criação de servidor têm hierarquia irregular

O texto introdutório fica perto demais de Conectar, enquanto sobra espaço
entre marca e descrição. Expandir duas ajudas transforma a entrada numa página
longa. O retorno de Preparar servidor vai à entrada, apesar de ter sido aberto
pela lista de servidores. Escolher imagem compete em laranja com Hospedar.

Separar introdução e ação; recolher informação técnica com menor protagonismo;
respeitar a navegação anterior e reservar destaque principal para Hospedar.

### V06 — Mensagem de conexão não reflete o endereço testado

A tentativa foi para `127.0.0.1:1`, mas a falha recomenda abrir 8383 UDP.
O estado de tentativa aparece no fim do cartão, sem cancelamento visível.
Mostrar destino real, ação de cancelar e um diagnóstico que não presuma uma
porta diferente. Evitar apresentar toda a explicação em vermelho pequeno.

### V07 — Identidade e atualização usam texto explicativo em excesso

Perfil nativo explica que o limite está sendo mostrado antes de escolher a
imagem, em vez de simplesmente informar o limite. TIRAR aparece disponível
sem imagem. Atualização exibe três parágrafos de advertência sobre sistemas,
reinício e atomicidade. A versão atual não aparece antes da busca nessa cópia.

Simplificar a mensagem principal; colocar detalhes técnicos em expansão;
mostrar a consequência concreta da ação e a versão atual. Desabilitar ou
ocultar TIRAR quando não há imagem.

## Continuação da cobertura nativa

- Aceite dos três MODs (lista e ações fixas); convite de hospedagem.
- Sessão vazia, mensagem de dois parágrafos enviada, busca com uma ocorrência
  e sem ocorrência. Compositor cresce com texto; a instrução vazia é longa e
  seu encaixe inicial merece ajuste.
- Cartão lateral corrigido: nesta rodada o retrato, nome, pronomes e status
  foram vistos completos. Fecha a lacuna da captura na rodada anterior.
- Ajuda; criação e renomeação de canal; confirmação de exclusão cancelada;
  criação de sala (cancelada); chamada de uma pessoa e palco sem transmissão.
- Compartilhamento: seletor e estado de módulo de vídeo ausente.
- Configurações do servidor: A porta, Servidor (nome/imagem), portaria sem
  pedido pendente e com pedido do cliente fictício Visitante QA.
- Segundo aplicativo isolado: `target/qa-auditoria/SEELE QA Visitante.app`,
  casa `/tmp/SEELE-QA-auditoria-visitante`. Entrada aguardando admissão e aceite
  dos MODs após admissão. Não foi usado outro dispositivo nem uma pessoa real.

### V08 — Os modais não seguem uma composição única

Novo canal e renomear têm cabeçalho pequeno; excluir canal tem título muito
maior, ações de largura inteira e FECHAR adicional no rodapé. Perfil nativo,
ajuda e MODs usam outras combinações de cabeçalho, saída e margem. A geometria
quadrada é comum, mas a família visual ainda não é coerente.

Criar variantes explícitas de um mesmo sistema: formulário, confirmação e
consulta. Preservar destaque proporcional à consequência, com régua comum de
margem, título, corpo, rodapé e foco. Evitar FECHAR e CANCELAR redundantes.

### V09 — Informação técnica ocupa espaço de tarefas na sessão

O nome do MOD na navegação é truncado para caber o identificador técnico.
Na chamada, o grande SINAL 100 disputa a atenção com a identidade da pessoa;
D·05 aparece no título do palco. O convite inclui diagnóstico de rede extenso,
com trecho concatenado como «outro.o roteador».

Priorizar nomes e tarefas, deslocar autoria técnica para tooltip/detalhes,
remover códigos editoriais da interface e recolher telemetria. No convite,
mostrar disponibilidade e copiar link; diagnóstico de rede em expansão.

### V10 — Seletor de compartilhamento sem orientação visual suficiente

O seletor nativo lista monitores por número e janelas auxiliares como
WindowManager, StatusIndicator e entradas 0×0. Não há miniaturas úteis para
identificar a janela. O estado de módulo ausente ocupa quase metade do modal
com explicação de codec, licença e URL extensa.

Filtrar superfícies internas/inválidas; apresentar miniaturas e nomes de
aplicativos; separar Monitores e Janelas. Explicar a dependência numa frase
com download e detalhes opcionais. Não foi iniciada transmissão de conteúdo
pessoal ou de outros aplicativos.

### V11 — Espera de admissão exige ler um manual

O visitante vê uma coluna estreita com quatro parágrafos em marcadores sobre
persistência do pedido, conexão, retentativa e bloqueio. A maior parte da tela
fica reservada a dois registros técnicos. O estado principal diz «tente de
novo mais tarde» enquanto o contador informa retentativa automática.

Usar um estado central «Aguardando aprovação de quem hospeda», nome/endereço
do servidor, contador discreto e Voltar. Detalhes técnicos recolhidos. Dizer
claramente que a tela tentará novamente sozinha, sem ameaçar quem usa com o
funcionamento do controle de frequência.

### V12 — Portaria prioriza a chave antes da pessoa

O pedido começa por uma impressão digital de 64 caracteres; o apelido aparece
em texto secundário como «diz chamar-se». Senha, convite e admissão ficam
amontoados no mesmo modal. A confirmação repete a chave em parágrafo longo.

Apresentar a identidade declarada, o estado e a origem do pedido primeiro;
manter a verificação da chave disponível e inequívoca, num bloco próprio.
Separar configurações de acesso da fila de pessoas. Não reduzir garantias de
identidade para simplificar a apresentação.

## Percurso com visitante e conteúdo longo

O visitante foi admitido pela interface do administrador. A primeira tentativa
de obter os MODs falhou porque a candidata local é 3.0.0 e o catálogo público
continha 2.0.0. Para prosseguir, as três pastas dos pacotes já conferidos da
casa QA foram copiadas para o cache da casa visitante; os fontes de cliente
foram comparados byte a byte. Não foram copiados consentimentos ou identidade.
O visitante repetiu o aceite pela interface. Esta preparação é uma fixture
local; a falha de distribuição da candidata não prova falha da v0.12.1 pública.

Foram observados no visitante:

- Primeiro contato com impressão digital e dispensa do aviso do próprio
  servidor QA; sessão fora de sala e chamada com dois clientes locais.
- PERFIS: consulta do perfil de outra pessoa; editor inicialmente vazio;
  prévia com nome longo, status longo e biografia de três parágrafos; gravação;
  fechamento por Escape; cartão lateral; diretório com duas pessoas.
- ESTILO: consulta sem permissão administrativa, tema compartilhado desligado.
- MESA: cinco abas como jogador, estados vazios de tabuleiro, fichas,
  compêndio, iniciativa e registro; rolagem de dado e registro preenchido.
- Navegação PERFIS → MESA e MESA → entrar na sala, que revelaram V14.

### V13 — Cartão longo é cortado e prévia não representa o destino

O cartão curto ficou completo com o teto de 180px. Isso **não** resolve conteúdo
longo: com nome de duas linhas, status e biografia, a lateral corta o status
horizontalmente e interrompe a biografia no limite do cartão, sem indicação
visual de continuação. A prévia larga do editor apresenta o mesmo conteúdo
inteiro, portanto não antecipa o resultado na coluna de pessoas.

Reprodução: preencher nome «Visitante de uma campanha com nome longo», um
status de cerca de cinquenta caracteres e biografia de três parágrafos;
gravar, fechar e comparar a lateral com a prévia. A digitação automatizada
perdeu alguns acentos; isso não foi atribuído ao produto e não interfere no
comprimento que exercita o layout.

Caminhos conferidos: `mods-controles.css` fixa 180px em
`.pessoa-apresentada-porta .pessoa-cartao`; `cartaoDaPessoa` em
`SEELE-MOD-PERFIS/ferramentas/perfis.js` usa distintivo para status e recorte de
altura na biografia. O teto deve conter a lista sem cortar texto arbitrariamente.
Definir resumo deliberado: nome com quebra limitada, status em texto que caiba,
biografia com reticências e acesso ao perfil completo. Mostrar uma prévia na
largura do destino, ou permitir alternar «cartão lateral» e «perfil completo».
Não resolver aumentando indefinidamente a altura.

O diretório dispõe melhor nome e status longos, mas as ações ficam em alturas
diferentes conforme o conteúdo. Alinhar o rodapé dos cartões e preservar a
mesma margem interna. No perfil consultado, o cabeçalho usa a identidade
nativa (`pessoa-d609`) enquanto o corpo usa «Perfil Visual QA»; a identidade
verificada pode permanecer em metadados sem competir com o título do perfil.

### V14 — Páginas de MODs se empilham e escondem a navegação nativa

Reprodução nativa: abrir PERFIS na navegação e depois MESA, sem pressionar
Voltar. O diretório permanece na metade superior e MESA nasce embaixo, com
um segundo cabeçalho e outra rolagem. Fechar PERFIS deixa MESA ocupar a área.
Depois, entrar na sala muda a participação e o botão para VER A CHAMADA, mas
MESA continua cobrindo a chamada. Fechar a página revela a chamada correta.

É um defeito de navegação do host, além de composição. `abrir()` em
`mods-superficies.js` acrescenta cada página ao mesmo palco sem selecionar
uma única página; `mods-superficies.css` dispõe esse palco como coluna flex
sobre a área da conversa. Ambos foram conferidos no código após a reprodução.

O produto precisa possuir o destino central ativo. Abrir outra página deve
ocultar a anterior preservando seu estado; escolher conversa/chamada deve
ativar esse destino nativo. Voltar deve ter resultado previsível. Ocultar não
é descartar o MOD nem desligar voz. Na saída do servidor, todos os destinos e
suas referências continuam pertencendo ao ciclo de vida da sessão.

Aceite: PERFIS → MESA → conversa → chamada → PERFIS mostra exatamente um
destino central por vez, sem cabeçalho residual nem perda dos rascunhos que o
contrato permite preservar. Incluir duas páginas de MODs distintos no reteste.

### V15 — Rolagem funciona, mas seu resultado fica em outra aba

No Tabuleiro vazio, o visitante pressionou ROLAR. A tela permaneceu igual,
sem resultado junto do controle. Ao abrir Registro, apareceu
`Dados · 1d20 = 19 [19]`. A operação funcionou; faltou feedback no ponto onde
foi iniciada. Quem joga pode clicar repetidamente achando que nada aconteceu.

Mostrar o último resultado ao lado dos dados, manter o histórico em Registro
e sinalizar processamento/falha sem deslocar o foco. Não exigir trocar de aba
para descobrir se uma ação acabou.

### V16 — Estados vazios do MESA têm acabamento desigual

Tabuleiro usa uma caixa grande tracejada; Fichas tem título e uma frase;
Compêndio, Iniciativa e Registro têm apenas uma frase encostada na linha das
abas. A mesma ausência de conteúdo produz ritmos visuais distintos. Fichas
informa «Nenhuma ficha nesta mesa ainda» mesmo no contexto «Suas fichas»;
a mensagem deve distinguir ausência global de falta de ficha acessível ao
jogador. Esta rodada não demonstrou vazamento de ficha nem falha de permissão.

Padronizar margem entre abas e conteúdo, título opcional, frase orientadora
e próximo passo conforme o papel. No visitante, informar quem pode criar ou
atribuir o conteúdo; não oferecer uma ação que ele não pode executar.

### V17 — Estado do microfone continua contraditório no nome acessível

O rodapé informa «microfone abre na tecla», mas a árvore acessível do botão
anuncia MICROFONE ABERTO, com ajuda «silenciar o microfone». Observado nos dois
clientes QA. O ícone visual não oferece explicação equivalente. Não é uma
medição de captura de áudio: a cópia QA não abre dispositivos de som.

Unificar o estado que nomeia botão, tooltip e frase do operador, distinguindo
modo de ativação, silenciamento e transmissão atual. A correção anterior da
frase visível não concluiu o nome acessível do controle.

## Avaliação do espaçamento

O editor do PERFIS tem separação reconhecível entre identidade e aparência,
campos alinhados em duas colunas e rodapé de gravação visível mesmo quando a
prévia torna o corpo rolável. Esse avanço foi observado com campos preenchidos.
Não aprova automaticamente os demais destinos.

Persistem quatro problemas de composição: texto pequeno demais para o espaço
que ocupa; descrição e ação muito próximas na entrada; conteúdo quase colado
às abas vazias do MESA; e cartões com recorte que destroem a margem aparente.
A correção deve usar a régua compartilhada com variantes compactas explícitas,
sem espalhar novos números isolados em cada pacote. Geometria quadrada ajuda
a identidade do SEELE, mas não substitui hierarquia, largura e tratamento do
conteúdo. Preservar círculos com função clara, como peças do tabuleiro.

## Prioridade da próxima implementação

| Prioridade | Entrega | Aceite visual e funcional |
|---|---|---|
| P1 | Destino central único, V14 | Alternar dois MODs, canal e chamada sem empilhar páginas nem ocultar o destino escolhido. |
| P1 | Conteúdo longo e prévia fiel, V13 | Nome, status e biografia longos têm resumo intencional, indicação de continuação e perfil completo acessível. |
| P1 | Feedback de ações, V15 | Resultado de dados visível na aba de origem; falha e processamento distinguíveis. |
| P1 | Estado de microfone, V17 | Texto, nome acessível e ação concordam nos modos tecla/voz/aberto e silenciado. |
| P2 | Configurações, V01–V04 e V07 | Navegação sem parágrafos, revisões distinguíveis, ação de downgrade nomeada corretamente, uma rolagem principal. |
| P2 | Família de diálogos, V08 | Margens, títulos, ações secundárias, foco e rodapés seguem variantes do mesmo sistema. |
| P2 | Entrada, conexão e portaria, V05–V06 e V11–V12 | Estado principal e próximo passo compreensíveis sem ler diagnóstico; endereço correto; informações de segurança preservadas. |
| P2 | Sessão e compartilhamento, V09–V10 | Nomes legíveis, telemetria secundária, seletor sem janelas inválidas. |
| P2 | Estados vazios de MODs, V16 | Mesmo ritmo visual e orientação adequada ao papel. |

## Matriz de cobertura desta rodada

«Visual» significa captura nativa legível, examinada durante o percurso;
«AX» significa navegação/conteúdo conferido sem captura visual aproveitável.
Uma tela observada não implica todos os seus estados aprovados.

| Área | Telas/estados | Evidência e limite |
|---|---|---|
| Entrada | Inicial, ajudas expandidas, tentativa e falha | Visual; destino local inexistente. |
| Conectar | Histórico vazio, endereço | Visual. |
| Hospedar | Lista existente, preparar novo servidor, convite | Visual; formulário novo cancelado. |
| Identidade nativa | Perfil com imagem vazia e nome | Visual; sem substituição de arquivo pessoal. |
| Configurações locais | Microfone e som, Atalhos, Aparência, Identidade | Visual, incluindo rolagem; som efetivo não testado. |
| Gestão de MODs | Instalados, Catálogo, Armazenamento, Permissões | Visual; catálogo antes/depois da busca e permissões antes/depois da revogação QA. |
| Aplicativo | Versões, Malha, Atualização | Visual; atualização disponível observada, não instalada. |
| Configurações do servidor | A porta, Servidor | Visual; sem alteração de senha. |
| Portaria | Sem pedido, pedido e admissão do visitante | Sem pedido: visual; pedido e confirmação: AX após perda de enquadramento. |
| Autorização de entrada | Aguardar admissão, aceite de MODs, falha na obtenção, primeiro contato | Visual; pacotes locais preparados como descrito acima. |
| Conversa | Vazia, mensagem multilinha, busca com/sem resultado | Visual; dados fictícios no servidor QA. |
| Canais/salas | Criar canal, renomear, confirmar exclusão, criar sala | Visual; exclusão e criação de sala canceladas. |
| Pessoas | Vazia de perfil, cartão curto/longo, fora/dentro de sala | Visual com dois clientes locais. |
| Chamada | Uma e duas pessoas, palco sem transmissão | Visual; sem homologação de voz. |
| Compartilhar | Escolha de superfície e módulo de vídeo ausente | Visual; transmissão não iniciada. |
| Ajuda | Vocabulário e teclas | Visual. |
| PERFIS | Diretório, detalhes alheios, editor vazio/preenchido, salvar e Escape | Visual; sem nova imagem/faixa enviada nesta rodada. |
| ESTILO | Tema desligado, consulta do visitante | Visual; três abas administrativas ainda não revistas nesta rodada. |
| MESA | Cinco abas como jogador, dado e registro | Visual; editores administrativos ainda não revistos nesta rodada. |
| Navegação entre MODs | PERFIS → MESA e MESA → chamada | Visual; reprovada em V14. |
| Moderação de pessoa | Expulsar/banir/mover e seus estados | Pendente; não confundir com confirmação genérica de excluir canal. |
| Anexos | Prévia, envio, recebimento, erro e remoção | Pendente nesta rodada. |
| Fim/reconexão | Desconexão, contagem, fim de sessão e retorno | Pendente nesta rodada. |
| Variações de ambiente | Janela pequena, zoom de texto, alto contraste, Windows/Linux | Não homologadas; a tentativa de redimensionar não produziu tamanho menor verificado. |
| Segurança/erros condicionais | Chave alterada, credencial inválida, permissão do SO negada | Não provocados nesta rodada. |

## Interrupção e continuação

O Stage Manager transformou a captura do administrador numa miniatura inclinada
quando o segundo cliente abriu. A árvore acessível continuou disponível;
essas miniaturas não foram contadas como inspeção visual. A auditoria continuou
no visitante, cuja janela permaneceu legível. Mais tarde, o controle nativo
informou que o Mac estava bloqueado e não pôde desbloqueá-lo. Foi solicitado
que o usuário desbloqueasse a sessão. Até a retomada, as linhas pendentes da
matriz são lacunas reais, não aprovações por leitura do código.

O objetivo «todas as telas» exige concluir essas linhas e seus estados
representativos. Não há aprovação visual global nesta rodada. Os achados
acima já são concretos e podem orientar as correções sem repetir toda a
bateria Rust. Depois de cada correção, repetir a jornada correspondente no
aplicativo e uma passagem curta de navegação integrada.

Ao encerrar esta rodada, o processo do visitante já havia terminado. O
administrador QA foi encerrado por SIGTERM e sua ausência foi conferida na
lista de processos, pois a sessão bloqueada impedia usar sua interface. As
casas de teste foram preservadas. Nenhuma instalação, atualização de release,
publicação ou push foi realizada. Nesta rodada foram alterados somente os
registros de auditoria; os ajustes de código anteriores permanecem na árvore.
`git diff --check` passou para os arquivos rastreados modificados.


## Implementação autorizada — 20/09, rodada seguinte

O usuário autorizou as alterações diretamente neste checkout. Este registro
complementa os achados anteriores: não transforma a cobertura parcial da
auditoria em aprovação global. Nenhum commit foi enviado nem houve publicação.

### Alterações e alcance

| Achado | Alteração nesta rodada | Validação / limite |
|---|---|---|
| V01 | Navegação de Configurações sem as quatro notas repetidas; grupo do servidor oculto quando vazio; seção lembrada relida ao reabrir. | Navegação nativa antes/depois de hospedar; estado do servidor voltou corretamente. |
| V02 | Rótulos de Configurações em 12px onde eram microtexto; contraste secundário aumentado em Configurações e superfícies de MODs. | Capturas nativas em 1230×768; não é revisão de contraste de todos os temas personalizados. |
| V03 | Revisão identificada pelo hash curto, alternativas recolhidas por MOD, revisão selecionada/ativa em destaque, sem rolagem interna da lista; fase de execução só na revisão exigida. | Instalação por pasta, troca de revisões e aplicação conjunta no servidor QA; revisões inativas não dizem mais «carregado». |
| V04 | Catálogo usa «INSTALAR versão» para versão diferente e «REINSTALAR» para mesma versão com conteúdo diferente. | Código e verificações de frontend; não houve instalação de catálogo externo nesta rodada. |
| V05 | Menor distância na entrada; imagem do servidor como ação secundária; Voltar do formulário retorna à lista da qual veio. | Novo servidor aberto e cancelado no nativo, retornando à lista preservada. |
| V06 | Erro de conexão deixa de recomendar uma porta fixa errada. | Parcial: ainda falta cancelamento real da tentativa e apresentar o destino efetivamente testado na mensagem. |
| V07 | Texto de imagem/atualização reduzido; remover imagem fica desabilitado quando vazia. | Código e guardas; atualização de release não foi executada. |
| V08 | Diálogo de nome/identidade aproxima título, borda, margens e ações da família de MODs; confirmação deixa de oferecer fechar e cancelar redundantes. | Parcial: a família inteira de diálogos ainda merece passagem visual conjunta; não se declara uniformização completa. |
| V09 | Origem do MOD em segunda linha, nome com largura livre, número de sinal menos dominante, D·05 removido; separação textual do diagnóstico do convite. | Sessão, chamada e convite inspecionados no nativo. |
| V10 | Fontes 0×0 e janelas conhecidas de infraestrutura saem da lista; monitores vêm antes das janelas; texto do módulo de vídeo reduzido. | Parcial: miniaturas não foram implementadas; não houve transmissão nem nova homologação do seletor nesta rodada. |
| V11 | Espera em coluna central, duas instruções de próximo passo e diagnóstico recolhível; texto informa a verificação automática enquanto aberta. | Código e guardas; nova espera de admissão ainda não foi vista no nativo. |
| V12 | Pedido prioriza nome declarado/contexto; impressão digital completa fica em bloco de conferência e continua na confirmação. | Código e guardas de identidade; sem nova admissão nesta rodada. A reorganização das três decisões de acesso continua pendente. |
| V13 | Cartão com nome/status em até duas linhas e biografia com reticências; identidade em fluxo sem sobrepor faixa; acesso completo preservado; prévia limitada à largura lateral; ações do diretório alinhadas por composição. | Nome/status longos e biografia de dois parágrafos salvos e vistos no nativo. O preenchimento passou para a caixa externa: no próprio texto o WebKit mostrava parte de uma terceira linha. |
| V14 | Um destino central ativo, compartilhado entre MODs; abrir outra página oculta a anterior; canal/chamada devolvem o palco ao destino nativo. | PERFIS → MESA → chamada visto no aplicativo; bancada conserva rascunho e testa descarte, além de reprovar com o conserto revertido. |
| V15 | ROLAR mostra processamento, bloqueia repetição pendente e apresenta resultado/erro no Tabuleiro. | «Dados · 1d20 = 7 [7]» observado no Tabuleiro nativo; teste exige o resultado numérico nessa aba. |
| V16 | Estados vazios de tabuleiro, fichas, compêndio, iniciativa e registro compartilham composição e orientação por papel. | Tabuleiro vazio visto no nativo; demais estados cobertos pelo código/pacote, sem nova captura de todos eles. |
| V17 | Botão, tooltip e frase do microfone usam a mesma distinção entre tecla, voz, aberto e mudo. | Modo tecla conferido no nativo, fora/dentro de sala; esta QA não homologou áudio. |

### Ajustes encontrados durante o reteste

- Os seletores de MODs mantinham altura e cantos do macOS apesar do CSS comum.
  O campo fechado agora usa aparência própria, 36px e canto reto; ao abrir,
  mantém o menu e a operação de teclado do sistema.
- Configurações reabria a gestão com o retrato anterior à hospedagem. Reabrir
  agora relê a seção selecionada, além de reiniciar sua rolagem.
- A fase de execução era consultada só pelo nome do MOD, marcando todas as
  revisões como carregadas. A gestão confere também o hash exigido.
- O caminho do convite para Configurações fixava a origem na entrada, mesmo
  com sessão ativa. Agora usa a tela de origem real e aguarda a abertura antes
  de selecionar A porta.
- PERFIS aceitava editar além dos limites do servidor e devolvia somente
  «Texto maior que o permitido». O pacote agora identifica campo, limite e
  tamanho atual, preservando o rascunho. O renderer também aplica `erro` em
  campos de uma linha enquanto têm foco, sem reescrever o valor digitado.

### Código e distribuição local

A API 4 ganhou a propriedade opcional `estilo.linhasMaximas` (inteiro de 1 a
20), documentada em `api/v4.json` e no guia-fonte do indexador. O guia HTML e
a cópia para download foram regenerados localmente. O guia explica a caixa
externa de preenchimento e a navegação por destino único.

Os três clientes foram regenerados dos respectivos fontes; PERFIS e MESA
receberam mudanças de comportamento, e ESTILO recebe a geometria e o contraste
do renderer compartilhado. Os três foram instalados pela interface na cópia
QA. As revisões anteriores de teste permanecem disponíveis no cache.

### Evidência e condições de teste

A mesma cópia `target/qa-08b3788/SEELE QA.app`, com casa
`/tmp/SEELE-QA-08b3788/home`, hospedou o próprio servidor. A inspeção desta rodada
incluiu entrada, Configurações, instalação/seleção de revisões, novo servidor,
convite, encerramento por troca de MODs e reconexão, PERFIS (diretório/editor e
perfil longo), MESA (Tabuleiro e dado), ESTILO (Cores/Forma/Conjuntos) e chamada.

A compilação QA continua **sem captura de áudio**, pela limitação CoreAudio já
registrada na rodada anterior. A alteração temporária de compilação foi
restaurada e o binário release normal reconstruído com áudio habilitado. Isso
permite validar a apresentação nativa, não a voz, uso de memória ou transmissão.
Não houve medição de desempenho nesta rodada, nem execução em Windows/Linux.

Capturas desta rodada: [perfil longo](evidencias/ajustes-visuais-2026-09-20/perfil-longo.png),
[Configurações](evidencias/ajustes-visuais-2026-09-20/configuracoes.png),
[ESTILO — Forma](evidencias/ajustes-visuais-2026-09-20/estilo-forma.png),
[erro por campo no PERFIS](evidencias/ajustes-visuais-2026-09-20/perfil-validacao.png) e
[diálogo nativo](evidencias/ajustes-visuais-2026-09-20/dialogo-nativo.png).

Na última compilação também foram vistos: seletor de 36px no ESTILO, erro do
STATUS acima de 60 caracteres e sua remoção ao corrigir, diálogo Novo canal
com título e margens novos, e convite → Configurações → Voltar ao servidor.
Novo canal foi cancelado, sem criar conteúdo adicional.

### Verificação técnica

- 237 guardas de frontend aprovados; `cargo xtask check-api` aprovado.
- Quatro bancadas de `check-runtime`, incluindo destino único, resumo por
  linhas e validade do campo com foco. Reversões em memória reprovaram nos
  sintomas esperados; os arquivos de produção não foram alterados para isso.
- Pacotes: PERFIS 40, MESA 48 e ESTILO 23 testes aprovados; geração e sintaxe
  conferidas. Clientes exercitados no prelúdio real com QuickJS.
- Clippy do aplicativo com todos os alvos e avisos como erro aprovado.
- `git diff --check` aprovado nos cinco repositórios alterados. O arquivo de
  testes Rust editado foi formatado. `cargo fmt --all -- --check` ainda acusa
  diferenças preexistentes em `apps/seele-app/src/main.rs`,
  `crates/seele-ffi/src/mods.rs` e `crates/seele-proto/src/mods.rs`, que não
  foram modificados nesta rodada.
- Builds release normais e builds QA concluídos. A suíte Rust inteira não foi
  repetida: as mudanças funcionais desta rodada estão na apresentação JS e nos
  pacotes, e a validação foi concentrada nesses caminhos.

As lacunas da matriz original — anexos, moderação completa, espera com novo
visitante, janela pequena e estados condicionais do SO, entre outras —
continuam abertas quando não explicitamente cobertas acima. Cancelamento de
conexão exige interromper/revogar a tentativa nativa, não apenas esconder o
formulário: isso não foi acrescentado como um botão que fingisse cancelar.
Miniaturas de compartilhamento também exigem ampliar os dados de captura,
que hoje fornecem identificador, nome, dimensões e tipo da fonte.


Ao terminar, a cópia QA foi encerrada para não manter o servidor de teste no
ar. A casa QA, os pacotes de teste e as capturas foram preservados. O binário
normal reconstruído permanece em `target/release/seele-app`; ele não substituiu
a instalação do usuário. A entrega continua local, sem commit, push ou release.
