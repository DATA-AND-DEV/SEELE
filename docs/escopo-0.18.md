# SEELE 0.18 — escopo do lançamento

Este é o novo nome do lote antes chamado 0.17.4. A versão de empacotamento será **0.18.0**. A definição não anuncia publicação nem altera os downloads atuais.

## Entregas

1. **Comunicação navegador ↔ desktop:** chamadas por código nos dois sentidos, conversa pessoal, voz e compartilhamento de tela conforme a capacidade da plataforma. Também mantém navegador ↔ navegador. A VPS serve apenas para encontro e sinalização; não transporta mídia/chat e não utiliza TURN.
2. **Código pessoal na web:** identidade persistente, código, amigos, agenda e perfil no armazenamento do navegador. Receber chamadas exige página aberta; apagar os dados do site pode apagar a identidade. Não inclui hospedar servidores pelo navegador.
3. **Reformulação visual completa:** aplicativo desktop/web, mobile, entrada, perfil, agenda, hospedagem, servidor, salas, conversa pessoal, configurações, diálogos, site de apresentação, documentação e catálogo de MODs. Segue o kit aprovado, com menos texto e controles acessíveis em janelas estreitas/baixas e com texto ampliado. O conteúdo interno de MODs externos não é automaticamente redesenhado pela casca.
4. **Descontinuação dos endereços antigos:** retirar `seele.app.br` e seus subdomínios da navegação, geração de convites e dependências ativas. Auditar separadamente a leitura de convites e registros guardados antigos, com tratamento explícito de endereço aposentado. DNS, serviços e redirecionamentos antigos precisam de conferência no fechamento; não considerar a aposentadoria concluída apenas por trocar os textos.

## Endereços oficiais

| Endereço | Uso |
| --- | --- |
| `seele.sh` | Aplicativo web, entregue pelo Cloudflare via repositório |
| `get.seele.sh` | Apresentação e downloads desktop |
| `docs.seele.sh` | Documentação do produto e MODs |
| `mods.seele.sh` | Catálogo de MODs |
| `rendzvous.seele.sh` | Encontro e sinalização na VPS |

As identidades da origem web antiga não serão transferidas: no domínio novo são novas identidades, conforme decisão já tomada. Documentos históricos e evidências podem mencionar os endereços anteriores sem constituir dependência operacional.

## Estado e critérios de fechamento

Os três primeiros itens têm implementação local e evidências em [integração](evidencias/0.17.4/integracao.md). O histórico conserva o caminho 0.17.4; isso não representa uma release separada.

- [x] Validar chamadas por código, chat e áudio sintético entre o aplicativo Windows de teste e o navegador nos dois sentidos.
- [ ] Homologar instaladores finais e Brave/iPhone físico por 5G; não equivalem à prova controlada local.
- [x] Ensaio automatizado de 15 minutos em Chromium/WebKit e regressões de saída/cancelamento.
- [ ] Validação física de permissões, microfone, seletor de compartilhamento e teclado mobile; Xcode passou em tela cheia/lifecycle curto, mas a captura longa falhou também no Safari sem SEELE.
- [x] Aplicar e testar a aposentadoria dos convites antigos, incluindo a entrada de chamadas pessoais, preservando os servidores já guardados.
- [ ] Retirar os vínculos e registros públicos antigos na janela de publicação; a [auditoria de infraestrutura](evidencias/0.18/dominios-infra.md) ainda encontrou respostas 200 nos sites antigos.
- [ ] Conferir builds finais de web, desktop e encontro com a mesma revisão, versão 0.18.0 e assinaturas do atualizador.
- [ ] Publicar somente após a autorização de lançamento e os critérios de fechamento; não alterar antecipadamente os downloads públicos.

O [relatório de fechamento local](evidencias/0.18/fechamento.md) separa testes aprovados, limites e etapas da publicação.

Não prometer conexão direta em todas as redes, recepção com a página fechada ou reconexão automática da mídia: as limitações atuais estão registradas no [guia em desenvolvimento](usar-a-0.18.md).

## Tratamento concluído dos endereços antigos

Novos convites de servidor e chamadas pessoais recusam `seele.app.br` e subdomínios. Convites pessoais também verificam origem, esquema, porta e ausência de credenciais. A configuração canônica de origem/encontro/STUN vem do protocolo pelo WASM, sem cópias de endereço na interface.

Somente a leitura de servidores já guardados pode migrar o encontro antigo offline; ela preserva identidade, impressão e demais endereços. Configurações explícitas de encontro antigo são recusadas antes de DNS/registro. Os guardas de produção continuam reconhecendo os nomes antigos para impedir testes contra o serviço público.

Testes de protocolo, core, servidor, web e interface passaram, além da revisão independente somente leitura. A [evidência JavaScript](evidencias/0.18/links-js.md) detalha os vetores. DNS, serviços e vínculos públicos foram inspecionados, mas não alterados nesta rodada: a retirada pública continua etapa de lançamento.
