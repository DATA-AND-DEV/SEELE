# Auditoria de UX/UI do SEELE e dos MODs oficiais

Data: 20/09/2026. Avaliação do aplicativo nativo macOS **0.12.1**, do pacote em `target/release/bundle/macos/SEELE.app`, e dos três MODs **2.0.0 instalados pelo catálogo oficial**. Código SEELE: `655a137c44907c7ee8fe2951a384197e9f09d827`.

## Resultado

A apresentação dos MODs não atende ao objetivo de criar experiências próprias dentro de um servidor. A faixa inferior limita atividades diferentes ao mesmo formulário estreito. Existem também regressões funcionais que os testes anteriores não detectaram. A conclusão anterior de que os três MODs estavam completos não se sustenta nesta auditoria.

O executor isolado e a limpeza por sessão devem ser preservados. O contrato de apresentação, a integração com a interface principal e os ports precisam mudar. A proposta está em [Plano da API de criação de interfaces](plano-api-mods-criacao-de-interfaces.md).

## Método e alcance real

Navegação por ferramentas de acessibilidade e capturas do aplicativo, sem substituir cliques por chamadas internas. Servidor separado **QA UI MODS 20-09**, identidade de teste, sem outras pessoas. A primeira rodada desta conversa observou a janela inteira em aproximadamente 1229 × 768 px; a faixa dos MODs ocupava cerca de 230 px. A segunda rodada ampliou a cobertura das telas e cruzou os achados com o código atual e versões anteriores dos MODs.

Durante a segunda rodada, a captura passou a retornar uma miniatura inclinada da janela. Alguns controles dinâmicos de sala retornaram `elementHasNoFrame`/`invalidUIElement`. Isso é **limitação da automação**, não prova de falha para uma pessoa. Depois que o usuário trouxe a janela ao espaço atual, a captura inteira voltou e o clique por coordenadas permitiu entrar na sala. Foram então verificados chamada, cartão PERFIS, silenciar/reabrir microfone, ajustes de áudio em sessão, abertura/cancelamento do diálogo de compartilhamento e saída. A permissão de gravação de tela não foi solicitada nem concedida.

Legenda de evidência: **UI** = observado no aplicativo; **C** = confirmado no código; **R** = reprodução isolada executada; **H** = hipótese que ainda exige reprodução. Não se afirma ter encontrado todos os defeitos possíveis, nem ter homologado fluxos bloqueados.

| Área | O que foi percorrido | Alcance |
|---|---|---|
| Entrada e conexão | Entrada, lista de servidores conhecidos, campo de endereço, retorno | UI; sem conectar a servidor de terceiros |
| Hospedagem | Lista, criação de servidor separado, versão escolhida, convite, reentrada | UI; servidor criado na 0.12.1 |
| Perfil nativo | Modal, apelido, escolha/remoção de imagem disponíveis | UI; sem alterar a identidade global |
| Conversa | Criar `qa-fluxos`, enviar mensagem de teste em `geral`, procurar `usabilidade` | UI; criação/envio/busca funcionaram |
| Salas de voz | Entrar, ver chamada, silenciar/reabrir microfone, ajustes de áudio e sair | UI; sem segundo participante ou avaliação da qualidade sonora |
| Pessoas | Lista dentro/fora de sala, próprio nome, sinal e PERFIS | UI + C; cartão adicional apareceu ao entrar e sumiu ao sair da sala |
| Ajuda | Vocabulário e teclas | UI |
| Configurações | Microfone/som, atalhos, aparência, identidade, MODs, versões, malha, atualização | UI; sem alterar permissões do sistema ou atualizar o app |
| Administração | Nome/imagem, convite e portaria | UI; sem definir senha, revogar acesso ou publicar convites |
| Gestão de MODs | Catálogo, instalação, ativação conjunta, reconexão, estado de pacotes antigos | UI; três pacotes oficiais instalados e ativos |
| PERFIS | Editar, gravar, fechar, reentrar, abrir seletor de arquivo e cancelar | UI; nome persistiu; upload completo e efeitos não homologados |
| ESTILO | Cor, gravação, reentrada, arredondamento e gravação | UI; cor persistiu; aviso contraditório reproduzido |
| MESA | Digitar nome e tentar criar campanha | UI; bloqueado por `invalid-id` |
| Compartilhamento | Abrir diálogo, ler estado sem permissão e cancelar com Escape | UI; sem transmitir tela nem alterar permissão do sistema |
| Saída | Sair após uso dos três MODs nas duas rodadas | UI; faixa e tema desapareceram visualmente; hospedagem QA encerrada |
| Transmissão de tela, moderação de outra pessoa, anexos completos, falha/reconexão de rede | Revisão dos pontos de entrada no código | C; faltam segundo participante e/ou ação nativa |
| Windows/Linux, zoom textual, leitor de tela completo, latência e memória sob carga | Não executados nesta auditoria | Não apresentar como aprovados |

## Achados no produto

Prioridades: **P1** bloqueia a tarefa ou contraria requisito central; **P2** prejudica uso/compreensão; **P3** acabamento e consistência. Prioridade não é classificação de segurança.

| ID | Prioridade / prova | Problema e impacto | Correção proposta |
|---|---|---|---|
| U01 | P1 · UI+C | Todos os MODs disputam uma faixa de até 240 px. O centro da janela fica disponível para conversa vazia, enquanto editar perfil/tema e jogar exige rolar um rodapé. | Superfícies próprias: páginas, painéis e diálogos abertos por ações identificáveis. |
| U02 | P1 · UI+C | A rolagem pertence ao contêiner dos três MODs. Rolar PERFIS desloca ESTILO e faz MESA desaparecer. | Rolagem por superfície; estado de navegação independente; barra de ações acessível. |
| U03 | P1 · UI+C | Não há caminho de MOD para abrir uma experiência ampla, modal próprio ou painel ajustável. Os MODs aparecem todos ao conectar, sem escolha da atividade. | Registro de entradas de navegação e superfícies sob demanda. |
| U04 | P2 · UI | Rótulos muito pequenos, botões visualmente pouco diferenciados e campos sem hierarquia de tarefa. As ações principais parecem controles auxiliares. | Escala de texto e espaçamento legível, variantes de ação, grupos e títulos proporcionais à tarefa. Medir contraste e alvos; não declarar conformidade sem medida. |
| U05 | P2 · UI | A tela de entrada expõe comando `seeled`, QUIC/TLS/UDP e chave antes da pessoa escolher entrar ou hospedar. | Primeiro mostrar propósito e ações; detalhes técnicos em explicação expansível. |
| U06 | P2 · UI | Configurações se chamam “TERMINAL SERVER”, embora misturem preferências locais e administração de um servidor. | Nome “Configurações”; separar “Este dispositivo”, “Este servidor” e “MODs”, com contexto visível. |
| U07 | P2 · UI | Aparência explica uma correção histórica de contraste e `prefers-contrast`, mas não oferece um controle direto equivalente. | Oferecer aparência/legibilidade; explicar a configuração do sistema em linguagem de uso, com ação para acessá-la quando disponível. |
| U08 | P2 · UI | Microfone/som fora de sessão diz “SEM SESSÃO DE ÁUDIO” e, logo abaixo, “Fale normalmente”. O título promete ouvir antes de entrar. | Teste local de microfone e saída independente de servidor, com início/fim explícitos; ou instrução coerente com a disponibilidade. |
| U09 | P2 · UI | Identidade fora de servidor mostra apelido `——` e fala em “neste servidor”. | Estado vazio contextual; separar identidade do dispositivo e apelido reservado por servidor. |
| U10 | P2 · UI | MODs antigos aparecem como hashes compridos, `api-too-old`, versão vazia e 0 B. O usuário perde a identidade do pacote justamente quando precisa atualizá-lo. | Preservar metadados para diagnóstico, explicar incompatibilidade, mostrar versão instalada/compatível e caminho de atualização. |
| U11 | P2 · UI | Após ativar os MODs, permanece uma mensagem “instalado, e desligado”; fora de sessão há texto dizendo o que “o servidor exige agora”. | Mensagens derivadas do estado atual, distinguir instalado/selecionado/ativo/em execução e indicar servidor selecionado versus conectado. |
| U12 | P2 · UI | Gestão de MODs junta ativação, catálogo, cache, consentimentos e contadores técnicos numa página longa. | Separar atividades em abas; diagnóstico expansível; títulos amigáveis com ID técnico secundário. |
| U13 | P2 · UI | Link/convite, situação de NAT e “PORTA · FECHADA” ficam próximos, mas descrevem mecanismos diferentes. O resumo concatena frases (“outro.o roteador…”). | Separar “Conexão externa” de “Quem pode entrar”; convite com nome de servidor e resumo curto, detalhes de rede recolhidos. |
| U14 | P2 · UI | Portaria lista uma identidade admitida como hash e “diz chamar-se «»”. | Nome conhecido ou “Identidade sem apelido”, origem/data legíveis, chave em detalhe copiável. |
| U15 | P2 · UI | A identidade domina pouco a faixa de pessoas; um sinal 100 e “TRANSMITINDO” dominam inclusive fora de sala de voz. | Nome/avatar/presença como hierarquia principal; diferenciar microfone capturando de voz enviada à sala; diagnóstico recolhível. Não inferir transmissão real só pelo texto. |
| U16 | P2 · UI | Criar canal funciona, mas mantém a conversa no canal anterior sem uma ação clara para abrir o novo. | Abrir o canal criado ou mostrar confirmação com “Abrir canal”; preservar decisão de contexto explicitamente. |
| U17 | P2 · UI | Enviar texto exige Enter; não há botão de enviar, como a própria ajuda confirma. | Ação visível e acessível, com atalho secundário e suporte declarado a texto multilinha. |
| U18 | P3 · UI | Atalhos dizem “como no terminal” e exibem Ctrl+V no macOS; ajuda e configurações distribuem instruções parcialmente diferentes. | Uma fonte de atalhos por plataforma e contexto; confirmar comportamento real de Cmd/Ctrl antes de corrigir só o texto. |
| U19 | P2 · UI+C | PERFIS permite editar nome, mas ele não aparece no próprio cartão fora de sala. As duas ramificações “fora de sala” não passam `cartoesDaPessoa` ao renderer. | Um único modelo de apresentação de pessoa para sala, saguão e próprio usuário. |
| U20 | P2 · UI+C | Editar PERFIS não abre modal: troca a lista por formulário longo; “Sobre mim” é input de uma linha. | Perfil de leitura com identidade visual e editor em modal, biografia multilinha e prévia. |
| U21 | P1 · UI+C | Os dois botões de arquivo de PERFIS estão vazios visualmente e sem nome acessível. Um deles abre seletor genérico, sem dizer avatar/banner. | Corrigir renderer e contrato de arquivo: rótulo, finalidade, tipos, limites e resultado. |
| U22 | P2 · UI | Seletor de avatar abre “Escolha um arquivo para este MOD” e mostra arquivos JSON como selecionáveis. Cancelar retorna corretamente “nenhum arquivo escolhido”. | Filtro e orientação antes da escolha, mantendo validação por conteúdo; cancelamento neutro e silencioso quando apropriado. |
| U23 | P2 · UI+C | ESTILO usa inputs de hexadecimal em vez de seletor e amostra; a cor muda após gravar, sem prévia reversível explícita. | Seletor de cor + hexadecimal opcional, prévia local separada de publicar para o servidor, salvar/cancelar fixos. |
| U24 | P1 · UI+C | Após gravar arredondamento 8, ESTILO afirma “a API de tema recusa os dois”. O código já aplica raio/brilho. | Remover a afirmação obsoleta e mostrar o resultado real da aplicação, inclusive erros. |
| U25 | P1 · UI+R+C | MESA não cria campanha; exibe somente `invalid-id`. | Corrigir contrato de escrita e erro para usuário; não maquiar erro apenas com tradução. |
| U26 | P2 · C | “FECHAR” no editor PERFIS apaga `rascunho` sem tratar alterações pendentes. | Rascunho por entidade; cancelar/descartar explícitos e aviso apenas quando houver perda. |
| U27 | P1 · C | Cartões na API são conteúdo adicional, sem interação, sem composição de banner e sem substituição dos campos nativos. Isso é inferior ao PERFIS anterior. | Contribuições semânticas para substituir/apresentar identidade, cartão, perfil detalhado e ações, com restauração nativa. |
| U28 | P2 · UI | Na chamada, o estado vazio diz “Use COMPARTILHAR, aqui em cima”, mas o botão está na base da coluna esquerda. Após sair da sala, o centro permanece na chamada vazia. | Orientação ligada à ação real, botão próximo do estado vazio e retorno claro à conversa ou navegação da sessão. |
| U29 | P2 · UI+C | Navegação extensa e tarefas longas num modal limitado a 840 × 620 px, com coluna de 216 px e rolagens separadas. | Página na área útil do app; painel rápido para áudio durante a conversa. |
| U30 | P2 · UI | Explicações longas misturam orientação, funcionamento interno e histórico entre os controles. | Controle e estado primeiro; ajuda curta; detalhes sob demanda e histórico fora do fluxo. |
| U31 | P2 · UI | Microfone, Aparência, Identidade, MODs, Versões, Malha e Atualização no mesmo nível, sem distinguir frequência e escopo. | Agrupar por dispositivo, servidor, MODs e aplicativo; usar vocabulário de tarefa. |
| U32 | P2 · UI | Pacotes repetidos em instalados, catálogo e disco; hashes e contadores interrompem a gestão cotidiana. | Lista principal única e destinos separados para descoberta, armazenamento e permissões. |
| U33 | P2 · UI+C | Voltar fica no fim da coluna que também rola; navegação longa compete com a saída. | Cabeçalho persistente com voltar, categoria atual e contexto. |

## Configurações: validação complementar e proposta de reorganização

Revisão adicional solicitada pelo usuário em 20/09/2026. Foram reabertas **Microfone e som, MODs, Aparência e Malha**, sem alterar opções, permissões ou pacotes. O contexto de servidor e demais categorias já consta da navegação anterior. A árvore de acessibilidade confirmou o conteúdo; o código confirmou estrutura e dimensões. A captura desta revisão voltou a aparecer como miniatura, portanto não há nova medição visual em diferentes tamanhos. **A possibilidade de assustar um usuário iniciante é uma avaliação de UX, ainda não resultado de teste com participantes.**

### Diagnóstico

A interface entrega documentação junto com cada tarefa, exigindo leitura excessiva para localizar escolhas simples. MODs reúne pelo menos cinco atividades: administrar instalados, descobrir novos, instalar uma pasta, liberar espaço e rever permissões. Inclui contadores de eventos recusados e explicação de hashes/assinaturas. Aparência destaca a antiga razão de contraste `4,11:1` antes de oferecer uma ação. Em Áudio, a explicação longa sobre acompanhar o dispositivo padrão se repete para entrada e saída.

O problema não se resolve apenas aumentando a caixa. Em `tela-server.css`, o modal tem largura máxima de 840 px e altura `min(620px, calc(100vh - 96px))`; a grade reserva 216 px à navegação. No limite máximo, sobram aproximadamente 574 px internos para conteúdo, descontados bordas e padding. Navegação e painel têm rolagens independentes. Esses valores vêm do código, não da captura reduzida.

A justificativa atual no `index.html` é permitir trocar microfone durante a conversa sem perder o contexto. Essa necessidade é válida, mas não exige que catálogo, armazenamento, administração e documentação inteira habitem o mesmo modal.

### Formato recomendado

**Uma página de configurações na área útil da janela do SEELE**, substituindo o modal central como destino geral. Não significa entrar no modo de tela cheia do sistema, abrir outra WebView ou encerrar a sessão. Voz e conexão continuam ativas; uma faixa compacta informa sala, microfone e como retornar à conversa.

Manter um **painel rápido de áudio**, junto ao controle de microfone/fone, com entrada, saída, estado atual e acesso a “Todas as configurações”. Reutilizar o mesmo estado e comandos da página, sem criar preferências concorrentes. Diálogos continuam apropriados para ações curtas: confirmar descarte, escolher arquivo ou revisar permissão.

| Alternativa | Avaliação |
|---|---|
| Apenas aumentar o modal | Alivia espaço, mas mantém a mistura de atividades e o excesso de leitura. Não é a solução final recomendada. |
| Modal reorganizado e detalhes recolhidos | Melhoria intermediária possível; exige saída fixa e dimensões responsivas. |
| Página ampla + painel rápido de áudio | Recomendação: acomoda administração e MODs, preservando ajustes rápidos durante a conversa. |

A página aproveita espaço sem esticar parágrafos pela largura inteira: coluna de leitura confortável, grupos delimitados e espaço em branco. Em janela estreita, categorias viram uma lista que abre uma seção por vez, com voltar no cabeçalho; não reduzir fontes para caber duas colunas. Busca de configurações pode complementar categorias compreensíveis.

### Organização proposta

| Grupo / destino | Conteúdo e escopo |
|---|---|
| Este dispositivo → Áudio | Entrada, saída, modo de microfone e teste local; indicar que a escolha vale nesta máquina. |
| Este dispositivo → Aparência e acessibilidade | Tema, contraste, texto e movimento conforme capacidades implementadas; explicar quando segue o sistema. Não mostrar controles fictícios. |
| Este dispositivo → Atalhos / Identidade | Teclas por plataforma; perfil local e opções disponíveis. Detalhes técnicos da identidade em área expansível. |
| MODs → Instalados | Lista principal com nome, versão, compatibilidade e ações. Execução sempre identificada pelo servidor correspondente. |
| MODs → Catálogo / Armazenamento / Permissões | Destinos separados: descoberta sob demanda, espaço usado e permissões por servidor. Evitar repetir todos os pacotes na mesma página. |
| Servidor: nome atual → Geral / Acesso / Conexão / MODs | Administração autorizada daquele servidor. Fora de sessão, não dizer “este servidor” sem identificar o selecionado. |
| Aplicativo → Atualizações / Sobre e diagnóstico | Versão atual, atualização e versões alternativas; logs, transporte, hashes e contadores sob demanda. |

“Malha” deve aparecer como **Compartilhamento de tela → Conexão e uso de internet** ou nas opções avançadas de conexão. Compreender o efeito deve preceder conhecer o nome da arquitetura. Receber diretamente e retransmitir continuam sendo escolhas separadas.

### O que mostrar e o que recolher

| Área | Visível no uso comum | Sob demanda |
|---|---|---|
| Áudio | Aparelho selecionado, estado real, medidor/teste quando disponível e modo | Comportamento quando aparelho some ou padrão muda; solução de falhas |
| Aparência | Opções disponíveis, amostra e estado atual | Como segue acessibilidade do sistema; histórico de correções vai às notas da versão |
| MOD instalado | Nome, versão, descrição curta, situação e ação relevante | ID/hash, capacidades detalhadas, dependências e diagnóstico |
| Catálogo | Nome, finalidade, origem e instalar/abrir detalhes | Como se verifica assinatura/integridade; falhas continuam visíveis e acionáveis |
| Armazenamento | Espaço usado e pacotes identificáveis | Detalhes por pacote; explicar a diferença entre remover pacote e dados antes da remoção |
| Conexão | Estado compreensível e ação necessária | Endereços, NAT, caminhos e medições para diagnóstico |
| Permissão | Consequência necessária à decisão e a quem se aplica | Explicação técnica complementar |

**Reduzir texto não significa esconder problemas ou consequências.** Erros ativos ficam junto da ação afetada. Antes de aceitar conexão direta, informar que o endereço de rede será compartilhado; antes de retransmitir, informar o uso de upload. Não recolher essas consequências num bloco descoberto só depois de ligar. Retirar justificativas do desenvolvimento e frases filosóficas do fluxo cotidiano, preservando ajuda acessível.

Cada preferência deve ter rótulo, controle, valor/estado e, quando necessário, uma explicação curta. Evitar título, subtítulo repetido e dois parágrafos para uma seleção. Preferências imediatas mostram resultado real ou erro; formulários que exigem confirmação apresentam salvar/cancelar e rascunho. Não impor Salvar global a controles de áudio que precisam produzir efeito imediato.

### Critérios de aceite e implementação

1. Encontrar e trocar entrada/saída sem ler documentação extensa; painel rápido confirma aparelho em uso e retorna à conversa sem desconectar.
2. Encontrar MOD instalado e ação principal sem atravessar catálogo, cache e hashes; permissões e armazenamento continuam descobríveis.
3. Distinguir preferência local de alteração para todas as pessoas do servidor antes de modificar o valor.
4. Cabeçalho, voltar e contexto sempre alcançáveis em janela pequena e com texto ampliado; sem rolagem horizontal para leitura comum. Validar teclado, foco visível e retorno à origem.
5. Fechar configurações não interrompe voz nem perde edição silenciosamente. Saída/troca de servidor invalida ações antigas e preserva acesso às preferências locais válidas.
6. Validar protótipo com pessoas sem familiaridade com SEELE: localizar microfone, instalar/identificar MOD, encontrar permissões e retornar à conversa. Registrar sucesso, hesitação e leitura necessária; não aprovar apenas por opinião estética ou número de cliques.

Implementação: [estrutura e textos](../apps/seele-app/ui/index.html), [layout e rolagem](../apps/seele-app/ui/tela-server.css), [abertura, retorno e estado](../apps/seele-app/ui/tela-server.js) e [estilos da gestão de MODs](../apps/seele-app/ui/camada-mods.css). Atualizar comentários e guardas que assumem “configurações sempre em diálogo”, preservando foco, retorno e continuidade de voz. Substituir a premissa de apresentação por verificações do novo comportamento.

Esta seção é **proposta baseada na inspeção**, não alteração já aplicada ao aplicativo. Complementa U06–U14 e U29–U33 e deve entrar no redesenho junto da integração das configurações dos MODs.

## Defeitos funcionais e causas

### MESA: falha de escrita mascarada pelo teste

Em `ferramentas/mesa.js`, `escrever(canal, pedido)` repassa o pedido sem `nonce` e sem `revision`. `servidor/main.js` valida `key(r.nonce)` antes de qualquer escrita; para operações após criação exige também `r.revision === c.revision`.

Reprodução isolada executada com o servidor original, contexto administrador e armazenamento somente em memória:

```text
setup como o cliente envia -> { ok: false, error: 'invalid-id' }
setup com nonce válido -> ok: true
scene-create com nonce mas sem revision -> { ok: false, error: 'conflict' }
scene-create com nonce e revision atual -> ok: true
```

O teste `test/cliente-api3.test.cjs`, função `world().call`, acrescenta automaticamente `revision` e `nonce`. Portanto, o teste do cliente prova um caminho diferente do produto. Corrigir exige capturar o pedido real do cliente e entregá-lo intacto ao servidor. O produto não deve completar campos de negócio invisivelmente para fazer um teste passar.

Também constatado em código: o estado vazio não usa `canSetup` para orientar/limitar o botão, e a criação fixa sistema `free` e GM atual; a versão anterior oferecia sistema e GM no diálogo. O mapa é montado como mídia separada do canvas, em vez de fundo sob as peças. As escolhas de trilha alteram estado no servidor, mas o cliente atual não declara tocador de som nem sintetizador equivalente ao anterior. Esses fluxos posteriores não foram executados nativamente porque criar a campanha já falhou.

### PERFIS: dados guardados sem apresentação equivalente

`ferramentas/perfis.js` lê e grava `accent` e `effect`, mas não os usa na árvore visual. `cartoesDaLista` inclui avatar, nome, pronome e status; não inclui banner, efeito nem estilo de cor. O renderer do produto acrescenta esse conteúdo à linha existente; não substitui a identidade visual e não recebe clique de dentro do cartão.

A confirmação nativa dentro da sala mostrou o nome `PERFIL QA` como linha adicional abaixo de `teste (você)`, do sinal 100 e do estado de microfone. Ao sair da sala, a linha do MOD desapareceu. O controle de silenciar mudou corretamente o estado para `MUDO`; isso não avalia a entrega de áudio a outra pessoa.

A versão anterior `ce976fd` possuía banner, avatar sobreposto, cor, animações, cartão clicável e substituição das partes nativas da faixa direita. Não se deve restaurar os seletores DOM antigos: deve-se restaurar **a capacidade e o resultado para a pessoa**, por uma API explícita.

### Renderer: por que os botões de arquivo não têm texto

Em `apps/seele-app/ui/mods-regiao.js`, `arquivo` não pertence a `FORMAS_COM_FILHOS`; `montarArquivo` registra o clique e `atualizarArquivo` só altera `disabled`. Nenhum desses caminhos escreve `dentro` como rótulo. A ausência observada é compatível com esse caminho, não com um pacote que esqueceu o texto: PERFIS fornece `ENVIAR RETRATO` e `ENVIAR FAIXA`.

### ESTILO: estado correto, explicação errada

`paraOProduto` envia arredondamento e brilho; `oQueEstaGuardadoENaoSeAplica` ainda gera a recusa antiga. O aviso foi reproduzido na UI depois de escolher 8 px e gravar. Manter duas descrições concorrentes do mesmo recurso causa essa regressão.

### Infraestrutura de desenvolvimento que não acompanha o produto

`ferramentas/preview.cjs` do MESA procura `PRELUDIO_DO_MOD` em `base.js`, que não está mais ali, e monta Worker. `test-ui.cjs` espera ausência de `button`/`input` e semeia a campanha pelo servidor. Não foi executado como validação do produto atual: o código já mostra que o contrato do laboratório está obsoleto. Corrigir o laboratório é parte da entrega da API, pois é o que o criador de MOD usa para aprender e validar.

### Riscos a reproduzir, sem apresentá-los como falhas já medidas

- Rascunhos e respostas em voo atravessando mudança de canal: o helper consulta periodicamente, e o evento usa `canalAtual`. Prender abertura/edição à entidade e ao canal de origem.
- Envio de imagem com erro: MESA chama `soltar` apenas depois do laço bem-sucedido. Verificar liberação em `finally`, cancelamento e limpeza de uploads incompletos no servidor.
- Árvore grande de ficha/compêndio excedendo 32 campos, 512 nós ou 12 KiB de mensagem; não aceitar apenas cenário vazio.
- Reconciliação das linhas nativas destrói/recria grupos de pessoas. Medir foco e clique com atualizações frequentes; os erros da automação não bastam para classificar isso como defeito humano.
- Validação e uso de mídia real: formatos, conteúdo inválido, orientação, recorte, limites de decodificação e bytes retidos.

## O que funcionou nesta execução

Instalação oficial e ativação dos três MODs; gravação/persistência de nome no PERFIS; gravação/persistência de cor no ESTILO; seleção e gravação de arredondamento; abertura/cancelamento do seletor de arquivo; criação de canal; envio e busca de mensagem; entrada/saída de sala e mudança visual de estado do microfone; abertura/cancelamento do diálogo de compartilhamento; saída visual limpa nas duas rodadas. Esses resultados não homologam avatar, banner, efeitos, jogo, som, transmissão de tela, voz sob carga ou outros sistemas operacionais.

## Próxima entrega e definição de pronto

1. Correções funcionais dos pedidos, rótulos, cartões fora de sala e estados contraditórios.
2. Apresentação sob demanda, layouts e modais com qualidade equivalente às experiências anteriores.
3. PERFIS refazendo de fato a identidade na faixa direita; ESTILO com prévia visual; MESA como espaço de jogo, não rodapé.
4. Verificação nativa dos fluxos completos com pacote final, papel administrador e participante, dados existentes e servidor novo.

Não usar contagem de suítes verdes como substituto da execução desses fluxos. Não declarar “paridade” quando só existe armazenamento de uma opção que o produto não desenha ou toca.

## Pontos de código para a implementação

- [Renderer e quotas de região](../apps/seele-app/ui/mods-regiao.js)
- [Faixa fixa e controles](../apps/seele-app/ui/tela-sessao.css)
- [Registro, ponte, tema e cartões](../apps/seele-app/ui/base.js)
- [Lista de pessoas e conversa](../apps/seele-app/ui/tela-sessao.js)
- [Executor e prelúdio](../apps/seele-app/src/executor.rs)
- [Contrato API 3](../api/v3.json)
- [Política estética atual](../specs/07-estetica.md)
- [MESA — cliente-fonte](../../SEELE-MOD-MESA/ferramentas/mesa.js)
- [MESA — servidor](../../SEELE-MOD-MESA/servidor/main.js)
- [MESA — teste que completa o pedido](../../SEELE-MOD-MESA/test/cliente-api3.test.cjs)
- [PERFIS — cliente-fonte](../../SEELE-MOD-PERFIS/ferramentas/perfis.js)
- [ESTILO — cliente-fonte](../../SEELE-MOD-ESTILO/ferramentas/estilo.js)

Os links para repositórios irmãos são relativos à disposição local usada nesta auditoria. A hospedagem QA foi encerrada pela interface; sua configuração e os dados de teste são preservados para reprodução. Nenhum pacote ou release foi publicado por esta auditoria.

## Retorno ao aplicativo: candidata `d96d71a`

A [validação nativa dos três candidatos API 4](validacao-nativa-api4-d96d71a.md)
registra o que mudou desde esta auditoria. Criar campanha, cena, rolar dados,
gravar perfil e confirmar descarte funcionaram no macOS. O ESTILO não abriu seu
editor: a montagem tem 14.164 bytes para uma ponte com teto de 12.288. A faixa
permanente continua nos três MODs. Há ainda regressões no perfil vazio, na
preferência pela apresentação nativa, no término da criação de campanha e no
compositor do chat. O documento separa reprodução, causa verificada, correção
esperada e aceites ainda não executados.

As configurações já usam área ampla e grupos por contexto. A observação nativa
também pede reduzir o texto fixo da navegação, recolher explicações secundárias
e corrigir a posição de rolagem ao trocar de seção. A melhora estrutural não
encerra o trabalho de densidade e hierarquia visual.
