# Contratos operacionais propostos

Este anexo integra o [contrato de arquitetura](../contrato-mods-api-propria-para-claude.md) e o [roteiro de execução](roteiro-de-execucao.md). **Todas as operações novas abaixo são propostas para implementar.** Os nomes podem ser ajustados por Claude antes do congelamento, mantendo semântica, validação e exemplos sincronizados. Nada aqui afirma que os métodos já existem no SEELE.

## 1. Identidades e representação

IDs de sessão/geração/instância são strings opacas criadas pelo anfitrião. IDs de pessoa com representação Rust `u64` também atravessam a nova fachada como strings decimais, evitando arredondamento em JavaScript. Não converter contratos antigos de modo silencioso: um adaptador versionado faz a tradução quando necessário.

IDs de superfície, nó, recurso, assinatura e pedido são locais à instância. Para contadores numéricos, aceitar somente inteiros seguros positivos; ao chegar ao limite, encerrar/recriar a instância ou devolver erro, sem reutilizar silenciosamente IDs. Coordenadas e dimensões devem ser finitas, com intervalos validados. Rejeitar `NaN`, infinito, ciclos de árvore e referências a donos diferentes.

`Json` significa apenas null, boolean, número finito, string, array e objeto de dados. Não transportar funções, protótipos, elementos DOM, caminhos locais livres ou strings que o anfitrião executará. Binários usam recursos/transferências próprios.

## 2. Canal de controle

O anfitrião cria o canal e associa a instância antes de executar o pacote. Um nome enviado pelo autor não concede autoridade. Uma mensagem do executor tem esta forma lógica:

```json
{
  "tipo": "chamada",
  "protocolo": "seele-mod-ui/1",
  "pedido": 17,
  "operacao": "ui.aplicar",
  "argumentos": {
    "superficie": "painel-1",
    "base": 4,
    "operacoes": []
  }
}
```

`seele-mod-ui/1` é o identificador proposto do protocolo interno, **não** uma declaração de API pública 1 nem uma escolha da próxima versão do manifesto. A mensagem não aceita `servidor`, `sessao` ou `mod` para escolher o destino. O receptor usa o vínculo que ele próprio criou. Se uma camada interna precisar carregar esses campos, eles são acrescentados pelo anfitrião e conferidos no destino.

Resposta correlacionada:

```json
{"tipo":"resposta","pedido":17,"ok":true,"valor":{"revisao":5}}
```

```json
{"tipo":"resposta","pedido":17,"ok":false,"erro":{"codigo":"instance-closed","mensagem":"A sessão deste MOD terminou."}}
```

Pedidos crescem monotonicamente por instância. IDs antigos ou duplicados não repetem efeitos. Não reiniciar o contador na mesma instância para reutilizá-los. O servidor continua responsável pela idempotência de operações de domínio quando o autor decidir repetir uma chamada com um novo ID; uma resposta perdida não prova que a gravação não aconteceu.

No bootstrap, o anfitrião informa versão pública da API, protocolo, capacidades e limites aplicados. O SDK só resolve `pronto` após a associação da instância. A gestão distingue bytes carregados de inicialização concluída. O pacote não pode declarar a si mesmo autorizado a capacidades que o anfitrião não concedeu.

## 3. Superfície pública do SDK

Manter as famílias existentes `SeeleMods` e `SeeleUI` onde viável. Métodos novos não são aliases para `invoke` geral.

| Operação proposta | Entrada | Saída e comportamento |
| --- | --- | --- |
| `SeeleMods.ambiente()` | Nenhuma | Versão, capacidades e limites efetivos da instância |
| `SeeleMods.snapshot()` | Nenhuma | Fachada documentada dos dados autorizados da sessão; não expor automaticamente todo campo novo de `Snapshot` interno |
| `SeeleMods.request(id, canal, valor)` | Compatibilidade com a assinatura existente | O `id` deve coincidir com o MOD vinculado; canal validado no servidor. Resposta pertence à conexão de origem |
| `SeeleMods.assinar(topico, filtro, callback)` | Tópico de domínio permitido e filtro tipado | Handle de assinatura. O callback vive no executor; a ponte transporta apenas dados |
| `SeeleMods.cancelarAssinatura(handle)` | Assinatura da instância | Idempotente; eventos já enfileirados são descartados pela geração |
| `SeeleUI.criarSuperficie(descricao)` | ID local, destino, escopo e metadados acessíveis | Handle e revisão inicial; criação revogável e autorização para o destino |
| `SeeleUI.aplicar(handle, base, operacoes)` | Lote de mutações tipadas | Revisão confirmada; validação completa antes de aplicar. Revisão errada é recusada |
| `SeeleUI.aoEvento(handle, callback)` | Superfície e callback local | Retorna função local de cancelamento; descarte da superfície/instância cancela automaticamente. O anfitrião nunca executa o callback |
| `SeeleUI.medir(handle, ids)` | Nós da superfície | Geometria assíncrona relativa à superfície, sem seletores externos; rejeita se a superfície foi descartada |
| `SeeleUI.destruirSuperficie(handle)` | Superfície da instância | Descarte idempotente da árvore e recursos cujo último uso era dela |
| `SeeleUI.tema(valores)` | Camada de propriedades suportadas | Substitui atomicamente a camada deste MOD na sessão; `null` remove somente essa camada |
| `SeeleRecursos.abrir(descricao)` | Recurso do pacote ou ticket autorizado do servidor, tipo e uso | Handle de recurso pronto, sem expor caminho local ou URL de IPC |
| `SeeleRecursos.escolherArquivo(opcoes)` | Tipos e finalidade declarados | Handle temporário ou `null` se o usuário cancelar. O anfitrião valida o gesto e abre o seletor |
| `SeeleRecursos.enviar(handle, ticket)` | Arquivo local autorizado e ticket de transferência | Handle de transferência, progresso e conclusão; não serializar arquivo no canal de controle |
| `SeeleRecursos.cancelar(handle)` | Trabalho pendente da instância | Cancelamento local idempotente; gravações confirmadas não são desfeitas |
| `SeeleRecursos.liberar(handle)` | Recurso da instância | Solta a posse; último uso encerra o recurso real. Uso posterior gera `resource-closed` |
| `SeeleAudio.criar(descricao)` | Recurso de áudio ou grafo de síntese tipado | Handle de player/grafo, inicialmente parado, subordinado às opções locais de áudio |
| `SeeleAudio.controlar(handle, comando)` | Tocar, pausar, parar, buscar, volume ou agendamento | Comando validado; agendamentos e reprodução pertencem à sessão |
| `SeeleVideo.controlar(handle, comando)` | Recurso de vídeo associado a nó de vídeo, com controles equivalentes aplicáveis | Reprodução gerenciada; não inicia câmera, compartilhamento de tela ou microfone implicitamente |

`SeeleUI.regiao(conteudo)` pode permanecer como adaptador para uma superfície e a gramática antiga, caso a política de versões preserve esses pacotes. Não é o único modo de criação. A compatibilidade não pode voltar a executar a API 2 na janela.

Temas, nós, grafos e comandos precisam de tipos/schema publicados antes de E3/E4 terminar. Claude deve declarar todos os campos aceitos, enums, unidades, defaults, erros e exemplos; o receptor recusa campos desconhecidos. Não usar um `Record<string, unknown>` encaminhado diretamente a `style`, atributos ou objetos de mídia como implementação desses contratos.

Cada assinatura pública deve informar se a operação é síncrona ou assíncrona, a propriedade dos handles devolvidos e o comportamento na saída. Chamadas que atravessam o anfitrião retornam Promise; registros puramente locais de callbacks retornam cancelamento local. As APIs de transferência retornam um handle depois de admitir o trabalho, não depois de copiar todos os bytes; progresso e conclusão chegam por eventos correlacionados. Cancelamento ou erro antes da criação não devolve um handle utilizável.

## 4. Nós, mutações e eventos

Conjunto inicial de primitivas: contêiner, texto, entrada, seleção, botão, imagem, canvas e vídeo. A aparência e o layout são combináveis; não criar nós especiais chamados campanha, ficha ou perfil para substituir composição geral.

Cada nó tem `id`, `tipo`, propriedades tipadas e, quando aplicável, filhos. O contrato de propriedades deve cobrir as famílias da arquitetura principal. Valores de cor e tipografia são dados; imagens e fontes são handles; flags de acessibilidade, foco e papéis são declarados. Um nó de texto usa texto como texto, nunca como HTML executável.

Mutações iniciais: `criarNo`, `atualizarNo`, `inserirFilho`, `removerNo`, `focar`, `desenhar`. Criar IDs duplicados, formar ciclos, apontar para nó ausente, mudar o tipo de nó existente ou acessar outra superfície é erro. Remover uma subárvore remove suas assinaturas visuais e referências de recursos. `focar` não move foco para fora da sessão nem toma foco de diálogo confiável do produto.

O lote identifica a revisão que espera atualizar. Validar estrutura, referências, valores e orçamento antes de alterar a apresentação. Uma falha de validação não aplica metade do lote. Se a execução falhar após validação por erro do motor/recurso, tornar a superfície explicitamente inválida e recuperá-la/destruí-la; não confirmar uma revisão cujo estado real é desconhecido. Isso não é uma transação distribuída com gravações no servidor.

Eventos incluem `eventoId`, superfície/nó de origem, tipo e dados mínimos: clique; valor/seleção/composição de entrada; tecla e modificadores; ponteiro em coordenadas locais; arraste; estado/progresso/erro de mídia. Não enviar objetos `Event` ou referências DOM. Coalescer movimento de ponteiro e progresso; preservar ordem de eventos discretos. Remoção do nó invalida eventos pendentes daquele nó.

Texto digitado mantém estado local até reconciliação. Composição de IME não pode ser interrompida a cada snapshot do servidor. No arraste, oferecer feedback local confiável e confirmar o estado autorizado sem exigir uma ida ao servidor por pixel. Testar confirmação atrasada, rejeição do movimento e fechamento da superfície durante captura de ponteiro.

## 5. Desenho, áudio e transferência

Canvas recebe comandos agrupados para limpar, transformar, recortar, desenhar caminhos/formas/texto e referenciar imagens. A API não entrega o contexto real de desenho ao MOD. Limitar dimensões, buffers, comandos e complexidade de caminhos no receptor, com unidades e erros documentados. Atualizar uma peça não reenvia todos os bytes do mapa.

Animações são descrições de propriedades, duração e repetição, executadas pelo renderer e canceladas automaticamente. Evitar mensagens por quadro quando a animação puder ser expressa por parâmetros. Consultas geométricas são assíncronas e não constituem acesso ao DOM global.

MESA usa ambientação sintetizada no cliente anterior. Portanto, oferecer somente reprodução de arquivo não recupera toda a função: definir grafo de áudio com oscilador/buffer, ganho, filtro, delay, conexões e parâmetros/agendamentos suficientes, sob propriedade da sessão. Callbacks de AudioWorklet de terceiros e extensões DSP não entram implicitamente por esse contrato; requerem executor supervisionado e capacidade específica se forem acrescentados.

A seleção de arquivo produz um handle local, nunca bytes ou caminho sem necessidade. O fluxo de envio é: selecionar → pedir autorização ao MOD no servidor → obter ticket → enviar bytes no transporte de volume → confirmar no servidor → atualizar a interface. O token não pode ser escolhido unilateralmente pelo cliente para conceder acesso a disco ou a outro MOD.

O [ADR 0048](../adr/0048-mods-ganham-o-caminho-de-volume-que-os-anexos-ja-tem.md) já define autorização de espera por pessoa/MOD, caminho e tipo. Na revisão consultada, `ModVolume` ainda não tem consumidor no sentido servidor→cliente, e a fachada completa de mídia não está pronta. Claude deve fechar download/serviço de recursos e a ponte FFI/UI necessários. Não recolocar arquivos grandes em JSON para contornar a lacuna, nem criar uma rota pública de arquivos sem autorização.

Servir um recurso exige validar dono, ticket, tipo real, tamanho e destino. Limites de memória de decodificação não são um teto global de disco do servidor; preservar a decisão do ADR 0048. Fontes e imagens empacotadas continuam verificadas pelo hash do pacote e pelos caminhos aceitos em `inner_path`.

## 6. Limites e erros

Limites são declarados pelo anfitrião, conferidos por ele e expostos ao autor para tratamento de erro. Devem existir campos para pedidos em voo, bytes por mensagem/lote, operações por lote, nós/profundidade, superfícies, filas de eventos, imagens decodificadas, buffers de desenho, áudio/vídeo e transferências simultâneas. A unidade de cada campo precisa estar documentada.

Não inventar números de produção neste documento. Na E1/E3, Claude deve escolher valores de bancada, testar excessos e a carga funcional, e registrar os valores que serão publicados. **Uma implementação sem limites concretos no receptor não passa a etapa.** O limite de oito pedidos no SDK atual não é suficiente, pois o autor pode chamar `postMessage` diretamente.

| Código proposto | Significado |
| --- | --- |
| `instance-closed` | Instância encerrando/encerrada; nenhuma repetição nela será aceita |
| `capability-denied` | Operação fora das capacidades/autoridade concedidas |
| `invalid-message` | Envelope, operação, campo ou representação inválidos |
| `stale-revision` | Lote baseado em revisão antiga; obter/reconciliar o estado, sem aplicar parcialmente |
| `resource-closed` | Recurso ausente, descartado ou inválido para esse dono |
| `limit-exceeded` | Orçamento excedido; resposta identifica a categoria sem expor dados de outros MODs |
| `unsupported` | Recurso não implementado nessa API/plataforma; não fingir sucesso |
| `cancelled` | Operação cancelada antes de concluir localmente |
| `timeout` | A espera acabou; isso não prova que uma operação remota não gravou dados |

Erros remotos preservam sua classificação e semântica. IDs, nomes e capacidades de recursos de outros MODs não aparecem em mensagens de erro. Respostas de erro também consomem orçamento: uma inundação não pode forçar o anfitrião a responder sem limite.

## 7. Três fluxos que a bancada deve controlar

**Montagem interrompida:** começar carga do pacote A → suspender resposta de `codigo_do_mod` → sair → entrar de novo com mesmo ID de MOD → liberar resposta antiga. A resposta antiga não cria Worker, superfície nem recurso. Somente a montagem vinculada à nova geração pode terminar.

**Pedido atrasado:** instância A envia pedido 1 → suspender resposta → revogar A → instância B envia pedido 1 → liberar resposta de A. B não recebe a resposta, nem `Ended` de A, e nenhum efeito é aplicado. O listener guarda a geração de origem; ele não lê a geração corrente para etiquetar um evento antigo.

**Mídia após saída:** iniciar arquivo/decodificação/agendamento → revogar a sessão → concluir cada operação em ordens diferentes. Não nasce nó, áudio, buffer ou URL ativo; recursos já criados são liberados. O teste confere o registro do anfitrião e a saída real de mídia, não apenas a rejeição de uma Promise.

Ao terminar cada fluxo, registrar o estado da instância e contagens de recursos por tipo. O supervisor só declara o descarte concluído quando os donos confiáveis confirmam a liberação, ou quando a recuperação comprovadamente destrói o ambiente que os continha. Se faltar confirmação, registrar falha de limpeza e executar a recuperação; nunca marcar sucesso apenas porque a interface foi escondida.
